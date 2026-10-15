//! `events.StatePusher` (`events.py:893-1035`): pushes `Brain.desired()` to every clock: at once
//! on a change (after a short coalesce), then every `RESEND_EVERY` seconds while something is up
//! so the clock's TTL never lapses, and one final clear when it all goes quiet.
//!
//! Stops while a pause lease holds (the *engine* decides: it passes a `targets` function that
//! returns nothing then, see `clawd_store::pause::PauseReader`).

use std::sync::Arc;

use clawd_contracts::{Clock, DesiredState, StopRx, TargetsFn, WireState};

use crate::brain::SharedBrain;

/// `events.state_url(target)`: an `http_targets` push URL (`.../api/usage`) -> that clock's
/// `/api/state` URL.
pub fn state_url(target: &str) -> String {
    todo!("phase 04")
}

/// `events.heap_url(target)`: -> that clock's `/api/heap` URL.
pub fn heap_url(target: &str) -> String {
    todo!("phase 04")
}

/// `events.wire(d)`: the `POST /api/state` body for a desired state (byte-exact JSON key order).
pub fn wire(d: &DesiredState) -> WireState {
    todo!("phase 04")
}

/// Seconds since the last keyboard/mouse input; `None` if unknown.
pub type IdleFn = Arc<dyn Fn() -> Option<f64> + Send + Sync>;

/// `events.StatePusher`. Shared (`Arc`) between the engine, which calls
/// [`Self::invalidate`] when a clock restarts, and its own [`Self::run`] loop.
pub struct StatePusher {
    _private: (),
}

impl StatePusher {
    /// `StatePusher(brain, targets)`: system clock and [`crate::read_hid_idle`].
    pub fn new(brain: SharedBrain, targets: TargetsFn) -> Self {
        todo!("phase 04")
    }

    /// Same with an injected clock and idle probe (tests).
    pub fn with_clock(
        brain: SharedBrain,
        targets: TargetsFn,
        clock: Arc<dyn Clock>,
        idle: IdleFn,
    ) -> Self {
        todo!("phase 04")
    }

    /// `pusher.last_key = None`: forget what was last sent so the next step resends the state
    /// (a clock that restarted lost it). The engine calls this from `BootWatch.on_restart`.
    pub fn invalidate(&self) {
        todo!("phase 04")
    }

    /// `StatePusher.probe_desk()`: read every clock's animation count (`desk` in `/api/heap`);
    /// the newer idle picks play only when all clocks that answer have them.
    pub async fn probe_desk(&self) {
        todo!("phase 04")
    }

    /// `StatePusher.step()`: one decision cycle; true when something was sent.
    pub async fn step(&self) -> bool {
        todo!("phase 04")
    }

    /// `StatePusher.run(stop)`: loop until `stop`; a failing cycle is logged and the loop goes on.
    pub async fn run(&self, stop: StopRx) {
        todo!("phase 04")
    }
}
