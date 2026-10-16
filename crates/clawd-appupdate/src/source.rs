//! The release source, GitHub (`appupdate.Source`). The client never follows redirects by
//! itself: a manual hop loop calls [`Source::trusted`] on **every** hop (release assets redirect
//! to a CDN host), a redirect to a foreign host is an error.

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use clawd_config::read_config_value;
use clawd_contracts::{
    AppPaths, AppRelease, Env, UpdateError, APP_VERSION, MINISIGN_PUBKEY, SUMS_NAME, SUMS_SIG_NAME,
    UPDATE_REPO,
};
use serde_json::Value;
use tokio::io::AsyncWriteExt;

use crate::http::{
    is_loopback_host, is_redirect, join_location, read_limited, trusted, ReqwestTransport,
    Transport, MAX_HOPS,
};
use crate::minisig::{verify_sums, SIGNATURE_MISSING};
use crate::version::{asset_name, is_newer, parse_sums, parse_version, this_platform};

/// `appupdate.API`.
pub const API: &str = "https://api.github.com";
/// `appupdate.TIMEOUT`.
pub const TIMEOUT: Duration = Duration::from_secs(30);
/// `appupdate.DOWNLOAD_TIMEOUT`.
pub const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(300);
/// `appupdate.MAX_DOWNLOAD`.
pub const MAX_DOWNLOAD: u64 = 500 * 1024 * 1024;
/// `appupdate.MAX_JSON`.
pub const MAX_JSON: u64 = 2 * 1024 * 1024;
/// `appupdate.CHECK_EVERY` (seconds): the background check.
pub const CHECK_EVERY: f64 = 6.0 * 3600.0;
/// `appupdate.FIRST_CHECK` (seconds): first check after the app starts.
pub const FIRST_CHECK: f64 = 90.0;
/// `appupdate.RETRY` (seconds): after a failed check.
pub const RETRY: f64 = 1800.0;
/// `appupdate.RECHECK` (seconds): the settings window asks again after this long.
pub const RECHECK: f64 = 600.0;

/// Size cap of `SHA256SUMS` and `SHA256SUMS.minisig` (`64 * 1024`).
const MAX_SMALL: u64 = 64 * 1024;

const UNTRUSTED: &str = "địa chỉ tải không đáng tin";
const TOO_MANY_REDIRECTS: &str = "quá nhiều lần chuyển hướng";

/// The verified `SHA256SUMS` of the last release `latest` accepted, kept so the updater can
/// store the pair beside the download (a restart re-verifies it, see `Updater::staged`).
#[derive(Debug, Clone)]
struct SignedSums {
    version: String,
    sums: Vec<u8>,
    signature: String,
}

/// `appupdate.Source`.
pub struct Source {
    repo: String,
    api: String,
    loopback: bool,
    transport: Arc<dyn Transport>,
    public_key: String,
    signed: Mutex<Option<SignedSums>>,
}

/// Python's `repr()` of a str, enough for the repository name in an error message.
fn py_repr(s: &str) -> String {
    let quote = if s.contains('\'') && !s.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut out = String::from(quote);
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c == quote => {
                out.push('\\');
                out.push(c);
            }
            c => out.push(c),
        }
    }
    out.push(quote);
    out
}

/// `REPO_RE`: `owner/name`, each 1-100 of `[A-Za-z0-9_.-]`.
fn repo_name_ok(repo: &str) -> bool {
    let part_ok = |p: &str| {
        (1..=100).contains(&p.len())
            && p.bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-'))
    };
    match repo.split_once('/') {
        Some((owner, name)) => part_ok(owner) && part_ok(name),
        None => false,
    }
}

/// Python truthiness of an optional JSON value.
fn truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|f| f != 0.0),
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(a)) => !a.is_empty(),
        Some(Value::Object(o)) => !o.is_empty(),
    }
}

/// `str(value or "")`.
fn text_of(v: Option<&Value>) -> String {
    if !truthy(v) {
        return String::new();
    }
    match v {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Bool(_)) => "True".to_owned(),
        Some(other) => other.to_string(),
        None => String::new(),
    }
}

impl Source {
    /// `Source(repo, api)`: `repo` must look like `owner/name`; `api` loopback `http` is allowed
    /// (tests, dev), nothing else but `https`.
    pub fn new(repo: &str, api: &str) -> Result<Self, UpdateError> {
        let transport = ReqwestTransport::new(&format!("Clawd/{APP_VERSION}"))?;
        Self::with_transport(repo, api, Arc::new(transport))
    }

