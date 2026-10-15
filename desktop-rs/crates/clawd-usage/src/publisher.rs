//! `usage.Publisher` (`usage.py:1194-1345`): one poll loop feeding every clock.
//!
//! Owns the poll schedule and the attention state, so an alert is consumed once and delivered
//! to every destination in the same payload. Alerts and clears always ride on a fresh poll,
//! never on their own. `POLL_INTERVAL` 60 s, `TICK` 5 s.

use std::sync::Arc;

use clawd_contracts::{BoxFuture, StopRx, TargetsFn, UsagePayload};

use crate::sinks::HttpSinks;
use crate::UsageEnv;

/// `usage.POLL_INTERVAL` (seconds).
pub const POLL_INTERVAL: f64 = 60.0;
/// `usage.TICK` (seconds): how often the loop looks for work.
pub const TICK: f64 = 5.0;

/// `Publisher.on_payload`: the engine sees every delivered payload (the pet, `Brain.usage`,
/// `paths.set_usage_payload`).
pub type PayloadHook = Arc<dyn Fn(&UsagePayload) + Send + Sync>;

/// Replaces the whole poll (`usage.poll_active`): returns `(payload, all_dead)`. Tests inject a
/// canned one (Python `test_engine` monkeypatches `usage.poll_active`).
pub type PollFn = Arc<dyn Fn() -> BoxFuture<(Option<UsagePayload>, bool)> + Send + Sync>;

/// `usage.Publisher`. Shared (`Arc`) between its own [`Self::run`] loop and the engine
/// (`resend` from `BootWatch`, `request_poll` from settings), hence `&self` everywhere.
pub struct Publisher {
    _private: (),
}

impl Publisher {
    /// `Publisher(http, targets)`: `targets` `None` = `read_http_targets` from the config file.
    pub fn new(env: UsageEnv, http: HttpSinks, targets: Option<TargetsFn>) -> Self {
        todo!("phase 07")
    }

    /// Use `poll` instead of [`crate::poll_active`] (tests; the real one is the default).
    pub fn with_poller(self, poll: PollFn) -> Self {
        todo!("phase 07")
    }

    /// Set `Publisher.on_payload`.
    pub fn set_on_payload(&self, hook: PayloadHook) {
        todo!("phase 07")
    }

    /// `pub.force = True; pub.wake.set()` (`Engine.poll_now`): poll and send on the next tick
    /// whatever the schedule says (the usage source changed). Any thread.
    pub fn request_poll(&self) {
        todo!("phase 07")
    }

    /// One tick: consume alerts/clears, poll when due or forced, deliver. A poll that yields no
    /// usable source stays silent (backs the source off) rather than hammering the API.
    pub async fn step(&self) {
        todo!("phase 07")
    }

    /// `Publisher.resend(url)`: push the last delivered payload again to one clock that just
    /// restarted so it leaves its "no data" splash now; never calls the usage API; with nothing
    /// cached it asks for a poll instead. False only when the clock did not take the data.
    pub async fn resend(&self, url: &str) -> bool {
        todo!("phase 07")
    }

    /// `Publisher.run(stop)`: `step`, then wait for a wake or `TICK`; a failing step is logged
    /// and the loop goes on.
    pub async fn run(&self, stop: StopRx) {
        todo!("phase 07")
    }
}
