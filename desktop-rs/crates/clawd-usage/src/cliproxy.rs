//! CLIProxyAPI as a usage source (`usage.CliProxySource`): the proxy's management API makes the
//! usage request with an account's token (`$TOKEN$` is substituted proxy-side), so the Claude
//! token never leaves the proxy. Settings (`read_cliproxy_key`/`url`) are in `clawd-config`.

use std::collections::HashMap;

use clawd_contracts::UsagePayload;

use crate::selector::PlanKey;
use crate::UsageEnv;

/// `usage.CLIPROXY_ACCOUNTS_TTL` (seconds): re-list proxy accounts this often.
pub const CLIPROXY_ACCOUNTS_TTL: f64 = 300.0;
/// `usage.CLIPROXY_TIMEOUT` (seconds).
pub const CLIPROXY_TIMEOUT: f64 = 20.0;

/// `usage.CliProxySource`: every active Claude account in CLIProxyAPI, each polled as its own
/// plan (`cliproxy:<auth index[:8]>`). A proxy-level failure (not running, wrong key, timeout)
/// makes the whole source yield nothing this cycle; a 401/403 for one account only kills that
/// account.
pub struct CliProxySource {
    _private: (),
}

impl CliProxySource {
    pub fn new() -> Self {
        todo!("phase 07")
    }

    /// `CliProxySource.poll()`: `({plan key: payload}, any_live)` for this cycle.
    pub async fn poll(&mut self, env: &UsageEnv) -> (HashMap<PlanKey, UsagePayload>, bool) {
        todo!("phase 07")
    }
}
