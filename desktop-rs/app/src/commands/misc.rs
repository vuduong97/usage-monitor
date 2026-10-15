//! Settings commands: small actions (`settings_api.py`). STUB owned by phase 13.
//! Each takes the positional JS `args` and returns the JSON the Python method returned
//! (`{"ok": true, ...}` or `{"ok": false, "code", "error"}`); `Err` rejects the promise.
#![allow(unused_variables)]

use serde_json::Value;

use super::CommandCtx;

/// `SettingsApi.set_autostart`.
pub async fn set_autostart(ctx: &CommandCtx<'_>, args: Vec<Value>) -> Result<Value, String> {
    todo!("phase 13")
}

/// `SettingsApi.open_log`.
pub async fn open_log(ctx: &CommandCtx<'_>, args: Vec<Value>) -> Result<Value, String> {
    todo!("phase 13")
}

/// `SettingsApi.open_web`.
pub async fn open_web(ctx: &CommandCtx<'_>, args: Vec<Value>) -> Result<Value, String> {
    todo!("phase 13")
}
