//! The updater: state, staging, applying (`appupdate.Updater`).
//!
//! One per process. Everything it keeps lives in `<config dir>/app-update/` (`ready.json`,
//! `swap.sh`, `apply.log`, `failed`; a lock file beside it guards the download). Only **staging**
//! is automatic; applying needs the user's click (the `ShellCmd::AppUpdateApply` path in the
//! app). A failed swap leaves a `failed` marker so the same version is not offered in a loop.
//! `apply` refuses while [`clawd_store::paths::ota_busy`] (a firmware upload is running).
//!
//! Trust (new, Q2): the SHA-256 the installer is checked against comes from a `SHA256SUMS` whose
//! minisign signature verified. It is kept in memory from the download on. `ready.json` is only
//! believed while that in-memory value agrees; after a restart the stored `SHA256SUMS` +
//! `.minisig` are verified again and must list the same hash.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use clawd_config::{CommandRunner, SystemRunner};
use clawd_contracts::{
    AppPaths, AppRelease, Clock, Env, SystemClock, UpdateError, INNO_SILENT_ARGS, MINISIGN_PUBKEY,
    SUMS_NAME, SUMS_SIG_NAME,
};
use serde::Serialize;
use serde_json::{json, Value};

use crate::codesign::{bundle_team, check_bundle, mac_swap_script, NO_TEAM};
use crate::launch::{LaunchKind, LaunchSpec, Launcher, SystemLauncher};
use crate::minisig::verify_sums;
use crate::source::{from_settings, Source, RECHECK};
use crate::version::{asset_name, is_newer, parse_sums, sha256_file, this_platform};

/// `appupdate.LOCK_STALE` (seconds): a download lock not touched for this long is dead.
pub const LOCK_STALE: f64 = 120.0;

/// `timeout=300` of the `ditto -x -k` unpack.
const UNPACK_TIMEOUT: Duration = Duration::from_secs(300);

const FAILED_BEFORE: &str =
    "Lần thay app trước không thành công. Hãy tải bản mới từ trang phát hành và cài đè.";
const FAILED_BEFORE_SHORT: &str =
    "Lần thay app trước không thành công. Hãy tải bản mới từ trang phát hành.";

fn log(msg: &str) {
    tracing::info!(target: "app-update", "{msg}");
}

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
    /// `idle | checking | downloading | ready | error | current | available`.
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
    let exe = match executable {
        Some(path) if !path.as_os_str().is_empty() => path.to_path_buf(),
        _ => std::env::current_exe().ok()?,
    };
    let exe = std::fs::canonicalize(&exe).unwrap_or(exe);
    let parents: Vec<&Path> = exe.ancestors().skip(1).collect();
    if parents.len() > 3
        && parents[2].extension().is_some_and(|e| e == "app")
        && parents[1].file_name().is_some_and(|n| n == "Contents")
    {
        return Some(parents[2].to_path_buf());
    }
    None
}

