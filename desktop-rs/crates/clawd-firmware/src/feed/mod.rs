//! The firmware feed: clock firmware and packs newer than what the app ships (`feed.py`).
//!
//! A release is published to a private bucket behind a small Worker. The app asks it with its
//! app token: `GET /v1/latest?channel=<channel>` (manifest v1: seq, notes, one row per file
//! `{name, kind, size, enc, sha256, crc}`, plus `renderers[]`), `POST /v1/grant {"seq": N}` (the
//! AES-256-GCM release key and short-lived signed URLs). Every file is
//! `nonce(12) | ciphertext | tag(16)` with AAD `clawd-fw/<seq>/<name>`; it is decrypted in
//! memory and checked against the manifest's size, SHA-256 and CRC. The encrypted files are
//! cached per release in `feed-cache/`.
//!
//! Security (red team #15, frozen): the feed URL must be `https://` (or loopback `http://` for
//! tests); the client **never follows redirects** (a 3xx is an error: the Authorization header
//! therefore never leaves the configured origin, and file URLs from a grant must be relative
//! `/v1/file/...` paths on that origin); the token goes nowhere else.
//! URL/token/channel: env `CLAWD_FEED_URL` / `CLAWD_FEED_TOKEN` / `CLAWD_FEED_CHANNEL` -> config
//! `feed_url` / `feed_token` / `feed_channel` -> build time
//! (`clawd_contracts::BUILD_FEED_URL` / `BUILD_FEED_TOKEN`); without URL and token there is no
//! feed ([`from_settings`] is `None`).

mod cache;
mod crypto;
mod manifest;

use std::collections::HashMap;
use std::ops::Deref;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use base64::Engine as _;
use clawd_contracts::{AppPaths, Env, FeedError};
use serde_json::{Map, Value};
use tokio::task::JoinSet;

use crate::crc32::crc32_hex;
use crate::firmware::{
    is_valid_style_id, sha256_hex, update_state, Bundle, StylePack,
};
use crate::pyval;
use cache::Cache;

pub use clawd_contracts::{FeedFile, FeedManifest as Manifest};
pub use crypto::{decrypt, encrypt, open};
pub use manifest::parse_manifest;

/// `feed.TIMEOUT`.
pub const TIMEOUT: Duration = Duration::from_secs(30);
/// `feed.DOWNLOAD_TIMEOUT`.
pub const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(180);
/// `feed.DOWNLOAD_WORKERS`: files fetched at once; each is a small request, mostly latency.
pub const DOWNLOAD_WORKERS: usize = 8;
/// `feed.CHECK_EVERY` (seconds): the tray's background check.
pub const CHECK_EVERY: f64 = 6.0 * 3600.0;
/// `feed.KINDS`: file kinds a manifest may carry.
pub const KINDS: [&str; 5] = ["fw", "clawdpak", "deskpak", "styles", "style"];
/// `feed.PACK_KINDS`.
pub const PACK_KINDS: [&str; 2] = ["clawdpak", "deskpak"];
/// `feed.STYLE_KINDS`.
pub const STYLE_KINDS: [&str; 2] = ["styles", "style"];
/// `feed.MAX_FILE`.
pub const MAX_FILE: u64 = 4 * 1024 * 1024;
/// `feed.RENDERER_MAX`.
pub const RENDERER_MAX: u64 = 32 * 1024 * 1024;
/// `feed.RENDERER_PLATFORMS`.
pub const RENDERER_PLATFORMS: [&str; 3] = ["win-x64", "mac", "linux-x64"];

/// `feed.renderer_platform()`: this computer's key in `renderers`: `win-x64` (also Windows on
/// Arm), `mac` (one universal file), `linux-x64`.
pub fn renderer_platform() -> &'static str {
    if cfg!(windows) {
        "win-x64"
    } else if cfg!(target_os = "macos") {
        "mac"
    } else {
        "linux-x64"
    }
}

/// `feed.Release`: a feed release in memory; the rest of the app treats it like the bundled
/// firmware (derefs to [`Bundle`]) plus the packs. A bundled firmware is wrapped with
/// [`Release::from_bundle`] (no manifest).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub bundle: Bundle,
    pub manifest: Option<Manifest>,
    /// File name -> plaintext (only what was fetched).
    pub blobs: HashMap<String, Vec<u8>>,
}

