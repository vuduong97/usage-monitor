//! Settings commands: the app's own update (`settings_api.py`; the tray downloads too, they
//! share one folder).
//! Each takes the positional JS `args` and returns the JSON the Python method returned
//! (`{"ok": true, ...}` or `{"ok": false, "code", "error"}`); `Err` rejects the promise.

use clawd_contracts::UPDATE_REPO;
use serde_json::{json, Value};

use super::session::{fail, merged, Bridge};
use super::CommandCtx;
use crate::state::ShellCmd;

/// The updater's state as the JSON the page polls.
fn state_json(b: &Bridge) -> Value {
    serde_json::to_value(b.services.updater.state()).unwrap_or_else(|_| json!({}))
}

/// `SettingsApi.app_update`: the state of the app's update, for the page's polling.
pub async fn app_update(ctx: &CommandCtx<'_>, _args: Vec<Value>) -> Result<Value, String> {
    Ok(merged(json!({"ok": true}), state_json(&Bridge::of(ctx))))
}

/// `SettingsApi.app_update_check`: the Update tab's "Kiểm tra bản mới" for the app: ask GitHub
/// now, download in the background.
pub async fn app_update_check(ctx: &CommandCtx<'_>, _args: Vec<Value>) -> Result<Value, String> {
    let b = Bridge::of(ctx);
    b.services.updater.check_async(true);
    Ok(merged(
        merged(json!({"ok": true}), state_json(&b)),
        json!({"status": "checking"}),
    ))
}

/// `SettingsApi.app_update_apply`: restart into the downloaded version. The tray owns the
/// process that has to quit, so it does the swap (`ShellCmd::AppUpdateApply`); a firmware
/// install in progress is never interrupted.
pub async fn app_update_apply(ctx: &CommandCtx<'_>, _args: Vec<Value>) -> Result<Value, String> {
    let b = Bridge::of(ctx);
    if b.job_running() {
        return Ok(fail(
            "busy",
            "Đang cập nhật firmware cho đồng hồ; hãy đợi xong rồi khởi động lại.",
        ));
    }
    let st = b.services.updater.state();
    if st.status != "ready" {
        return Ok(fail("bad", "Chưa có bản nào được tải xong."));
    }
    if !st.can_apply {
        let why = if st.why.is_empty() {
            "Bản này không tự cập nhật được."
        } else {
            st.why.as_str()
        };
        return Ok(fail("bad", why));
    }
    if !b.notify(ShellCmd::AppUpdateApply) {
        return Ok(fail(
            "bad",
            "Mở lại Clawd từ biểu tượng trên thanh menu / khay hệ thống rồi thử lại.",
        ));
    }
    Ok(json!({"ok": true}))
}

/// The release page to open: the updater's own when it is on GitHub, else the latest release.
fn release_page(page: &str) -> String {
    let github = format!("https://github.com/{UPDATE_REPO}/");
    if page.starts_with(&github) {
        page.to_owned()
    } else {
        format!("{github}releases/latest")
    }
}

/// `SettingsApi.app_update_page`: open the release page (an install that cannot replace itself).
pub async fn app_update_page(ctx: &CommandCtx<'_>, _args: Vec<Value>) -> Result<Value, String> {
    let b = Bridge::of(ctx);
    clawd_store::open::open_path(&release_page(&b.services.updater.state().page));
    Ok(json!({"ok": true}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_github_release_page_is_opened() {
        let own = format!("https://github.com/{UPDATE_REPO}/releases/tag/v1.2.3");
        assert_eq!(release_page(&own), own);
        let latest = format!("https://github.com/{UPDATE_REPO}/releases/latest");
        assert_eq!(release_page(""), latest);
        assert_eq!(release_page("https://evil.example/x"), latest);
        assert_eq!(release_page("file:///c:/windows/system32/calc.exe"), latest);
    }
}
