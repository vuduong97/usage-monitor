//! The updater: state, staging, applying (`appupdate.Updater`).
//!
//! One per process. Everything it keeps lives in `<config dir>/app-update/` (`ready.json`,
//! `swap.sh`, `apply.log`, `failed`; a lock file beside it guards the download). Only **staging**
//! is automatic; applying needs the user's click (the `ShellCmd::AppUpdateApply` path in the
//! app). A failed swap leaves a `failed` marker so the same version is not offered in a loop.
//! `apply` refuses while [`clawd_store::paths::ota_busy`] (a firmware upload is running).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use clawd_config::CommandRunner;
use clawd_contracts::{AppPaths, Clock, Env, SystemClock};
use serde::Serialize;

use crate::source::Source;

/// `appupdate.Staged`: the verified update waiting to be applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Staged {
    pub version: String,
    /// `Clawd.app` (macOS) or the installer (Windows).
    pub path: PathBuf,
    /// Of the installer / the zip it came from.
    pub sha256: String,
    pub notes: String,
    pub page: String,
}

/// `Updater.state()`: what the settings page and the tray read. JSON keys as Python's dict.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UpdaterState {
    /// `idle | checking | downloading | ready | error`.
    pub status: String,
    pub version: String,
    pub notes: String,
    pub page: String,
    pub progress: f64,
    pub error: String,
    pub checked: f64,
    pub current: String,
    #[serde(rename = "canApply")]
    pub can_apply: bool,
    pub why: String,
}

/// `appupdate.running_bundle(executable)`: `/Applications/Clawd.app` for the packaged macOS
/// app (the executable is `<x>.app/Contents/MacOS/Clawd`), else `None`.
pub fn running_bundle(executable: Option<&Path>) -> Option<PathBuf> {
    todo!("phase 10")
}

/// `appupdate.can_apply(frozen, system, executable)`: `(ok, why not)`. Only an installed,
/// packaged app (not running from `target/`) replaces itself; on macOS not from
/// AppTranslocation or a mounted volume, and the folder must be writable.
pub fn can_apply(
    packaged: Option<bool>,
    system: Option<&str>,
    executable: Option<&Path>,
) -> (bool, String) {
    todo!("phase 10")
}

/// Everything `Updater.__init__` takes as keyword arguments, injectable for tests.
///
/// `#[non_exhaustive]`: build it with [`UpdaterOptions::new`] and assign the fields you override.
#[derive(Clone)]
#[non_exhaustive]
pub struct UpdaterOptions {
    /// For `ota_busy` (apply refuses during a firmware upload) and the default stage dir.
    pub paths: AppPaths,
    /// Stage dir; default `paths.app_update_stage_dir()`.
    pub stage: PathBuf,
    /// The running version ([`clawd_contracts::APP_VERSION`]).
    pub current: String,
    pub runner: Arc<dyn CommandRunner>,
    /// Wall clock.
    pub now: Arc<dyn Clock>,
    pub system: Option<String>,
    pub arch: Option<String>,
    pub packaged: Option<bool>,
    pub executable: Option<PathBuf>,
}

impl UpdaterOptions {
    /// Real machine defaults for `paths`: stage dir `paths.app_update_stage_dir()`, version
    /// [`clawd_contracts::APP_VERSION`], the system clock, platform/arch/executable detected
    /// at use time (`None`).
    pub fn new(paths: AppPaths, runner: Arc<dyn CommandRunner>) -> Self {
        Self {
            stage: paths.app_update_stage_dir(),
            paths,
            current: clawd_contracts::APP_VERSION.to_owned(),
            runner,
            now: Arc::new(SystemClock),
            system: None,
            arch: None,
            packaged: None,
            executable: None,
        }
    }
}

/// `appupdate.Updater`. Shared (`Arc`): the tray's background loop and the settings commands
/// use the same one, hence `&self` everywhere.
pub struct Updater {
    _private: (),
}

impl Updater {
    /// `Updater(source, stage, current, ...)`; `source` `None` = updates not offered.
    pub fn new(source: Option<Source>, opts: UpdaterOptions) -> Self {
        todo!("phase 10")
    }

    /// `Updater.state()`.
    pub fn state(&self) -> UpdaterState {
        todo!("phase 10")
    }

    /// `Updater.staged()`: the verified update waiting, `None` when there is none or it is stale
    /// (older than what runs: the stage is cleaned). With `ready.json` from a previous run it is
    /// trusted only when the `.minisig` verifies again.
    pub fn staged(&self) -> Option<Staged> {
        todo!("phase 10")
    }

    /// `Updater.clean()`: remove the stage dir (also leftovers of the Python app).
    pub fn clean(&self) {
        todo!("phase 10")
    }

    /// `Updater.check(force)`: ask GitHub; when a newer release exists and this install can apply
    /// it, download and stage it (hash kept in memory); returns the state. A second call while one
    /// runs returns at once.
    pub async fn check(&self, force: bool) -> UpdaterState {
        todo!("phase 10")
    }

    /// `Updater.check_async(force)`: `check` on a background task.
    pub fn check_async(self: &Arc<Self>, force: bool) {
        todo!("phase 10")
    }

    /// `Updater.apply(pid)`: start the replacement. Windows: verify the installer against the
    /// in-memory SHA-256, spawn it **detached** (`DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP`)
    /// with `/SILENT /SUPPRESSMSGBOXES /NORESTART`. macOS: unzip, [`crate::codesign::check_bundle`]
    /// again, write `swap.sh`, spawn `/bin/sh swap.sh <pid> <new> <app> <stage> <version>` in a
    /// new session. `(true, "")`: the caller must quit the app now (`engine.stop()` then exit);
    /// `(false, reason)` otherwise. `pid` default: this process.
    pub fn apply(&self, pid: Option<u32>) -> (bool, String) {
        todo!("phase 10")
    }
}

/// `appupdate.updater()`: the updater this install uses (repo from env/config); with a bad
/// repo name it logs and returns one without a source.
pub fn updater(paths: &AppPaths, env: &Env) -> Updater {
    todo!("phase 10")
}
