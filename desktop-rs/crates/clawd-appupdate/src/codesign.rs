//! macOS bundle checks and the swap script (`appupdate.py:285-350`). `codesign` runs through
//! `clawd_config::CommandRunner` (the same trait the token readers use) so tests fake it on
//! every OS.

use std::path::Path;

use clawd_config::CommandRunner;
use clawd_contracts::UpdateError;

/// `appupdate.NO_TEAM`: the Vietnamese message for an ad-hoc signed app (no Team ID to match).
pub const NO_TEAM: &str =
    "Bản này chỉ có chữ ký ad-hoc (không có Team ID) nên không tự cập nhật được. \
Hãy cài bản chính thức một lần, từ đó app sẽ tự cập nhật.";

/// `appupdate.MAC_SWAP`: the text of `swap.sh` (waits for the old process, swaps the bundle
/// keeping the old one until the new one is in place, writes `failed` on failure, opens the
/// app). Byte-identical to Python's.
pub fn mac_swap_script() -> &'static str {
    todo!("phase 10")
}

/// `appupdate.bundle_team(app, run)`: Team ID from `codesign -dv --verbose=2`; `None` ad-hoc.
pub fn bundle_team(app: &Path, runner: &dyn CommandRunner) -> Result<Option<String>, UpdateError> {
    todo!("phase 10")
}

/// `appupdate.check_bundle(new, version, running, run)`: the unpacked app is the version
/// announced (`CFBundleIdentifier == vn.clawd.desktop`, `CFBundleShortVersionString`), validly
/// signed (`codesign --verify --deep --strict`) and by the **same Team ID** as the running one;
/// no Team ID on the running app is [`NO_TEAM`].
pub fn check_bundle(
    new: &Path,
    version: &str,
    running: Option<&Path>,
    runner: &dyn CommandRunner,
) -> Result<(), UpdateError> {
    todo!("phase 10")
}
