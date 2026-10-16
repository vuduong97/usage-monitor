//! `paths.RotatingLog` / `paths.open_log`: the packaged app has no console, so logs go to
//! `clawd-desktop.log`, which rotates to `clawd-desktop.log.1` past 1 MB (never more than ~2 MB).
//! Rust code logs through `tracing`; [`init_logging`] installs a layer writing to this file.

use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use clawd_contracts::{AppPaths, LOG_MAX};
use tracing_subscriber::fmt::format::Writer;
use tracing_subscriber::fmt::time::FormatTime;
use tracing_subscriber::fmt::MakeWriter;

/// A text stream that appends to `path` and keeps one previous file.
#[derive(Debug)]
pub struct RotatingLog {
    path: PathBuf,
    limit: u64,
    file: Option<File>,
    /// Bytes in the current file (`self._f.tell()`).
    size: u64,
}

fn open_append(path: &PathBuf) -> io::Result<(File, u64)> {
    let file = OpenOptions::new().create(true).append(true).open(path)?;
    let size = file.metadata()?.len();
    Ok((file, size))
}

impl RotatingLog {
    /// `RotatingLog(path, limit)`: opens (creating the parent dir) for append.
    pub fn new(path: PathBuf, limit: u64) -> io::Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let (file, size) = open_append(&path)?;
        Ok(Self {
            path,
            limit,
            file: Some(file),
            size,
        })
    }

    fn rotate(&mut self) -> io::Result<()> {
        self.file = None; // close before moving: Windows will not rename an open file
        let mut rotated = self.path.clone().into_os_string();
        rotated.push(".1");
        let moved = std::fs::rename(&self.path, PathBuf::from(rotated));
        let (file, size) = open_append(&self.path)?;
        self.file = Some(file);
        self.size = size;
        moved
    }

    /// `RotatingLog.write(s)`: append; once the file passes the limit it is moved to
    /// `<name>.log.1` and a fresh one is started. Write errors are swallowed (returns 0).
    /// Returns the bytes written (Python returned the character count).
    pub fn write_str(&mut self, s: &str) -> usize {
        let Some(file) = self.file.as_mut() else {
            return 0;
        };
        if file.write_all(s.as_bytes()).is_err() {
            return 0;
        }
        self.size += s.len() as u64;
        if self.size > self.limit {
            let _ = self.rotate();
        }
        s.len()
    }

    /// `RotatingLog.flush()`.
    pub fn flush(&mut self) {
        if let Some(file) = self.file.as_mut() {
            let _ = file.flush();
        }
    }
}

/// The log shared by every `tracing` writer.
#[derive(Clone)]
struct SharedLog(Arc<Mutex<RotatingLog>>);

struct SharedLogWriter(Arc<Mutex<RotatingLog>>);

impl Write for SharedLogWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut log = self.0.lock().unwrap_or_else(|e| e.into_inner());
        log.write_str(&String::from_utf8_lossy(buf));
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).flush();
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for SharedLog {
    type Writer = SharedLogWriter;

    fn make_writer(&'a self) -> Self::Writer {
        SharedLogWriter(Arc::clone(&self.0))
    }
}

/// `[HH:MM:SS]` (UTC) in front of every line.
struct BracketClock;

impl FormatTime for BracketClock {
    fn format_time(&self, w: &mut Writer<'_>) -> std::fmt::Result {
        let secs = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs())
            % 86_400;
        write!(
            w,
            "[{:02}:{:02}:{:02}]",
            secs / 3600,
            secs % 3600 / 60,
            secs % 60
        )
    }
}

/// `paths.open_log()` for the Rust app: route `tracing` (and the panic hook's message) to the
/// rotating log in `paths.log_file()` with a `[HH:MM:SS]`-style timestamp (UTC). Call once,
/// early; a second call is a no-op.
pub fn init_logging(paths: &AppPaths) -> io::Result<()> {
    let log = RotatingLog::new(paths.log_file(), LOG_MAX)?;
    let installed = tracing_subscriber::fmt()
        .with_writer(SharedLog(Arc::new(Mutex::new(log))))
        .with_timer(BracketClock)
        .with_ansi(false)
        .with_max_level(tracing::Level::INFO)
        .try_init()
        .is_ok();
    if installed {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            tracing::error!("panic: {info}");
            previous(info);
        }));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotates_past_the_limit_and_keeps_one_previous_file() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("sub").join("clawd-desktop.log");
        let mut log = RotatingLog::new(path.clone(), 10).expect("open");
        assert_eq!(log.write_str("12345\n"), 6);
        assert!(!dir.path().join("sub").join("clawd-desktop.log.1").exists());
        log.write_str("67890\n"); // 12 bytes > 10: rotate
        let rotated = dir.path().join("sub").join("clawd-desktop.log.1");
        assert_eq!(
            std::fs::read_to_string(&rotated).expect("rotated"),
            "12345\n67890\n"
        );
        assert_eq!(std::fs::read_to_string(&path).expect("fresh"), "");
        log.write_str("abc\n");
        log.write_str("defghijk\n"); // rotate again: the old .1 is replaced
        assert_eq!(
            std::fs::read_to_string(&rotated).expect("rotated"),
            "abc\ndefghijk\n"
        );
        log.flush();
    }

    #[test]
    fn reopening_appends_and_counts_existing_bytes() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("l.log");
        std::fs::write(&path, "123456789").expect("seed");
        let mut log = RotatingLog::new(path.clone(), 10).expect("open");
        log.write_str("ab");
        assert!(dir.path().join("l.log.1").exists());
    }
}
