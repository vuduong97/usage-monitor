//! Which plan is "active" and the poll across every Claude source (`usage.py:915-1051`).

use std::collections::HashMap;
use std::path::PathBuf;

use clawd_contracts::UsagePayload;

use crate::cliproxy::CliProxySource;
use crate::UsageEnv;

/// A plan the selector chooses between: a Claude config dir, or `cliproxy:<auth index>`
/// (Python used a `Path` or a `str` as the dict key).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PlanKey {
    Dir(PathBuf),
    /// `"cliproxy:<first 8 chars of the auth index>"`.
    Proxy(String),
}

/// `usage.PlanSelector`: "active" = the plan whose session % rose most recently; a rise stamps a
/// poll counter so the choice is sticky and a window reset (a drop to 0) is not taken for use.
/// Before any rise (startup) the highest current session % wins.
pub struct PlanSelector {
    _private: (),
}

impl PlanSelector {
    pub fn new() -> Self {
        todo!("phase 07")
    }

    /// `PlanSelector.choose(sessions)`: update from this cycle's `{plan: session %}` and return
    /// the active plan. `sessions` must not be empty.
    pub fn choose(&mut self, sessions: &HashMap<PlanKey, i64>) -> PlanKey {
        todo!("phase 07")
    }
}

/// The module-level state of `usage.py` (`_SELECTOR`, `_BACKOFF`, `_CLIPROXY`) as one value,
/// owned by the [`crate::Publisher`] so the active-plan choice survives between polls.
pub struct PollState {
    pub selector: PlanSelector,
    pub cliproxy: CliProxySource,
    /// Sources that answered with a non-auth client error: plan -> time (epoch s) before
    /// which they are not retried (the publisher's 5 s tick must not hammer the API).
    pub backoff: HashMap<PlanKey, f64>,
}

impl PollState {
    pub fn new() -> Self {
        todo!("phase 07")
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
    todo!("phase 07")
}

/// `usage.poll_active(selector)`: the usage source the user chose (`read_usage_source`):
/// `off` -> `{ok: false, src: "off"}` plus clock fields; `codex` -> [`crate::codex::read`];
/// else [`poll_claude`]. Same `(payload, all_dead)` result.
pub async fn poll_active(env: &UsageEnv, state: &mut PollState) -> (Option<UsagePayload>, bool) {
    todo!("phase 07")
}

/// `usage.poll_active_payload(selector)`: just the payload of [`poll_active`].
pub async fn poll_active_payload(env: &UsageEnv, state: &mut PollState) -> Option<UsagePayload> {
    todo!("phase 07")
}
