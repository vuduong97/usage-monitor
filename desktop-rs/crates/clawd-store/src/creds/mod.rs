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
//! `for_test(..)` set in tests, which must never touch the production service). The `_with`
//! variants take any [`Keyring`], so tests run against an in-memory one
//! (`clawd_testkit::store::MemKeyring`).

use clawd_contracts::OsStoreNames;

#[cfg(target_os = "macos")]
mod mac;
#[cfg(windows)]
mod wincred;

/// `creds.DEFAULT_USER`.
pub const DEFAULT_USER: &str = clawd_contracts::DEFAULT_CLOCK_USER;

/// The three keyring calls `creds.py` makes (`keyring.get_password` / `set_password` /
/// `delete_password`), without the exceptions: a failure is `None` / `false`.
pub trait Keyring: Send + Sync {
    /// The stored secret of `(service, username)`.
    fn get_password(&self, service: &str, username: &str) -> Option<String>;
    /// Store a secret; false when the store fails.
    fn set_password(&self, service: &str, username: &str, secret: &str) -> bool;
    /// Delete a secret; false when there was none or the store fails.
    fn delete_password(&self, service: &str, username: &str) -> bool;
}

/// The real OS store: Windows Credential Manager, macOS Keychain. On other systems nothing is
/// ever stored (reads are `None`, writes `false`).
#[derive(Debug, Clone, Copy, Default)]
pub struct OsKeyring;

impl Keyring for OsKeyring {
    fn get_password(&self, service: &str, username: &str) -> Option<String> {
        #[cfg(windows)]
        return wincred::get_password(service, username);
        #[cfg(target_os = "macos")]
        return mac::get_password(service, username);
        #[cfg(not(any(windows, target_os = "macos")))]
        {
            let _ = (service, username);
            None
        }
    }

    fn set_password(&self, service: &str, username: &str, secret: &str) -> bool {
        #[cfg(windows)]
        return wincred::set_password(service, username, secret);
        #[cfg(target_os = "macos")]
        return mac::set_password(service, username, secret);
        #[cfg(not(any(windows, target_os = "macos")))]
        {
            let _ = (service, username, secret);
            false
        }
    }

    fn delete_password(&self, service: &str, username: &str) -> bool {
        #[cfg(windows)]
        return wincred::delete_password(service, username);
        #[cfg(target_os = "macos")]
        return mac::delete_password(service, username);
        #[cfg(not(any(windows, target_os = "macos")))]
        {
            let _ = (service, username);
            false
        }
    }
}

/// `creds.get(clock_key)`: `(user, password)`, or `None` when nothing is stored or the store
/// fails.
pub fn get(names: &OsStoreNames, clock_key: &str) -> Option<(String, String)> {
    get_with(&OsKeyring, names, clock_key)
}

/// [`get`] over any [`Keyring`].
pub fn get_with(
    keyring: &dyn Keyring,
    names: &OsStoreNames,
    clock_key: &str,
) -> Option<(String, String)> {
    let raw = keyring.get_password(&names.keyring_service, clock_key)?;
    if raw.is_empty() {
        return None;
    }
    Some(match raw.split_once('\n') {
        Some((user, password)) => (user.to_owned(), password.to_owned()),
        None => (DEFAULT_USER.to_owned(), raw),
    })
}

/// `creds.put(clock_key, user, password)`: store `"user\npassword"`; false when the store fails.
pub fn put(names: &OsStoreNames, clock_key: &str, user: &str, password: &str) -> bool {
    put_with(&OsKeyring, names, clock_key, user, password)
}

/// [`put`] over any [`Keyring`].
pub fn put_with(
    keyring: &dyn Keyring,
    names: &OsStoreNames,
    clock_key: &str,
    user: &str,
    password: &str,
) -> bool {
    keyring.set_password(
        &names.keyring_service,
        clock_key,
        &format!("{user}\n{password}"),
    )
}

