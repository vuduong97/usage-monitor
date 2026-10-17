//! The packs a clock needs besides its image, and the usage style packs that arrive later.
//!
//! **Animation packs** (`clawd.pak`, `desk.pak`) ship next to the bundled image
//! ([`crate::firmware::Bundle::packs`]); `/api/heap` reports the CRC of what the clock has, so
//! [`plan`] says per pack whether it must be sent.
//!
//! **Remote style packs**: the author publishes new usage screens between app releases as a
//! fixed GitHub Release per style API, tag `packs-api{N}` of this repo, holding `index.json`,
//! `index.json.minisig` and `<id>.pak` (`tools/publish_packs.sh`). [`StyleSource::latest`]
//! downloads the index, checks its minisign signature against the app's key
//! ([`clawd_contracts::MINISIGN_PUBKEY`], the one that signs app releases), then every pack
//! against the SHA-256 the signed index gives and its own header. Packs are cached by SHA-256 in
//! `style-packs/` (the preview renderer reads them there). [`choose`] uses them instead of the
//! bundled ones only when they are for the bundled image's style API and newer (`rev`).
//!
//! Nothing here installs firmware: the image only ever comes with the app.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use clawd_contracts::{AppPaths, Env, MINISIGN_PUBKEY, UPDATE_REPO};
use serde_json::{Map, Value};
use tokio::task::JoinSet;

use crate::firmware::{is_valid_style_id, sha256_hex, update_state, Bundle, StylePack, STYLE_MAX};
use crate::pyval;

/// The animation pack kinds, in the order they are sent.
pub const PACK_KINDS: [&str; 2] = ["clawdpak", "deskpak"];
/// Tag of the remote style packs for style API `N`: `packs-api{N}`.
pub const TAG_PREFIX: &str = "packs-api";
/// The signed index of a remote release.
pub const INDEX_NAME: &str = "index.json";
/// Its minisign signature.
pub const SIGNATURE_NAME: &str = "index.json.minisig";
/// Caps on what a remote release may send.
pub const INDEX_MAX: usize = 256 * 1024;
pub const SIGNATURE_MAX: usize = 4096;
pub const MAX_STYLES: usize = 200;
/// One request; packs are a few KB each.
pub const TIMEOUT: Duration = Duration::from_secs(30);
/// Packs fetched at once.
pub const DOWNLOAD_WORKERS: usize = 8;

// ---- animation packs --------------------------------------------------------------------

/// `/api/heap` keys of a pack kind: (count, CRC).
pub fn heap_keys(kind: &str) -> Option<(&'static str, &'static str)> {
    match kind {
        "clawdpak" => Some(("pak", "pakCrc")),
        "deskpak" => Some(("desk", "deskCrc")),
        _ => None,
    }
}

/// `"ok"` (the clock has this pack) | `"need"` | `"wait"` (it is still checksumming it) |
/// `"unknown"` (its firmware does not say: decided after the firmware update).
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

/// What installing the bundle means for this clock: `{"fw": "need"}` when its image is newer,
/// and per animation pack its [`pack_state`]; `{}` when nothing applies (no bundle, not a clawd
/// clock). Insertion order is `fw`, `clawdpak`, `deskpak`.
pub fn plan(heap: &Value, bundle: Option<&Bundle>) -> Map<String, Value> {
    let mut out = Map::new();
    let Some(b) = bundle else {
        return out;
    };
    let state = update_state(heap, Some(b));
    if state != "available" && state != "current" {
        return out;
    }
    if state == "available" {
        out.insert("fw".to_owned(), Value::from("need"));
    }
    for kind in PACK_KINDS {
        if let Some(p) = b.pack(kind) {
            out.insert(kind.to_owned(), Value::from(pack_state(heap, kind, &p.crc)));
        }
    }
    out
}

/// Something to install now: a newer image, or a pack known to differ.
pub fn worth_it(heap: &Value, bundle: Option<&Bundle>) -> bool {
    let p = plan(heap, bundle);
    p.get("fw").and_then(Value::as_str) == Some("need")
        || PACK_KINDS
            .iter()
            .any(|k| p.get(*k).and_then(Value::as_str) == Some("need"))
}

