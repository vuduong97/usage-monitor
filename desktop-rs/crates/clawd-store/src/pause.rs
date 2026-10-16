//! `pause.py`: hold the engine's pushes while the settings window works on the clock.
//!
//! The ESP8266 serves one HTTP client at a time. During a firmware upload and the reboot after
//! it, the engine's usage/state pushes and restart probes would compete with the upload, then
//! fail and get the clock dropped from discovery. The firmware job writes `pause.json` with an
//! `until` time (wall clock); while it is in the future the engine's target list is empty. It is
//! a lease: the writer renews it while it waits and removes it when done, and a crashed
//! writer's pause simply runs out. At most [`clawd_contracts::MAX_LEASE`] (300 s) is honoured.
//!
//! This is NOT `ota.busy` ([`crate::paths::ota_busy`], 900 s, read by the app updater): the two
//! have different readers and lifetimes (red team #9).

use std::io;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use clawd_contracts::{AppPaths, Clock, SystemClock, MAX_LEASE};
use serde_json::{json, Value};

use crate::prefs::replace_file;
use crate::pyjson::{dumps, py_float_repr, Style};

/// `pause.hold(seconds, reason)`: write `{"until": now + min(seconds, MAX_LEASE), "reason",
/// "pid"}` atomically (temp file named with the pid, then replace). Called by
/// `clawd_firmware::ota` at the start and on every `wait_back` tick.
pub fn hold(paths: &AppPaths, seconds: f64, reason: &str) -> io::Result<()> {
    hold_at(paths, seconds, reason, SystemClock.now())
}

/// [`hold`] with an injected `now` (epoch seconds), for tests (`pause.hold(..., now=)`).
pub fn hold_at(paths: &AppPaths, seconds: f64, reason: &str, now: f64) -> io::Result<()> {
    let file = paths.pause_file();
    std::fs::create_dir_all(paths.config_dir())?;
    let pid = std::process::id();
    let tmp = file.with_file_name(format!("{}.{pid}.tmp", clawd_contracts::PAUSE_FILE_NAME));
    let body = json!({"until": now + seconds.min(MAX_LEASE), "reason": reason, "pid": pid});
    let result = dumps(&body, Style::Default)
        .and_then(|text| std::fs::write(&tmp, text))
        .and_then(|()| replace_file(&tmp, &file));
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

/// `pause.release()`: remove `pause.json`; a missing file is fine.
pub fn release(paths: &AppPaths) -> io::Result<()> {
    match std::fs::remove_file(paths.pause_file()) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

/// What identifies one version of `pause.json` (a rewrite is a new file: replace).
#[derive(Debug, Clone, PartialEq, Eq)]
struct Stamp {
    modified: Option<SystemTime>,
    len: u64,
    #[cfg(unix)]
    ino: u64,
}

impl Stamp {
    fn of(meta: &std::fs::Metadata) -> Self {
        Self {
            modified: meta.modified().ok(),
            len: meta.len(),
            #[cfg(unix)]
            ino: std::os::unix::fs::MetadataExt::ino(meta),
        }
    }
}

#[derive(Debug, Default)]
struct Cached {
    stamp: Option<Stamp>,
    until: f64,
    reason: String,
}

/// `pause.PauseReader`: for the engine. [`Self::reason`] is the active pause's reason, or `None`.
/// Re-reads the file only when its (mtime, size) stamp changes, since it is asked every second.
/// Shared between engine tasks, hence `&self`.
pub struct PauseReader {
    file: std::path::PathBuf,
    clock: Arc<dyn Clock>,
    cached: Mutex<Cached>,
}

/// Python's `str(value)` for the JSON `reason` (`str(d.get("reason") or "settings")`).
fn reason_text(value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) | Some(Value::Bool(false)) => "settings".to_owned(),
        Some(Value::String(s)) if s.is_empty() => "settings".to_owned(),
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => match n.as_f64() {
            Some(0.0) => "settings".to_owned(),
            Some(f) if n.is_f64() => py_float_repr(f),
            _ => n.to_string(),
        },
        Some(Value::Bool(true)) => "True".to_owned(),
        Some(other) => {
            if other.as_array().is_some_and(Vec::is_empty)
                || other.as_object().is_some_and(|o| o.is_empty())
            {
                "settings".to_owned()
            } else {
                other.to_string()
            }
        }
    }
}

