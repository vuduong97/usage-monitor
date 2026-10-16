//! `usage.HttpSinks` (`usage.py:1160-1192`): POST each payload to every clock URL.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use clawd_contracts::UsagePayload;
use futures_util::future::join_all;

use crate::payload::dumps;

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
    client: reqwest::Client,
    on_result: Option<ResultHook>,
    reachable: Mutex<HashMap<String, bool>>,
}

impl HttpSinks {
    /// `HttpSinks()` / `TrackingSinks(on_result)`, with a client of its own.
    pub fn new(on_result: Option<ResultHook>) -> Self {
        let client = reqwest::Client::builder()
            .build()
            .expect("the TLS backend initialises");
        Self::with_client(client, on_result)
    }

    /// Like [`Self::new`] but sharing an existing client (`UsageEnv::client`; tests pass one
    /// without proxy settings).
    pub fn with_client(client: reqwest::Client, on_result: Option<ResultHook>) -> Self {
        Self {
            client,
            on_result,
            reachable: Mutex::new(HashMap::new()),
        }
    }

    /// `HttpSinks.send(targets, payload, timeout)`: true if at least one target answered 200.
    /// `timeout` default [`HTTP_PUSH_TIMEOUT`]; targets are pushed concurrently.
    pub async fn send(
        &self,
        targets: &[String],
        payload: &UsagePayload,
        timeout: Option<Duration>,
    ) -> bool {
        let timeout = timeout.unwrap_or(HTTP_PUSH_TIMEOUT);
        let body = dumps(payload);
        let results = join_all(targets.iter().map(|url| self.post(url, &body, timeout))).await;
        results.into_iter().any(|ok| ok)
    }

    async fn post(&self, url: &str, body: &str, timeout: Duration) -> bool {
        let sent = self
            .client
            .post(url)
            .header("content-type", "application/json")
            .body(body.to_owned())
            .timeout(timeout)
            .send()
            .await;
        let (ok, why) = match sent {
            Ok(resp) => (
                resp.status().as_u16() == 200,
                format!("HTTP {}", resp.status().as_u16()),
            ),
            Err(e) => (false, e.without_url().to_string()),
        };
        let changed = {
            let mut seen = self.reachable.lock().unwrap_or_else(|e| e.into_inner());
            seen.insert(url.to_owned(), ok) != Some(ok)
        };
        if changed {
            if ok {
                tracing::info!("WiFi push to {url}: ok");
            } else {
                tracing::warn!("WiFi push to {url} failed: {why}");
            }
        }
        if let Some(hook) = &self.on_result {
            hook(url, ok);
        }
        ok
    }
}
