//! FROZEN (phase 02, plan.md rule 2): the state shared by the tray shell (phase 12) and the
//! settings commands (phase 13). Changes are additive, through a Contract CR. Every struct that
//! phases build with a literal is `#[non_exhaustive]` with a `new(..)`, so adding a field later
//! does not break the other phase.
//!
//! NOTE: inside `clawd-app` (phases 12 and 13 both live here) `#[non_exhaustive]` does NOT stop a
//! struct literal: the protection is by convention only, so always build `AppState`, `Services`
//! and `SettingsSession` with `::new(..)`. It is effective for types built from another crate
//! (`UsageEnv`, `OtaOptions`, `UpdaterOptions`).
//!
//! In Python the settings window was a separate process that talked to the tray over a stdin/
//! stdout pipe; here both live in one process and the same messages travel as [`ShellCmd`]
//! through `AppState.shell_tx`.

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use clawd_agents::Registry;
use clawd_appupdate::Updater;
use clawd_config::CommandRunner;
use clawd_contracts::{AppPaths, OsStoreNames};
use clawd_firmware::{feed::Feed, firmware::Bundle};
use serde_json::{json, Value};
use tokio::sync::{mpsc, watch};

pub use crate::commands::session::SessionData;
pub use crate::engine::{EngineHandle, Status};

/// Lock a mutex, ignoring poisoning: one panicking command must not kill every later one.
pub fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The long-lived services settings commands need (paths, OS-store names, updater, agent
/// registry, feed, bundled firmware, command runner). Built once by phase 12, shared by every
/// settings window. Python passed these to `SettingsApi.__init__` as keyword arguments
/// (`settings_api.py:133-176`).
#[non_exhaustive]
pub struct Services {
    pub paths: AppPaths,
    /// Keyring service / Run value / LaunchAgent label ([`OsStoreNames::production`] in the app).
    pub names: OsStoreNames,
    pub updater: Arc<Updater>,
    pub registry: Arc<Registry>,
    /// `None` when the install has no feed URL/token.
    pub feed: Option<Arc<Feed>>,
    /// `None` in a public build (no bundled firmware).
    pub bundle: Option<Bundle>,
    pub runner: Arc<dyn CommandRunner>,
}

impl Services {
    pub fn new(
        paths: AppPaths,
        names: OsStoreNames,
        updater: Arc<Updater>,
        registry: Arc<Registry>,
        feed: Option<Arc<Feed>>,
        bundle: Option<Bundle>,
        runner: Arc<dyn CommandRunner>,
    ) -> Self {
        Self {
            paths,
            names,
            updater,
            registry,
            feed,
            bundle,
            runner,
        }
    }
}

/// Registered once with `app.manage(AppState::new(..))`: the only thing to manage.
#[non_exhaustive]
pub struct AppState {
    /// The engine (finder, publisher, event server, ...). Its owner is phase 12.
    pub engine: EngineHandle,
    /// Settings -> shell messages (handled by phase 12's shell loop).
    pub shell_tx: mpsc::Sender<ShellCmd>,
    /// Long-running settings jobs (OTA, feed check, app update).
    pub jobs: Arc<JobState>,
    /// Latest engine status, for the tray menu and the popover.
    pub status: watch::Receiver<Status>,
    /// Paths, names, updater, registry, feed, bundle, runner.
    pub services: Arc<Services>,
    /// The live settings window's session (set on open, cleared on destroy by phase 12).
    pub session: SessionSlot,
}

impl AppState {
    pub fn new(
        engine: EngineHandle,
        shell_tx: mpsc::Sender<ShellCmd>,
        jobs: Arc<JobState>,
        status: watch::Receiver<Status>,
        services: Arc<Services>,
    ) -> Self {
        Self {
            engine,
            shell_tx,
            jobs,
            status,
            services,
            session: SessionSlot::default(),
        }
    }
}

/// What the settings window tells the tray; exactly the protocol of `settings_app.py:7-14`
/// (`{"cmd": ...}` lines).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellCmd {
    /// `{"cmd": "refresh"}`: autostart or hooks changed (update the menu).
    Refresh,
    /// `{"cmd": "app_update_apply"}`: restart into the downloaded app version.
    AppUpdateApply,
    /// `{"cmd": "set_clock", "host": "10.0.0.5" | ""}`: a typed clock, or `""` = search again.
    SetClock { host: String },
    /// `{"cmd": "use_clock", "name", "host", "port", "version"}`: picked in the setup wizard.
    UseClock {
        name: String,
        host: String,
        port: u16,
        version: String,
    },
    /// `{"cmd": "usage_source"}`: send usage from the newly chosen source now.
    UsageSource,
    /// `{"cmd": "setup"}`: a wizard step or an agent changed (the tray's items).
    Setup,
}

