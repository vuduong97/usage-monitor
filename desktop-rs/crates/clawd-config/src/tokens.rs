//! Claude OAuth token sources (`usage.py:55-96, 131-290`). Pure free-ride: read whatever token
//! Claude Code currently holds and never refresh it. A read error is `None`, never a panic;
//! nothing here logs a token.
//!
//! Order for the default config dir: Anthropic credentials store v5 (`~/.config/anthropic`,
//! unexpired OAuth token) -> `<dir>/.credentials.json` -> macOS Keychain
//! (`security find-generic-password`, same mechanism and ACL behaviour as Python).

use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use clawd_contracts::AppPaths;
use serde_json::Value;

use crate::command::CommandRunner;
use crate::dirs::{read_config_dirs, same_path};
use crate::host::Host;
use crate::pytext::{is_py_space, strip};

/// `usage.KEYCHAIN_SERVICE`: Claude Code's Keychain item on macOS.
pub const KEYCHAIN_SERVICE: &str = "Claude Code-credentials";

/// `subprocess.run(..., timeout=10)` of `_read_token_keychain`.
const KEYCHAIN_TIMEOUT: Duration = Duration::from_secs(10);

/// `usage._extract_access_token(blob)`: the `accessToken` of a credentials blob (JSON object,
/// possibly nested as `{"claudeAiOauth": {"accessToken": ...}}`); falls back to a raw
/// `sk-ant-...` token. `None` when nothing usable.
pub fn extract_access_token(blob: &str) -> Option<String> {
    let blob = strip(blob);
    if blob.is_empty() {
        return None;
    }
    if let Ok(Value::Object(data)) = serde_json::from_str::<Value>(blob) {
        // direct: {"accessToken": "..."}
        if let Some(token) = non_blank_string(data.get("accessToken")) {
            return Some(token.to_owned());
        }
        // nested: {"claudeAiOauth": {"accessToken": "..."}}
        for value in data.values() {
            if let Value::Object(inner) = value {
                if let Some(token) = non_blank_string(inner.get("accessToken")) {
                    return Some(token.to_owned());
                }
            }
        }
    }
    if let Some(token) = scan_access_token(blob) {
        return Some(token.to_owned());
    }
    // Raw token (no JSON wrapper): must look plausible (sk-ant-... etc.)
    let raw_ok = blob.chars().count() >= 20
        && blob
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "_-.~+/=".contains(c));
    raw_ok.then(|| blob.to_owned())
}

fn non_blank_string(value: Option<&Value>) -> Option<&str> {
    match value {
        Some(Value::String(token)) if !strip(token).is_empty() => Some(token),
        _ => None,
    }
}

/// `re.search(r'"accessToken"\s*:\s*"([^"]+)"', blob)`: the first `"accessToken": "<token>"`
/// anywhere in the text (also inside JSON that did not parse).
fn scan_access_token(blob: &str) -> Option<&str> {
    const KEY: &str = "\"accessToken\"";
    let mut from = 0;
    while let Some(found) = blob[from..].find(KEY) {
        let after_key = from + found + KEY.len();
        from += found + 1;
        let rest = blob[after_key..].trim_start_matches(is_py_space);
        let Some(rest) = rest.strip_prefix(':') else {
            continue;
        };
        let Some(rest) = rest.trim_start_matches(is_py_space).strip_prefix('"') else {
            continue;
        };
        if let Some(end) = rest.find('"') {
            if end > 0 {
                return Some(&rest[..end]);
            }
        }
    }
    None
}

/// `usage._decode_keychain_blob(raw)`: `security ... -w` prints a secret as a continuous hex
/// string when the stored bytes are not cleanly printable; decode it back to text, else return
/// `raw` unchanged.
pub fn decode_keychain_blob(raw: &str) -> String {
    let s = strip(raw);
    if s.is_empty() || !s.len().is_multiple_of(2) || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return raw.to_owned();
    }
    let bytes: Vec<u8> = s
        .as_bytes()
        .chunks(2)
        .map(|pair| {
            let hex = std::str::from_utf8(pair).unwrap_or("00");
            u8::from_str_radix(hex, 16).unwrap_or(0)
        })
        .collect();
    String::from_utf8(bytes).unwrap_or_else(|_| raw.to_owned())
}

/// `usage._read_token_keychain()`: the access token from the macOS Keychain, or `None`
/// (also on other OSes, where `security` does not exist).
pub fn read_token_keychain(runner: &dyn CommandRunner) -> Option<String> {
    read_token_keychain_for(runner, &Host::system())
}