/// `os.access(dir, os.W_OK)`: can a file be created in `dir`?
fn dir_writable(dir: &Path) -> bool {
    if !dir.is_dir() {
        return false;
    }
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    let probe = dir.join(format!(".clawd-write-probe-{}-{nanos}", std::process::id()));
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
    {
        Ok(file) => {
            drop(file);
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

/// `appupdate.can_apply(frozen, system, executable)`: `(ok, why not)`. Only an installed,
/// packaged app (not running from `target/`) replaces itself; on macOS not from
/// AppTranslocation or a mounted volume, and the folder must be writable.
/// `packaged` `None` asks [`clawd_store::paths::is_packaged`].
pub fn can_apply(
    packaged: Option<bool>,
    system: Option<&str>,
    executable: Option<&Path>,
) -> (bool, String) {
    let system = match system.filter(|s| !s.is_empty()) {
        Some(system) => system.to_owned(),
        None => this_platform().0,
    };
    let packaged = packaged.unwrap_or_else(clawd_store::paths::is_packaged);
    if !packaged {
        return (false, "Bản chạy từ mã nguồn không tự cập nhật.".to_owned());
    }
    if system == "windows" {
        return (true, String::new());
    }
    if system != "mac" {
        return (
            false,
            "Hệ điều hành này chưa hỗ trợ tự cập nhật.".to_owned(),
        );
    }
    let Some(app) = running_bundle(executable) else {
        return (false, "Không tìm thấy Clawd.app đang chạy.".to_owned());
    };
    let text = app.to_string_lossy().replace('\\', "/");
    if text.contains("/AppTranslocation/") || text.starts_with("/Volumes/") {
        return (
            false,
            "Hãy kéo Clawd vào thư mục Applications rồi mở lại để app tự cập nhật.".to_owned(),
        );
    }
    let parent = app.parent().map(Path::to_path_buf).unwrap_or_default();
    if !dir_writable(&parent) {
        return (
            false,
            format!(
                "Không có quyền ghi vào {}; hãy cài bản mới bằng tay.",
                parent.display()
            ),
        );
    }
    (true, String::new())
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
    /// Starts the installer / swap script (default: [`SystemLauncher`]; tests record instead).
    pub launcher: Arc<dyn Launcher>,
    /// The minisign key a stored `SHA256SUMS.minisig` is re-verified with after a restart
    /// (default [`MINISIGN_PUBKEY`]; a [`Source`] has its own, see `Source::with_public_key`).
    pub public_key: String,
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
            launcher: Arc::new(SystemLauncher),
            public_key: MINISIGN_PUBKEY.to_owned(),
        }
    }
}

/// The changing part of `Updater._st`.
#[derive(Debug, Clone)]
struct Fields {
    status: String,
    version: String,
    notes: String,
    page: String,
    progress: f64,
    error: String,
    checked: f64,
}

impl Fields {
    fn idle() -> Self {
        Self {
            status: "idle".to_owned(),
            version: String::new(),
            notes: String::new(),
            page: String::new(),
            progress: 0.0,
            error: String::new(),
            checked: 0.0,
        }
    }
}

/// What this process verified when it downloaded: not read back from disk.
#[derive(Debug, Clone)]
struct Verified {
    version: String,
    /// File name inside the stage dir (`Clawd.app` or the installer).
    file: String,
    sha256: String,
}

/// Resets the "one check at a time" flag when a check ends.
struct WorkGuard<'a>(&'a AtomicBool);

impl<'a> WorkGuard<'a> {
    fn try_acquire(flag: &'a AtomicBool) -> Option<Self> {
        flag.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .ok()
            .map(|_| Self(flag))
    }
}

impl Drop for WorkGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

/// Removes the cross-process download lock when a check ends.
struct LockGuard(PathBuf);

impl Drop for LockGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// `appupdate.Updater`. Shared (`Arc`): the tray's background loop and the settings commands
/// use the same one, hence `&self` everywhere.
pub struct Updater {
    source: Option<Source>,
    paths: AppPaths,
    stage: PathBuf,
    current: String,
    runner: Arc<dyn CommandRunner>,
    now: Arc<dyn Clock>,
    launcher: Arc<dyn Launcher>,
    public_key: String,
    system: String,
    arch: String,
    packaged: Option<bool>,
    executable: Option<PathBuf>,
    /// The running app's Team ID (`Some("")`: none), read once.
    team: Mutex<Option<String>>,
    /// When this process last asked GitHub.
    tried: Mutex<f64>,
    fields: Mutex<Fields>,
    working: AtomicBool,
    verified: Mutex<Option<Verified>>,
}

fn io_err(e: std::io::Error) -> UpdateError {
    UpdateError(e.to_string())
}

/// Modification time as epoch seconds.
fn mtime(path: &Path) -> Option<f64> {
    let modified = std::fs::metadata(path).ok()?.modified().ok()?;
    modified
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs_f64())
}

impl Updater {
    /// `Updater(source, stage, current, ...)`; `source` `None` = updates not offered.
    pub fn new(source: Option<Source>, opts: UpdaterOptions) -> Self {
        let (system, arch) = match opts.system.filter(|s| !s.is_empty()) {
            Some(system) => (system, opts.arch.unwrap_or_default()),
            None => this_platform(),
        };
        Self {
            source,
            paths: opts.paths,
            stage: opts.stage,
            current: opts.current,
            runner: opts.runner,
            now: opts.now,
            launcher: opts.launcher,
            public_key: opts.public_key,
            system,
            arch,
            packaged: opts.packaged,
            executable: opts.executable,
            team: Mutex::new(None),
            tried: Mutex::new(0.0),
            fields: Mutex::new(Fields::idle()),
            working: AtomicBool::new(false),
            verified: Mutex::new(None),
        }
    }

    fn now(&self) -> f64 {
        self.now.now()
    }

