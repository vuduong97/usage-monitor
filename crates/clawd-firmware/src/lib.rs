//! `clawd-firmware`: the clock firmware the app carries, the style packs that arrive later, and
//! how they get onto the clock.
//!
//! The firmware source lives in this repo (`clock/`); every app release bundles the image, its
//! animation packs, style packs and the preview renderer (`tools/build_clock.sh`). Only usage
//! style packs are updated between releases ([`packs`]): firmware changes only with the app.
//!
//! Python origin: `firmware.py`, `ota.py`, `preview.py` (see
//! `plans/261015-0726-rust-tauri-port/PORT-MAP.md`). Depends on `clawd-config` (the packs URL
//! comes from env -> config file), `clawd-clock` (`ClockApi`) and `clawd-store` (the
//! `pause.json` lease). Test fixtures live under `fixtures/fw/`.
//!
//! Blocking file I/O in this crate (`firmware`, `preview` loaders, `PakCache`) is meant to be
//! called from `spawn_blocking` by async callers, as every store-like API of the port.

pub mod crc32;
pub mod firmware;
pub mod ota;
pub mod packs;
pub mod preview;
pub mod pyval;

use std::path::PathBuf;

/// The app's resources directory, best effort and without a Tauri handle: next to the executable
/// (`Contents/Resources` for a macOS bundle). `bundle_dir` / `preview_dir` look for `firmware/`
/// and `renderer/` inside it; the app can pass Tauri's `resource_dir()` to `bundle_dir_in` /
/// `preview_dir_in` instead.
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