/// [`read_token_keychain`] for an explicit account name: `security find-generic-password
/// -s "Claude Code-credentials" -a <user> -w`, hex-decoded, then the access token.
pub fn read_token_keychain_for(runner: &dyn CommandRunner, host: &Host) -> Option<String> {
    if host.user.is_empty() {
        return None;
    }
    let args = [
        "find-generic-password",
        "-s",
        KEYCHAIN_SERVICE,
        "-a",
        host.user.as_str(),
        "-w",
    ];
    match runner.run("security", &args, KEYCHAIN_TIMEOUT) {
        Ok(out) if out.status == 0 => extract_access_token(&decode_keychain_blob(&out.stdout)),
        Ok(out) => {
            tracing::warn!(
                "Keychain read failed (rc={}): {}",
                out.status,
                out.stderr.trim()
            );
            None
        }
        Err(e) => {
            tracing::warn!("Keychain access error: {e}");
            None
        }
    }
}

/// `usage._read_token_store(store, now)`: the unexpired OAuth access token from the Anthropic
/// credentials store at `store` (`<store>/active_config` names the profile,
/// `<store>/credentials/<profile>.json` holds `{"type": "oauth_token", "access_token",
/// "expires_at"}`, ms or s); expired or non-OAuth entries are `None`. `now` = epoch seconds,
/// `None` = the real time. Callers pass [`AppPaths::anthropic_store_dir`].
pub fn read_token_store(store: &Path, now: Option<f64>) -> Option<String> {
    let profile = std::fs::read(store.join("active_config"))
        .map(|bytes| strip(&String::from_utf8_lossy(&bytes)).to_owned())
        .ok()
        .filter(|profile| !profile.is_empty())
        .unwrap_or_else(|| "default".to_owned());
    // the profile names a file inside the store: it must not climb out of it
    if profile.contains('/')
        || profile.starts_with('.')
        || (cfg!(windows) && profile.contains('\\'))
    {
        return None;
    }
    let bytes = std::fs::read(store.join("credentials").join(format!("{profile}.json"))).ok()?;
    let Ok(Value::Object(data)) = serde_json::from_slice::<Value>(&bytes) else {
        return None;
    };
    if data.get("type").and_then(Value::as_str) != Some("oauth_token") {
        return None;
    }
    let token = non_blank_string(data.get("access_token"))?;
    if let Some(expires) = expires_at(data.get("expires_at")) {
        let expires_s = if expires > 1e11 {
            expires / 1000.0
        } else {
            expires
        };
        if expires_s <= now.unwrap_or_else(epoch_now) {
            return None;
        }
    }
    Some(strip(token).to_owned())
}

/// `exp > 0` of a number (a JSON `true` counts as 1 like Python's `bool`), else no expiry.
fn expires_at(value: Option<&Value>) -> Option<f64> {
    let exp = match value? {
        Value::Number(n) => n.as_f64()?,
        Value::Bool(true) => 1.0,
        _ => return None,
    };
    (exp > 0.0).then_some(exp)
}

fn epoch_now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64())
}

/// `usage.read_token_for(config_dir)`: the OAuth token for one Claude config dir (store v5 for
/// the default dir, then its `.credentials.json`, then the Keychain on macOS for the default dir).
pub fn read_token_for(
    paths: &AppPaths,
    runner: &dyn CommandRunner,
    config_dir: &Path,
) -> Option<String> {
    read_token_for_on(paths, runner, config_dir, &Host::system())
}

/// [`read_token_for`] for an explicit [`Host`] (`sys.platform`, `getpass.getuser()`).
///
/// A `.credentials.json` that exists decides the answer, even when it holds no token (a logged-out
/// CLI blanks the values in place); only an absent or unreadable file falls through to the Keychain.
pub fn read_token_for_on(
    paths: &AppPaths,
    runner: &dyn CommandRunner,
    config_dir: &Path,
    host: &Host,
) -> Option<String> {
    let is_default = same_path(config_dir, &paths.default_claude_dir());
    if is_default {
        if let Some(token) = read_token_store(&paths.anthropic_store_dir(), None) {
            return Some(token);
        }
    }
    match std::fs::read(config_dir.join(".credentials.json")) {
        Ok(bytes) => return extract_access_token(&String::from_utf8_lossy(&bytes)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => tracing::warn!("Error reading credentials in {}: {e}", config_dir.display()),
    }
    if host.macos && is_default {
        return read_token_keychain_for(runner, host);
    }
    None
}

/// `engine.has_claude_token()`: true when some configured Claude dir holds a token (does not
/// call the API).
pub fn has_claude_token(paths: &AppPaths, runner: &dyn CommandRunner) -> bool {
    has_claude_token_on(paths, runner, &Host::system())
}

/// [`has_claude_token`] for an explicit [`Host`].
pub fn has_claude_token_on(paths: &AppPaths, runner: &dyn CommandRunner, host: &Host) -> bool {
    read_config_dirs(paths)
        .iter()
        .any(|dir| read_token_for_on(paths, runner, dir, host).is_some())
}
