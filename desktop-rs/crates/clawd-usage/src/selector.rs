//! Which plan is "active" and the poll across every Claude source (`usage.py:915-1051`).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use clawd_config::{read_config_dirs, read_onoff_setting, read_token_for, read_usage_source};
use clawd_contracts::{Clock, TokenExpired, UsagePayload};
use serde_json::json;

use crate::cliproxy::CliProxySource;
use crate::oauth::poll_token;
use crate::payload::{add_chime_field, add_clock_fields};
use crate::publisher::POLL_INTERVAL;
use crate::util::{blocking, expand_user};
use crate::{codex, UsageEnv};

/// A plan the selector chooses between: a Claude config dir, or `cliproxy:<auth index>`
/// (Python used a `Path` or a `str` as the dict key).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PlanKey {
    Dir(PathBuf),
    /// `"cliproxy:<first 8 chars of the auth index>"`.
    Proxy(String),
}

impl std::fmt::Display for PlanKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PlanKey::Dir(d) => write!(f, "{}", d.display()),
            PlanKey::Proxy(p) => f.write_str(p),
        }
    }
}

/// `usage.PlanSelector`: "active" = the plan whose session % rose most recently; a rise stamps a
/// poll counter so the choice is sticky and a window reset (a drop to 0) is not taken for use.
/// Before any rise (startup) the highest current session % wins.
#[derive(Debug, Default)]
pub struct PlanSelector {
    prev_s: HashMap<PlanKey, i64>,
    last_active: HashMap<PlanKey, u64>,
    seq: u64,
}

impl PlanSelector {
    pub fn new() -> Self {
        Self::default()
    }

    /// `PlanSelector.choose(sessions)`: update from this cycle's `{plan: session %}` and return
    /// the active plan. `sessions` must not be empty. A `HashMap` has no order, so ties are
    /// broken by plan key; [`Self::choose_ordered`] keeps Python's insertion-order tie-break.
    pub fn choose(&mut self, sessions: &HashMap<PlanKey, i64>) -> PlanKey {
        let mut ordered: Vec<(PlanKey, i64)> =
            sessions.iter().map(|(k, v)| (k.clone(), *v)).collect();
        ordered.sort_by(|a, b| a.0.cmp(&b.0));
        self.choose_ordered(&ordered)
    }

    /// [`Self::choose`] for plans in the order Python's dict would hold them (config dirs, then
    /// proxy accounts): `max()` returns the first of equal candidates, so the order is the tie-break.
    pub fn choose_ordered(&mut self, sessions: &[(PlanKey, i64)]) -> PlanKey {
        self.seq += 1;
        for (plan, s) in sessions {
            if self.prev_s.get(plan).is_some_and(|prev| s > prev) {
                self.last_active.insert(plan.clone(), self.seq);
            }
            self.prev_s.insert(plan.clone(), *s);
        }
        let rank = |plan: &PlanKey, s: i64| (self.last_active.get(plan).copied().unwrap_or(0), s);
        let mut best = &sessions[0];
        for candidate in &sessions[1..] {
            if rank(&candidate.0, candidate.1) > rank(&best.0, best.1) {
                best = candidate;
            }
        }
        best.0.clone()
    }
}

/// The module-level state of `usage.py` (`_SELECTOR`, `_BACKOFF`, `_CLIPROXY`) as one value,
/// owned by the [`crate::Publisher`] so the active-plan choice survives between polls.
#[derive(Default)]
pub struct PollState {
    pub selector: PlanSelector,
    pub cliproxy: CliProxySource,
    /// Sources that answered with a non-auth client error: plan -> time (epoch s) before
    /// which they are not retried (the publisher's 5 s tick must not hammer the API).
    pub backoff: HashMap<PlanKey, f64>,
}

impl PollState {
    pub fn new() -> Self {
        Self::default()
    }
}

/// `usage._poll_dir(d)`: `(payload, live)` for one Claude Code config dir.
async fn poll_dir(
    env: &UsageEnv,
    state: &mut PollState,
    dir: &Path,
) -> (Option<UsagePayload>, bool) {
    let token = {
        let (paths, runner, dir) = (env.paths.clone(), env.runner.clone(), dir.to_path_buf());
        blocking(move || read_token_for(&paths, runner.as_ref(), &dir))
            .await
            .flatten()
    };
    let Some(token) = token else {
        tracing::info!("No token in {}; skipping", dir.display());
        return (None, false);
    };
    let payload = match poll_token(env, &token).await {
        Ok(payload) => payload,
        Err(TokenExpired) => {
            tracing::info!("Token in {} expired/invalid; skipping", dir.display());
            return (None, false);
        }
    };
    if payload.is_none() {
        // Authenticated but no answer: wait a full poll interval before asking this dir again
        // rather than retrying every tick.
        state.backoff.insert(
            PlanKey::Dir(dir.to_path_buf()),
            env.clock.now() + POLL_INTERVAL,
        );
    }
    (payload, true)
}

