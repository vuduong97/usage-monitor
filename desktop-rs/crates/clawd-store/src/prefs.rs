//! `desktop.json` (`paths.py:42-155`): written by two processes in Python (tray and settings
//! window), so every read-modify-write runs under the `desktop.lock` file lock. A file being
//! replaced answers a transient permission error on Windows: that is waited out, never read as
//! empty. Prefs is a map, not a struct: unknown and wrongly typed keys survive (red team #11).

use std::cell::Cell;
use std::io;
use std::path::Path;
use std::time::{Duration, Instant};

use clawd_contracts::{AppPaths, Prefs, PREFS_IO_WAIT};
use serde_json::Value;

use crate::lock::prefs_lock;
use crate::pyjson::{dumps, Style};

/// Python's `time.sleep(0.02)` between retries.
const RETRY_SLEEP: Duration = Duration::from_millis(20);

/// Python's `PermissionError`: on Windows also a sharing (32) or lock (33) violation, which
/// Python maps to the same exception.
fn is_permission_error(e: &io::Error) -> bool {
    e.kind() == io::ErrorKind::PermissionDenied
        || (cfg!(windows) && matches!(e.raw_os_error(), Some(5 | 32 | 33)))
}

/// `paths._retrying`: repeat `op` while it answers a permission error, until the deadline.
pub(crate) fn retrying<T>(mut op: impl FnMut() -> io::Result<T>) -> io::Result<T> {
    let deadline = Instant::now() + Duration::from_secs_f64(PREFS_IO_WAIT);
    loop {
        match op() {
            Err(e) if is_permission_error(&e) && Instant::now() < deadline => {
                std::thread::sleep(RETRY_SLEEP);
            }
            other => return other,
        }
    }
}

/// `paths._read_prefs()`: `desktop.json`, `{}` when missing, corrupt or not an object. Errors
/// when it is there but stays unreadable: a caller that writes must not take that for empty.
fn read_prefs(paths: &AppPaths) -> io::Result<Prefs> {
    let file = paths.prefs_file();
    let bytes = match retrying(|| std::fs::read(&file)) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Prefs::new()),
        Err(e) => return Err(e),
    };
    let Ok(text) = String::from_utf8(bytes) else {
        return Ok(Prefs::new());
    };
    Ok(match serde_json::from_str::<Value>(&text) {
        Ok(Value::Object(map)) => Prefs(map),
        _ => Prefs::new(),
    })
}

thread_local! {
    static THREAD_TAG: Cell<u64> = const { Cell::new(0) };
}

/// Stands in for `threading.get_ident()` in the temp file name: unique per thread of a process.
pub(crate) fn thread_tag() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);
    THREAD_TAG.with(|tag| {
        if tag.get() == 0 {
            tag.set(NEXT.fetch_add(1, Ordering::Relaxed));
        }
        tag.get()
    })
}

