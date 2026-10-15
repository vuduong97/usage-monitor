//! `usage.HttpSinks` (`usage.py:1160-1192`): POST each payload to every clock URL.

use std::sync::Arc;
use std::time::Duration;

use clawd_contracts::UsagePayload;

/// `usage.HTTP_PUSH_PATH` lives in `clawd-contracts`; this is the per-request bound
/// (`HTTP_PUSH_TIMEOUT`), so an offline screen never stalls the poll.
pub const HTTP_PUSH_TIMEOUT: Duration = Duration::from_secs(5);
/// `usage.RESEND_TIMEOUT`: a clock that just booted answers HTTP slowly for a while.
pub const RESEND_TIMEOUT: Duration = Duration::from_secs(15);

/// Called after every push with `(url, accepted)`. The engine's `TrackingSinks` (feeds
/// `ClockFinder::report`) is just `HttpSinks::new(Some(hook))`.
pub type ResultHook = Arc<dyn Fn(&str, bool) + Send + Sync>;

/// POSTs each payload to every `http_targets` URL (WiFi screens). Logs only when a target's
/// reachability changes, so an unplugged screen adds no line per poll.
pub struct HttpSinks {
    _private: (),
}

impl HttpSinks {
    /// `HttpSinks()` / `TrackingSinks(on_result)`.
    pub fn new(on_result: Option<ResultHook>) -> Self {
        todo!("phase 07")
    }

    /// `HttpSinks.send(targets, payload, timeout)`: true if at least one target answered 200.
    /// `timeout` default [`HTTP_PUSH_TIMEOUT`]; targets are pushed concurrently.
    pub async fn send(
        &self,
        targets: &[String],
        payload: &UsagePayload,
        timeout: Option<Duration>,
    ) -> bool {
        todo!("phase 07")
    }
}
