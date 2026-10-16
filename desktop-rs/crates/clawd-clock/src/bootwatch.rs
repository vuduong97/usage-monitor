//! `bootwatch.py`: notice when the clock reboots and push its data again. A restarted clock
//! lost both the usage bars and Clawd's state; its uptime (`/api/heap`) going backwards (beyond
//! `BOOT_JITTER` 30 s) or being under `FRESH_BOOT` 120 s after a failed delivery gives it away.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use clawd_contracts::{BoxFuture, Clock, StopRx, TargetsFn};
use serde_json::Value;

/// `bootwatch.PROBE_EVERY` (seconds).
pub const PROBE_EVERY: f64 = 5.0;
/// `bootwatch.PROBE_TIMEOUT` (seconds): the clock answers HTTP slowly for minutes after boot.
pub const PROBE_TIMEOUT: f64 = 12.0;
/// `bootwatch.BOOT_JITTER` (seconds).
pub const BOOT_JITTER: f64 = 30.0;
/// `bootwatch.FRESH_BOOT` (seconds).
pub const FRESH_BOOT: f64 = 120.0;
/// `bootwatch.HEAP_PATH`.
const HEAP_PATH: &str = "/api/heap";

/// `on_restart(usage_url, uptime_seconds)`: send the data again; true when the clock took it.
pub type OnRestart = Arc<dyn Fn(String, u64) -> BoxFuture<bool> + Send + Sync>;

/// `bootwatch.heap_url`: an `http_targets` push URL (`.../api/usage`) -> that clock's
/// `/api/heap` URL. A private duplicate of `clawd_events::heap_url` (no edge to that crate).
fn heap_url(target: &str) -> String {
    match reqwest::Url::parse(target) {
        Ok(url) => {
            let mut heap = url;
            heap.set_path(HEAP_PATH);
            heap.set_query(None);
            heap.set_fragment(None);
            heap.to_string()
        }
        Err(_) => target.to_owned(),
    }
}

/// `urlsplit(url).netloc`, for log lines.
fn netloc(url: &str) -> String {
    match reqwest::Url::parse(url) {
        Ok(u) => match (u.host_str(), u.port()) {
            (Some(h), Some(p)) => format!("{h}:{p}"),
            (Some(h), None) => h.to_owned(),
            _ => String::new(),
        },
        Err(_) => String::new(),
    }
}

/// `bootwatch.BootWatch`.
pub struct BootWatch {
    targets: TargetsFn,
    on_restart: OnRestart,
    clock: Arc<dyn Clock>,
    http: reqwest::Client,
    /// usage url -> estimated boot time (monotonic).
    booted: HashMap<String, f64>,
    /// restarted, but the data did not get through.
    retry: HashSet<String>,
}

impl BootWatch {
    /// `BootWatch(targets, on_restart, clock)`: `clock` is monotonic.
    pub fn new(targets: TargetsFn, on_restart: OnRestart, clock: Arc<dyn Clock>) -> Self {
        let http = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs_f64(PROBE_TIMEOUT))
            .build()
            .unwrap_or_default();
        Self {
            targets,
            on_restart,
            clock,
            http,
            booted: HashMap::new(),
            retry: HashSet::new(),
        }
    }

    /// `BootWatch._uptime`: whole seconds of uptime, `None` when the clock does not answer.
    async fn uptime(&self, url: &str) -> Option<u64> {
        let response = self.http.get(heap_url(url)).send().await.ok()?;
        if response.status().as_u16() != 200 {
            return None;
        }
        let body: Value = serde_json::from_slice(&response.bytes().await.ok()?).ok()?;
        match body.get("uptime")? {
            Value::Number(n) => {
                let up = n.as_f64()?;
                (up >= 0.0).then_some(up as u64)
            }
            // Python: `isinstance(True, int)`
            Value::Bool(b) => Some(u64::from(*b)),
            _ => None,
        }
    }

    /// `BootWatch.step()`: probe every clock once; the usage URLs that restarted.
    pub async fn step(&mut self) -> Vec<String> {
        let urls = (self.targets)();
        let gone: Vec<String> = self
            .booted
            .keys()
            .filter(|k| !urls.contains(k))
            .cloned()
            .collect();
        for g in gone {
            self.booted.remove(&g);
            self.retry.remove(&g);
        }
        let mut restarted = Vec::new();
        for url in urls {
            let Some(up) = self.uptime(&url).await else {
                continue; // not answering: keep the last estimate
            };
            let booted = self.clock.now() - up as f64;
            let prev = self.booted.insert(url.clone(), booted);
            let fresh = match prev {
                None => (up as f64) < FRESH_BOOT,
                Some(prev) => booted - prev > BOOT_JITTER,
            };
            let retry = self.retry.remove(&url) && (up as f64) < FRESH_BOOT;
            if fresh || retry {
                let host = netloc(&url);
                if fresh {
                    tracing::info!("Clock {host} restarted (up {up} s): sending its data again");
                }
                restarted.push(url.clone());
                if (self.on_restart)(url.clone(), up).await {
                    tracing::info!("Clock {host} took its data again (up {up} s)");
                } else {
                    self.retry.insert(url);
                    if fresh {
                        tracing::info!("Clock {host} did not take the data yet; retrying");
                    }
                }
            }
        }
        restarted
    }

    /// `BootWatch.run(stop)`: probe every [`PROBE_EVERY`] until stopped; failures are logged.
    pub async fn run(mut self, mut stop: StopRx) {
        while !*stop.borrow() {
            self.step().await;
            tokio::select! {
                stopped = stop.wait_for(|s| *s) => {
                    if stopped.is_err() {
                        break; // the sender is gone: nobody can ask for more
                    }
                }
                _ = tokio::time::sleep(Duration::from_secs_f64(PROBE_EVERY)) => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heap_url_keeps_scheme_and_authority() {
        assert_eq!(heap_url("http://10.0.0.5/api/usage"), "http://10.0.0.5/api/heap");
        assert_eq!(
            heap_url("http://clawd.local:8080/custom"),
            "http://clawd.local:8080/api/heap"
        );
        assert_eq!(netloc("http://clawd.local:8080/custom"), "clawd.local:8080");
    }
}
