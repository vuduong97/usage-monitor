//! Settings commands: autostart, log, clock web page (`settings_api.py`).
//! Each takes the positional JS `args` and returns the JSON the Python method returned
//! (`{"ok": true, ...}` or `{"ok": false, "code", "error"}`); `Err` rejects the promise.

use clawd_store::autostart::Autostart;
use serde_json::{json, Value};

use super::session::{arg, fail, truthy, Bridge};
use super::CommandCtx;
use crate::state::ShellCmd;

/// `SettingsApi.set_autostart(on)`.
pub async fn set_autostart(ctx: &CommandCtx<'_>, args: Vec<Value>) -> Result<Value, String> {
    let b = Bridge::of(ctx);
    let autostart = Autostart::new(b.paths(), b.services.names.clone());
    if let Err(e) = autostart.set_enabled(truthy(arg(&args, 0))) {
        return Ok(fail("error", &format!("Không đổi được: {e}")));
    }
    b.notify(ShellCmd::Refresh);
    Ok(json!({"ok": true, "app": b.app_info()}))
}

/// `SettingsApi.open_log`: the log file, with the OS.
pub async fn open_log(ctx: &CommandCtx<'_>, _args: Vec<Value>) -> Result<Value, String> {
    let b = Bridge::of(ctx);
    clawd_store::open::open_path(&b.paths().log_file().display().to_string());
    Ok(json!({"ok": true}))
}

/// `SettingsApi.open_web`: the clock's own web page. Only this window's clock address is ever
/// opened, never an address the page sends.
pub async fn open_web(ctx: &CommandCtx<'_>, _args: Vec<Value>) -> Result<Value, String> {
    let (base, _) = Bridge::of(ctx).address();
    if !base.starts_with("http://") {
        return Ok(fail("bad", "Chưa có địa chỉ đồng hồ."));
    }
    clawd_store::open::open_path(&format!("{base}/"));
    Ok(json!({"ok": true}))
}