/// `paths._write_prefs(data)`: `json.dumps(data, indent=2)` to a temp file of our own, then
/// replace (retrying while the target is busy). No trailing newline, like Python.
fn write_prefs(paths: &AppPaths, data: &Prefs) -> io::Result<()> {
    let file = paths.prefs_file();
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = file.with_file_name(format!(
        "desktop.{}.{}.tmp",
        std::process::id(),
        thread_tag()
    ));
    let text = dumps(&data.0, Style::Indent2)?;
    let result = std::fs::write(&tmp, text).and_then(|()| replace_file(&tmp, &file));
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

/// `Path.replace` with the retry of `paths._retrying`.
pub(crate) fn replace_file(from: &Path, to: &Path) -> io::Result<()> {
    retrying(|| std::fs::rename(from, to))
}

/// `paths.load_prefs()`: for reading only (empty when missing, corrupt, not an object, or
/// unreadable); changes go through [`update_prefs`].
pub fn load_prefs(paths: &AppPaths) -> Prefs {
    read_prefs(paths).unwrap_or_default()
}

/// `paths.update_prefs(change)`: under the lock, read `desktop.json` (`{}` when missing or not a
/// JSON object), let `change` edit it in place, write it back (pretty, 2-space indent, atomic)
/// when anything changed or the file did not exist, and return the result. Errors, writing
/// nothing, when the file exists but cannot be read, or the lock is not taken within
/// [`clawd_contracts::PREFS_IO_WAIT`].
pub fn update_prefs<F: FnOnce(&mut Prefs)>(paths: &AppPaths, change: F) -> io::Result<Prefs> {
    let _lock = prefs_lock(paths)?;
    let mut prefs = read_prefs(paths)?;
    let before = prefs.clone();
    change(&mut prefs);
    if prefs != before || !paths.prefs_file().exists() {
        write_prefs(paths, &prefs)?;
    }
    Ok(prefs)
}

/// `paths.save_prefs(data)`: replace all of `desktop.json` with `data` (use [`update_prefs`] to
/// change some keys).
pub fn save_prefs(paths: &AppPaths, data: &Prefs) -> io::Result<()> {
    let _lock = prefs_lock(paths)?;
    write_prefs(paths, data)
}

/// `paths.clock_screen()`: the clock's "Màn hình chính" as the settings window last saw it, or
/// `None`. The engine reads it (`Brain` stops pushing animations while it is
/// [`clawd_contracts::SCREEN_USAGE`]).
pub fn clock_screen(paths: &AppPaths) -> Option<i64> {
    load_prefs(paths).clock_screen()
}

/// `paths.set_clock_screen(screen)`: any integer is stored (a bool is not an integer).
pub fn set_clock_screen(paths: &AppPaths, screen: i64) -> io::Result<()> {
    update_prefs(paths, |prefs| {
        prefs.insert("clock_screen".to_owned(), Value::from(screen));
    })
    .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn temp_paths() -> (tempfile::TempDir, AppPaths) {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(dir.path().join("home"));
        (dir, paths)
    }

    fn write_raw(paths: &AppPaths, text: &str) {
        std::fs::create_dir_all(paths.config_dir()).unwrap();
        std::fs::write(paths.prefs_file(), text).unwrap();
    }

    #[test]
    fn missing_file_reads_empty_and_update_creates_it() {
        let (_dir, paths) = temp_paths();
        assert!(load_prefs(&paths).is_empty());
        let out = update_prefs(&paths, |_| {}).unwrap();
        assert!(out.is_empty());
        assert_eq!(std::fs::read_to_string(paths.prefs_file()).unwrap(), "{}");
    }

    #[test]
    fn corrupt_or_non_object_reads_empty() {
        let (_dir, paths) = temp_paths();
        for text in ["{not json", "[1, 2]", "\"x\"", "", "\u{feff}{}"] {
            write_raw(&paths, text);
            assert!(load_prefs(&paths).is_empty(), "{text:?}");
        }
        std::fs::write(paths.prefs_file(), [0xff, 0xfe, 0x00]).unwrap();
        assert!(load_prefs(&paths).is_empty());
    }

    #[test]
    fn unreadable_file_is_an_error_not_an_empty_prefs() {
        let (_dir, paths) = temp_paths();
        // a directory where the file should be: reading it fails, which must not be taken for {}
        std::fs::create_dir_all(paths.prefs_file()).unwrap();
        assert!(read_prefs(&paths).is_err());
        assert!(update_prefs(&paths, |p| {
            p.insert("a".into(), json!(1));
        })
        .is_err());
        assert!(load_prefs(&paths).is_empty());
    }

    #[test]
    fn update_changes_one_key_and_keeps_the_rest_in_order() {
        let (_dir, paths) = temp_paths();
        write_raw(
            &paths,
            r#"{"setup_done": "clock", "usage": {"screen": 300}, "ratio": 1.0, "name": "\u0110", "clock_screen": 0}"#,
        );
        set_clock_screen(&paths, 1).unwrap();
        let text = std::fs::read_to_string(paths.prefs_file()).unwrap();
        assert_eq!(
            text,
            "{\n  \"setup_done\": \"clock\",\n  \"usage\": {\n    \"screen\": 300\n  },\n  \"ratio\": 1.0,\n  \"name\": \"\\u0110\",\n  \"clock_screen\": 1\n}"
        );
        assert_eq!(clock_screen(&paths), Some(1));
    }

    #[test]
    fn unchanged_update_does_not_rewrite() {
        let (_dir, paths) = temp_paths();
        write_raw(&paths, "{\"a\":1}");
        update_prefs(&paths, |p| {
            p.insert("a".into(), json!(1));
        })
        .unwrap();
        assert_eq!(
            std::fs::read_to_string(paths.prefs_file()).unwrap(),
            "{\"a\":1}"
        );
    }

    #[test]
    fn clock_screen_ignores_wrong_types() {
        let (_dir, paths) = temp_paths();
        for (text, want) in [
            (r#"{"clock_screen": 2}"#, Some(2)),
            (r#"{"clock_screen": true}"#, None),
            (r#"{"clock_screen": 1.0}"#, None),
            (r#"{"clock_screen": "1"}"#, None),
            (r#"{}"#, None),
        ] {
            write_raw(&paths, text);
            assert_eq!(clock_screen(&paths), want, "{text}");
        }
    }

    #[test]
    fn save_prefs_replaces_everything() {
        let (_dir, paths) = temp_paths();
        write_raw(&paths, r#"{"old": 1}"#);
        let mut data = Prefs::new();
        data.insert("new".into(), json!([1, "x"]));
        save_prefs(&paths, &data).unwrap();
        assert_eq!(load_prefs(&paths), data);
    }

    #[test]
    fn concurrent_updates_lose_nothing() {
        let (_dir, paths) = temp_paths();
        let threads: Vec<_> = (0..4)
            .map(|t| {
                let paths = paths.clone();
                std::thread::spawn(move || {
                    for i in 0..8 {
                        update_prefs(&paths, |p| {
                            p.insert(format!("k{t}_{i}"), json!(i));
                        })
                        .unwrap();
                    }
                })
            })
            .collect();
        for t in threads {
            t.join().unwrap();
        }
        assert_eq!(load_prefs(&paths).len(), 32);
        let leftovers = std::fs::read_dir(paths.config_dir())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
            .count();
        assert_eq!(leftovers, 0);
    }
}