/// `creds.forget(clock_key)`: delete the entry; failures are ignored.
pub fn forget(names: &OsStoreNames, clock_key: &str) {
    forget_with(&OsKeyring, names, clock_key);
}

/// [`forget`] over any [`Keyring`].
pub fn forget_with(keyring: &dyn Keyring, names: &OsStoreNames, clock_key: &str) {
    let _ = keyring.delete_password(&names.keyring_service, clock_key);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// Local stand-in (unit tests cannot use testkit fakes of this crate's own types).
    #[derive(Default)]
    struct Mem(Mutex<HashMap<(String, String), String>>);

    impl Keyring for Mem {
        fn get_password(&self, service: &str, username: &str) -> Option<String> {
            self.0
                .lock()
                .unwrap()
                .get(&(service.to_owned(), username.to_owned()))
                .cloned()
        }
        fn set_password(&self, service: &str, username: &str, secret: &str) -> bool {
            self.0
                .lock()
                .unwrap()
                .insert((service.to_owned(), username.to_owned()), secret.to_owned());
            true
        }
        fn delete_password(&self, service: &str, username: &str) -> bool {
            self.0
                .lock()
                .unwrap()
                .remove(&(service.to_owned(), username.to_owned()))
                .is_some()
        }
    }

    fn names() -> OsStoreNames {
        let names = OsStoreNames::for_test("creds-unit");
        assert!(!names.is_production());
        names
    }

    #[test]
    fn put_get_forget_round_trip_with_user_and_newlines() {
        let kr = Mem::default();
        let n = names();
        assert_eq!(get_with(&kr, &n, "clock-1"), None);
        assert!(put_with(&kr, &n, "clock-1", "root", "pa\nss"));
        assert_eq!(
            kr.get_password(&n.keyring_service, "clock-1").as_deref(),
            Some("root\npa\nss")
        );
        assert_eq!(
            get_with(&kr, &n, "clock-1"),
            Some(("root".into(), "pa\nss".into()))
        );
        forget_with(&kr, &n, "clock-1");
        forget_with(&kr, &n, "clock-1");
        assert_eq!(get_with(&kr, &n, "clock-1"), None);
    }

    #[test]
    fn secret_without_newline_is_the_default_users_password() {
        let kr = Mem::default();
        let n = names();
        kr.set_password(&n.keyring_service, "k", "plain");
        assert_eq!(
            get_with(&kr, &n, "k"),
            Some((DEFAULT_USER.to_owned(), "plain".to_owned()))
        );
        kr.set_password(&n.keyring_service, "empty", "");
        assert_eq!(get_with(&kr, &n, "empty"), None);
    }

    #[test]
    fn empty_password_keeps_its_user() {
        let kr = Mem::default();
        let n = names();
        assert!(put_with(&kr, &n, "k", "admin", ""));
        assert_eq!(
            get_with(&kr, &n, "k"),
            Some(("admin".into(), String::new()))
        );
    }

    #[test]
    fn names_decide_the_service() {
        let kr = Mem::default();
        let a = OsStoreNames::for_test("a");
        let b = OsStoreNames::for_test("b");
        put_with(&kr, &a, "k", "u", "p");
        assert_eq!(get_with(&kr, &b, "k"), None);
        assert!(get_with(&kr, &a, "k").is_some());
    }

    #[cfg(windows)]
    #[test]
    fn os_keyring_round_trip_under_a_throwaway_service() {
        let names = OsStoreNames::for_test(&format!("os-roundtrip-{}", std::process::id()));
        assert!(!names.is_production());
        let key = "clock-x";
        // clean even if the assertions below fail
        struct Cleanup<'a>(&'a OsStoreNames);
        impl Drop for Cleanup<'_> {
            fn drop(&mut self) {
                forget(self.0, "clock-x");
            }
        }
        let _cleanup = Cleanup(&names);
        assert!(put(&names, key, "admin", "s3cret"));
        assert_eq!(get(&names, key), Some(("admin".into(), "s3cret".into())));
        forget(&names, key);
        assert_eq!(get(&names, key), None);
    }
}
