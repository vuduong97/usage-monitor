//! The release source, GitHub (`appupdate.Source`). The client never follows redirects by
//! itself: a manual hop loop calls [`Source::trusted`] on **every** hop (release assets redirect
//! to a CDN host), a redirect to a foreign host is an error.

use std::path::Path;
use std::time::Duration;

use clawd_contracts::{AppPaths, AppRelease, UpdateError};

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

/// `appupdate.Source`.
pub struct Source {
    _private: (),
}

impl Source {
    /// `Source(repo, api)`: `repo` must look like `owner/name`; `api` loopback `http` is allowed
    /// (tests, dev), nothing else but `https`.
    pub fn new(repo: &str, api: &str) -> Result<Self, UpdateError> {
        todo!("phase 10")
    }

    /// `Source.trusted(url)`: `https://api.github.com`, `github.com`, any
    /// `*.githubusercontent.com`; and `http` loopback when the API override is loopback. Exactly
    /// the Python matrix (golden).
    pub fn trusted(&self, url: &str) -> bool {
        todo!("phase 10")
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
        todo!("phase 10")
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
        todo!("phase 10")
    }
}

/// `appupdate.from_settings()`: repo from env `CLAWD_UPDATE_REPO` / config `update_repo` /
/// [`clawd_contracts::UPDATE_REPO`]; API from env `CLAWD_UPDATE_API` / [`API`].
pub fn from_settings(paths: &AppPaths) -> Result<Source, UpdateError> {
    todo!("phase 10")
}

/// `appupdate.auto_enabled()`: `auto_update = off|false|0|no` in the config file stops the
/// background check (the button still works).
pub fn auto_enabled(paths: &AppPaths) -> bool {
    todo!("phase 10")
}
