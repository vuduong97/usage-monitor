//! The engine (`engine.py`): STUB owned by phase 12, which fills in the wiring of the crates'
//! loops (finder, publisher, event server, boot watch, token watch, screen sync), the
//! `catch_unwind` + restart supervision (red team #12) and `targets()` returning nothing while a
//! pause lease holds. Only the shapes `state.rs` needs are fixed here.

use clawd_contracts::{ClockTarget, UsagePayload};

/// Cloneable handle to the running engine (`Engine` in Python), stored in `AppState`.
#[derive(Clone)]
pub struct EngineHandle {
    _private: (),
}

impl EngineHandle {
    /// `Engine.poll_now()`: ask for usage and send it now (the usage source changed). Any thread.
    pub fn poll_now(&self) {
        todo!("phase 12")
    }

    /// `Engine.snapshot()`: what the tray and popover show.
    pub fn snapshot(&self) -> Status {
        todo!("phase 12")
    }

    /// `Engine.stop()`: idempotent (commit `03928c8`); quitting calls it, then `app.exit(0)`.
    pub fn stop(&self) {
        todo!("phase 12")
    }
}

/// `Engine.snapshot()` as a value: the finder's `describe()` plus the last payload, `token`,
/// `events_on` and `paused`. Input of `tray.status()` and `popover.view()`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Status {
    /// The clock in use (`None` when `manual_config` or nothing is known).
    pub clock: Option<ClockTarget>,
    pub manual_config: bool,
    pub config_targets: Vec<String>,
    /// Epoch seconds of the last accepted push.
    pub last_ok: Option<f64>,
    pub failing: bool,
    pub payload: Option<UsagePayload>,
    pub payload_at: Option<f64>,
    /// `None` until checked; `Some(false)` = Claude login problem.
    pub token: Option<bool>,
    pub events_on: bool,
    /// Reason of the active pause lease, if any.
    pub paused: Option<String>,
}