    // ---- what the page and the tray read ---------------------------------------
    /// `Updater._can()`: `can_apply` plus, on macOS, "the running app has a Team ID".
    fn can(&self) -> (bool, String) {
        let (ok, why) = can_apply(self.packaged, Some(&self.system), self.executable.as_deref());
        if ok && self.system == "mac" {
            let cached = self.team.lock().expect("team lock").clone();
            let team = match cached {
                Some(team) => team,
                None => {
                    // asked once: codesign is not free
                    let team = running_bundle(self.executable.as_deref())
                        .and_then(|app| bundle_team(&app, self.runner.as_ref()).ok().flatten())
                        .unwrap_or_default();
                    *self.team.lock().expect("team lock") = Some(team.clone());
                    team
                }
            };
            if team.is_empty() {
                return (false, NO_TEAM.to_owned());
            }
        }
        (ok, why)
    }

    /// `Updater.state()`.
    pub fn state(&self) -> UpdaterState {
        let (mut ok, why) = self.can();
        let mut st = self.fields.lock().expect("state lock").clone();
        if st.status != "checking" && st.status != "downloading" {
            let staged = self.staged();
            if staged.is_none() && self.held_elsewhere() {
                st.status = "downloading".to_owned();
                st.progress = 0.0; // the other process is on it
            }
            if let Some(s) = staged {
                st.version = s.version.clone();
                st.notes = s.notes;
                st.page = s.page;
                if self.failed() == s.version {
                    st.status = "error".to_owned();
                    st.error = FAILED_BEFORE.to_owned();
                    ok = false;
                } else {
                    st.status = "ready".to_owned();
                    st.error = String::new();
                }
            }
        }
        UpdaterState {
            status: st.status,
            version: st.version,
            notes: st.notes,
            page: st.page,
            progress: st.progress,
            error: st.error,
            checked: st.checked,
            current: self.current.clone(),
            can_apply: ok,
            why,
        }
    }

    fn set(&self, change: impl FnOnce(&mut Fields)) {
        change(&mut self.fields.lock().expect("state lock"));
    }

    // ---- the staging directory ---------------------------------------------------
    /// The version a failed swap left in `failed`, or `""`.
    fn failed(&self) -> String {
        std::fs::read_to_string(self.stage.join("failed"))
            .map(|t| t.trim().to_owned())
            .unwrap_or_default()
    }

    /// Re-check a `ready.json` the in-memory value does not vouch for (the app restarted, or
    /// another process staged it): the stored `SHA256SUMS` must verify under minisign and list
    /// `sha256` for this version's file.
    fn trusted_on_disk(&self, version: &str, sha256: &str) -> bool {
        let Ok(sums) = std::fs::read(self.stage.join(SUMS_NAME)) else {
            return false;
        };
        let Ok(sig) = std::fs::read_to_string(self.stage.join(SUMS_SIG_NAME)) else {
            return false;
        };
        if verify_sums(&sums, &sig, &self.public_key).is_err() {
            return false;
        }
        let Some(name) = asset_name(version, &self.system, &self.arch) else {
            return false;
        };
        parse_sums(&String::from_utf8_lossy(&sums))
            .get(&name)
            .is_some_and(|digest| digest == sha256)
    }

    /// `Updater.staged()`: the verified update waiting, `None` when there is none or it is stale
    /// (older than what runs: the stage is cleaned). With `ready.json` from a previous run it is
    /// trusted only when the `.minisig` verifies again.
    pub fn staged(&self) -> Option<Staged> {
        let parsed = std::fs::read_to_string(self.stage.join("ready.json"))
            .ok()
            .and_then(|text| serde_json::from_str::<Value>(&text).ok())
            .and_then(|d| {
                let text = |key: &str| d.get(key).map(value_text);
                Some((
                    text("version")?,
                    text("sha256")?,
                    text("notes").unwrap_or_default(),
                    text("page").unwrap_or_default(),
                    text("path")?,
                ))
            });
        let Some((version, disk_sha, notes, page, file)) = parsed else {
            if self.stage.exists() && !self.lockfile().exists() {
                self.clean(); // the swap script removed ready.json: its leftovers go too
            }
            return None;
        };
        let path = self.stage.join(&file);
        if !is_newer(&version, &self.current) || !path.exists() {
            if !self.held_elsewhere() {
                self.clean(); // applied (or older than what runs): tidy up
            }
            return None;
        }
        let remembered = self
            .verified
            .lock()
            .expect("verified lock")
            .clone()
            .filter(|v| v.version == version && v.file == file);
        let sha256 = match remembered {
            Some(v) => v.sha256,
            None if self.trusted_on_disk(&version, &disk_sha) => {
                *self.verified.lock().expect("verified lock") = Some(Verified {
                    version: version.clone(),
                    file,
                    sha256: disk_sha.clone(),
                });
                disk_sha
            }
            None => {
                log(&format!("{version}: staged files carry no valid signature, dropped"));
                if !self.held_elsewhere() {
                    self.clean();
                }
                return None;
            }
        };
        Some(Staged {
            version,
            path,
            sha256,
            notes,
            page,
        })
    }