    /// `Source(repo, api, transport=...)`: the same with another [`Transport`] (tests).
    pub fn with_transport(
        repo: &str,
        api: &str,
        transport: Arc<dyn Transport>,
    ) -> Result<Self, UpdateError> {
        if !repo_name_ok(repo) {
            return Err(UpdateError(format!(
                "bad repository name: {}",
                py_repr(repo)
            )));
        }
        let api = api.trim_end_matches('/').to_owned();
        let loopback = is_loopback_host(&api);
        let scheme_ok = api.starts_with("https://") || (loopback && api.starts_with("http://"));
        if !scheme_ok {
            return Err(UpdateError(format!(
                "địa chỉ API không hợp lệ (chỉ https, hoặc http tới máy này): {api}"
            )));
        }
        Ok(Self {
            repo: repo.to_owned(),
            api,
            loopback,
            transport,
            public_key: MINISIGN_PUBKEY.to_owned(),
            signed: Mutex::new(None),
        })
    }

    /// Verify `SHA256SUMS` under `key` instead of [`MINISIGN_PUBKEY`] (tests with a throwaway
    /// key pair; a fork build overrides the constant at build time instead).
    pub fn with_public_key(mut self, key: &str) -> Self {
        self.public_key = key.to_owned();
        self
    }

    /// `Source.repo`.
    pub fn repo(&self) -> &str {
        &self.repo
    }

    /// `Source.api` (no trailing slash).
    pub fn api(&self) -> &str {
        &self.api
    }

    /// `Source.trusted(url)`: `https://api.github.com`, `github.com`, any
    /// `*.githubusercontent.com`; and `http` loopback when the API override is loopback. Exactly
    /// the Python matrix (golden).
    pub fn trusted(&self, url: &str) -> bool {
        trusted(url, self.loopback)
    }

    /// `Source._get_json(path)`.
    async fn get_json(&self, path: &str) -> Result<Option<Value>, UpdateError> {
        let url = format!("{}{}", self.api, path);
        let mut reply = self.transport.get(&url, TIMEOUT).await?;
        match reply.status {
            404 => return Ok(None),
            403 | 429 => {
                return Err(UpdateError(
                    "GitHub đang giới hạn số lần hỏi, thử lại sau".to_owned(),
                ))
            }
            s if s >= 400 => return Err(UpdateError(format!("GitHub trả HTTP {s}"))),
            _ => {}
        }
        let Some(bytes) = read_limited(reply.body.as_mut(), MAX_JSON).await? else {
            return Err(UpdateError("phản hồi quá lớn".to_owned()));
        };
        let data: Value = serde_json::from_slice(&bytes)
            .map_err(|_| UpdateError("phản hồi không phải JSON".to_owned()))?;
        Ok(data.is_object().then_some(data))
    }

    /// `Source._get_small(url)`: a small file (`SHA256SUMS`, its signature), following GitHub's
    /// redirect to its CDN and checking every hop.
    async fn get_small(&self, url: &str, name: &str) -> Result<Vec<u8>, UpdateError> {
        let mut url = url.to_owned();
        for _ in 0..MAX_HOPS {
            if !self.trusted(&url) {
                return Err(UpdateError(UNTRUSTED.to_owned()));
            }
            let mut reply = self.transport.get(&url, TIMEOUT).await?;
            if is_redirect(reply.status) {
                if let Some(location) = reply.location.as_deref().filter(|l| !l.is_empty()) {
                    url = join_location(&url, location)
                        .ok_or_else(|| UpdateError(UNTRUSTED.to_owned()))?;
                    continue;
                }
            }
            let unreadable =
                || UpdateError(format!("không đọc được {name} (HTTP {})", reply.status));
            if reply.status >= 400 {
                return Err(unreadable());
            }
            return match read_limited(reply.body.as_mut(), MAX_SMALL).await? {
                Some(bytes) => Ok(bytes),
                None => Err(unreadable()),
            };
        }
        Err(UpdateError(TOO_MANY_REDIRECTS.to_owned()))
    }