impl Deref for Release {
    type Target = Bundle;
    fn deref(&self) -> &Bundle {
        &self.bundle
    }
}

impl Release {
    /// A bundled firmware as a manifest-less release.
    pub fn from_bundle(bundle: Bundle) -> Self {
        Self {
            bundle,
            manifest: None,
            blobs: HashMap::new(),
        }
    }

    /// `Release.notes` (empty without manifest).
    pub fn notes(&self) -> &str {
        self.manifest.as_ref().map_or("", |m| m.notes.as_str())
    }

    /// `Release.date`.
    pub fn date(&self) -> &str {
        self.manifest.as_ref().map_or("", |m| m.date.as_str())
    }

    /// `Release.has(kind)`: the release has files of that kind and all are in memory.
    pub fn has(&self, kind: &str) -> bool {
        let Some(m) = &self.manifest else {
            return false;
        };
        let mut rows = m.files.iter().filter(|f| f.kind == kind).peekable();
        rows.peek().is_some() && rows.all(|f| self.blobs.contains_key(&f.name))
    }

    /// `Release.missing(kinds)`: of `kinds`, the ones the release has but not in memory yet.
    pub fn missing(&self, kinds: &[&str]) -> Vec<String> {
        let Some(m) = &self.manifest else {
            return Vec::new();
        };
        let mut out: Vec<String> = Vec::new();
        for k in kinds {
            if m.files.iter().any(|f| f.kind == *k)
                && !self.has(k)
                && !out.iter().any(|o| o == k)
            {
                out.push((*k).to_owned());
            }
        }
        out
    }

    /// `Release.read()`: the firmware image (once fetched), checked against the manifest.
    pub fn read(&self) -> Result<Vec<u8>, String> {
        let data = self
            .manifest
            .as_ref()
            .and_then(|m| m.file("fw"))
            .and_then(|f| self.blobs.get(&f.name))
            .ok_or_else(|| "the firmware is not downloaded yet".to_owned())?;
        if data.len() != self.bundle.size || sha256_hex(data) != self.bundle.sha256 {
            return Err("feed firmware does not match its manifest".to_owned());
        }
        Ok(data.clone())
    }

    /// `Release.pack(kind)`: `(pack, its CRC-32 hex)` for `clawdpak` / `deskpak`, `None` when the
    /// release has none (or it is not fetched).
    pub fn pack(&self, kind: &str) -> Option<(Vec<u8>, String)> {
        let f = self.manifest.as_ref()?.file(kind)?;
        Some((self.blobs.get(&f.name)?.clone(), f.crc.clone()))
    }
}

/// Style packs of a release from its `styles` index, each verified; empty when absent or not for
/// the manifest's style API (`feed._styles`).
pub fn styles_of(m: &Manifest, blobs: &HashMap<String, Vec<u8>>) -> Vec<StylePack> {
    styles_inner(m, blobs).unwrap_or_default()
}

fn styles_inner(m: &Manifest, blobs: &HashMap<String, Vec<u8>>) -> Option<Vec<StylePack>> {
    let idx = m.file("styles")?;
    let index = blobs.get(&idx.name)?;
    if m.style_api < 1 {
        return None;
    }
    let d: Value = serde_json::from_slice(index).ok()?;
    if pyval::int_of(d.get("api")?)? != i64::from(m.style_api) {
        return None;
    }
    let base = idx
        .name
        .rsplit_once('/')
        .map_or(String::new(), |(dir, _)| format!("{dir}/"));
    let mut out: Vec<StylePack> = Vec::new();
    for e in d.get("styles")?.as_array()? {
        let sid = pyval::str_of(e.get("id")?);
        let fname = pyval::str_of(e.get("file")?);
        let data = blobs.get(&format!("{base}{fname}"));
        if !is_valid_style_id(&sid) || data.is_none() || fname.contains('/') {
            return None;
        }
        out.push(StylePack {
            id: sid,
            name: pyval::str_of(e.get("name")?).chars().take(23).collect(),
            path: PathBuf::from(&fname),
            size: usize::try_from(pyval::int_of(e.get("size")?)?).ok()?,
            crc: u32::try_from(pyval::int_from_str(&pyval::str_of(e.get("crc")?), 16)?).ok()?,
            light: e.get("light").is_some_and(pyval::truthy),
            data: data.cloned(),
        });
    }
    let mut ids: Vec<&str> = out.iter().map(|p| p.id.as_str()).collect();
    ids.sort_unstable();
    ids.dedup();
    if ids.len() != out.len() {
        return None;
    }
    for p in &out {
        p.read().ok()?; // every pack checks against the index and its header
    }
    Some(out)
}

