//! `creds.py`: the clock's web password, kept in the OS keychain (macOS Keychain, Windows
//! Credential Manager). Keyed by the clock's name (so a new IP keeps it); nothing is written to
//! the config files. Service `vn.clawd.desktop`, account = clock key, secret `"user\npassword"`
//! (a secret without a newline is the password of [`DEFAULT_USER`]).
//!
//! Byte-compatible with what python-keyring wrote, so a password saved by 0.1.4 reads back:
//! * Windows: raw `CredReadW`/`CredWriteW` reproducing `keyring/backends/Windows.py`
//!   (TargetName = service, fallback `user@service`, swap on a different user, UTF-16LE blob,
//!   `CRED_PERSIST_ENTERPRISE`, Comment `Stored using python-keyring`);
//! * macOS: generic password (service, account) via `security-framework`.
//!
//! All functions take the names to use ([`OsStoreNames::production`] in the app, a
//! `for_test(..)` set in tests, which must never touch the production service).
#![allow(unused_variables, dead_code)] // milestone stub: removed when implemented

use clawd_contracts::OsStoreNames;

#[cfg(target_os = "macos")]
mod mac;
#[cfg(windows)]
mod wincred;

/// `creds.DEFAULT_USER`.
pub const DEFAULT_USER: &str = clawd_contracts::DEFAULT_CLOCK_USER;

/// `creds.get(clock_key)`: `(user, password)`, or `None` when nothing is stored or the store
/// fails.
pub fn get(names: &OsStoreNames, clock_key: &str) -> Option<(String, String)> {
    todo!("phase 03")
}

/// `creds.put(clock_key, user, password)`: store `"user\npassword"`; false when the store fails.
pub fn put(names: &OsStoreNames, clock_key: &str, user: &str, password: &str) -> bool {
    todo!("phase 03")
}

/// `creds.forget(clock_key)`: delete the entry; failures are ignored.
pub fn forget(names: &OsStoreNames, clock_key: &str) {
    todo!("phase 03")
}
