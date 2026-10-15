//! Pure helpers (`appupdate.py:82-130`), all tested by golden fixtures.

use std::collections::HashMap;
use std::io;
use std::path::Path;

/// `appupdate.parse_version(s)`: `v1.2.3` / `1.2.3` -> `(1, 2, 3)` (each part 1-4 digits).
/// Anything else (`1.2`, `1.2.3-rc1`, ...) is not a release.
pub fn parse_version(s: &str) -> Option<(u32, u32, u32)> {
    todo!("phase 10")
}

/// `appupdate.is_newer(candidate, current)`: both parse and `candidate > current`.
pub fn is_newer(candidate: &str, current: &str) -> bool {
    todo!("phase 10")
}

/// `appupdate.this_platform()`: `("mac" | "windows" | "linux", "arm64" | "x86_64" | ...)`.
pub fn this_platform() -> (String, String) {
    todo!("phase 10")
}

/// `appupdate.asset_name(version, system, arch)`: the release file for this platform
/// (`Clawd-X.Y.Z-mac-<arch>.zip`, `Clawd-X.Y.Z-windows-setup.exe`); `None` where there is no
/// self-update.
pub fn asset_name(version: &str, system: &str, arch: &str) -> Option<String> {
    todo!("phase 10")
}

/// `appupdate.parse_sums(text)`: `sha256sum` output (`<64 hex>  <name>`, `*` before the name
/// marks binary mode) -> `{name: lowercase hex}`.
pub fn parse_sums(text: &str) -> HashMap<String, String> {
    todo!("phase 10")
}

/// `appupdate.team_id(codesign_output)`: the Team ID in `codesign -dv --verbose=2` output;
/// `None` for an ad-hoc signature.
pub fn team_id(codesign_output: &str) -> Option<String> {
    todo!("phase 10")
}

/// `appupdate.sha256_file(path)`: streaming SHA-256, lowercase hex.
pub fn sha256_file(path: &Path) -> io::Result<String> {
    todo!("phase 10")
}