/// `feed.make_release(m, blobs)`: build a [`Release`] from a manifest and the fetched files
/// (style packs from the `styles` index, each checked).
pub fn make_release(m: &Manifest, blobs: HashMap<String, Vec<u8>>) -> Release {
    let (name, size, sha) = m
        .file("fw")
        .map_or((String::new(), 0, String::new()), |f| {
            (f.name.clone(), f.size as usize, f.sha256.clone())
        });
    let bundle = Bundle {
        path: PathBuf::from(name),
        size,
        sha256: sha,
        variant: m.variant.clone(),
        seq: m.seq,
        layouts: m.layouts,
        commit: m.commit.clone(),
        style_api: m.style_api,
        styles: styles_of(m, &blobs),
    };
    Release {
        bundle,
        manifest: Some(m.clone()),
        blobs,
    }
}

/// `progress(kind, done_bytes, total_bytes)` during [`Feed::fetch`].
pub type FetchProgress<'a> = &'a (dyn Fn(&str, u64, u64) + Send + Sync);

/// Whether the token may be sent to `url`: `https://`, or `http://` to a loopback host (tests,
/// a local feed). Python only checked the string prefix `http://127.0.0.1` / `http://localhost`
/// (so `http://127.0.0.1.example.com` passed); the host is parsed here.
pub fn url_allowed(url: &str) -> bool {
    let Ok(u) = reqwest::Url::parse(url) else {
        return false;
    };
    match u.scheme() {
        "https" => u.host_str().is_some_and(|h| !h.is_empty()),
        "http" => matches!(u.host_str(), Some("127.0.0.1" | "localhost" | "[::1]")),
        _ => false,
    }
}

struct FeedInner {
    url: String,
    channel: String,
    http: reqwest::Client,
    cache: Cache,
}

/// `feed.Feed`: the client. One per use; cheap to build and to clone.
#[derive(Clone)]
pub struct Feed {
    inner: Arc<FeedInner>,
}

fn net_error(e: &reqwest::Error) -> FeedError {
    FeedError(format!("feed: {e}"))
}

impl Feed {
    /// `Feed(url, token, channel, cache_dir)`: `channel` other than `stable`/`beta` becomes
    /// `stable`; `cache_dir` `None` = no cache. No redirects are followed.
    pub fn new(url: &str, token: &str, channel: &str, cache_dir: Option<PathBuf>) -> Self {
        let mut auth = reqwest::header::HeaderValue::from_str(&format!("Bearer {token}"))
            .unwrap_or_else(|_| reqwest::header::HeaderValue::from_static("Bearer"));
        auth.set_sensitive(true);
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(reqwest::header::AUTHORIZATION, auth);
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(TIMEOUT)
            .default_headers(headers)
            .build()
            .expect("reqwest client with the bundled TLS backend");
        Self {
            inner: Arc::new(FeedInner {
                url: url.trim_end_matches('/').to_owned(),
                channel: if channel == "stable" || channel == "beta" {
                    channel.to_owned()
                } else {
                    "stable".to_owned()
                },
                http,
                cache: Cache::new(cache_dir),
            }),
        }
    }

    /// The (normalised) channel this client asks for.
    pub fn channel(&self) -> &str {
        &self.inner.channel
    }

    /// The feed's base URL without a trailing `/`.
    pub fn url(&self) -> &str {
        &self.inner.url
    }

    /// `Feed._get`: a request that must answer below 400. A redirect (3xx) is an error too,
    /// because it is never followed.
    async fn send(
        &self,
        req: reqwest::RequestBuilder,
        path: &str,
    ) -> Result<reqwest::Response, FeedError> {
        if !url_allowed(&self.inner.url) {
            return Err(FeedError(
                "feed: the URL must be https (or loopback http)".to_owned(),
            ));
        }
        let r = req.send().await.map_err(|e| net_error(&e))?;
        let code = r.status().as_u16();
        if code >= 300 {
            return Err(FeedError(format!(
                "feed: HTTP {code} for {}",
                path.split('?').next().unwrap_or(path)
            )));
        }
        Ok(r)
    }

