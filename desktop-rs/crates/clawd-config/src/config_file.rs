//! The optional `~/.config/claude-usage-monitor/config` file: `key = value` lines, `#`
//! comments, keys case-insensitive (`usage.read_config_value` and friends).

use clawd_contracts::AppPaths;

/// `usage.read_config_value(name)`: the raw value of `name = value` (comment stripped, trimmed;
/// `name` is matched against the lower-cased key, so pass it lower-case), or `None`. The last
/// matching line wins, like Python's loop.
pub fn read_config_value(paths: &AppPaths, name: &str) -> Option<String> {
    todo!("phase 06")
}

/// `usage.read_onoff_setting(name, default)`: `"on"` or `"off"` (case-insensitive), else `default`.
pub fn read_onoff_setting(paths: &AppPaths, name: &str, default: &str) -> String {
    todo!("phase 06")
}

/// `usage.read_chime_setting()`: the `chime` option, `off|on` (default `off`).
pub fn read_chime_setting(paths: &AppPaths) -> String {
    todo!("phase 06")
}

/// `usage.read_clock_setting()`: the `clock` option, one of `off|auto|12|24` (default `auto`).
pub fn read_clock_setting(paths: &AppPaths) -> String {
    todo!("phase 06")
}

/// `usage.read_usage_source()`: which usage the clock shows, from `desktop.json`: `claude` (the
/// default), `claude:<config dir>`, `codex` or `off`; anything else reads as `claude`.
pub fn read_usage_source(paths: &AppPaths) -> String {
    todo!("phase 06")
}
