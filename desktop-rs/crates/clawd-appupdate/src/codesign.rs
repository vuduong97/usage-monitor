//! macOS bundle checks and the swap script (`appupdate.py:285-350`). `codesign` runs through
//! `clawd_config::CommandRunner` (the same trait the token readers use) so tests fake it on
//! every OS.

use std::path::Path;
use std::time::Duration;

use clawd_config::{CommandOutput, CommandRunner};
use clawd_contracts::{UpdateError, BUNDLE_ID};

use crate::version::team_id;

/// `appupdate.NO_TEAM`: the Vietnamese message for an ad-hoc signed app (no Team ID to match).
pub const NO_TEAM: &str =
    "Bản này chỉ có chữ ký ad-hoc (không có Team ID) nên không tự cập nhật được. \
Hãy cài bản chính thức một lần, từ đó app sẽ tự cập nhật.";

/// `timeout=120` of every `codesign` call.
const CODESIGN_TIMEOUT: Duration = Duration::from_secs(120);

/// `appupdate.MAC_SWAP`: the text of `swap.sh` (waits for the old process, swaps the bundle
/// keeping the old one until the new one is in place, writes `failed` on failure, opens the
/// app). Byte-identical to Python's.
pub fn mac_swap_script() -> &'static str {
    r##"#!/bin/sh
# Written by clawd_desktop/appupdate.py. Waits for Clawd to quit, swaps the bundle (the old
# one stays until the new one is in place) and opens it again.
PID="$1"; NEW="$2"; APP="$3"; STAGE="$4"; VER="$5"
log() { printf '%s %s\n' "$(date '+%H:%M:%S')" "$*" >> "$STAGE/apply.log"; }
[ -n "$APP" ] && [ -d "$NEW" ] || { log "bad arguments"; exit 2; }
i=0
while kill -0 "$PID" 2>/dev/null; do
  i=$((i + 1))
  if [ "$i" -gt 150 ]; then log "Clawd did not quit"; echo "$VER" > "$STAGE/failed"; exit 1; fi
  sleep 0.2
done
TMP="$APP.update-$$"; OLD="$APP.old-$$"
fail() { log "failed: $1"; echo "$VER" > "$STAGE/failed"; rm -rf "$TMP"; open "$APP"; exit 1; }
rm -rf "$TMP"
ditto "$NEW" "$TMP" || fail "copy"
mv "$APP" "$OLD" || fail "move the old app"
if ! mv "$TMP" "$APP"; then mv "$OLD" "$APP"; fail "move the new app"; fi
rm -rf "$OLD" "$NEW" "$STAGE/ready.json"
log "updated to $VER"
open "$APP"
"##
}

/// `appupdate._codesign(args, run)`.
fn codesign(args: &[&str], runner: &dyn CommandRunner) -> Result<CommandOutput, UpdateError> {
    runner
        .run("codesign", args, CODESIGN_TIMEOUT)
        .map_err(|e| UpdateError(format!("codesign: {e}")))
}

/// `appupdate.bundle_team(app, run)`: Team ID from `codesign -dv --verbose=2`; `None` ad-hoc.
pub fn bundle_team(app: &Path, runner: &dyn CommandRunner) -> Result<Option<String>, UpdateError> {
    let app = app.to_string_lossy();
    let out = codesign(&["-dv", "--verbose=2", &app], runner)?;
    Ok(team_id(&format!("{}{}", out.stderr, out.stdout)))
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
    let info = plist::Value::from_file(new.join("Contents").join("Info.plist"))
        .map_err(|e| UpdateError(format!("Info.plist: {e}")))?;
    let field = |key: &str| {
        info.as_dictionary()
            .and_then(|d| d.get(key))
            .and_then(plist::Value::as_string)
            .map(str::to_owned)
    };
    if field("CFBundleIdentifier").as_deref() != Some(BUNDLE_ID) {
        return Err(UpdateError("không phải app Clawd".to_owned()));
    }
    let found = field("CFBundleShortVersionString");
    if found.as_deref() != Some(version) {
        return Err(UpdateError(format!(
            "app bên trong là {}, không phải {version}",
            found.as_deref().unwrap_or("None")
        )));
    }
    let new_path = new.to_string_lossy();
    let verified = codesign(&["--verify", "--deep", "--strict", &new_path], runner)?;
    if verified.status != 0 {
        return Err(UpdateError("chữ ký của bản mới không hợp lệ".to_owned()));
    }
    let want = match running {
        Some(running) => bundle_team(running, runner)?,
        None => None,
    };
    let Some(want) = want else {
        return Err(UpdateError(NO_TEAM.to_owned())); // nothing to match the new signature against
    };
    if bundle_team(new, runner)?.as_deref() != Some(want.as_str()) {
        return Err(UpdateError(
            "bản mới không cùng Team ID với app đang chạy".to_owned(),
        ));
    }
    Ok(())
}