    /// `Feed.latest()`: the channel's newest release manifest; `Ok(None)` on 404.
    pub async fn latest(&self) -> Result<Option<Manifest>, FeedError> {
        if !url_allowed(&self.inner.url) {
            return Err(FeedError(
                "feed: the URL must be https (or loopback http)".to_owned(),
            ));
        }
        let req = self
            .inner
            .http
            // the channel is `stable` or `beta` only (see `Feed::new`): nothing to escape
            .get(format!("{}/v1/latest?channel={}", self.inner.url, self.inner.channel));
        let r = req.send().await.map_err(|e| net_error(&e))?;
        let code = r.status().as_u16();
        if code == 404 {
            return Ok(None);
        }
        if code >= 300 {
            return Err(FeedError(format!("feed: HTTP {code} for /v1/latest")));
        }
        let body = r.bytes().await.map_err(|e| net_error(&e))?;
        let parsed = serde_json::from_slice::<Value>(&body)
            .map_err(|e| FeedError(e.to_string()))
            .and_then(|v| parse_manifest(&v));
        parsed
            .map(Some)
            .map_err(|e| FeedError(format!("feed: bad manifest ({})", e.0)))
    }

    /// `Feed.fetch(rel, kinds, progress)`: `rel` with the files of `kinds` downloaded (8 at a
    /// time), decrypted and verified; the first failure ends the fetch.
    pub async fn fetch(
        &self,
        rel: &Release,
        kinds: &[&str],
        progress: Option<FetchProgress<'_>>,
    ) -> Result<Release, FeedError> {
        let Some(m) = &rel.manifest else {
            return Err(FeedError("feed: a bundled release has no manifest".to_owned()));
        };
        let want: Vec<FeedFile> = m
            .files
            .iter()
            .filter(|f| kinds.contains(&f.kind.as_str()) && !rel.blobs.contains_key(&f.name))
            .cloned()
            .collect();
        if want.is_empty() {
            return Ok(rel.clone());
        }
        let (key, urls) = self.grant(m.seq, &want).await?;
        let mut blobs = rel.blobs.clone();
        let total: u64 = want.iter().map(|f| f.enc).sum();
        let mut done = 0u64;
        let say = |kind: &str, d: u64| {
            if let Some(p) = progress {
                p(kind, d, total);
            }
        };
        say(&want[0].kind, 0);
        let gate = Arc::new(tokio::sync::Semaphore::new(DOWNLOAD_WORKERS));
        let mut jobs: JoinSet<Result<(FeedFile, Vec<u8>), FeedError>> = JoinSet::new();
        for f in want {
            let (this, key, url, gate) = (self.clone(), key.clone(), urls[&f.name].clone(), gate.clone());
            let seq = m.seq;
            jobs.spawn(async move {
                let _slot = gate.acquire().await.map_err(|e| FeedError(e.to_string()))?;
                let data = this.load(&key, seq, &f, &url).await?;
                Ok((f, data))
            });
        }
        while let Some(joined) = jobs.join_next().await {
            // the first failure ends the fetch: dropping `jobs` aborts the rest
            let (f, data) = joined.map_err(|e| FeedError(format!("feed: {e}")))??;
            done += f.enc;
            say(&f.kind, done);
            blobs.insert(f.name, data);
        }
        say("", total);
        Ok(make_release(m, blobs))
    }

    /// `Feed.fetch_renderer(rel, platform)`: the release's renderer zip for `platform`,
    /// decrypted and checked; `Ok(None)` when it has none.
    pub async fn fetch_renderer(
        &self,
        rel: &Release,
        platform: &str,
    ) -> Result<Option<(FeedFile, Vec<u8>)>, FeedError> {
        let Some(m) = &rel.manifest else {
            return Ok(None);
        };
        let Some(f) = m.renderer(platform) else {
            return Ok(None);
        };
        let (key, urls) = self.grant(m.seq, std::slice::from_ref(f)).await?;
        let data = self.load(&key, m.seq, f, &urls[&f.name]).await?;
        Ok(Some((f.clone(), data)))
    }

