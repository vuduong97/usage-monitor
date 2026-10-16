//! `usage.Publisher` (`usage.py:1194-1345`): one poll loop feeding every clock.
//!
//! Owns the poll schedule and the attention state, so an alert is consumed once and delivered
//! to every destination in the same payload. Alerts and clears always ride on a fresh poll,
//! never on their own. `POLL_INTERVAL` 60 s, `TICK` 5 s.

use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use clawd_config::{read_http_targets, read_onoff_setting, read_usage_source};
use clawd_contracts::{BoxFuture, Clock, StopRx, TargetsFn, UsagePayload};
use futures_util::FutureExt;
use serde_json::json;
use tokio::sync::Notify;

use crate::payload::{add_clock_fields, take_attention_flag, take_clears, Attention};
use crate::selector::{poll_active, PollState};
use crate::sinks::{HttpSinks, RESEND_TIMEOUT};
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

/// The per-step state of `Publisher` (`last_poll`, `alert`, ...), behind one async lock so a
/// step is never interleaved with another.
struct StepState {
    poll: PollState,
    last_poll: f64,
    /// Latest alert, kept to match its session's clear.
    alert: Option<Attention>,
    alert_unsent: bool,
    clear_unsent: bool,
    /// "No usable source" already logged.
    quiet: bool,
}