/// The firmware number the tray offers for this clock (the bundle's), `0` for none.
pub fn offer(heap: &Value, bundle: Option<&Bundle>) -> u64 {
    match bundle {
        Some(b) if worth_it(heap, Some(b)) => b.seq,
        _ => 0,
    }
}

// ---- remote style packs -----------------------------------------------------------------

/// The remote packs did not arrive, or did not check out. The message is for the log and the
/// Update tab; the bundled packs stay in use.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("style packs: {0}")]
pub struct PacksError(pub String);

/// A checked remote release: its style API, `rev` and packs (in memory; `path` is the cached
/// copy when there is a cache, which the preview renderer reads).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteStyles {
    pub api: u32,
    pub rev: u64,
    pub styles: Vec<StylePack>,
}

/// One row of a signed index, checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexRow {
    pub id: String,
    pub name: String,
    pub file: String,
    pub size: usize,
    pub crc: u32,
    pub light: bool,
    pub sha256: String,
}

/// A signed index, checked: `{"api", "rev", "styles": [{id, name, file, size, crc, light,
/// sha256}]}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Index {
    pub api: u32,
    pub rev: u64,
    pub rows: Vec<IndexRow>,
}

fn bad(what: impl Into<String>) -> PacksError {
    PacksError(what.into())
}

