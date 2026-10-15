//! `paths.py` (the parts that are not prefs): packaged-app test, `ota.busy`, the usage payload
//! file. The path constants live on [`clawd_contracts::AppPaths`].

use std::io;

use clawd_contracts::{AppPaths, UsagePayload};

/// `paths.frozen()`: true for an installed, packaged app (not `cargo run` from `target/`).
pub fn is_packaged() -> bool {
    todo!("phase 03")
}

/// `paths.set_ota_busy(on)`: mark (or touch, or clear) a firmware install in progress, so an
/// app update never restarts the app in the middle of an upload to the clock. Best effort:
/// I/O errors are swallowed like Python. Set by the settings job (phase 13) only.
pub fn set_ota_busy(paths: &AppPaths, on: bool) {
    todo!("phase 03")
}

/// `paths.ota_busy(path, now)`: true while `ota.busy` exists and was touched less than
/// [`clawd_contracts::OTA_BUSY_STALE`] (900 s) before `now` (epoch seconds; `None` = the real
/// time). Only the app updater reads this; the engine reads [`crate::pause`] instead.
pub fn ota_busy(paths: &AppPaths, now: Option<f64>) -> bool {
    todo!("phase 03")
}

/// `paths.set_usage_payload(payload, now)`: keep the last usage payload the engine delivered
/// (only [`UsagePayload::persisted`] keys, plus `at`) for the settings window, which is another
/// process in Python and still reads this file. Atomic (temp file + replace). `now` = epoch
/// seconds, `None` = the real time.
pub fn set_usage_payload(
    paths: &AppPaths,
    payload: &UsagePayload,
    now: Option<f64>,
) -> io::Result<()> {
    todo!("phase 03")
}

/// `paths.usage_payload()`: `(payload, time it was sent)`, or `None` when the engine has not sent
/// one yet or the file is unusable.
pub fn usage_payload(paths: &AppPaths) -> Option<(UsagePayload, f64)> {
    todo!("phase 03")
}
