//! Fakes for `clawd-store` tests (phase 03 owns this file): an in-memory keyring and sandboxed
//! autostart entries keyed by `OsStoreNames::for_test`. Nothing here touches the real Credential
//! Manager, Run key or LaunchAgents folder.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use clawd_contracts::{AppPaths, OsStoreNames, Platform};
use clawd_store::autostart::Autostart;
use clawd_store::creds::Keyring;

/// An in-memory [`Keyring`]: `(service, user) -> secret`, shared between clones of the `Arc`.
#[derive(Debug, Default)]
pub struct MemKeyring {
    entries: Mutex<BTreeMap<(String, String), String>>,
}

impl MemKeyring {
    pub fn new() -> Self {
        Self::default()
    }

    /// Pre-load a raw secret (what an older version stored).
    pub fn insert(&self, service: &str, username: &str, secret: &str) {
        self.lock()
            .insert((service.to_owned(), username.to_owned()), secret.to_owned());
    }

    /// The raw secret stored for `(service, username)`.
    pub fn secret(&self, service: &str, username: &str) -> Option<String> {
        self.lock()
            .get(&(service.to_owned(), username.to_owned()))
            .cloned()
    }

    pub fn len(&self) -> usize {
        self.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.lock().is_empty()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, BTreeMap<(String, String), String>> {
        self.entries.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl Keyring for MemKeyring {
    fn get_password(&self, service: &str, username: &str) -> Option<String> {
        self.secret(service, username)
    }

    fn set_password(&self, service: &str, username: &str, secret: &str) -> bool {
        self.insert(service, username, secret);
        true
    }

    fn delete_password(&self, service: &str, username: &str) -> bool {
        self.lock()
            .remove(&(service.to_owned(), username.to_owned()))
            .is_some()
    }
}

/// An [`Autostart`] over a throwaway home that removes its sandbox registry key when dropped.
/// Refuses (panics) on production names.
pub struct SandboxAutostart {
    pub autostart: Autostart,
}

impl SandboxAutostart {
    /// `command` is what the entry launches; `platform` picks the plist or Run-value branch.
    pub fn new(
        paths: &AppPaths,
        names: &OsStoreNames,
        platform: Platform,
        command: Vec<String>,
    ) -> Self {
        assert!(
            !names.is_production(),
            "store tests never use production OS-store names"
        );
        let mut autostart = Autostart::new(paths, names.clone());
        autostart.platform = platform;
        autostart.command = command;
        Self { autostart }
    }
}

impl Drop for SandboxAutostart {
    fn drop(&mut self) {
        self.autostart.remove_sandbox();
    }
}

/// Write `desktop.json` (or any config file) with exactly `text`; `None` removes it.
pub fn put_config_file(paths: &AppPaths, name: &str, text: Option<&str>) {
    let path = paths.config_dir().join(name);
    match text {
        Some(text) => {
            std::fs::create_dir_all(paths.config_dir()).expect("mkdir config dir");
            std::fs::write(&path, text).expect("write config file");
        }
        None => {
            let _ = std::fs::remove_file(&path);
        }
    }
}

/// The text of a config file, `None` when it does not exist.
pub fn read_config_file(paths: &AppPaths, name: &str) -> Option<String> {
    std::fs::read_to_string(paths.config_dir().join(name)).ok()
}

/// Set a file's modification time to `epoch_secs` (`os.utime`).
pub fn set_mtime(path: &Path, epoch_secs: u64) {
    let when = SystemTime::UNIX_EPOCH + Duration::from_secs(epoch_secs);
    std::fs::File::options()
        .write(true)
        .open(path)
        .and_then(|f| f.set_modified(when))
        .expect("set mtime");
}

/// Remove every file in the config dir between golden cases.
pub fn reset_config_dir(paths: &AppPaths) {
    let _ = std::fs::remove_dir_all(paths.config_dir());
    std::fs::create_dir_all(paths.config_dir()).expect("mkdir config dir");
}
