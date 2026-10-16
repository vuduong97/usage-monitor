//! CLIProxyAPI as a usage source (`usage.CliProxySource`): the proxy's management API makes the
//! usage request with an account's token (`$TOKEN$` is substituted proxy-side), so the Claude
//! token never leaves the proxy. Settings (`read_cliproxy_key`/`url`) are in `clawd-config`.

use std::collections::HashMap;
use std::time::Duration;

use clawd_config::cliproxy::{
    CLIPROXY_API_CALL_PATH, CLIPROXY_AUTH_FILES_PATH, CLIPROXY_KEYCHAIN_SERVICE,
};
use clawd_config::read_cliproxy_url;
use clawd_contracts::{Clock, TokenExpired, UsageNotApplicable, UsagePayload};
use serde_json::{json, Value};

use crate::oauth::{usage_result, PollError, USAGE_HEADERS, USAGE_URL};
use crate::publisher::POLL_INTERVAL;
use crate::selector::PlanKey;
use crate::util::{blocking, py_int, truthy};
use crate::UsageEnv;

/// `usage.CLIPROXY_ACCOUNTS_TTL` (seconds): re-list proxy accounts this often.
pub const CLIPROXY_ACCOUNTS_TTL: f64 = 300.0;
/// `usage.CLIPROXY_TIMEOUT` (seconds).
pub const CLIPROXY_TIMEOUT: f64 = 20.0;

/// `usage.CliProxySource`: every active Claude account in CLIProxyAPI, each polled as its own
/// plan (`cliproxy:<auth index[:8]>`). A proxy-level failure (not running, wrong key, timeout)
/// makes the whole source yield nothing this cycle; a 401/403 for one account only kills that
/// account.
#[derive(Debug, Default)]
pub struct CliProxySource {
    /// Last source-level failure, logged once.
    problem: String,
    /// Auth index -> retry time.
    backoff: HashMap<String, f64>,
    accounts: Vec<String>,
    listed_at: f64,
    listed_url: String,
}

fn timeout() -> Duration {
    Duration::from_secs_f64(CLIPROXY_TIMEOUT)
}

impl CliProxySource {
    pub fn new() -> Self {
        Self::default()
    }

    /// Log a source-level failure once, not on every 5 s retry tick.
    fn report(&mut self, problem: &str) {
        if !problem.is_empty() && problem != self.problem {
            tracing::warn!("{problem}");
        }
        self.problem = problem.to_owned();
    }

    async fn list_accounts(&mut self, env: &UsageEnv, url: &str, key: &str) -> Option<Vec<String>> {
        let now = env.clock.now();
        if url == self.listed_url && now - self.listed_at < CLIPROXY_ACCOUNTS_TTL {
            return Some(self.accounts.clone());
        }
        let sent = env
            .client
            .get(format!("{url}{CLIPROXY_AUTH_FILES_PATH}"))
            .bearer_auth(key)
            .timeout(timeout())
            .send()
            .await;
        let resp = match sent {
            Ok(resp) => resp,
            Err(e) => {
                let kind = if e.is_timeout() { "timeout" } else { "error" };
                self.report(&format!("cliproxy: {url} unreachable ({kind})"));
                return None;
            }
        };
        let status = resp.status().as_u16();
        if status != 200 {
            let hint = if status == 401 || status == 403 {
                ", check the management key"
            } else {
                ""
            };
            self.report(&format!(
                "cliproxy: listing accounts failed (HTTP {status}{hint})"
            ));
            return None;
        }
        let Ok(data) = resp.json::<Value>().await else {
            self.report("cliproxy: account list is not JSON");
            return None;
        };
        let files = match &data {
            Value::Object(o) => o.get("files"),
            other => Some(other),
        };
        let mut accounts = Vec::new();
        for f in files.and_then(Value::as_array).into_iter().flatten() {
            let Some(f) = f.as_object() else { continue };
            let provider = f
                .get("provider")
                .filter(|v| truthy(v))
                .or_else(|| f.get("type"));
            if provider.and_then(Value::as_str) != Some("claude") {
                continue;
            }
            if f.get("disabled").is_some_and(truthy) || f.get("unavailable").is_some_and(truthy) {
                continue;
            }
            let idx = f
                .get("auth_index")
                .filter(|v| truthy(v))
                .or_else(|| f.get("authIndex"));
            if let Some(Value::String(idx)) = idx {
                if !idx.is_empty() {
                    accounts.push(idx.clone());
                }
            }
        }
        if accounts != self.accounts || url != self.listed_url {
            tracing::info!("cliproxy: {} claude account(s)", accounts.len());
        }
        self.accounts = accounts.clone();
        self.listed_at = now;
        self.listed_url = url.to_owned();
        Some(accounts)
    }

