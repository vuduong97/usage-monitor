//! Persistence of the remembered clock: `~/.config/claude-usage-monitor/clock.json`
//! (`discovery.ClockStore`, `stored_clock`). Same schema as Python
//! (`{name, host, port, version}` plus `manual_host` when pinned by hand).

use std::path::PathBuf;

use clawd_contracts::ClockTarget;
use serde_json::Value;

/// `discovery.ClockStore(path)`.
pub struct ClockStore {
    pub path: PathBuf,
}

impl ClockStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// `ClockStore.load()`: the JSON object, `{}` when missing, unreadable or not an object.
    pub fn load(&self) -> Value {
        todo!("phase 08")
    }

    /// `ClockStore.save(data)`: atomic (temp file + replace), 2-space indent; errors are logged,
    /// not raised.
    pub fn save(&self, data: &Value) {
        todo!("phase 08")
    }

    /// `ClockStore.unpin()`: turn a typed address into a remembered one (mDNS may now replace
    /// it) and return the clock to try; `None` when nothing was ever saved.
    pub fn unpin(&self) -> Option<ClockTarget> {
        todo!("phase 08")
    }
}

/// `discovery.stored_clock(data)`: the clock `clock.json` points at: the typed address
/// (`manual_host`, source `Manual`), else the remembered one (source `Saved`).
pub fn stored_clock(data: &Value) -> Option<ClockTarget> {
    todo!("phase 08")
}