/// Long-running settings jobs. `busy` is read by the window manager to refuse closing the window
/// during a firmware install (`clawd://busy-close`); `job` is the progress dict `api.job()`
/// returns (`{"running": false}` when idle). The OTA job also holds `ota.busy`
/// (`clawd_store::paths::set_ota_busy`) for its whole duration, cleared by a `Drop` guard.
pub struct JobState {
    pub busy: AtomicBool,
    pub job: Mutex<Value>,
}

impl Default for JobState {
    fn default() -> Self {
        Self {
            busy: AtomicBool::new(false),
            job: Mutex::new(json!({"running": false})),
        }
    }
}

/// One settings window's session, created **fresh every time the window opens** (phase 12):
/// the clock it talks to (`base` like `http://10.0.0.5`), the keychain key of its web password
/// (`key`, the clock's name or host), and the tab to open on (`""`, `update`, `agents`,
/// `setup`). Python: `SettingsApi(base, key, tab=...)` / `_connect(base, key)`.
///
/// `data` is the window's mutable state (clock API client, last heap/config, feed release, frame
/// cache, ...): phase 13 defines it in `commands/session.rs`.
#[non_exhaustive]
pub struct SettingsSession {
    pub base: String,
    pub key: String,
    pub tab: String,
    pub data: Mutex<SessionData>,
}

impl SettingsSession {
    pub fn new(base: impl Into<String>, key: impl Into<String>, tab: impl Into<String>) -> Self {
        Self {
            base: base.into(),
            key: key.into(),
            tab: tab.into(),
            data: Mutex::new(SessionData::default()),
        }
    }
}

/// Where the live settings window's session is kept: phase 12 calls [`Self::set`] when it opens
/// the window and [`Self::clear`] when it is destroyed; `py_api` hands [`Self::get`] to the
/// commands. It lives inside [`AppState`], so there is nothing extra to `manage`. There is at
/// most one settings window.
#[derive(Default)]
pub struct SessionSlot(Mutex<Option<Arc<SettingsSession>>>);

impl SessionSlot {
    pub fn set(&self, session: SettingsSession) -> Arc<SettingsSession> {
        let session = Arc::new(session);
        *lock(&self.0) = Some(Arc::clone(&session));
        session
    }

    pub fn clear(&self) {
        *lock(&self.0) = None;
    }

    pub fn get(&self) -> Option<Arc<SettingsSession>> {
        lock(&self.0).clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_session_replaces_the_old_one() {
        let slot = SessionSlot::default();
        assert!(slot.get().is_none());
        slot.set(SettingsSession::new("http://a", "a", ""));
        let second = slot.set(SettingsSession::new("http://b", "b", "setup"));
        let got = slot.get().expect("session");
        assert!(Arc::ptr_eq(&got, &second));
        assert_eq!(got.base, "http://b");
        slot.clear();
        assert!(slot.get().is_none());
    }

    #[test]
    fn a_poisoned_lock_does_not_kill_later_commands() {
        let slot = Arc::new(SessionSlot::default());
        let s = Arc::clone(&slot);
        let _ = std::thread::spawn(move || {
            let _guard = s.0.lock().unwrap();
            panic!("a command panicked while holding the slot");
        })
        .join();
        assert!(slot.get().is_none());
        slot.set(SettingsSession::new("http://c", "c", ""));
        assert!(slot.get().is_some());
    }

    #[test]
    fn jobs_start_idle() {
        let jobs = JobState::default();
        assert_eq!(*lock(&jobs.job), json!({"running": false}));
    }
}
