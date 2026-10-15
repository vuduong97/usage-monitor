//! `discovery.ClockFinder`: find the clock so nobody types an IP more than once.
//!
//! Order of preference, re-evaluated every couple of seconds:
//! 1. A manual address: `http_targets` in the config file, or one typed in the tray / settings
//!    window. Used as-is.
//! 2. The remembered clock (`clock.json`), looked up by its mDNS name so a new IP from the
//!    router is followed; its last address is tried right away at startup.
//! 3. The first clock that answers mDNS; it becomes the remembered one.
//!
//! Push results feed back through [`ClockFinder::report`]: a clock that fails `FAIL_LIMIT` (3)
//! pushes in a row is dropped for `BAD_HOLD` (60 s), then its address is tried again.
//! Every method is `&self` (lock-protected inside): the engine, tray and settings call it
//! from different tasks.

use std::sync::Arc;

use clawd_contracts::{Clock, ClockTarget, StopRx, TargetsFn};

use crate::mdns::MdnsSource;
use crate::store::ClockStore;

/// `discovery.FAIL_LIMIT`.
pub const FAIL_LIMIT: u32 = 3;
/// `discovery.BAD_HOLD` (seconds).
pub const BAD_HOLD: f64 = 60.0;
/// `discovery.TICK` (seconds).
pub const TICK: f64 = 2.0;

/// `ClockFinder.describe()`: the snapshot for the tray menu and the engine's status.
#[derive(Debug, Clone, PartialEq)]
pub struct FinderSnapshot {
    /// `http_targets` is set in the config file.
    pub manual_config: bool,
    /// The clock in use; `None` when `manual_config` or nothing is known yet.
    pub clock: Option<ClockTarget>,
    pub config_targets: Vec<String>,
    /// Wall time (epoch s) of the last accepted push.
    pub last_ok: Option<f64>,
    /// `FAIL_LIMIT` failed pushes in a row, or no clock and no manual config.
    pub failing: bool,
}

/// `discovery.ClockFinder`.
pub struct ClockFinder {
    _private: (),
}

impl ClockFinder {
    /// `ClockFinder(store, manual, browser, clock)`: `manual` returns the config's
    /// `http_targets`; `clock` is the monotonic clock for `BAD_HOLD`.
    pub fn new(
        store: ClockStore,
        manual: TargetsFn,
        browser: Arc<dyn MdnsSource>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        todo!("phase 08")
    }

    /// Production wiring: the real [`crate::MdnsBrowser`] and a monotonic system clock.
    pub fn with_defaults(store: ClockStore, manual: TargetsFn) -> Self {
        todo!("phase 08")
    }

    /// `ClockFinder.current`: the clock tried now.
    pub fn current(&self) -> Option<ClockTarget> {
        todo!("phase 08")
    }

    /// `ClockFinder.targets()`: the `/api/usage` URLs to push to: the config's `http_targets`
    /// when set, else the current clock's.
    pub fn targets(&self) -> Vec<String> {
        todo!("phase 08")
    }

    /// `ClockFinder.describe()`.
    pub fn describe(&self) -> FinderSnapshot {
        todo!("phase 08")
    }

    /// `ClockFinder.report(url, ok)`: feedback from a usage push (fed by
    /// `clawd_usage::ResultHook`).
    pub fn report(&self, url: &str, ok: bool) {
        todo!("phase 08")
    }

    /// `ClockFinder.set_manual(raw)`: the user typed an address (tray "Nhập địa chỉ đồng hồ…",
    /// settings); pins it in `clock.json` (`manual_host`). `None` when `raw` is not an address.
    pub fn set_manual(&self, raw: &str) -> Option<ClockTarget> {
        todo!("phase 08")
    }

    /// `ClockFinder.use(c)` (`use` is a Rust keyword): the clock the user picked from the ones
    /// mDNS sees (setup wizard): remembered by name, not pinned, so a new DHCP address follows it.
    pub fn use_clock(&self, c: &ClockTarget) {
        todo!("phase 08")
    }

    /// `ClockFinder.retry()` ("Tìm lại"): keep the address, try it again now and let mDNS replace
    /// it; a typed address loses its pin, not its value.
    pub fn retry(&self) {
        todo!("phase 08")
    }

    /// `ClockFinder.step()`: one evaluation (mDNS names -> resolve -> choose/adopt/remember).
    pub async fn step(&self) {
        todo!("phase 08")
    }

    /// `ClockFinder.run(stop)`: `step` every [`TICK`] until stopped; failures are logged.
    pub async fn run(&self, stop: StopRx) {
        todo!("phase 08")
    }
}
