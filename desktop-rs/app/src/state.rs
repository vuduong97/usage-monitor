//! FROZEN (phase 02, plan.md rule 2): the state shared by the tray shell (phase 12) and the
//! settings commands (phase 13). Changes are additive, through a Contract CR.
//!
//! In Python the settings window was a separate process that talked to the tray over a stdin/
//! stdout pipe; here both live in one process and the same messages travel as [`ShellCmd`]
//! through `AppState.shell_tx`.

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};
use tokio::sync::{mpsc, watch};

pub use crate::engine::{EngineHandle, Status};

/// Registered once with `app.manage(AppState { .. })`.
pub struct AppState {
    /// The engine (finder, publisher, event server, ...). Its owner is phase 12.
    pub engine: EngineHandle,
    /// Settings -> shell messages (handled by phase 12's shell loop).
    pub shell_tx: mpsc::Sender<ShellCmd>,
    /// Long-running settings jobs (OTA, feed check, app update).
    pub jobs: Arc<JobState>,
    /// Latest engine status, for the tray menu and the popover.
    pub status: watch::Receiver<Status>,
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsSession {
    pub base: String,
    pub key: String,
    pub tab: String,
}

/// Where the live settings window's session is kept: phase 12 calls [`Self::set`] when it opens
/// the window and [`Self::clear`] when it is destroyed; `py_api` hands [`Self::get`] to the
/// commands. Managed once with `app.manage(SessionSlot::default())`. There is at most one
/// settings window.
#[derive(Default)]
pub struct SessionSlot(Mutex<Option<Arc<SettingsSession>>>);

impl SessionSlot {
    pub fn set(&self, session: SettingsSession) -> Arc<SettingsSession> {
        let session = Arc::new(session);
        *self.0.lock().expect("session slot") = Some(Arc::clone(&session));
        session
    }

    pub fn clear(&self) {
        *self.0.lock().expect("session slot") = None;
    }

    pub fn get(&self) -> Option<Arc<SettingsSession>> {
        self.0.lock().expect("session slot").clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_session_replaces_the_old_one() {
        let slot = SessionSlot::default();
        assert!(slot.get().is_none());
        slot.set(SettingsSession {
            base: "http://a".into(),
            key: "a".into(),
            tab: String::new(),
        });
        let second = slot.set(SettingsSession {
            base: "http://b".into(),
            key: "b".into(),
            tab: "setup".into(),
        });
        assert_eq!(slot.get().as_deref(), Some(&*second));
        slot.clear();
        assert!(slot.get().is_none());
    }

    #[test]
    fn jobs_start_idle() {
        let jobs = JobState::default();
        assert_eq!(*jobs.job.lock().unwrap(), json!({"running": false}));
    }
}