/// A Python dict of `{plan: payload}`: insertion order, a repeated key keeps its first slot.
fn put(payloads: &mut Vec<(PlanKey, UsagePayload)>, plan: PlanKey, payload: UsagePayload) {
    match payloads.iter_mut().find(|(k, _)| *k == plan) {
        Some(slot) => slot.1 = payload,
        None => payloads.push((plan, payload)),
    }
}

/// `usage.poll_claude(selector, only)`: poll every Claude source (each configured config dir,
/// and with `cliproxy = on` every proxy account); `(active payload, all_dead)`. `all_dead` is
/// true when *every* source lacked a usable token this cycle, so the caller can signal "No
/// data". `only` restricts to one dir (`usage_source = claude:<dir>`).
pub async fn poll_claude(
    env: &UsageEnv,
    state: &mut PollState,
    only: Option<PathBuf>,
) -> (Option<UsagePayload>, bool) {
    let dirs = match &only {
        Some(dir) => vec![dir.clone()],
        None => read_config_dirs(&env.paths),
    };
    let mut payloads: Vec<(PlanKey, UsagePayload)> = Vec::new();
    let mut any_live = false;
    let now = env.clock.now();
    for dir in &dirs {
        let plan = PlanKey::Dir(dir.clone());
        if state.backoff.get(&plan).is_some_and(|until| *until > now) {
            any_live = true; // answered recently, just not usefully
            continue;
        }
        let (payload, live) = poll_dir(env, state, dir).await;
        any_live = any_live || live;
        if let Some(payload) = payload {
            state.backoff.remove(&plan);
            put(&mut payloads, plan, payload);
        }
    }
    if only.is_none() && read_onoff_setting(&env.paths, "cliproxy", "off") == "on" {
        let (proxy_payloads, proxy_live) = state.cliproxy.poll_ordered(env).await;
        any_live = any_live || proxy_live;
        for (plan, payload) in proxy_payloads {
            put(&mut payloads, plan, payload);
        }
    }
    if payloads.is_empty() {
        return (None, !any_live);
    }
    let sessions: Vec<(PlanKey, i64)> = payloads
        .iter()
        .map(|(k, p)| (k.clone(), p.s().unwrap_or(0)))
        .collect();
    let active = state.selector.choose_ordered(&sessions);
    if payloads.len() > 1 || dirs.len() > 1 {
        let s = sessions
            .iter()
            .find(|(k, _)| *k == active)
            .map_or(0, |(_, s)| *s);
        tracing::info!("Active plan: {active} (s={s})");
    }
    let chosen = payloads
        .into_iter()
        .find(|(k, _)| *k == active)
        .map(|(_, p)| p);
    (chosen, false)
}

/// `usage.poll_active(selector)`: the usage source the user chose (`read_usage_source`):
/// `off` -> `{ok: false, src: "off"}` plus clock fields; `codex` -> [`crate::codex::read`];
/// else [`poll_claude`]. Same `(payload, all_dead)` result.
pub async fn poll_active(env: &UsageEnv, state: &mut PollState) -> (Option<UsagePayload>, bool) {
    let source = read_usage_source(&env.paths);
    if source == "off" {
        let mut out = UsagePayload::new();
        out.insert("ok".into(), json!(false));
        out.insert("src".into(), json!("off"));
        add_clock_fields(env, &mut out); // the clock still keeps time
        return (Some(out), false);
    }
    if source == "codex" {
        let Some(mut payload) = codex::read(&env.paths, &env.env, Some(env.clock.now())) else {
            return (None, true);
        };
        add_chime_field(env, &mut payload);
        add_clock_fields(env, &mut payload);
        return (Some(payload), false);
    }
    let only = source
        .strip_prefix("claude:")
        .map(|dir| expand_user(&env.paths, dir));
    poll_claude(env, state, only).await // no "src": the clock reads that as Claude
}

/// `usage.poll_active_payload(selector)`: just the payload of [`poll_active`].
pub async fn poll_active_payload(env: &UsageEnv, state: &mut PollState) -> Option<UsagePayload> {
    poll_active(env, state).await.0
}
