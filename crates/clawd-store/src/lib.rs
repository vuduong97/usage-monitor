//! `clawd-store`: everything the app keeps on disk or in an OS store (phase 03).
//!
//! Python origin: `paths.py`, `pause.py`, `creds.py`, `autostart.py`, `firstrun.py`,
//! `tray.open_path` (see `plans/261015-0726-rust-tauri-port/PORT-MAP.md`). Every function takes the injected
//! [`clawd_contracts::AppPaths`] and/or [`clawd_contracts::OsStoreNames`] instead of reading
//! globals (red team #14).
//!
//! Everything here blocks (the prefs lock waits up to 5 s): from async code call it through
//! `tokio::task::spawn_blocking`.
//!
//! Two different "stop pushing" files, never mixed up (red team #9):
//! * [`pause`]: `pause.json`, a lease of at most 300 s the engine reads;
//! * [`paths::ota_busy`]: `ota.busy`, 900 s, read by the app updater only.

pub mod autostart;
pub mod creds;
pub mod firstrun;
pub mod lock;
pub mod log;
pub mod open;
pub mod paths;
pub mod pause;
pub mod prefs;
pub mod pyjson;
