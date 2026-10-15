//! Usage from Anthropic: the read-only `GET /api/oauth/usage` first (costs no quota), the Haiku
//! rate-limit-header probe as fallback for accounts where it reports nothing usable
//! (Enterprise spend limits, API organisations). Pure free-ride: a 401/403 means the token's
//! owner (Claude Code) must renew it; we never refresh. Token goes only to `api.anthropic.com`
//! and never into a log line.
//!
//! The numbers must match Python exactly, rounding included (`int(round(x))` is banker's
//! rounding in Python; `.5` boundaries are in the golden fixtures).

use clawd_contracts::{TokenExpired, UsageNotApplicable, UsagePayload};
use serde_json::Value;
use thiserror::Error;

use crate::UsageEnv;

/// `usage.API_URL`: the Haiku probe endpoint.
pub const API_URL: &str = "https://api.anthropic.com/v1/messages";
/// `usage.USAGE_URL`: the read-only usage endpoint.
pub const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";

/// What the usage endpoint can fail with (`TokenExpired` / `UsageNotApplicable` in Python).
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PollError {
    #[error(transparent)]
    TokenExpired(#[from] TokenExpired),
    #[error(transparent)]
    NotApplicable(#[from] UsageNotApplicable),
}

/// `usage.poll_api(token)`: the Haiku probe: payload from the `anthropic-ratelimit-unified-*`
/// response headers (`acct: "ent"` plus billing-period fields). `Ok(None)` on a network error or
/// non-auth HTTP error (try again next poll); 401/403 -> [`TokenExpired`].
pub async fn poll_api(env: &UsageEnv, token: &str) -> Result<Option<UsagePayload>, TokenExpired> {
    todo!("phase 07")
}

/// `usage.usage_from_oauth_usage(body, now)`: map an `/api/oauth/usage` body to the device
/// payload (`s sr w wr st acct ok`). `now` = epoch seconds, `None` = the real time.
/// [`UsageNotApplicable`] when the body carries no 5h window.
pub fn usage_from_oauth_usage(
    body: &Value,
    now: Option<f64>,
) -> Result<UsagePayload, UsageNotApplicable> {
    todo!("phase 07")
}

/// `usage.poll_usage_endpoint(token)`: usage for a Claude Code token via the read-only
/// endpoint, with `c`/`t`/`tf` fields added per the config. `Ok(None)` for a transient failure;
/// 401/403 -> [`TokenExpired`]; 404 or a "not applicable" 400 -> [`UsageNotApplicable`].
pub async fn poll_usage_endpoint(
    env: &UsageEnv,
    token: &str,
) -> Result<Option<UsagePayload>, PollError> {
    todo!("phase 07")
}

/// `usage.poll_token(token)`: the endpoint first, the probe ([`poll_api`]) only when the
/// endpoint says [`UsageNotApplicable`].
pub async fn poll_token(env: &UsageEnv, token: &str) -> Result<Option<UsagePayload>, TokenExpired> {
    todo!("phase 07")
}
