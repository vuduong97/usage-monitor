//! `clawd-appupdate`: updates of the app itself, from the project's GitHub Releases (phase 10).
//! The clock's firmware has its own feed (`clawd-firmware`); nothing here touches that feed or
//! its token. Python origin: `appupdate.py` (see `desktop-rs/PORT-MAP.md`).
//!
//! How it works (frozen): ask `GET /repos/<owner>/<name>/releases/latest` (public API, no
//! token; draft and prerelease are never offered; tag `vX.Y.Z` is the version) -> a newer release
//! has the platform's installer plus `SHA256SUMS` **and (new, Q2) `SHA256SUMS.minisig`** ->
//! download in the background, verify the minisign signature of `SHA256SUMS` against
//! [`clawd_contracts::MINISIGN_PUBKEY`], then the file's SHA-256 (the expected hash is kept in
//! memory, not trusted from disk) -> stage in `<config dir>/app-update/` -> **only the user's
//! "Khởi động lại để cập nhật" applies it** (Windows: Inno installer, silent, detached;
//! macOS: `swap.sh` after bundle id, version, `codesign` and same Team ID checks).
//! Every redirect hop is re-checked with [`Source::trusted`]; never `tauri-plugin-updater`.

pub mod codesign;
pub mod http;
pub mod launch;
pub mod minisig;
pub mod source;
pub mod updater;
pub mod version;

pub use clawd_contracts::{AppRelease, UpdateError};
pub use http::{Body, Fut, Reply, ReqwestTransport, Transport};
pub use launch::{LaunchKind, LaunchSpec, Launcher, SystemLauncher};
pub use source::{auto_enabled, from_settings, Source};
pub use updater::{
    can_apply, running_bundle, updater, Staged, Updater, UpdaterOptions, UpdaterState,
};
pub use version::{
    asset_name, is_newer, parse_sums, parse_version, sha256_file, team_id, this_platform,
};
