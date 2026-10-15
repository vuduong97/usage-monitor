//! Settings commands: usage source and preview (`settings_api.py`). STUB owned by phase 13.
//! Each takes the positional JS `args` and returns the JSON the Python method returned
//! (`{"ok": true, ...}` or `{"ok": false, "code", "error"}`); `Err` rejects the promise.
#![allow(unused_variables)]

use serde_json::Value;

use super::CommandCtx;

/// `SettingsApi.usage_preview`.
pub async fn usage_preview(ctx: &CommandCtx<'_>, args: Vec<Value>) -> Result<Value, String> {
    todo!("phase 13")
}

/// `SettingsApi.usage_sources`.
pub async fn usage_sources(ctx: &CommandCtx<'_>, args: Vec<Value>) -> Result<Value, String> {
    todo!("phase 13")
}

/// `SettingsApi.set_usage_source`.
pub async fn set_usage_source(ctx: &CommandCtx<'_>, args: Vec<Value>) -> Result<Value, String> {
    todo!("phase 13")
}

/// `SettingsApi.clawd_pak`.
pub async fn clawd_pak(ctx: &CommandCtx<'_>, args: Vec<Value>) -> Result<Value, String> {
    todo!("phase 13")
}
