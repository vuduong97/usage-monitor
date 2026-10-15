//! Codex CLI usage for the clock, read from Codex's own session logs; no network
//! (`codex_usage.py`).
//!
//! Codex writes a `token_count` event into its session file
//! (`~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl`, `CODEX_HOME` moves it) after each model
//! reply, carrying the account's rate limits. The numbers are as fresh as the last Codex turn: a
//! window whose reset time has passed reads as 0 %. Plans with null windows: no data.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use clawd_contracts::{AppPaths, UsagePayload};
use serde_json::Value;

/// `codex_usage.TAIL`: bytes read from the end of a session file.
pub const TAIL: u64 = 256 * 1024;
/// `codex_usage.FILES`: newest session files looked at.
pub const FILES: usize = 8;

/// `codex_usage.sessions_dir(home, env)`: `$CODEX_HOME/sessions` or `<home>/.codex/sessions`.
/// `env` `None` = the process environment.
pub fn sessions_dir(paths: &AppPaths, env: Option<&HashMap<String, String>>) -> PathBuf {
    todo!("phase 07")
}

/// `codex_usage.newest_files(root, limit)`: newest session files, walking `YYYY/MM/DD` folders
/// newest first (no full scan).
pub fn newest_files(root: &Path, limit: usize) -> Vec<PathBuf> {
    todo!("phase 07")
}

/// `codex_usage.payload_from(rl, written, now)`: map Codex `rate_limits` to the clock's usage
/// fields (`s sr w wr st acct ok src`); `None` when no window is usable.
pub fn payload_from(rate_limits: &Value, written: f64, now: Option<f64>) -> Option<UsagePayload> {
    todo!("phase 07")
}

/// `codex_usage.read(home, env, now)`: the payload from the newest session log with limits, or
/// `None`.
pub fn read(
    paths: &AppPaths,
    env: Option<&HashMap<String, String>>,
    now: Option<f64>,
) -> Option<UsagePayload> {
    todo!("phase 07")
}

/// `codex_usage.available(home, env)`: Codex has left session logs on this machine (the setup
/// wizard offers the source).
pub fn available(paths: &AppPaths, env: Option<&HashMap<String, String>>) -> bool {
    todo!("phase 07")
}
