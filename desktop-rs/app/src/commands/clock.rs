//! Settings commands: the clock's own settings (`settings_api.py`). STUB owned by phase 13.
//! Each takes the positional JS `args` and returns the JSON the Python method returned
//! (`{"ok": true, ...}` or `{"ok": false, "code", "error"}`); `Err` rejects the promise.
#![allow(unused_variables)]

use serde_json::Value;

use super::CommandCtx;

/// `SettingsApi.login`.
pub async fn login(ctx: &CommandCtx<'_>, args: Vec<Value>) -> Result<Value, String> {
    todo!("phase 13")
}

/// `SettingsApi.scan`.
pub async fn scan(ctx: &CommandCtx<'_>, args: Vec<Value>) -> Result<Value, String> {
    todo!("phase 13")
}

/// `SettingsApi.set_clock`.
pub async fn set_clock(ctx: &CommandCtx<'_>, args: Vec<Value>) -> Result<Value, String> {
    todo!("phase 13")
}

/// `SettingsApi.reboot`.
pub async fn reboot(ctx: &CommandCtx<'_>, args: Vec<Value>) -> Result<Value, String> {
    todo!("phase 13")
}

/// `SettingsApi.factory_reset`.
pub async fn factory_reset(ctx: &CommandCtx<'_>, args: Vec<Value>) -> Result<Value, String> {
    todo!("phase 13")
}

/// `SettingsApi.save`.
pub async fn save(ctx: &CommandCtx<'_>, args: Vec<Value>) -> Result<Value, String> {
    todo!("phase 13")
}
