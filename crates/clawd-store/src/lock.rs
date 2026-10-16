//! The cross-process lock on `desktop.lock` (`paths._prefs_lock`). It must interlock with the
//! Python app on the same file: Windows `LockFileEx` on byte 0 non-blocking (== `msvcrt.locking
//! (LK_NBLCK, 1)`), macOS `flock(LOCK_EX | LOCK_NB)`. Lock the sidecar, never `desktop.json`.

use std::fs::{File, OpenOptions};
use std::io;
use std::time::{Duration, Instant};

use clawd_contracts::{AppPaths, PREFS_IO_WAIT};

/// How often a busy lock is polled (`time.sleep(0.02)`).
const POLL: Duration = Duration::from_millis(20);

/// Held while the lock is taken; dropping it unlocks and closes the handle. Exclusive across
/// processes and threads (each holder opens its own handle); not reentrant.
#[derive(Debug)]
pub struct PrefsLockGuard {
    file: File,
}

impl Drop for PrefsLockGuard {
    fn drop(&mut self) {
        sys::unlock(&self.file);
    }
}

/// `paths._prefs_lock()`: take the lock, polling every 20 ms for up to
/// [`clawd_contracts::PREFS_IO_WAIT`]; `TimedOut` when another Clawd process keeps it.
pub fn prefs_lock(paths: &AppPaths) -> io::Result<PrefsLockGuard> {
    prefs_lock_within(paths, Duration::from_secs_f64(PREFS_IO_WAIT))
}

/// [`prefs_lock`] with an explicit wait (tests use a short one).
pub fn prefs_lock_within(paths: &AppPaths, wait: Duration) -> io::Result<PrefsLockGuard> {
    let path = paths.prefs_lock_file();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let file = open_lock_file(&path)?;
    let deadline = Instant::now() + wait;
    while !sys::try_lock(&file)? {
        if Instant::now() >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                format!(
                    "{} is locked by another Clawd process",
                    clawd_contracts::PREFS_FILE_NAME
                ),
            ));
        }
        std::thread::sleep(POLL);
    }
    Ok(PrefsLockGuard { file })
}

fn open_lock_file(path: &std::path::Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

#[cfg(windows)]
mod sys {
    use std::fs::File;
    use std::io;
    use std::os::windows::io::AsRawHandle;

    use windows::Win32::Foundation::{ERROR_IO_PENDING, ERROR_LOCK_VIOLATION, HANDLE};
    use windows::Win32::Storage::FileSystem::{
        LockFileEx, UnlockFileEx, LOCKFILE_EXCLUSIVE_LOCK, LOCKFILE_FAIL_IMMEDIATELY,
    };
    use windows::Win32::System::IO::OVERLAPPED;

    /// Lock byte 0 exclusively without waiting: `Ok(false)` when someone else holds it.
    pub(super) fn try_lock(file: &File) -> io::Result<bool> {
        let handle = HANDLE(file.as_raw_handle());
        let mut overlapped = OVERLAPPED::default();
        // SAFETY: `handle` is a live file handle owned by `file`; `overlapped` outlives the call
        // and (offset 0, no event) describes byte 0.
        let res = unsafe {
            LockFileEx(
                handle,
                LOCKFILE_EXCLUSIVE_LOCK | LOCKFILE_FAIL_IMMEDIATELY,
                None,
                1,
                0,
                &mut overlapped,
            )
        };
        match res {
            Ok(()) => Ok(true),
            Err(e) if e.code() == ERROR_LOCK_VIOLATION.to_hresult() => Ok(false),
            Err(e) if e.code() == ERROR_IO_PENDING.to_hresult() => Ok(false),
            Err(e) => Err(io::Error::from_raw_os_error(
                (e.code().0 as u32 & 0xFFFF) as i32,
            )),
        }
    }

    pub(super) fn unlock(file: &File) {
        let handle = HANDLE(file.as_raw_handle());
        let mut overlapped = OVERLAPPED::default();
        // SAFETY: as in `try_lock`; failure is harmless (closing the handle unlocks anyway).
        let _ = unsafe { UnlockFileEx(handle, None, 1, 0, &mut overlapped) };
    }
}

#[cfg(not(windows))]
mod sys {
    use std::fs::{File, TryLockError};
    use std::io;

    /// `flock(LOCK_EX | LOCK_NB)`: `Ok(false)` when someone else holds it.
    pub(super) fn try_lock(file: &File) -> io::Result<bool> {
        match file.try_lock() {
            Ok(()) => Ok(true),
            Err(TryLockError::WouldBlock) => Ok(false),
            Err(TryLockError::Error(e)) => Err(e),
        }
    }

    pub(super) fn unlock(file: &File) {
        let _ = file.unlock();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_paths() -> (tempfile::TempDir, AppPaths) {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(dir.path().join("home"));
        (dir, paths)
    }

    #[test]
    fn second_holder_times_out_until_the_first_drops() {
        let (_dir, paths) = temp_paths();
        let first = prefs_lock(&paths).unwrap();
        let err = prefs_lock_within(&paths, Duration::from_millis(100)).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::TimedOut);
        assert!(err.to_string().contains("desktop.json"));
        drop(first);
        prefs_lock_within(&paths, Duration::from_millis(500)).unwrap();
    }
}
