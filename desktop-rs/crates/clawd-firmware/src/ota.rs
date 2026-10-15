//! Install the bundled firmware on the clock, then (optionally) the settings that were waiting
//! for it, e.g. a style pack the old image cannot draw (`ota.py`).
//!
//! Order matters: a pack is sent only after the new image is up and says it draws that style
//! API (an old image has no `/api/style` at all). Hold the engine's pushes with
//! `clawd_store::pause::{hold, release}` **for the whole operation, including on error or panic**
//! (a `Drop` guard), renewing the lease on every `wait_back` tick (`ota.py:28-37, 84-100`).
//! This module does **not** set `ota.busy`: the settings job does (phase 13), `settings_api.py:
//! 612-644`.
//!
//! Do not "improve" the sequence: a different order can brick a clock.

use std::sync::Arc;
use std::time::Duration;

use clawd_clock::ClockApi;
use clawd_contracts::{AppPaths, Clock, ClockError};
use serde_json::Value;

use crate::feed::Release;
use crate::firmware::Bundle;

/// `ota.SETTLE`: let a push the engine already started finish before the upload.
pub const SETTLE: Duration = Duration::from_secs(6);
/// `ota.LEASE` (seconds): push pause, renewed while we wait.
pub const LEASE: f64 = 60.0;
/// `ota.UPLOAD_LEASE` (seconds): longer than `clawd_clock::api::UPLOAD_TIMEOUT`.
pub const UPLOAD_LEASE: f64 = 200.0;
/// `ota.CRC_WAIT`: a clock checksums packs it did not receive itself in the background.
pub const CRC_WAIT: Duration = Duration::from_secs(150);
/// `ota.PACK_TRIES`.
pub const PACK_TRIES: u32 = 2;

/// `progress(step)`: `check`, `pause`, `upload`, `reboot`, `done`, or a pack kind.
pub type Progress = Arc<dyn Fn(&str) + Send + Sync>;

/// The `progress` / `hold` / `release` / `settle` keyword arguments of the Python functions.
/// Sleeps are tokio time (`tokio::time::pause` in tests). The monotonic clock handed to
/// `ClockApi::wait_back` is `clock` (default `MonotonicClock`, which is also tokio time); never
/// pass a `FakeClock` together with paused tokio time.
#[derive(Clone)]
pub struct OtaOptions {
    /// Where `pause.json` lives (`clawd_store::pause`).
    pub paths: AppPaths,
    pub progress: Option<Progress>,
    /// Override [`SETTLE`] (tests).
    pub settle: Option<Duration>,
    /// Monotonic clock for `wait_back`; `None` = `MonotonicClock` (tokio time).
    pub clock: Option<Arc<dyn Clock>>,
}

/// `ota.install(api, bundle, ...)`: push the bundle; returns the clock's `/api/heap` after it
/// came back. `ClockError` ("no update to install (<state>)", "the clock came back on its old
/// firmware") otherwise.
pub async fn install(
    api: &ClockApi,
    bundle: &Bundle,
    opts: &OtaOptions,
) -> Result<Value, ClockError> {
    todo!("phase 09")
}

/// `ota.install_packs(api, rel, heap, ...)`: send the release's `clawd.pak`, then `desk.pak`,
/// where the clock's differs (CRC in `/api/heap`), after waiting out any checksum in progress.
/// One retry each. Returns the clock's heap afterwards.
pub async fn install_packs(
    api: &ClockApi,
    rel: &Release,
    heap: Value,
    opts: &OtaOptions,
) -> Result<Value, ClockError> {
    todo!("phase 09")
}

/// `ota.install_release(api, rel, changes, need_style_api, ...)`: everything a feed release
/// brings that this clock lacks: the firmware (when newer), then the packs that differ, then POST
/// `changes`. Returns the clock's heap.
pub async fn install_release(
    api: &ClockApi,
    rel: &Release,
    changes: &Value,
    need_style_api: u32,
    opts: &OtaOptions,
) -> Result<Value, ClockError> {
    todo!("phase 09")
}

/// `ota.install_then_save(api, bundle, changes, need_style_api, ...)`: [`install`], then POST
/// `changes`; `ClockError` when the new image does not draw style packs of API `need_style_api`
/// (0: no need).
pub async fn install_then_save(
    api: &ClockApi,
    bundle: &Bundle,
    changes: &Value,
    need_style_api: u32,
    opts: &OtaOptions,
) -> Result<Value, ClockError> {
    todo!("phase 09")
}
