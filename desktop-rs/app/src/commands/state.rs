//! Settings commands: state (`settings_api.py`).
//! Each takes the positional JS `args` and returns the JSON the Python method returned
//! (`{"ok": true, ...}` or `{"ok": false, "code", "error"}`); `Err` rejects the promise.

use serde_json::Value;

use super::session::Bridge;
use super::CommandCtx;

/// `SettingsApi.state`: heap (no login), config, firmware and app facts. The tab the window
/// was opened on rides on the first answer.
pub async fn state(ctx: &CommandCtx<'_>, args: Vec<Value>) -> Result<Value, String> {
    state_on(&Bridge::of(ctx), &args).await
}

/// `state` on a bridge (the command without the Tauri context).
pub async fn state_on(b: &Bridge, _args: &[Value]) -> Result<Value, String> {
    Ok(b.clone().state().await)
}
