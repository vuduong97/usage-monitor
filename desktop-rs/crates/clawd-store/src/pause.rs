//! `pause.py`: hold the engine's pushes while the settings window works on the clock.
//!
//! The ESP8266 serves one HTTP client at a time. During a firmware upload and the reboot after
//! it, the engine's usage/state pushes and restart probes would compete with the upload, then
//! fail and get the clock dropped from discovery. The firmware job writes `pause.json` with an
//! `until` time (wall clock); while it is in the future the engine's target list is empty. It is
//! a lease: the writer renews it while it waits and removes it when done, and a crashed
//! writer's pause simply runs out. At most [`clawd_contracts::MAX_LEASE`] (300 s) is honoured.
//!
//! This is NOT `ota.busy` ([`crate::paths::ota_busy`], 900 s, read by the app updater): the two
//! have different readers and lifetimes (red team #9).

use std::io;
use std::sync::Arc;

use clawd_contracts::{AppPaths, Clock};

/// `pause.hold(seconds, reason)`: write `{"until": now + min(seconds, MAX_LEASE), "reason",
/// "pid"}` atomically (temp file named with the pid, then replace). Called by
/// `clawd_firmware::ota` at the start and on every `wait_back` tick.
pub fn hold(paths: &AppPaths, seconds: f64, reason: &str) -> io::Result<()> {
    todo!("phase 03")
}

/// [`hold`] with an injected `now` (epoch seconds), for tests (`pause.hold(..., now=)`).
pub fn hold_at(paths: &AppPaths, seconds: f64, reason: &str, now: f64) -> io::Result<()> {
    todo!("phase 03")
}

/// `pause.release()`: remove `pause.json`; a missing file is fine.
pub fn release(paths: &AppPaths) -> io::Result<()> {
    todo!("phase 03")
}

/// `pause.PauseReader`: for the engine. [`Self::reason`] is the active pause's reason, or `None`.
/// Re-reads the file only when its (mtime, size) stamp changes, since it is asked every second.
/// Shared between engine tasks, hence `&self`.
pub struct PauseReader {
    _private: (),
}

impl PauseReader {
    /// Reader over `paths.pause_file()` using the system clock.
    pub fn new(paths: &AppPaths) -> Self {
        todo!("phase 03")
    }

    /// Reader with an injected clock (tests).
    pub fn with_clock(paths: &AppPaths, clock: Arc<dyn Clock>) -> Self {
        todo!("phase 03")
    }

    /// The active pause's `reason` (default `"settings"`), or `None` when there is none, it has
    /// expired, or its `until` is further out than `MAX_LEASE` (not trusted).
    pub fn reason(&self) -> Option<String> {
        todo!("phase 03")
    }
}