/// `usage.Publisher`. Shared (`Arc`) between its own [`Self::run`] loop and the engine
/// (`resend` from `BootWatch`, `request_poll` from settings), hence `&self` everywhere.
pub struct Publisher {
    env: UsageEnv,
    http: HttpSinks,
    targets: Option<TargetsFn>,
    poller: Option<PollFn>,
    state: tokio::sync::Mutex<StepState>,
    on_payload: Mutex<Option<PayloadHook>>,
    /// The last delivered payload, for [`Self::resend`].
    last_payload: Mutex<Option<UsagePayload>>,
    /// `Publisher.force`: poll on the next step regardless of schedule.
    force: AtomicBool,
    /// `Publisher.wake`.
    wake: Notify,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Publisher {
    /// `Publisher(http, targets)`: `targets` `None` = `read_http_targets` from the config file.
    pub fn new(env: UsageEnv, http: HttpSinks, targets: Option<TargetsFn>) -> Self {
        Self {
            env,
            http,
            targets,
            poller: None,
            state: tokio::sync::Mutex::new(StepState {
                poll: PollState::new(),
                last_poll: 0.0,
                alert: None,
                alert_unsent: false,
                clear_unsent: false,
                quiet: false,
            }),
            on_payload: Mutex::new(None),
            last_payload: Mutex::new(None),
            force: AtomicBool::new(false),
            wake: Notify::new(),
        }
    }

    /// Use `poll` instead of [`crate::poll_active`] (tests; the real one is the default).
    pub fn with_poller(mut self, poll: PollFn) -> Self {
        self.poller = Some(poll);
        self
    }

    /// Set `Publisher.on_payload`.
    pub fn set_on_payload(&self, hook: PayloadHook) {
        *lock(&self.on_payload) = Some(hook);
    }

    /// `pub.force = True; pub.wake.set()` (`Engine.poll_now`): poll and send on the next tick
    /// whatever the schedule says (the usage source changed). Any thread.
    pub fn request_poll(&self) {
        self.force.store(true, Ordering::SeqCst);
        self.wake.notify_one();
    }

    fn current_targets(&self) -> Vec<String> {
        match &self.targets {
            Some(f) => f(),
            None => read_http_targets(&self.env.paths),
        }
    }

    /// One tick: consume alerts/clears, poll when due or forced, deliver. A poll that yields no
    /// usable source stays silent (backs the source off) rather than hammering the API.
    pub async fn step(&self) {
        let targets = self.current_targets();
        if targets.is_empty() {
            return; // nobody to show it to: leave attention flags for later
        }
        let mut st = self.state.lock().await;

        // One alert slot, latest wins: two sessions alerting in the same tick keep only the
        // last one.
        let new_alert = take_attention_flag(&self.env, None);
        // A session that is still waiting can notify twice (AskUserQuestion fires
        // PreToolUse, then a permission_prompt). Keep the first card and beep once until that
        // session clears.
        let repeat = match (&new_alert, &st.alert) {
            (Some(new), Some(old)) => !old.s.is_empty() && new.s == old.s,
            _ => false,
        };
        if let Some(new) = new_alert.filter(|_| !repeat) {
            st.alert = Some(new);
            st.alert_unsent = true;
            st.clear_unsent = false;
        }
        let clears = take_clears(&self.env);
        if let Some(alert) = &st.alert {
            if clears.get(&alert.s).copied().unwrap_or(0.0) > alert.t {
                st.clear_unsent = !st.alert_unsent; // never shown, nothing to clear
                st.alert = None;
                st.alert_unsent = false;
            }
        }

        let now = self.env.clock.now();
        let due = now - st.last_poll >= POLL_INTERVAL;
        let forced = self.force.load(Ordering::SeqCst);
        if !(forced || due || st.alert_unsent || st.clear_unsent) {
            return;
        }
        self.force.store(false, Ordering::SeqCst);

        // Pure free-ride: read whatever access token(s) Claude Code currently holds across the
        // configured config dirs and NEVER refresh them ourselves.
        let (payload, dead) = match &self.poller {
            Some(poll) => poll().await,
            None => poll_active(&self.env, &mut st.poll).await,
        };
        let mut extra = UsagePayload::new();
        if st.alert_unsent {
            if let Some(alert) = &st.alert {
                if read_onoff_setting(&self.env.paths, "beep", "off") == "on" {
                    extra.insert("b".into(), json!(1));
                }
                if !alert.m.is_empty() {
                    extra.insert("m".into(), json!(alert.m));
                }
            }
        }
        if st.clear_unsent {
            extra.insert("x".into(), json!(1));
        }
        let out = if let Some(payload) = payload {
            st.quiet = false;
            let mut out = payload;
            out.extend(extra.into_map());
            out
        } else if dead {
            // No live token in any config dir (missing, or a 401/expired token) -> show "No
            // data" now instead of stale numbers.
            let codex = read_usage_source(&self.env.paths) == "codex";
            let mut out = UsagePayload::new();
            out.insert("ok".into(), json!(false));
            if codex {
                tracing::info!(
                    "No Codex usage yet (no rate limits in ~/.codex/sessions); signalling no-data"
                );
                out.insert("src".into(), json!("codex"));
                out.extend(extra.into_map());
            } else {
                tracing::info!(
                    "No usable token; signalling no-data to device - run `claude login` or use \
                     the CLI to let Claude Code renew it"
                );
                // why = login: a clock with fwSeq 7+ says "Login expired" instead of "No
                // data", and keeps the time running (an older one ignores both).
                out.insert("why".into(), json!("login"));
                out.extend(extra.into_map());
                add_clock_fields(&self.env, &mut out);
            }
            out
        } else {
            // A live token that didn't answer usefully this cycle -> stay silent. The poll backs
            // that source off for a poll interval, so the next ticks don't hammer the API.
            if !st.quiet {
                tracing::info!("No usable source this cycle; retrying quietly");
            }
            st.quiet = true;
            return;
        };

        // Guard last_poll on delivery so a failed write retries next tick instead of
        // throttling a healthy link for a full POLL_INTERVAL.
        if self.send(&out, &targets).await {
            st.last_poll = self.env.clock.now();
            st.alert_unsent = false;
            st.clear_unsent = false;
            // One-shot extras (beep, alert line, clear) are not replayed.
            let kept = out
                .iter()
                .filter(|(k, _)| !matches!(k.as_str(), "b" | "m" | "x"))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            *lock(&self.last_payload) = Some(UsagePayload(kept));
        }
    }

    async fn send(&self, payload: &UsagePayload, targets: &[String]) -> bool {
        let hook = lock(&self.on_payload).clone();
        if let Some(hook) = hook {
            hook(payload);
        }
        tracing::debug!("Sending: {}", crate::payload::dumps(payload));
        self.http.send(targets, payload, None).await
    }

    /// `Publisher.resend(url)`: push the last delivered payload again to one clock that just
    /// restarted so it leaves its "no data" splash now; never calls the usage API; with nothing
    /// cached it asks for a poll instead. False only when the clock did not take the data.
    pub async fn resend(&self, url: &str) -> bool {
        let cached = lock(&self.last_payload).clone();
        let Some(mut out) = cached else {
            self.request_poll();
            return true;
        };
        if out.contains_key("t") {
            add_clock_fields(&self.env, &mut out); // the cached wall-clock time is up to a poll old
        }
        self.http
            .send(&[url.to_owned()], &out, Some(RESEND_TIMEOUT))
            .await
    }

    /// `Publisher.run(stop)`: `step`, then wait for a wake or `TICK`; a failing step is logged
    /// and the loop goes on.
    pub async fn run(&self, mut stop: StopRx) {
        while !*stop.borrow() {
            if AssertUnwindSafe(self.step()).catch_unwind().await.is_err() {
                tracing::error!("Publish cycle panicked; the next tick retries");
            }
            tokio::select! {
                _ = self.wake.notified() => {}
                _ = tokio::time::sleep(Duration::from_secs_f64(TICK)) => {}
                _ = stop.wait_for(|stopped| *stopped) => break,
            }
        }
    }
}
