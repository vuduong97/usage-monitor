//! `clawd-store`: everything the app keeps on disk or in an OS store (phase 03 implements it).
//!
//! STUB (phase 02): real signatures and docs, `todo!()` bodies. Python origin:
//! `paths.py`, `pause.py`, `creds.py`, `autostart.py`, `firstrun.py`, `tray.open_path`
//! (see `desktop-rs/PORT-MAP.md`). Every function takes the injected [`clawd_contracts::AppPaths`]
//! and/or [`clawd_contracts::OsStoreNames`] instead of reading globals (red team #14).
//!
//! Everything here blocks (the prefs lock waits up to 5 s): from async code call it through
//! `tokio::task::spawn_blocking`.
//!
//! Two different "stop pushing" files, never mixed up (red team #9):
//! * [`pause`]: `pause.json`, a lease of at most 300 s the engine reads;
//! * [`paths::ota_busy`]: `ota.busy`, 900 s, read by the app updater only.
#![allow(
    unused_variables,
    dead_code,
    clippy::new_without_default,
    clippy::too_many_arguments,
    clippy::type_complexity
)] // stub crate: the owning phase removes this when it implements the bodies

pub mod autostart;
pub mod creds;
pub mod firstrun;
pub mod lock;
pub mod log;
pub mod open;
pub mod paths;
pub mod pause;
pub mod prefs;
