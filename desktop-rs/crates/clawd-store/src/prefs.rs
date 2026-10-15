//! `desktop.json` (`paths.py:42-155`): written by two processes in Python (tray and settings
//! window), so every read-modify-write runs under the `desktop.lock` file lock. A file being
//! replaced answers a transient permission error on Windows: that is waited out, never read as
//! empty. Prefs is a map, not a struct: unknown and wrongly typed keys survive (red team #11).

use std::io;

use clawd_contracts::{AppPaths, Prefs};

/// `paths.load_prefs()`: for reading only (empty when missing, corrupt, not an object, or
/// unreadable); changes go through [`update_prefs`].
pub fn load_prefs(paths: &AppPaths) -> Prefs {
    todo!("phase 03")
}

/// `paths.update_prefs(change)`: under the lock, read `desktop.json` (`{}` when missing or not a
/// JSON object), let `change` edit it in place, write it back (pretty, 2-space indent, atomic)
/// when anything changed or the file did not exist, and return the result. Errors, writing
/// nothing, when the file exists but cannot be read, or the lock is not taken within
/// [`clawd_contracts::PREFS_IO_WAIT`].
pub fn update_prefs<F: FnOnce(&mut Prefs)>(paths: &AppPaths, change: F) -> io::Result<Prefs> {
    todo!("phase 03")
}

/// `paths.save_prefs(data)`: replace all of `desktop.json` with `data` (use [`update_prefs`] to
/// change some keys).
pub fn save_prefs(paths: &AppPaths, data: &Prefs) -> io::Result<()> {
    todo!("phase 03")
}

/// `paths.clock_screen()`: the clock's "Màn hình chính" as the settings window last saw it, or
/// `None`. The engine reads it (`Brain` stops pushing animations while it is
/// [`clawd_contracts::SCREEN_USAGE`]).
pub fn clock_screen(paths: &AppPaths) -> Option<i64> {
    todo!("phase 03")
}

/// `paths.set_clock_screen(screen)`: any integer is stored (a bool is not an integer).
pub fn set_clock_screen(paths: &AppPaths, screen: i64) -> io::Result<()> {
    todo!("phase 03")
}