fn is_hex(s: &str, len: usize) -> bool {
    s.len() == len
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Parse and check a signed index for style API `api`. The whole index is refused on the first
/// row that does not check out: a partial set would silently drop screens.
pub fn parse_index(bytes: &[u8], api: u32) -> Result<Index, PacksError> {
    let v: Value = serde_json::from_slice(bytes).map_err(|_| bad("the index is not JSON"))?;
    let got_api = v.get("api").and_then(Value::as_u64);
    if got_api != Some(u64::from(api)) {
        return Err(bad(format!(
            "the index is for style API {}, the firmware draws {api}",
            got_api.map_or_else(|| "?".to_owned(), |a| a.to_string())
        )));
    }
    let rev = v
        .get("rev")
        .and_then(Value::as_u64)
        .filter(|r| *r > 0)
        .ok_or_else(|| bad("the index has no rev"))?;
    let list = v
        .get("styles")
        .and_then(Value::as_array)
        .filter(|l| !l.is_empty() && l.len() <= MAX_STYLES)
        .ok_or_else(|| bad("the index lists no packs (or too many)"))?;
    let mut rows = Vec::with_capacity(list.len());
    let mut seen = HashSet::new();
    for e in list {
        let s = |k: &str| e.get(k).and_then(Value::as_str).unwrap_or_default();
        let (id, file, crc, sha) = (s("id"), s("file"), s("crc"), s("sha256"));
        let size = e.get("size").and_then(Value::as_u64).unwrap_or(0);
        let stem = file.strip_suffix(".pak").unwrap_or_default();
        if !is_valid_style_id(id)
            || !is_valid_style_id(stem)
            || !(1..=STYLE_MAX as u64).contains(&size)
            || !is_hex(crc, 8)
            || !is_hex(sha, 64)
            || !seen.insert(id.to_owned())
        {
            return Err(bad(format!(
                "index row {} does not check out",
                pyval::str_repr(id)
            )));
        }
        rows.push(IndexRow {
            id: id.to_owned(),
            name: s("name").chars().take(23).collect(),
            file: file.to_owned(),
            size: size as usize,
            crc: u32::from_str_radix(crc, 16).map_err(|_| bad("bad crc"))?,
            light: e.get("light").is_some_and(pyval::truthy),
            sha256: sha.to_owned(),
        });
    }
    Ok(Index { api, rev, rows })
}

/// `index` signed by `public_key` (the base64 line of a minisign `.pub`)? A placeholder key, a
/// malformed signature or a wrong one all fail closed.
pub fn verify_index(index: &[u8], sig: &str, public_key: &str) -> Result<(), PacksError> {
    let key = minisign_verify::PublicKey::from_base64(public_key.trim())
        .map_err(|_| bad("the app has no valid signing key"))?;
    let signature = minisign_verify::Signature::decode(sig)
        .map_err(|_| bad("the index signature is unreadable"))?;
    key.verify(index, &signature, true)
        .map_err(|_| bad("the index signature does not match: not from the author"))
}

/// The bundle as the window shows it: its own style packs, or the remote ones when they are for
/// the image's style API and newer than the bundled ones.
pub fn choose(bundle: &Bundle, remote: Option<&RemoteStyles>) -> Bundle {
    let mut b = bundle.clone();
    if let Some(r) = remote.filter(|r| uses_remote(bundle, r)) {
        b.styles = r.styles.clone();
        b.style_rev = r.rev;
    }
    b
}

/// [`choose`]'s rule.
pub fn uses_remote(bundle: &Bundle, remote: &RemoteStyles) -> bool {
    bundle.style_api >= 1 && remote.api == bundle.style_api && remote.rev > bundle.style_rev
}

/// Whether the client may ask `url`: `https://`, or `http://` to a loopback host (tests).
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

struct SourceInner {
    base: String,
    public_key: String,
    cache: Option<PathBuf>,
    http: reqwest::Client,
}

/// Where the remote style packs come from. Cheap to clone.
#[derive(Clone)]
pub struct StyleSource {
    inner: Arc<SourceInner>,
}

fn net(e: reqwest::Error) -> PacksError {
    PacksError(e.without_url().to_string())
}

impl StyleSource {
    /// `base`: the folder that holds one folder per release tag (GitHub:
    /// `https://github.com/<repo>/releases/download`). Redirects are followed only to https (or
    /// loopback) URLs: GitHub serves the files from another host, and every byte is checked
    /// against the signed index anyway.
    pub fn new(base: &str, public_key: &str, cache: Option<PathBuf>) -> Self {
        let policy = reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 5 || !url_allowed(attempt.url().as_str()) {
                attempt.stop()
            } else {
                attempt.follow()
            }
        });
        let http = reqwest::Client::builder()
            .redirect(policy)
            .timeout(TIMEOUT)
            .user_agent(concat!("Clawd/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("reqwest client with the bundled TLS backend");
        Self {
            inner: Arc::new(SourceInner {
                base: base.trim_end_matches('/').to_owned(),
                public_key: public_key.to_owned(),
                cache,
                http,
            }),
        }
    }

    /// The base URL without a trailing `/`.
    pub fn base(&self) -> &str {
        &self.inner.base
    }

    /// The URL of `name` in the release of style API `api`.
    pub fn url(&self, api: u32, name: &str) -> String {
        format!("{}/{TAG_PREFIX}{api}/{name}", self.inner.base)
    }

    /// GET `url`: at most `max` bytes, `Ok(None)` on 404.
    async fn get(&self, url: &str, max: usize) -> Result<Option<Vec<u8>>, PacksError> {
        if !url_allowed(url) {
            return Err(bad("the packs URL must be https (or loopback http)"));
        }
        let mut r = self.inner.http.get(url).send().await.map_err(net)?;
        let code = r.status().as_u16();
        if code == 404 {
            return Ok(None);
        }
        if !(200..300).contains(&code) {
            let name = url.rsplit('/').next().unwrap_or(url);
            return Err(bad(format!("HTTP {code} for {name}")));
        }
        let mut body = Vec::new();
        while let Some(chunk) = r.chunk().await.map_err(net)? {
            body.extend_from_slice(&chunk);
            if body.len() > max {
                return Err(bad("a file is larger than allowed"));
            }
        }
        Ok(Some(body))
    }

    /// The newest remote packs for style API `api`, checked; `Ok(None)` when the author has not
    /// published any for it (no release, 404).
    pub async fn latest(&self, api: u32) -> Result<Option<RemoteStyles>, PacksError> {
        if api < 1 {
            return Ok(None);
        }
        let Some(index) = self.get(&self.url(api, INDEX_NAME), INDEX_MAX).await? else {
            return Ok(None);
        };
        let sig = self
            .get(&self.url(api, SIGNATURE_NAME), SIGNATURE_MAX)
            .await?
            .ok_or_else(|| bad("the index has no signature"))?;
        let sig = String::from_utf8(sig).map_err(|_| bad("the index signature is unreadable"))?;
        verify_index(&index, &sig, &self.inner.public_key)?;
        let index = parse_index(&index, api)?;
        let styles = self.packs(api, &index.rows).await?;
        self.prune(&styles);
        Ok(Some(RemoteStyles {
            api,
            rev: index.rev,
            styles,
        }))
    }

    /// Every pack of the index, from the cache when it is there and intact, else downloaded;
    /// each checked against its row (SHA-256) and its own header. The first failure ends it.
    async fn packs(&self, api: u32, rows: &[IndexRow]) -> Result<Vec<StylePack>, PacksError> {
        let gate = Arc::new(tokio::sync::Semaphore::new(DOWNLOAD_WORKERS));
        let mut jobs: JoinSet<Result<(usize, StylePack), PacksError>> = JoinSet::new();
        for (i, row) in rows.iter().enumerate() {
            let (this, row, gate) = (self.clone(), row.clone(), Arc::clone(&gate));
            jobs.spawn(async move {
                let _slot = gate.acquire().await.map_err(|e| bad(e.to_string()))?;
                Ok((i, this.pack(api, &row).await?))
            });
        }
        let mut out: Vec<Option<StylePack>> = vec![None; rows.len()];
        while let Some(joined) = jobs.join_next().await {
            // the first failure ends it: dropping `jobs` aborts the rest
            let (i, p) = joined.map_err(|e| bad(e.to_string()))??;
            out[i] = Some(p);
        }
        Ok(out.into_iter().flatten().collect())
    }

    async fn pack(&self, api: u32, row: &IndexRow) -> Result<StylePack, PacksError> {
        let cached = self
            .inner
            .cache
            .as_ref()
            .map(|d| d.join(format!("{}.pak", row.sha256)));
        let from_cache = match &cached {
            Some(p) => std::fs::read(p)
                .ok()
                .filter(|d| sha256_hex(d) == row.sha256),
            None => None,
        };
        let data = match from_cache {
            Some(d) => d,
            None => {
                let d = self
                    .get(&self.url(api, &row.file), STYLE_MAX)
                    .await?
                    .ok_or_else(|| bad(format!("{} is missing from the release", row.file)))?;
                if sha256_hex(&d) != row.sha256 {
                    return Err(bad(format!("{} does not match the signed index", row.file)));
                }
                if let Some(p) = &cached {
                    write_cached(p, &d);
                }
                d
            }
        };
        let pack = StylePack {
            id: row.id.clone(),
            name: row.name.clone(),
            path: cached.unwrap_or_else(|| PathBuf::from(&row.file)),
            size: row.size,
            crc: row.crc,
            light: row.light,
            data: Some(data),
        };
        pack.read().map_err(PacksError)?; // size, CSTY magic and CRC, like a bundled pack
        Ok(pack)
    }

    /// Drop cached packs no longer in use (best effort).
    fn prune(&self, keep: &[StylePack]) {
        let Some(dir) = &self.inner.cache else {
            return;
        };
        let keep: HashSet<&PathBuf> = keep.iter().map(|p| &p.path).collect();
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for e in entries.flatten() {
            let path = e.path();
            if path.extension().is_some_and(|x| x == "pak") && !keep.contains(&path) {
                let _ = std::fs::remove_file(&path);
            }
        }
    }
}

/// Write a cached pack through a temp file, so a reader never sees half of one.
fn write_cached(path: &std::path::Path, data: &[u8]) {
    let Some(dir) = path.parent() else {
        return;
    };
    if std::fs::create_dir_all(dir).is_err() {
        return;
    }
    let tmp = path.with_extension("tmp");
    if std::fs::write(&tmp, data).is_ok() && std::fs::rename(&tmp, path).is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
}

/// The remote packs this install reads: env `CLAWD_PACKS_URL`, then config `packs_url`, then
/// this repo's GitHub Releases. `None` when switched off (`packs_url = off`) or not https.
pub fn from_settings(paths: &AppPaths, env: &Env) -> Option<StyleSource> {
    let base = env
        .get("CLAWD_PACKS_URL")
        .or_else(|| clawd_config::read_config_value(paths, "packs_url").filter(|v| !v.is_empty()))
        .unwrap_or_else(|| format!("https://github.com/{UPDATE_REPO}/releases/download"));
    if base.eq_ignore_ascii_case("off") || !url_allowed(&base) {
        return None;
    }
    Some(StyleSource::new(
        &base,
        MINISIGN_PUBKEY,
        Some(paths.style_packs_dir()),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crc32::{crc32, crc32_hex};
    use serde_json::json;

    fn style_pack(sid: &str) -> Vec<u8> {
        let mut rest = sid.as_bytes().to_vec();
        rest.resize(16, 0);
        rest.extend_from_slice(&[0; 64]);
        let mut out = b"CSTY".to_vec();
        out.extend_from_slice(&[2, 0, 96, 0]);
        out.extend_from_slice(&(rest.len() as u32).to_le_bytes());
        out.extend_from_slice(&crc32(&rest).to_le_bytes());
        out.extend_from_slice(&rest);
        out
    }

    fn row(sid: &str) -> Value {
        let d = style_pack(sid);
        json!({"id": sid, "name": sid, "file": format!("{sid}.pak"), "size": d.len(),
               "crc": format!("{:08x}", crc32(&d[16..])), "light": sid == "paper",
               "sha256": sha256_hex(&d)})
    }

    #[test]
    fn index_rows_are_all_checked() {
        let good = json!({"api": 2, "rev": 7, "styles": [row("synth"), row("paper")]});
        let idx = parse_index(good.to_string().as_bytes(), 2).unwrap();
        assert_eq!((idx.api, idx.rev, idx.rows.len()), (2, 7, 2));
        assert!(idx.rows[1].light && !idx.rows[0].light);
        assert!(
            parse_index(good.to_string().as_bytes(), 1).is_err(),
            "another style API"
        );
        type Edit = Box<dyn Fn(&mut Value)>;
        let edits: Vec<Edit> = vec![
            Box::new(|v| v["rev"] = json!(0)),
            Box::new(|v| v["styles"] = json!([])),
            Box::new(|v| v["styles"][0]["id"] = json!("Bad Id")),
            Box::new(|v| v["styles"][0]["file"] = json!("../x.pak")),
            Box::new(|v| v["styles"][0]["file"] = json!("synth.bin")),
            Box::new(|v| v["styles"][0]["size"] = json!(STYLE_MAX + 1)),
            Box::new(|v| v["styles"][0]["sha256"] = json!("ab")),
            Box::new(|v| v["styles"][0]["crc"] = json!("XYZ01234")),
            Box::new(|v| v["styles"][1]["id"] = json!("synth")),
        ];
        for (i, edit) in edits.iter().enumerate() {
            let mut v = good.clone();
            edit(&mut v);
            assert!(
                parse_index(v.to_string().as_bytes(), 2).is_err(),
                "edit {i}"
            );
        }
        assert!(parse_index(b"not json", 2).is_err());
    }

    #[test]
    fn a_placeholder_key_or_a_bad_signature_fails_closed() {
        assert!(verify_index(b"{}", "x", clawd_contracts::MINISIGN_PUBKEY_PLACEHOLDER).is_err());
        let kp = minisign::KeyPair::generate_unencrypted_keypair().unwrap();
        let pk = kp.pk.to_base64();
        let sig = minisign::sign(
            Some(&kp.pk),
            &kp.sk,
            std::io::Cursor::new(b"{}"),
            None,
            None,
        )
        .unwrap()
        .to_string();
        assert!(verify_index(b"{}", &sig, &pk).is_ok());
        assert!(verify_index(b"{ }", &sig, &pk).is_err());
        assert!(verify_index(b"{}", "garbage", &pk).is_err());
    }

    fn bundle_with(style_api: u32, rev: u64) -> Bundle {
        Bundle {
            path: PathBuf::from("fw.bin"),
            size: 1,
            sha256: String::new(),
            variant: crate::firmware::CLAWD_VARIANT.to_owned(),
            seq: 8,
            layouts: 1,
            commit: "c".into(),
            style_api,
            styles: Vec::new(),
            style_rev: rev,
            packs: vec![crate::firmware::PackFile {
                kind: "clawdpak".into(),
                path: PathBuf::from("clawd.pak"),
                size: 3,
                crc: crc32_hex(b"abc"),
            }],
        }
    }

    #[test]
    fn remote_packs_win_only_for_the_same_api_and_a_higher_rev() {
        let remote = |api, rev| RemoteStyles {
            api,
            rev,
            styles: vec![StylePack {
                id: "new".into(),
                name: "New".into(),
                path: PathBuf::from("new.pak"),
                size: 1,
                crc: 0,
                light: false,
                data: Some(vec![1]),
            }],
        };
        let b = bundle_with(2, 100);
        assert_eq!(choose(&b, Some(&remote(2, 101))).styles.len(), 1);
        assert_eq!(choose(&b, Some(&remote(2, 101))).style_rev, 101);
        assert!(
            choose(&b, Some(&remote(2, 100))).styles.is_empty(),
            "same rev: bundled"
        );
        assert!(
            choose(&b, Some(&remote(1, 999))).styles.is_empty(),
            "other API"
        );
        assert!(choose(&bundle_with(0, 0), Some(&remote(0, 9)))
            .styles
            .is_empty());
        assert!(choose(&b, None).styles.is_empty());
    }

    #[test]
    fn plan_and_offer_from_the_bundle() {
        let b = bundle_with(2, 0);
        let crc = crc32_hex(b"abc");
        let heap = |seq: u64, crc: &str| json!({"variant": "esp8266-clawd", "fwSeq": seq, "pak": 17, "pakCrc": crc});
        assert_eq!(
            Value::Object(plan(&heap(8, &crc), Some(&b))),
            json!({"clawdpak": "ok"})
        );
        assert_eq!(offer(&heap(8, &crc), Some(&b)), 0);
        assert_eq!(
            Value::Object(plan(&heap(8, "00000000"), Some(&b))),
            json!({"clawdpak": "need"})
        );
        assert_eq!(offer(&heap(8, "00000000"), Some(&b)), 8);
        assert_eq!(
            Value::Object(plan(&heap(7, &crc), Some(&b))),
            json!({"fw": "need", "clawdpak": "ok"})
        );
        assert_eq!(offer(&heap(9, "00000000"), Some(&b)), 8, "a pack differs");
        assert!(plan(&json!({"version": "2.16.0"}), Some(&b)).is_empty());
        assert!(plan(&heap(1, &crc), None).is_empty());
        assert_eq!(
            pack_state(&json!({"pak": 17, "pakCrc": ""}), "clawdpak", &crc),
            "wait"
        );
        assert_eq!(pack_state(&json!({"pak": 0}), "clawdpak", &crc), "need");
        assert_eq!(pack_state(&json!({"pak": 17}), "clawdpak", &crc), "unknown");
    }

    #[test]
    fn only_https_or_loopback() {
        assert!(url_allowed("https://github.com/x"));
        assert!(url_allowed("http://127.0.0.1:9/x"));
        assert!(!url_allowed("http://example.com/x"));
        assert!(!url_allowed("http://127.0.0.1.example.com/x"));
        assert!(!url_allowed("file:///etc/passwd"));
    }
}
