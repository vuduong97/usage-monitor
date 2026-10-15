//! Settings commands: app self-update (`settings_api.py`). STUB owned by phase 13.
//! Each takes the positional JS `args` and returns the JSON the Python method returned
//! (`{"ok": true, ...}` or `{"ok": false, "code", "error"}`); `Err` rejects the promise.
#![allow(unused_variables)]

use serde_json::Value;

use super::CommandCtx;

/// `SettingsApi.app_update`.
pub async fn app_update(ctx: &CommandCtx<'_>, args: Vec<Value>) -> Result<Value, String> {
    todo!("phase 13")
}

/// `SettingsApi.app_update_check`.
pub async fn app_update_check(ctx: &CommandCtx<'_>, args: Vec<Value>) -> Result<Value, String> {
    todo!("phase 13")
}

/// `SettingsApi.app_update_apply`.
pub async fn app_update_apply(ctx: &CommandCtx<'_>, args: Vec<Value>) -> Result<Value, String> {
    todo!("phase 13")
}

/// `SettingsApi.app_update_page`.
pub async fn app_update_page(ctx: &CommandCtx<'_>, args: Vec<Value>) -> Result<Value, String> {
    todo!("phase 13")
}
