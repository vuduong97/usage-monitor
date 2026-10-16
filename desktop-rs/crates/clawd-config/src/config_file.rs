//! The optional `~/.config/claude-usage-monitor/config` file: `key = value` lines, `#`
//! comments, keys case-insensitive (`usage.read_config_value` and friends).

use clawd_contracts::AppPaths;
use serde_json::Value;

use crate::pytext::{splitlines, strip};

/// Every `key = value` line of the config file as `(lower-cased key, value)`, in file order;
/// empty when the file is missing or unreadable (Python: `CONFIG_FILE.exists()` + `OSError`).
pub(crate) fn config_entries(paths: &AppPaths) -> Vec<(String, String)> {
    let Ok(bytes) = std::fs::read(paths.config_file()) else {
        return Vec::new();
    };
    let text = String::from_utf8_lossy(&bytes);
    let mut entries = Vec::new();
    for line in splitlines(&text) {
        let line = strip(line.split('#').next().unwrap_or(""));
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        entries.push((strip(key).to_lowercase(), strip(value).to_owned()));
    }
    entries
}

/// `usage.read_config_value(name)`: the raw value of `name = value` (comment stripped, trimmed;
/// `name` is matched against the lower-cased key, so pass it lower-case), or `None`. The FIRST
/// matching line wins (Python returns from its loop at the first match).
pub fn read_config_value(paths: &AppPaths, name: &str) -> Option<String> {
    config_entries(paths)
        .into_iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value)
}

/// `usage.read_onoff_setting(name, default)`: `"on"` or `"off"` (case-insensitive), else `default`.
pub fn read_onoff_setting(paths: &AppPaths, name: &str, default: &str) -> String {
    let value = read_config_value(paths, name)
        .unwrap_or_default()
        .to_lowercase();
    if value == "on" || value == "off" {
        value
    } else {
        default.to_owned()
    }
}

/// `usage.read_chime_setting()`: the `chime` option, `off|on` (default `off`).
pub fn read_chime_setting(paths: &AppPaths) -> String {
    read_onoff_setting(paths, "chime", "off")
}

/// `usage.read_clock_setting()`: the `clock` option, one of `off|auto|12|24` (default `auto`).
/// The first `clock` line with a valid value wins; a line with an invalid value is skipped.
pub fn read_clock_setting(paths: &AppPaths) -> String {
    config_entries(paths)
        .into_iter()
        .filter(|(key, _)| key == "clock")
        .map(|(_, value)| value.to_lowercase())
        .find(|value| matches!(value.as_str(), "off" | "auto" | "12" | "24"))
        .unwrap_or_else(|| "auto".to_owned())
}

/// `usage.read_usage_source()`: which usage the clock shows, from `desktop.json`: `claude` (the
/// default), `claude:<config dir>`, `codex` or `off`; anything else reads as `claude`.
///
/// Reads `desktop.json` itself (read-only, like `paths.load_prefs()`) instead of going through
/// `clawd_store::prefs::load_prefs`, so this crate has no run-time dependency on the store phase.
pub fn read_usage_source(paths: &AppPaths) -> String {
    let source = std::fs::read(paths.prefs_file())
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .and_then(|prefs| match prefs.get("usage_source") {
            Some(Value::String(source)) => Some(source.clone()),
            _ => None,
        })
        .unwrap_or_default();
    let valid = matches!(source.as_str(), "claude" | "codex" | "off")
        || (source.starts_with("claude:") && source.chars().count() > "claude:".len());
    if valid {
        source
    } else {
        "claude".to_owned()
    }
}