/// Python's `float(d.get("until", 0))`; `None` where Python raises (`TypeError`/`ValueError`).
fn until_value(value: Option<&Value>) -> Option<f64> {
    match value {
        None => Some(0.0),
        Some(Value::Number(n)) => n.as_f64(),
        Some(Value::Bool(b)) => Some(f64::from(u8::from(*b))),
        Some(Value::String(s)) => s.trim().parse::<f64>().ok(),
        Some(_) => None,
    }
}

/// `(until, reason)` of a `pause.json` text; `(0, "")` when it is unusable.
fn parse_pause(text: &str) -> (f64, String) {
    let Ok(Value::Object(map)) = serde_json::from_str::<Value>(text) else {
        return (0.0, String::new());
    };
    match until_value(map.get("until")) {
        Some(until) => (until, reason_text(map.get("reason"))),
        None => (0.0, String::new()),
    }
}

impl PauseReader {
    /// Reader over `paths.pause_file()` using the system clock.
    pub fn new(paths: &AppPaths) -> Self {
        Self::with_clock(paths, Arc::new(SystemClock))
    }

    /// Reader with an injected clock (tests).
    pub fn with_clock(paths: &AppPaths, clock: Arc<dyn Clock>) -> Self {
        Self {
            file: paths.pause_file(),
            clock,
            cached: Mutex::new(Cached::default()),
        }
    }

