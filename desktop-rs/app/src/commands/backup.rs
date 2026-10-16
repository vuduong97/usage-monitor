//! Settings commands: backup of the clock's settings to a file and back (`settings_api.py`).
//! Each takes the positional JS `args` and returns the JSON the Python method returned
//! (`{"ok": true, ...}` or `{"ok": false, "code", "error"}`); `Err` rejects the promise.

use std::future::Future;
use std::path::{Path, PathBuf};

use clawd_contracts::WINDOW_SETTINGS;
use serde_json::{json, Value};
use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::DialogExt;

use super::session::{err, fail, Bridge};
use super::CommandCtx;

/// A backup of the clock's settings is a few KB; anything bigger is not one.
const BACKUP_MAX_BYTES: u64 = 1024 * 1024;

const NOT_A_BACKUP: &str = "File này không phải bản sao lưu của đồng hồ.";

/// The file dialog (`_dialog`): the picked path, `None` when cancelled. It blocks until the
/// user answers, so it runs off the async threads.
async fn pick_file(app: &AppHandle, save_as: Option<String>) -> Option<PathBuf> {
    let app = app.clone();
    tokio::task::spawn_blocking(move || {
        let mut dialog = app.dialog().file();
        if let Some(window) = app.get_webview_window(WINDOW_SETTINGS) {
            dialog = dialog.set_parent(&window);
        }
        let picked = match save_as {
            Some(name) => dialog
                .set_file_name(name)
                .add_filter("Bản sao lưu (*.json)", &["json"])
                .blocking_save_file(),
            None => dialog
                .add_filter("Bản sao lưu (*.json)", &["json"])
                .blocking_pick_file(),
        };
        picked.and_then(|p| p.into_path().ok())
    })
    .await
    .unwrap_or_else(|e| {
        // Typically the dialog plugin is not registered (`.plugin(tauri_plugin_dialog::init())`).
        tracing::warn!("[settings] file dialog failed: {e}");
        None
    })
}

fn cancelled() -> Value {
    json!({"ok": false, "code": "cancel", "error": ""})
}

/// `<hostname>-YYYYMMDD.json`, the date in UTC.
fn backup_name(hostname: &str, epoch_secs: f64) -> String {
    let days = (epoch_secs / 86_400.0).floor() as i64;
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    let host = if hostname.is_empty() {
        "clawd"
    } else {
        hostname
    };
    format!("{host}-{year:04}{month:02}{day:02}.json")
}

/// `SettingsApi.export_backup`: the clock's settings to a file the user picks.
pub async fn export_backup(ctx: &CommandCtx<'_>, _args: Vec<Value>) -> Result<Value, String> {
    let app = ctx.app.clone();
    export_with(&Bridge::of(ctx), |name| async move {
        pick_file(&app, Some(name)).await
    })
    .await
}

async fn export_with<F, Fut>(b: &Bridge, pick: F) -> Result<Value, String>
where
    F: FnOnce(String) -> Fut,
    Fut: Future<Output = Option<PathBuf>>,
{
    let data = match b.api().export().await {
        Ok(d) => d,
        Err(e) => return Ok(err(&e)),
    };
    let hostname = b
        .data()
        .cfg
        .get("hostname")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let Some(path) = pick(backup_name(&hostname, b.now())).await else {
        return Ok(cancelled());
    };
    let text = serde_json::to_string_pretty(&data).map_err(|e| e.to_string())?;
    if let Err(e) = std::fs::write(&path, text) {
        return Ok(fail("error", &format!("Lỗi: {e}")));
    }
    Ok(json!({"ok": true, "path": path.display().to_string()}))
}

/// `SettingsApi.import_backup`: restore the clock's settings from a file the user picks.
pub async fn import_backup(ctx: &CommandCtx<'_>, _args: Vec<Value>) -> Result<Value, String> {
    let app = ctx.app.clone();
    import_with(&Bridge::of(ctx), async move { pick_file(&app, None).await }).await
}

async fn import_with(
    b: &Bridge,
    pick: impl Future<Output = Option<PathBuf>>,
) -> Result<Value, String> {
    let Some(path) = pick.await else {
        return Ok(cancelled());
    };
    let Some(data) = read_backup(&path) else {
        return Ok(fail("bad", NOT_A_BACKUP));
    };
    Ok(match b.api().restore(&data).await {
        Ok(()) => json!({"ok": true}),
        Err(e) => err(&e),
    })
}

/// A backup is a JSON object with a `hostname`.
fn read_backup(path: &Path) -> Option<Value> {
    if std::fs::metadata(path).ok()?.len() > BACKUP_MAX_BYTES {
        return None;
    }
    let data: Value = serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()?;
    data.get("hostname").is_some().then_some(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_backup_is_named_after_the_clock_and_the_day() {
        assert_eq!(backup_name("clawd", 0.0), "clawd-19700101.json");
        assert_eq!(backup_name("", 1_700_000_000.0), "clawd-20231114.json");
        assert_eq!(backup_name("desk", 1_709_164_800.0), "desk-20240229.json");
    }

    #[test]
    fn only_a_json_object_with_a_hostname_is_a_backup() {
        let dir = std::env::temp_dir().join(format!("clawd-backup-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let write = |name: &str, text: &str| {
            let p = dir.join(name);
            std::fs::write(&p, text).unwrap();
            p
        };
        assert!(read_backup(&write(
            "ok.json",
            r#"{"hostname": "clawd", "brightness": 5}"#
        ))
        .is_some());
        assert!(read_backup(&write("nohost.json", r#"{"brightness": 5}"#)).is_none());
        assert!(read_backup(&write("list.json", "[1, 2]")).is_none());
        assert!(read_backup(&write("bad.json", "not json")).is_none());
        assert!(read_backup(&dir.join("missing.json")).is_none());
        let huge = format!(
            r#"{{"hostname": "{}"}}"#,
            "x".repeat(BACKUP_MAX_BYTES as usize)
        );
        assert!(read_backup(&write("huge.json", &huge)).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