    /// `Feed._grant`: the release key and a signed URL for every file in `want`.
    async fn grant(
        &self,
        seq: u64,
        want: &[FeedFile],
    ) -> Result<(Vec<u8>, HashMap<String, String>), FeedError> {
        let bad = || FeedError("feed: bad grant".to_owned());
        let req = self
            .inner
            .http
            .post(format!("{}/v1/grant", self.inner.url))
            .json(&serde_json::json!({ "seq": seq }));
        let r = self.send(req, "/v1/grant").await?;
        let body = r.bytes().await.map_err(|e| net_error(&e))?;
        let g: Value = serde_json::from_slice(&body).map_err(|_| bad())?;
        let key = g
            .get("key")
            .and_then(Value::as_str)
            .and_then(|k| base64::engine::general_purpose::STANDARD.decode(k).ok())
            .filter(|k| k.len() == 32)
            .ok_or_else(bad)?;
        let urls = g.get("urls").and_then(Value::as_object).ok_or_else(bad)?;
        let mut out = HashMap::new();
        for f in want {
            match urls.get(&f.name) {
                Some(Value::String(u)) if u.starts_with("/v1/file/") => {
                    out.insert(f.name.clone(), u.clone());
                }
                _ => return Err(FeedError(format!("feed: no URL for {}", f.name))),
            }
        }
        Ok((key, out))
    }

    /// `Feed._load`: one file, checked: from the cache when it is there and still decrypts, else
    /// downloaded.
    async fn load(
        &self,
        key: &[u8],
        seq: u64,
        f: &FeedFile,
        url: &str,
    ) -> Result<Vec<u8>, FeedError> {
        let cached = {
            let (inner, key, f) = (self.inner.clone(), key.to_vec(), f.clone());
            tokio::task::spawn_blocking(move || {
                let blob = inner.cache.read(seq, &f)?;
                match open(&key, seq, &f, &blob) {
                    Ok(data) => Some(data),
                    Err(_) => {
                        inner.cache.drop_file(seq, &f); // damaged or from another key: fetch it again
                        None
                    }
                }
            })
            .await
            .unwrap_or(None)
        };
        if let Some(data) = cached {
            return Ok(data);
        }
        let blob = self.download(url, f).await?;
        let (key, file, blob2) = (key.to_vec(), f.clone(), blob.clone());
        let data = tokio::task::spawn_blocking(move || open(&key, seq, &file, &blob2))
            .await
            .map_err(|e| FeedError(format!("feed: {e}")))??;
        let (inner, file) = (self.inner.clone(), f.clone());
        let _ = tokio::task::spawn_blocking(move || inner.cache.write(seq, &file, &blob)).await;
        Ok(data)
    }

    /// The encrypted bytes of one file, streamed; refuses more than the manifest's `enc`.
    async fn download(&self, url: &str, f: &FeedFile) -> Result<Vec<u8>, FeedError> {
        let req = self
            .inner
            .http
            .get(format!("{}{url}", self.inner.url))
            .timeout(DOWNLOAD_TIMEOUT);
        let mut r = self.send(req, url).await?;
        let mut blob = Vec::with_capacity(usize::try_from(f.enc).unwrap_or(0).min(1 << 24));
        while let Some(chunk) = r.chunk().await.map_err(|e| net_error(&e))? {
            blob.extend_from_slice(&chunk);
            if blob.len() as u64 > f.enc {
                break;
            }
        }
        Ok(blob)
    }
}

/// `feed.from_settings()`: the feed this install uses; `None` when it has no URL or token, or the
/// URL is neither `https://` nor loopback `http://`. Env first, then the config file
/// (`feed_url`, `feed_token`, `feed_channel`), then the values baked in at build time.
pub fn from_settings(paths: &AppPaths, env: &Env) -> Option<Feed> {
    from_sources(
        env,
        |name| clawd_config::read_config_value(paths, name),
        clawd_contracts::BUILD_FEED_URL.unwrap_or(""),
        clawd_contracts::BUILD_FEED_TOKEN.unwrap_or(""),
        Some(paths.feed_cache_dir()),
    )
}

/// [`from_settings`] with the config lookup and build defaults given. Lookups are lazy, like
/// Python's `a or b or c`.
pub fn from_sources(
    env: &Env,
    config: impl Fn(&str) -> Option<String>,
    build_url: &str,
    build_token: &str,
    cache_dir: Option<PathBuf>,
) -> Option<Feed> {
    let pick = |var: &str, key: &str, built: &str| {
        env.get(var)
            .or_else(|| config(key).filter(|v| !v.is_empty()))
            .unwrap_or_else(|| built.to_owned())
    };
    let url = pick("CLAWD_FEED_URL", "feed_url", build_url);
    let token = pick("CLAWD_FEED_TOKEN", "feed_token", build_token);
    if url.is_empty() || token.is_empty() || !url_allowed(&url) {
        return None;
    }
    let channel = pick("CLAWD_FEED_CHANNEL", "feed_channel", "stable");
    Some(Feed::new(&url, &token, &channel, cache_dir))
}

