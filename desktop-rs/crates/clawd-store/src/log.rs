//! `paths.RotatingLog` / `paths.open_log`: the packaged app has no console, so logs go to
//! `clawd-desktop.log`, which rotates to `clawd-desktop.log.1` past 1 MB (never more than ~2 MB).
//! Rust code logs through `tracing`; [`init_logging`] installs a layer writing to this file.

use std::io;
use std::path::PathBuf;

use clawd_contracts::AppPaths;

/// A text stream that appends to `path` and keeps one previous file.
pub struct RotatingLog {
    _private: (),
}

impl RotatingLog {
    /// `RotatingLog(path, limit)`: opens (creating the parent dir) for append.
    pub fn new(path: PathBuf, limit: u64) -> io::Result<Self> {
        todo!("phase 03")
    }

    /// `RotatingLog.write(s)`: append; once the file passes the limit it is moved to
    /// `<name>.log.1` and a fresh one is started. Write errors are swallowed (returns 0).
    pub fn write_str(&mut self, s: &str) -> usize {
        todo!("phase 03")
    }

    /// `RotatingLog.flush()`.
    pub fn flush(&mut self) {
        todo!("phase 03")
    }
}

/// `paths.open_log()` for the Rust app: route `tracing` (and the panic hook's message) to the
/// rotating log in `paths.log_file()` with a `[HH:MM:SS]`-style timestamp. Call once, early.
pub fn init_logging(paths: &AppPaths) -> io::Result<()> {
    todo!("phase 03")
}
