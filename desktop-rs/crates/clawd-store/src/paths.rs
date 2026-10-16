//! `paths.py` (the parts that are not prefs): packaged-app test, `ota.busy`, the usage payload
//! file. The path constants live on [`clawd_contracts::AppPaths`].

use std::io;
use std::time::SystemTime;

use clawd_contracts::{AppPaths, UsagePayload, OTA_BUSY_STALE};
use serde_json::{json, Map, Value};

use crate::prefs::{replace_file, thread_tag};
use crate::pyjson::{dumps, Style};

/// `paths.frozen()`: true for an installed, packaged app (not `cargo run` from `target/`).
pub fn is_packaged() -> bool {
    match std::env::current_exe() {
        Ok(exe) => !is_cargo_output(&exe),
        Err(_) => false,
    }
}

/// An exe inside a cargo target dir: one named `target`, or any dir cargo tagged with
/// `CACHEDIR.TAG` (covers a custom `CARGO_TARGET_DIR`).
fn is_cargo_output(exe: &std::path::Path) -> bool {
    exe.ancestors().skip(1).any(|dir| {
        dir.file_name()
            .is_some_and(|name| name.eq_ignore_ascii_case("target"))
            || dir.join("CACHEDIR.TAG").is_file()
    })
}

fn epoch_now() -> f64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64())
}

/// `paths.set_ota_busy(on)`: mark (or touch, or clear) a firmware install in progress, so an
/// app update never restarts the app in the middle of an upload to the clock. Best effort:
/// I/O errors are swallowed like Python. Set by the settings job (phase 13) only.
pub fn set_ota_busy(paths: &AppPaths, on: bool) {
    let path = paths.ota_busy_file();
    let _ = if on {
        std::fs::create_dir_all(paths.config_dir())
            .and_then(|()| std::fs::write(&path, crate::pyjson::py_float_repr(epoch_now())))
    } else {
        match std::fs::remove_file(&path) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            other => other,
        }
    };
}

/// `paths.ota_busy(path, now)`: true while `ota.busy` exists and was touched less than
/// [`clawd_contracts::OTA_BUSY_STALE`] (900 s) before `now` (epoch seconds; `None` = the real
/// time). Only the app updater reads this; the engine reads [`crate::pause`] instead.
pub fn ota_busy(paths: &AppPaths, now: Option<f64>) -> bool {
    let Ok(modified) = std::fs::metadata(paths.ota_busy_file()).and_then(|m| m.modified()) else {
        return false;
    };
    let mtime = match modified.duration_since(SystemTime::UNIX_EPOCH) {
        Ok(d) => d.as_secs_f64(),
        Err(e) => -e.duration().as_secs_f64(),
    };
    now.unwrap_or_else(epoch_now) - mtime < OTA_BUSY_STALE
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
    let file = paths.payload_file();
    let data = json!({"at": now.unwrap_or_else(epoch_now), "payload": payload.persisted().0});
    std::fs::create_dir_all(paths.config_dir())?;
    // Python: `path.with_suffix(".tmp")`; a pid/thread tag keeps two writers apart
    let tmp = file.with_file_name(format!(
        "usage_payload.{}.{}.tmp",
        std::process::id(),
        thread_tag()
    ));
    let result = dumps(&data, Style::Default)
        .and_then(|text| std::fs::write(&tmp, text))
        .and_then(|()| replace_file(&tmp, &file));
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

/// `paths.usage_payload()`: `(payload, time it was sent)`, or `None` when the engine has not sent
/// one yet or the file is unusable.
pub fn usage_payload(paths: &AppPaths) -> Option<(UsagePayload, f64)> {
    let text = std::fs::read_to_string(paths.payload_file()).ok()?;
    let Value::Object(mut data) = serde_json::from_str::<Value>(&text).ok()? else {
        return None;
    };
    let Value::Object(payload) = data.remove("payload")? else {
        return None;
    };
    let at = data.remove("at")?.as_f64()?;
    Some((UsagePayload(Map::from_iter(payload)), at))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn temp_paths() -> (tempfile::TempDir, AppPaths) {
        let dir = tempfile::tempdir().expect("temp dir");
        let paths = AppPaths::new(dir.path().join("home"));
        (dir, paths)
    }

    fn payload(v: Value) -> UsagePayload {
        serde_json::from_value(v).expect("payload")
    }

    #[test]
    fn test_binary_run_from_target_is_not_packaged() {
        assert!(!is_packaged());
    }

    #[test]
    fn installed_locations_are_packaged() {
        let (dir, _paths) = temp_paths();
        let exe = dir.path().join("Programs").join("Clawd").join("Clawd.exe");
        assert!(!is_cargo_output(&exe));
        assert!(is_cargo_output(
            &dir.path().join("target").join("debug").join("x.exe")
        ));
        std::fs::create_dir_all(dir.path().join("ct")).expect("mkdir");
        std::fs::write(dir.path().join("ct").join("CACHEDIR.TAG"), "").expect("tag");
        assert!(is_cargo_output(
            &dir.path().join("ct").join("debug").join("x.exe")
        ));
    }

    #[test]
    fn ota_busy_follows_the_file_and_goes_stale_after_900_s() {
        let (_dir, paths) = temp_paths();
        assert!(!ota_busy(&paths, None));
        set_ota_busy(&paths, true);
        assert!(ota_busy(&paths, None));
        let now = epoch_now();
        assert!(ota_busy(&paths, Some(now + 899.0)));
        assert!(!ota_busy(&paths, Some(now + 901.0)));
        set_ota_busy(&paths, false);
        assert!(!ota_busy(&paths, None));
        set_ota_busy(&paths, false); // clearing twice is fine
    }

    #[test]
    fn payload_round_trips_only_the_persisted_keys() {
        let (_dir, paths) = temp_paths();
        assert_eq!(usage_payload(&paths), None);
        let p = payload(json!({"s": 5, "m": "alert", "b": 1, "w": 9, "ok": true}));
        set_usage_payload(&paths, &p, Some(1760000000.5)).expect("write");
        assert_eq!(
            std::fs::read_to_string(paths.payload_file()).expect("read"),
            r#"{"at": 1760000000.5, "payload": {"s": 5, "w": 9, "ok": true}}"#
        );
        let (got, at) = usage_payload(&paths).expect("payload");
        assert_eq!(at, 1760000000.5);
        assert_eq!(got, payload(json!({"s": 5, "w": 9, "ok": true})));
    }

    #[test]
    fn unusable_payload_files_read_as_none() {
        let (_dir, paths) = temp_paths();
        std::fs::create_dir_all(paths.config_dir()).expect("mkdir");
        for text in [
            "{bad",
            "[]",
            r#"{"payload": [], "at": 1}"#,
            r#"{"payload": {}, "at": true}"#,
            r#"{"payload": {}, "at": "1"}"#,
            r#"{"payload": {}}"#,
        ] {
            std::fs::write(paths.payload_file(), text).expect("write");
            assert_eq!(usage_payload(&paths), None, "{text}");
        }
    }

    #[test]
    fn stale_mtime_older_than_now_minus_900() {
        let (_dir, paths) = temp_paths();
        set_ota_busy(&paths, true);
        let past = SystemTime::now() - Duration::from_secs(1000);
        std::fs::File::options()
            .write(true)
            .open(paths.ota_busy_file())
            .and_then(|f| f.set_modified(past))
            .expect("set mtime");
        assert!(!ota_busy(&paths, None));
    }
}
