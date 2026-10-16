#![allow(unused_variables)] // TEMP: stub until its milestone lands
//! Claude OAuth token sources (`usage.py:55-96, 131-290`). Pure free-ride: read whatever token
//! Claude Code currently holds and never refresh it. A read error is `None`, never a panic;
//! nothing here logs a token.
//!
//! Order for the default config dir: Anthropic credentials store v5 (`~/.config/anthropic`,
//! unexpired OAuth token) -> `<dir>/.credentials.json` -> macOS Keychain
//! (`security find-generic-password`, same mechanism and ACL behaviour as Python).

use std::path::Path;

use clawd_contracts::AppPaths;

use crate::command::CommandRunner;

/// `usage.KEYCHAIN_SERVICE`: Claude Code's Keychain item on macOS.
pub const KEYCHAIN_SERVICE: &str = "Claude Code-credentials";

/// `usage._extract_access_token(blob)`: the `accessToken` of a credentials blob (JSON object,
/// possibly nested as `{"claudeAiOauth": {"accessToken": ...}}`); falls back to a raw
/// `sk-ant-...` token. `None` when nothing usable.
pub fn extract_access_token(blob: &str) -> Option<String> {
    todo!("phase 06")
}

/// `usage._decode_keychain_blob(raw)`: `security ... -w` prints a secret as a continuous hex
/// string when the stored bytes are not cleanly printable; decode it back to text, else return
/// `raw` unchanged.
pub fn decode_keychain_blob(raw: &str) -> String {
    todo!("phase 06")
}

/// `usage._read_token_keychain()`: the access token from the macOS Keychain, or `None`
/// (also on other OSes).
pub fn read_token_keychain(runner: &dyn CommandRunner) -> Option<String> {
    todo!("phase 06")
}

/// `usage._read_token_store(store, now)`: the unexpired OAuth access token from the Anthropic
/// credentials store at `store` (`<store>/active_config` names the profile,
/// `<store>/credentials/<profile>.json` holds `{"type": "oauth_token", "access_token",
/// "expires_at"}`, ms or s); expired or non-OAuth entries are `None`. `now` = epoch seconds,
/// `None` = the real time. Callers pass [`AppPaths::anthropic_store_dir`].
pub fn read_token_store(store: &Path, now: Option<f64>) -> Option<String> {
    todo!("phase 06")
}

/// `usage.read_token_for(config_dir)`: the OAuth token for one Claude config dir (store v5 for
/// the default dir, then its `.credentials.json`, then the Keychain on macOS for the default dir).
pub fn read_token_for(
    paths: &AppPaths,
    runner: &dyn CommandRunner,
    config_dir: &Path,
) -> Option<String> {
    todo!("phase 06")
}

/// `engine.has_claude_token()`: true when some configured Claude dir holds a token (does not
/// call the API).
pub fn has_claude_token(paths: &AppPaths, runner: &dyn CommandRunner) -> bool {
    todo!("phase 06")
}
