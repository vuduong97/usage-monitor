//! The cross-process lock on `desktop.lock` (`paths._prefs_lock`). It must interlock with the
//! Python app on the same file: Windows `LockFileEx` on byte 0 non-blocking (== `msvcrt.locking
//! (LK_NBLCK, 1)`), macOS `flock(LOCK_EX | LOCK_NB)`. Lock the sidecar, never `desktop.json`.

use std::io;

use clawd_contracts::AppPaths;

/// Held while the lock is taken; dropping it unlocks and closes the handle. Exclusive across
/// processes and threads (each holder opens its own handle); not reentrant.
pub struct PrefsLockGuard {
    _private: (),
}

/// `paths._prefs_lock()`: take the lock, polling every 20 ms for up to
/// [`clawd_contracts::PREFS_IO_WAIT`]; `TimedOut` when another Clawd process keeps it.
pub fn prefs_lock(paths: &AppPaths) -> io::Result<PrefsLockGuard> {
    todo!("phase 03")
}
