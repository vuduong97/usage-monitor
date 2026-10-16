#![allow(unused_variables)] // TEMP: stub until its milestone lands
use clawd_contracts::AppPaths;

/// `usage.normalize_http_target(raw)`: an `http_targets` entry as a push URL, or `None` if
/// unusable. A bare host or IP (`192.168.0.125`, `clawd.local:80`) becomes
/// `http://<host>/api/usage`; a full `http(s)` URL keeps its own path.
pub fn normalize_http_target(raw: &str) -> Option<String> {
    todo!("phase 06")
}

/// `usage.read_http_targets()`: push URLs from the comma-separated `http_targets` option, in
/// order, deduplicated; empty = WiFi push by config off (the finder's mDNS pick is used).
pub fn read_http_targets(paths: &AppPaths) -> Vec<String> {
    todo!("phase 06")
}