    /// `Source.latest(current, system, arch)`: the newest published release when it is newer
    /// than `current` and has this platform's installer, `SHA256SUMS` **and `SHA256SUMS.minisig`
    /// (which must verify)**; `Ok(None)` otherwise (draft, prerelease, still uploading, not
    /// built for this platform). `UpdateError` for a bad asset URL/size, a missing checksum
    /// line, or a missing/invalid signature.
    pub async fn latest(
        &self,
        current: &str,
        system: Option<&str>,
        arch: Option<&str>,
    ) -> Result<Option<AppRelease>, UpdateError> {
        let (system, arch) = match system.filter(|s| !s.is_empty()) {
            Some(system) => (system.to_owned(), arch.unwrap_or("").to_owned()),
            None => this_platform(),
        };
        let path = format!("/repos/{}/releases/latest", self.repo);
        let Some(data) = self.get_json(&path).await? else {
            return Ok(None);
        };
        if data.as_object().is_none_or(|o| o.is_empty())
            || truthy(data.get("draft"))
            || truthy(data.get("prerelease"))
        {
            return Ok(None);
        }
        let tag = text_of(data.get("tag_name"));
        let Some((major, minor, patch)) = parse_version(&tag) else {
            return Ok(None);
        };
        if !is_newer(&tag, current) {
            return Ok(None);
        }
        let version = format!("{major}.{minor}.{patch}");
        let Some(name) = asset_name(&version, &system, &arch) else {
            return Ok(None);
        };
        let mut assets: Vec<(&Value, &Value)> = Vec::new();
        if let Some(list) = data.get("assets").and_then(Value::as_array) {
            for asset in list.iter().filter(|a| a.is_object()) {
                if let Some(asset_name) = asset.get("name") {
                    assets.push((asset_name, asset));
                }
            }
        }
        let find = |wanted: &str| {
            assets
                .iter()
                .rev()
                .find(|(n, _)| n.as_str() == Some(wanted))
                .map(|(_, a)| *a)
        };
        let (Some(asset), Some(sums)) = (find(&name), find(SUMS_NAME)) else {
            return Ok(None); // still uploading, or not built for this platform
        };
        let url = text_of(asset.get("browser_download_url"));
        let size = asset.get("size").and_then(Value::as_u64);
        let size = match size {
            Some(size) if self.trusted(&url) && size > 0 && size <= MAX_DOWNLOAD => size,
            _ => {
                return Err(UpdateError(format!(
                    "{name}: địa chỉ hoặc kích thước không hợp lệ"
                )))
            }
        };
        let Some(sig_asset) = find(SUMS_SIG_NAME) else {
            return Err(UpdateError(SIGNATURE_MISSING.to_owned()));
        };

        let sums_bytes = self
            .get_small(&text_of(sums.get("browser_download_url")), SUMS_NAME)
            .await?;
        let sig_bytes = self
            .get_small(
                &text_of(sig_asset.get("browser_download_url")),
                SUMS_SIG_NAME,
            )
            .await?;
        let signature = String::from_utf8(sig_bytes)
            .map_err(|_| UpdateError(crate::minisig::BAD_SIGNATURE_FILE.to_owned()))?;
        verify_sums(&sums_bytes, &signature, &self.public_key)?;

        let table = parse_sums(&String::from_utf8_lossy(&sums_bytes));
        let Some(digest) = table.get(&name).filter(|d| !d.is_empty()) else {
            return Err(UpdateError(format!("{SUMS_NAME} không có {name}")));
        };
        let notes: String = text_of(data.get("body"))
            .trim()
            .chars()
            .take(2000)
            .collect();
        let release = AppRelease {
            version: version.clone(),
            tag,
            notes,
            page: text_of(data.get("html_url")),
            name,
            url,
            size,
            sha256: digest.clone(),
        };
        *self.signed.lock().expect("signed sums lock") = Some(SignedSums {
            version,
            sums: sums_bytes,
            signature,
        });
        Ok(Some(release))
    }

    /// The `SHA256SUMS` bytes and `SHA256SUMS.minisig` text `latest` verified for `version`.
    pub fn signed_sums(&self, version: &str) -> Option<(Vec<u8>, String)> {
        let guard = self.signed.lock().expect("signed sums lock");
        guard
            .as_ref()
            .filter(|s| s.version == version)
            .map(|s| (s.sums.clone(), s.signature.clone()))
    }