    /// `Updater.clean()`: remove the stage dir (also leftovers of the Python app).
    pub fn clean(&self) {
        let _ = std::fs::remove_dir_all(&self.stage);
        *self.verified.lock().expect("verified lock") = None;
    }

    // ---- one downloader at a time across the tray and the settings window ---------
    fn lockfile(&self) -> PathBuf {
        let mut name = self
            .stage
            .file_name()
            .map(|n| n.to_os_string())
            .unwrap_or_default();
        name.push(".lock");
        self.stage.with_file_name(name)
    }

    /// Another process is downloading: its lock was touched in the last [`LOCK_STALE`] s.
    /// (No pid check: probing a pid is not portable.)
    fn held_elsewhere(&self) -> bool {
        let lock = self.lockfile();
        let Some(modified) = mtime(&lock) else {
            return false;
        };
        let Ok(owner) = std::fs::read_to_string(&lock) else {
            return false;
        };
        self.now() - modified < LOCK_STALE && owner.trim() != std::process::id().to_string()
    }

    fn acquire(&self) -> bool {
        let lock = self.lockfile();
        if let Some(parent) = lock.parent() {
            if std::fs::create_dir_all(parent).is_err() {
                return false;
            }
        }
        for _ in 0..2 {
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&lock)
            {
                Ok(mut file) => {
                    return file
                        .write_all(std::process::id().to_string().as_bytes())
                        .is_ok();
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    let Some(modified) = mtime(&lock) else {
                        continue; // it vanished meanwhile: try to take it
                    };
                    if self.held_elsewhere() || self.now() - modified < LOCK_STALE {
                        return false;
                    }
                    let _ = std::fs::remove_file(&lock); // a dead downloader's
                }
                Err(_) => return false,
            }
        }
        false
    }

    /// The downloaded file becomes the staged update.
    fn stage_download(&self, rel: &AppRelease, file: &Path) -> Result<(), UpdateError> {
        let path = if self.system == "mac" {
            let unpack = self.stage.join("unpack");
            let _ = std::fs::remove_dir_all(&unpack);
            std::fs::create_dir_all(&unpack).map_err(io_err)?;
            let out = self
                .runner
                .run(
                    "ditto",
                    &[
                        "-x",
                        "-k",
                        &file.to_string_lossy(),
                        &unpack.to_string_lossy(),
                    ],
                    UNPACK_TIMEOUT,
                )
                .map_err(io_err)?;
            let apps: Vec<PathBuf> = std::fs::read_dir(&unpack)
                .map_err(io_err)?
                .filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e == "app"))
                .collect();
            if out.status != 0 || apps.len() != 1 {
                return Err(UpdateError("không giải nén được bản mới".to_owned()));
            }
            check_bundle(
                &apps[0],
                &rel.version,
                running_bundle(self.executable.as_deref()).as_deref(),
                self.runner.as_ref(),
            )?;
            let finished = self.stage.join("Clawd.app");
            let _ = std::fs::remove_dir_all(&finished);
            std::fs::rename(&apps[0], &finished).map_err(io_err)?;
            let _ = std::fs::remove_dir_all(&unpack);
            let _ = std::fs::remove_file(file);
            finished
        } else {
            file.to_path_buf()
        };
        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let meta = json!({
            "version": rel.version,
            "sha256": rel.sha256,
            "path": file_name,
            "notes": rel.notes,
            "page": rel.page,
        });
        let tmp = self.stage.join("ready.json.tmp");
        std::fs::write(&tmp, meta.to_string()).map_err(io_err)?;
        std::fs::rename(&tmp, self.stage.join("ready.json")).map_err(io_err)?;
        *self.verified.lock().expect("verified lock") = Some(Verified {
            version: rel.version.clone(),
            file: file_name,
            sha256: rel.sha256.clone(),
        });
        Ok(())
    }

    // ---- checking and downloading ------------------------------------------------
    /// `Updater.check(force)`: ask GitHub; when a newer release exists and this install can apply
    /// it, download and stage it (hash kept in memory); returns the state. A second call while one
    /// runs returns at once.
    pub async fn check(&self, force: bool) -> UpdaterState {
        let Some(source) = self.source.as_ref() else {
            return self.state();
        };
        let Some(_work) = WorkGuard::try_acquire(&self.working) else {
            return self.state();
        };
        if !self.acquire() {
            return self.state(); // the other process is checking or downloading
        }
        let _lock = LockGuard(self.lockfile());

        {
            let mut tried = self.tried.lock().expect("tried lock");
            if !force && *tried != 0.0 && self.now() - *tried < RECHECK {
                drop(tried);
                return self.state(); // failures count too: GitHub allows 60 anonymous asks an hour
            }
            *tried = self.now();
        }
        self.set(|f| {
            f.status = "checking".to_owned();
            f.error = String::new();
            f.progress = 0.0;
        });
        let rel = match source
            .latest(&self.current, Some(&self.system), Some(&self.arch))
            .await
        {
            Ok(rel) => rel,
            Err(e) => {
                self.set(|f| {
                    f.status = "error".to_owned();
                    f.error = format!("Không hỏi được GitHub: {e}");
                });
                log(&format!("check failed: {e}"));
                return self.state();
            }
        };
        let checked = self.now();
        self.set(|f| f.checked = checked);
        let Some(rel) = rel else {
            self.set(|f| {
                f.status = "current".to_owned();
                f.version = String::new();
                f.notes = String::new();
                f.page = String::new();
            });
            return self.state();
        };
        if self.staged().is_some_and(|s| s.version == rel.version) {
            self.set(|f| f.status = "idle".to_owned());
            return self.state();
        }
        self.set(|f| {
            f.version = rel.version.clone();
            f.notes = rel.notes.clone();
            f.page = rel.page.clone();
        });
        let (ok, why) = self.can();
        if !ok {
            log(&format!("{} is out; not applied here: {why}", rel.version));
            self.set(|f| f.status = "available".to_owned());
            return self.state();
        }
        if self.failed() == rel.version {
            self.set(|f| {
                f.status = "error".to_owned();
                f.error = FAILED_BEFORE_SHORT.to_owned();
            });
            return self.state();
        }
        if let Err(e) = self.fetch(source, &rel).await {
            self.set(|f| {
                f.status = "error".to_owned();
                f.error = format!("Không tải được bản {}: {e}", rel.version);
            });
            log(&format!("{}: {e}", rel.version));
            let _ = std::fs::remove_dir_all(self.stage.join("unpack"));
            return self.state();
        }
        self.set(|f| f.status = "idle".to_owned());
        log(&format!("{} downloaded and verified", rel.version));
        self.state()
    }

    async fn fetch(&self, source: &Source, rel: &AppRelease) -> Result<(), UpdateError> {
        self.clean(); // an older staged download is of no use now
        std::fs::create_dir_all(&self.stage).map_err(io_err)?;
        self.set(|f| {
            f.status = "downloading".to_owned();
            f.progress = 0.0;
        });
        let file = self.stage.join(&rel.name);
        let beat = Mutex::new(self.now());
        let progress = |got: u64, total: u64| {
            let fraction = (got as f64 / total.max(1) as f64 * 1000.0).round() / 1000.0;
            self.set(|f| f.progress = fraction);
            let mut last = beat.lock().expect("beat lock");
            if self.now() - *last > 5.0 {
                *last = self.now(); // tell the other process we are alive
                if let Ok(lock) = std::fs::OpenOptions::new()
                    .write(true)
                    .open(self.lockfile())
                {
                    let _ = lock.set_modified(SystemTime::now());
                }
            }
        };
        source.download(rel, &file, Some(&progress)).await?;
        let (sums, signature) = source.signed_sums(&rel.version).ok_or_else(|| {
            UpdateError(format!("{SUMS_NAME} đã xác minh không còn trong bộ nhớ"))
        })?;
        std::fs::write(self.stage.join(SUMS_NAME), sums).map_err(io_err)?;
        std::fs::write(self.stage.join(SUMS_SIG_NAME), signature).map_err(io_err)?;
        self.stage_download(rel, &file)
    }

    /// `Updater.check_async(force)`: `check` on a background task.
    pub fn check_async(self: &Arc<Self>, force: bool) {
        let this = Arc::clone(self);
        match tokio::runtime::Handle::try_current() {
            Ok(handle) => {
                handle.spawn(async move {
                    this.check(force).await;
                });
            }
            Err(_) => {
                std::thread::Builder::new()
                    .name("clawd-appupdate".to_owned())
                    .spawn(move || {
                        if let Ok(rt) = tokio::runtime::Builder::new_current_thread()
                            .enable_all()
                            .build()
                        {
                            rt.block_on(this.check(force));
                        }
                    })
                    .ok();
            }
        }
    }

    // ---- applying ----------------------------------------------------------------
    /// `Updater.apply(pid)`: start the replacement. Windows: verify the installer against the
    /// in-memory SHA-256, spawn it **detached** (`DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP`)
    /// with `/SILENT /SUPPRESSMSGBOXES /NORESTART`. macOS: unzip, [`crate::codesign::check_bundle`]
    /// again, write `swap.sh`, spawn `/bin/sh swap.sh <pid> <new> <app> <stage> <version>` in a
    /// new session. `(true, "")`: the caller must quit the app now (`engine.stop()` then exit);
    /// `(false, reason)` otherwise. `pid` default: this process.
    pub fn apply(&self, pid: Option<u32>) -> (bool, String) {
        let (ok, why) = self.can();
        if !ok {
            return (false, why);
        }
        let Some(staged) = self.staged() else {
            return (false, "Chưa có bản nào được tải xong.".to_owned());
        };
        if clawd_store::paths::ota_busy(&self.paths, None) {
            return (
                false,
                "Đang cập nhật firmware cho đồng hồ; hãy đợi xong rồi khởi động lại.".to_owned(),
            );
        }
        let pid = pid.unwrap_or_else(std::process::id);
        match self.start_replacement(&staged, pid) {
            Ok(None) => {}
            Ok(Some(refusal)) => return (false, refusal),
            Err(e) => {
                log(&format!("apply failed: {e}"));
                return (false, format!("Không bắt đầu được cập nhật: {e}"));
            }
        }
        log(&format!("applying {}", staged.version));
        (true, String::new())
    }

    /// The platform half of `apply`: `Ok(Some(why))` is a refusal, `Err` a failure to start.
    fn start_replacement(&self, staged: &Staged, pid: u32) -> Result<Option<String>, UpdateError> {
        if self.system == "windows" {
            if sha256_file(&staged.path).map_err(io_err)? != staged.sha256 {
                self.clean();
                return Ok(Some(
                    "File cài đặt đã bị đổi, đã bỏ. Hãy kiểm tra lại bản mới.".to_owned(),
                ));
            }
            self.launcher
                .spawn_detached(&LaunchSpec {
                    program: staged.path.clone(),
                    args: INNO_SILENT_ARGS.iter().map(|a| (*a).to_owned()).collect(),
                    kind: LaunchKind::Installer,
                })
                .map_err(io_err)?;
            return Ok(None);
        }
        let app = running_bundle(self.executable.as_deref())
            .ok_or_else(|| UpdateError("Không tìm thấy Clawd.app đang chạy.".to_owned()))?;
        // again: it sat on disk since the download
        check_bundle(
            &staged.path,
            &staged.version,
            Some(&app),
            self.runner.as_ref(),
        )?;
        let _ = std::fs::remove_file(self.stage.join("failed"));
        let script = self.stage.join("swap.sh");
        std::fs::write(&script, mac_swap_script()).map_err(io_err)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700))
                .map_err(io_err)?;
        }
        self.launcher
            .spawn_detached(&LaunchSpec {
                program: PathBuf::from("/bin/sh"),
                args: vec![
                    script.to_string_lossy().into_owned(),
                    pid.to_string(),
                    staged.path.to_string_lossy().into_owned(),
                    app.to_string_lossy().into_owned(),
                    self.stage.to_string_lossy().into_owned(),
                    staged.version.clone(),
                ],
                kind: LaunchKind::SwapScript,
            })
            .map_err(io_err)?;
        Ok(None)
    }
}

/// `str(value)` of a `ready.json` field.
fn value_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => "None".to_owned(),
        Value::Bool(true) => "True".to_owned(),
        Value::Bool(false) => "False".to_owned(),
        other => other.to_string(),
    }
}

/// `appupdate.updater()`: the updater this install uses (repo from env/config); with a bad
/// repo name it logs and returns one without a source.
pub fn updater(paths: &AppPaths, env: &Env) -> Updater {
    let opts = UpdaterOptions::new(paths.clone(), Arc::new(SystemRunner));
    match from_settings(paths, env) {
        Ok(source) => Updater::new(Some(source), opts),
        Err(e) => {
            log(&e.to_string());
            Updater::new(None, opts)
        }
    }
}
