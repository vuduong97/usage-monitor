//! `bootwatch.py`: notice when the clock reboots and push its data again. A restarted clock
//! lost both the usage bars and Clawd's state; its uptime (`/api/heap`) going backwards (beyond
//! `BOOT_JITTER` 30 s) or being under `FRESH_BOOT` 120 s after a failed delivery gives it away.

use std::sync::Arc;

use clawd_contracts::{BoxFuture, Clock, StopRx, TargetsFn};

/// `bootwatch.PROBE_EVERY` (seconds).
pub const PROBE_EVERY: f64 = 5.0;
/// `bootwatch.PROBE_TIMEOUT` (seconds): the clock answers HTTP slowly for minutes after boot.
pub const PROBE_TIMEOUT: f64 = 12.0;
/// `bootwatch.BOOT_JITTER` (seconds).
pub const BOOT_JITTER: f64 = 30.0;
/// `bootwatch.FRESH_BOOT` (seconds).
pub const FRESH_BOOT: f64 = 120.0;

/// `on_restart(usage_url, uptime_seconds)`: send the data again; true when the clock took it.
pub type OnRestart = Arc<dyn Fn(String, u64) -> BoxFuture<bool> + Send + Sync>;

/// `bootwatch.BootWatch`.
pub struct BootWatch {
    _private: (),
}

impl BootWatch {
    /// `BootWatch(targets, on_restart, clock)`: `clock` is monotonic.
    pub fn new(targets: TargetsFn, on_restart: OnRestart, clock: Arc<dyn Clock>) -> Self {
        todo!("phase 08")
    }

    /// `BootWatch.step()`: probe every clock once; the usage URLs that restarted.
    pub async fn step(&mut self) -> Vec<String> {
        todo!("phase 08")
    }

    /// `BootWatch.run(stop)`: probe every [`PROBE_EVERY`] until stopped; failures are logged.
    pub async fn run(self, stop: StopRx) {
        todo!("phase 08")
    }
}
