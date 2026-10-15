//! Settings commands: the setup wizard (`settings_api.py`). STUB owned by phase 13.
//! Each takes the positional JS `args` and returns the JSON the Python method returned
//! (`{"ok": true, ...}` or `{"ok": false, "code", "error"}`); `Err` rejects the promise.
#![allow(unused_variables)]

use serde_json::Value;

use super::CommandCtx;

/// `SettingsApi.setup_state`.
pub async fn setup_state(ctx: &CommandCtx<'_>, args: Vec<Value>) -> Result<Value, String> {
    todo!("phase 13")
}

/// `SettingsApi.finish_setup`.
pub async fn finish_setup(ctx: &CommandCtx<'_>, args: Vec<Value>) -> Result<Value, String> {
    todo!("phase 13")
}

/// `SettingsApi.setup_clocks`.
pub async fn setup_clocks(ctx: &CommandCtx<'_>, args: Vec<Value>) -> Result<Value, String> {
    todo!("phase 13")
}

/// `SettingsApi.setup_use_clock`.
pub async fn setup_use_clock(ctx: &CommandCtx<'_>, args: Vec<Value>) -> Result<Value, String> {
    todo!("phase 13")
}