// ---- what a clock needs from a release -----------------------------------------------

/// `feed.HEAP_KEYS`: pack kind -> (count key, CRC key) in `/api/heap`.
pub fn heap_keys(kind: &str) -> Option<(&'static str, &'static str)> {
    match kind {
        "clawdpak" => Some(("pak", "pakCrc")),
        "deskpak" => Some(("desk", "deskCrc")),
        _ => None,
    }
}

/// `feed.pack_state(heap, kind, crc)`: `"ok"` (the clock has this pack) | `"need"` | `"wait"`
/// (it is still checksumming it) | `"unknown"` (its firmware does not say: decided after the
/// firmware update).
pub fn pack_state(heap: &Value, kind: &str, crc: &str) -> &'static str {
    let Some((count_key, crc_key)) = heap_keys(kind) else {
        return "unknown";
    };
    let zero_count = match heap.get(count_key) {
        Some(Value::Number(n)) => n.as_i64() == Some(0),
        Some(Value::Bool(false)) => true, // bool is an int in Python
        _ => false,
    };
    if zero_count {
        return "need"; // missing or unreadable
    }
    let Some(have) = heap.get(crc_key) else {
        return "unknown";
    };
    if have.as_str() == Some("") {
        "wait"
    } else if have.as_str() == Some(crc) {
        "ok"
    } else {
        "need"
    }
}

/// `feed.plan(heap, rel)`: what installing `rel` means for this clock: `{"fw": "need"}` when its
/// image is newer, and per pack of a feed release its [`pack_state`]; `{}` when nothing applies.
/// Insertion order is `fw`, `clawdpak`, `deskpak`.
pub fn plan(heap: &Value, rel: Option<&Release>) -> Map<String, Value> {
    let mut out = Map::new();
    let Some(rel) = rel else {
        return out;
    };
    let state = update_state(heap, Some(&rel.bundle));
    if state != "available" && state != "current" {
        return out;
    }
    if state == "available" {
        out.insert("fw".to_owned(), Value::from("need"));
    }
    for kind in PACK_KINDS {
        if let Some(f) = rel.manifest.as_ref().and_then(|m| m.file(kind)) {
            out.insert(kind.to_owned(), Value::from(pack_state(heap, kind, &f.crc)));
        }
    }
    out
}

/// `feed.needed(heap, rel)`: the kinds to download for an update: the firmware if newer, and
/// every pack not known to match (`need`, `wait`, `unknown`).
pub fn needed(heap: &Value, rel: Option<&Release>) -> Vec<String> {
    plan(heap, rel)
        .into_iter()
        .filter(|(_, s)| s.as_str() != Some("ok"))
        .map(|(k, _)| k)
        .collect()
}

/// `feed.worth_it(heap, rel)`: something to install now: a newer image, or a pack known to differ.
pub fn worth_it(heap: &Value, rel: Option<&Release>) -> bool {
    let p = plan(heap, rel);
    p.get("fw").and_then(Value::as_str) == Some("need")
        || PACK_KINDS
            .iter()
            .any(|k| p.get(*k).and_then(Value::as_str) == Some("need"))
}

/// `feed.offer(heap, m, bundle)`: the release number the tray should offer for this clock, `0` for
/// none. A bundled image at least as new as the feed's wins (a private build).
pub fn offer(heap: &Value, m: Option<&Manifest>, bundle: Option<&Bundle>) -> u64 {
    let Some(m) = m else {
        return 0;
    };
    if let Some(b) = bundle.filter(|b| b.seq >= m.seq) {
        return if update_state(heap, Some(b)) == "available" {
            b.seq
        } else {
            0
        };
    }
    if worth_it(heap, Some(&make_release(m, HashMap::new()))) {
        m.seq
    } else {
        0
    }
}

/// `f"{zlib.crc32(data):08x}"` re-export for tests and tools.
pub fn crc_hex(data: &[u8]) -> String {
    crc32_hex(data)
}
