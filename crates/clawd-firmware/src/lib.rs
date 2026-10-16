//! `clawd-firmware`: the clock firmware the app carries or fetches, and how it gets onto the
//! clock.
//!
//! Python origin: `firmware.py`, `feed.py`, `ota.py`, `preview.py` (modules keep their Python
//! names; see `plans/261015-0726-rust-tauri-port/PORT-MAP.md`). Depends on `clawd-config` (feed URL/token/channel come
//! from env -> config file -> build-time `option_env!`), `clawd-clock` (`ClockApi`) and
//! `clawd-store` (the `pause.json` lease). Fixtures live under `fixtures/fw/` because
//! `tools/check_public.py` forbids a directory named `firmware`, and no pack/zip/APNG/art may be
//! committed (hashes only).
//!
//! Blocking file I/O in this crate (`firmware`, `preview` loaders, `PakCache`) is meant to be
//! called from `spawn_blocking` by async callers, as every store-like API of the port.

pub mod crc32;
pub mod feed;
pub mod firmware;
pub mod ota;
pub mod preview;
pub mod pyval;
pub mod renderer_store;

use std::path::PathBuf;

/// The app's resources directory, best effort and without a Tauri handle: next to the executable
/// (`Contents/Resources` for a macOS bundle). `bundle_dir` / `preview_dir` look for `firmware/`
/// and `renderer/` inside it; the app can pass Tauri's `resource_dir()` to `bundle_dir_in` /
/// `preview_dir_in` instead. Only private builds carry anything there.
pub fn resources_dir() -> PathBuf {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(PathBuf::from))
        .unwrap_or_default();
    if cfg!(target_os = "macos") {
        exe_dir.join("..").join("Resources")
    } else {
        exe_dir
    }
}