    async fn poll_account(
        &mut self,
        env: &UsageEnv,
        url: &str,
        key: &str,
        idx: &str,
    ) -> Result<Option<UsagePayload>, PollError> {
        let short: String = idx.chars().take(8).collect();
        let mut header = serde_json::Map::new();
        for (name, value) in USAGE_HEADERS {
            header.insert(name.to_owned(), json!(value));
        }
        header.insert("Authorization".into(), json!("Bearer $TOKEN$"));
        let req = json!({
            "authIndex": idx,
            "method": "GET",
            "url": USAGE_URL,
            "header": header,
        });
        let sent = env
            .client
            .post(format!("{url}{CLIPROXY_API_CALL_PATH}"))
            .bearer_auth(key)
            .timeout(timeout())
            .json(&req)
            .send()
            .await;
        let resp = match sent {
            Ok(resp) => resp,
            Err(e) => {
                let kind = if e.is_timeout() { "timeout" } else { "error" };
                tracing::warn!("cliproxy:{short} call failed ({kind})");
                return Ok(None);
            }
        };
        let status = resp.status().as_u16();
        if status != 200 {
            tracing::warn!("cliproxy:{short} api-call HTTP {status}");
            if status == 401 || status == 403 {
                // management key rejected
                self.listed_at = 0.0;
            }
            return Ok(None);
        }
        let Ok(wrap) = resp.json::<Value>().await else {
            tracing::warn!("cliproxy:{short} api-call returned non-JSON");
            return Ok(None);
        };
        let body = match wrap.get("body") {
            Some(Value::String(s)) => s.clone(),
            Some(Value::Null) | None => String::new(),
            Some(other) => other.to_string(),
        };
        let upstream = wrap
            .get("status_code")
            .filter(|v| truthy(v))
            .or_else(|| wrap.get("status"));
        let status = py_int(upstream).unwrap_or(0);
        if status == 0 {
            tracing::warn!("cliproxy:{short} api-call without an upstream status");
            return Ok(None);
        }
        usage_result(env, status, &body)
    }

    /// `CliProxySource.poll()`: `({plan key: payload}, any_live)` for this cycle.
    pub async fn poll(&mut self, env: &UsageEnv) -> (HashMap<PlanKey, UsagePayload>, bool) {
        let (payloads, live) = self.poll_ordered(env).await;
        (payloads.into_iter().collect(), live)
    }

    /// [`Self::poll`] keeping the proxy's account order (what `PlanSelector` ties break on).
    pub async fn poll_ordered(&mut self, env: &UsageEnv) -> (Vec<(PlanKey, UsagePayload)>, bool) {
        let reader = env.cliproxy_key_reader;
        let runner = env.runner.clone();
        let key = blocking(move || reader(runner.as_ref())).await.flatten();
        let Some(key) = key else {
            self.report(&format!(
                "cliproxy: no management key in Keychain (service {CLIPROXY_KEYCHAIN_SERVICE})"
            ));
            return (Vec::new(), false);
        };
        let url = read_cliproxy_url(&env.paths);
        let mut payloads = Vec::new();
        let mut any_live = false;
        let accounts = self.list_accounts(env, &url, &key).await;
        if accounts.is_some() {
            self.report("");
        }
        let now = env.clock.now();
        for idx in accounts.unwrap_or_default() {
            let plan = format!("cliproxy:{}", idx.chars().take(8).collect::<String>());
            if self.backoff.get(&idx).is_some_and(|until| *until > now) {
                any_live = true; // answered recently, just not usefully
                continue;
            }
            match self.poll_account(env, &url, &key, &idx).await {
                Err(PollError::TokenExpired(TokenExpired)) => {
                    tracing::info!("{plan}: token expired/invalid in the proxy; skipping");
                }
                Err(PollError::NotApplicable(UsageNotApplicable(why))) => {
                    tracing::info!("{plan}: usage not applicable ({why}); skipping");
                    self.backoff.insert(idx, now + CLIPROXY_ACCOUNTS_TTL);
                    any_live = true;
                }
                Ok(None) => {
                    any_live = true;
                    self.backoff.insert(idx, now + POLL_INTERVAL);
                }
                Ok(Some(payload)) => {
                    any_live = true;
                    self.backoff.remove(&idx);
                    tracing::info!(
                        "{plan} s={} w={}",
                        payload.s().unwrap_or(0),
                        payload.w().unwrap_or(0)
                    );
                    payloads.push((PlanKey::Proxy(plan), payload));
                }
            }
        }
        (payloads, any_live)
    }
}