    /// The active pause's `reason` (default `"settings"`), or `None` when there is none, it has
    /// expired, or its `until` is further out than `MAX_LEASE` (not trusted).
    pub fn reason(&self) -> Option<String> {
        let mut cached = self.cached.lock().unwrap_or_else(|e| e.into_inner());
        let Ok(meta) = std::fs::metadata(&self.file) else {
            cached.stamp = None;
            return None;
        };
        let stamp = Stamp::of(&meta);
        if cached.stamp.as_ref() != Some(&stamp) {
            cached.stamp = Some(stamp);
            let (until, reason) = match std::fs::read(&self.file) {
                Ok(bytes) => String::from_utf8(bytes)
                    .map(|text| parse_pause(&text))
                    .unwrap_or((0.0, String::new())),
                Err(_) => (0.0, String::new()),
            };
            cached.until = until;
            cached.reason = reason;
        }
        let now = self.clock.now();
        if cached.until <= now || cached.until - now > MAX_LEASE {
            return None;
        }
        Some(cached.reason.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::ota_busy;

    struct Fixed(Mutex<f64>);

    impl Clock for Fixed {
        fn now(&self) -> f64 {
            *self.0.lock().expect("clock")
        }
    }

    fn temp_paths() -> (tempfile::TempDir, AppPaths) {
        let dir = tempfile::tempdir().expect("temp dir");
        let paths = AppPaths::new(dir.path().join("home"));
        (dir, paths)
    }

    fn reader(paths: &AppPaths, now: f64) -> (PauseReader, Arc<Fixed>) {
        let clock = Arc::new(Fixed(Mutex::new(now)));
        (PauseReader::with_clock(paths, clock.clone()), clock)
    }

    #[test]
    fn hold_writes_the_python_text_and_release_removes_it() {
        let (_dir, paths) = temp_paths();
        hold_at(&paths, 60.0, "firmware", 1000.5).expect("hold");
        assert_eq!(
            std::fs::read_to_string(paths.pause_file()).expect("read"),
            format!(
                r#"{{"until": 1060.5, "reason": "firmware", "pid": {}}}"#,
                std::process::id()
            )
        );
        release(&paths).expect("release");
        release(&paths).expect("release twice");
        assert!(!paths.pause_file().exists());
    }

    #[test]
    fn lease_is_capped_at_300_seconds() {
        let (_dir, paths) = temp_paths();
        hold_at(&paths, 3600.0, "x", 1000.0).expect("hold");
        let text = std::fs::read_to_string(paths.pause_file()).expect("read");
        assert!(text.starts_with(r#"{"until": 1300.0, "#), "{text}");
    }

    #[test]
    fn reader_reports_active_pause_until_it_expires() {
        let (_dir, paths) = temp_paths();
        let (reader, clock) = reader(&paths, 1000.0);
        assert_eq!(reader.reason(), None);
        hold_at(&paths, 60.0, "firmware", 1000.0).expect("hold");
        assert_eq!(reader.reason().as_deref(), Some("firmware"));
        *clock.0.lock().expect("clock") = 1059.0;
        assert_eq!(reader.reason().as_deref(), Some("firmware"));
        *clock.0.lock().expect("clock") = 1060.0;
        assert_eq!(reader.reason(), None);
        release(&paths).expect("release");
        assert_eq!(reader.reason(), None);
    }

    #[test]
    fn reader_does_not_trust_an_until_beyond_the_lease() {
        let (_dir, paths) = temp_paths();
        std::fs::create_dir_all(paths.config_dir()).expect("mkdir");
        let (reader, _clock) = reader(&paths, 1000.0);
        std::fs::write(paths.pause_file(), r#"{"until": 1301, "reason": "x"}"#).expect("write");
        assert_eq!(reader.reason(), None);
        std::fs::write(paths.pause_file(), r#"{"until": 1300, "reason": "yy"}"#).expect("write");
        assert_eq!(reader.reason().as_deref(), Some("yy")); // different size: new stamp
    }

    #[test]
    fn reader_rereads_after_a_rewrite_and_survives_garbage() {
        let (_dir, paths) = temp_paths();
        let (reader, _clock) = reader(&paths, 1000.0);
        hold_at(&paths, 60.0, "a", 1000.0).expect("hold");
        assert_eq!(reader.reason().as_deref(), Some("a"));
        hold_at(&paths, 60.0, "bb", 1000.0).expect("hold");
        assert_eq!(reader.reason().as_deref(), Some("bb"));
        for text in [
            "{garbage",
            "[]",
            r#"{"until": null}"#,
            r#"{"until": "soon"}"#,
            r#"{"until": [1]}"#,
        ] {
            std::fs::write(paths.pause_file(), text).expect("write");
            assert_eq!(reader.reason(), None, "{text}");
        }
    }

    #[test]
    fn reason_defaults_to_settings_like_python() {
        let (_dir, paths) = temp_paths();
        std::fs::create_dir_all(paths.config_dir()).expect("mkdir");
        let (reader, _clock) = reader(&paths, 1000.0);
        for (text, want) in [
            (r#"{"until": 1100}"#, "settings"),
            (r#"{"until": 1100, "reason": ""}"#, "settings"),
            (r#"{"until": 1100, "reason": null}"#, "settings"),
            (r#"{"until": 1100, "reason": 7}"#, "7"),
            (r#"{"until": "1100", "reason": "r"}"#, "r"),
        ] {
            std::fs::write(paths.pause_file(), text).expect("write");
            assert_eq!(reader.reason().as_deref(), Some(want), "{text}");
        }
    }

    #[test]
    fn pause_and_ota_busy_are_independent_files() {
        let (_dir, paths) = temp_paths();
        hold(&paths, 60.0, "x").expect("hold");
        assert!(!ota_busy(&paths, None));
        crate::paths::set_ota_busy(&paths, true);
        release(&paths).expect("release");
        assert!(ota_busy(&paths, None));
        assert!(PauseReader::new(&paths).reason().is_none());
    }
}