    /// `Source.download(rel, dest, progress)`: stream to `<dest>.part<pid>`, check size and
    /// SHA-256 against `rel`, then move it to `dest` (it appears only when right).
    /// `progress(done, total)`.
    pub async fn download(
        &self,
        rel: &AppRelease,
        dest: &Path,
        progress: Option<&(dyn Fn(u64, u64) + Send + Sync)>,
    ) -> Result<(), UpdateError> {
        let io_err = |e: std::io::Error| UpdateError(e.to_string());
        if let Some(parent) = dest.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(io_err)?;
        }
        let file_name = dest
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let part = dest.with_file_name(format!("{file_name}.part{}", std::process::id()));
        let result = self.download_to(rel, &part, progress).await;
        let result = match result {
            Ok(()) => tokio::fs::rename(&part, dest).await.map_err(io_err),
            Err(e) => Err(e),
        };
        let _ = tokio::fs::remove_file(&part).await;
        result
    }

    async fn download_to(
        &self,
        rel: &AppRelease,
        part: &Path,
        progress: Option<&(dyn Fn(u64, u64) + Send + Sync)>,
    ) -> Result<(), UpdateError> {
        use sha2::{Digest, Sha256};
        let io_err = |e: std::io::Error| UpdateError(e.to_string());
        let mut hasher = Sha256::new();
        let mut got: u64 = 0;
        let mut url = rel.url.clone();
        let mut finished = false;
        for _ in 0..MAX_HOPS {
            if !self.trusted(&url) {
                return Err(UpdateError(UNTRUSTED.to_owned()));
            }
            let mut reply = self.transport.get(&url, DOWNLOAD_TIMEOUT).await?;
            if is_redirect(reply.status) {
                if let Some(location) = reply.location.as_deref().filter(|l| !l.is_empty()) {
                    url = join_location(&url, location)
                        .ok_or_else(|| UpdateError(UNTRUSTED.to_owned()))?;
                    continue;
                }
            }
            if reply.status != 200 {
                return Err(UpdateError(format!(
                    "tải {}: HTTP {}",
                    rel.name, reply.status
                )));
            }
            let mut file = tokio::fs::File::create(part).await.map_err(io_err)?;
            while let Some(chunk) = reply.body.chunk().await? {
                got += chunk.len() as u64;
                if got > rel.size {
                    return Err(UpdateError(format!(
                        "{}: lớn hơn {} byte đã công bố",
                        rel.name, rel.size
                    )));
                }
                hasher.update(&chunk);
                file.write_all(&chunk).await.map_err(io_err)?;
                if let Some(progress) = progress {
                    progress(got, rel.size);
                }
            }
            file.flush().await.map_err(io_err)?;
            finished = true;
            break;
        }
        if !finished {
            return Err(UpdateError(TOO_MANY_REDIRECTS.to_owned()));
        }
        if got != rel.size {
            return Err(UpdateError(format!(
                "{}: nhận {} byte, đã công bố {}",
                rel.name, got, rel.size
            )));
        }
        if hex::encode(hasher.finalize()) != rel.sha256 {
            return Err(UpdateError(format!(
                "{}: SHA-256 không khớp, bỏ file",
                rel.name
            )));
        }
        Ok(())
    }
}

/// `appupdate.from_settings()`: (`env` = `Env::system()` in the app) repo from env `CLAWD_UPDATE_REPO` / config `update_repo` /
/// [`clawd_contracts::UPDATE_REPO`]; API from env `CLAWD_UPDATE_API` / [`API`].
pub fn from_settings(paths: &AppPaths, env: &Env) -> Result<Source, UpdateError> {
    let repo = env
        .get("CLAWD_UPDATE_REPO")
        .or_else(|| read_config_value(paths, "update_repo").filter(|v| !v.is_empty()))
        .unwrap_or_else(|| UPDATE_REPO.to_owned());
    let api = env
        .get("CLAWD_UPDATE_API")
        .unwrap_or_else(|| API.to_owned());
    Source::new(&repo, &api)
}

/// `appupdate.auto_enabled()`: `auto_update = off|false|0|no` in the config file stops the
/// background check (the button still works).
pub fn auto_enabled(paths: &AppPaths) -> bool {
    let value = read_config_value(paths, "auto_update")
        .unwrap_or_default()
        .to_lowercase();
    !matches!(value.as_str(), "off" | "false" | "0" | "no")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repo_names_follow_the_python_regex() {
        for ok in ["o/r", "vuduong97/usage-monitor", "a.b_c-d/e"] {
            assert!(repo_name_ok(ok), "{ok}");
        }
        for bad in [
            "not a repo",
            "",
            "x",
            "a/b/c",
            "/b",
            "a/",
            "a b/c",
            &"a".repeat(101),
        ] {
            assert!(!repo_name_ok(bad), "{bad}");
        }
    }

    #[test]
    fn bad_repo_message_is_pythons_repr() {
        assert_eq!(py_repr("not a repo"), "'not a repo'");
        assert_eq!(py_repr("it's"), "\"it's\"");
    }
}
