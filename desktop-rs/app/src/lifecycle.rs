//! The app's life (`tray.main`, `tray.Tray`, `cli._run_headless`): startup, the tray shell with
//! its loops and the `ShellCmd` handler, the settings-only mode, quitting.
//!
//! One process, tray-only, no window at start (`app.windows: []`). The engine starts after the
//! tray shows (tray <= 1 s). `ExitRequested` is stopped unless it carries an exit code, so closing
//! the last window never quits; "Thoát" stops the engine (idempotent) and calls `app.exit(0)`.
//!
//! The loops (refresh 3 s, feed check, app update check, agents rescan) run on Tauri's async
//! runtime under [`spawn_supervised`]: a panic is logged and the loop restarts after a backoff.
//! Menu changes go through `run_on_main_thread` (AppKit wants the main thread).

use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use clawd_agents::Registry;
use clawd_appupdate::Updater;
use clawd_clock::{ClockApi, ClockStore};
use clawd_contracts::{ClockSource, ClockTarget, Env, StopRx, APP_NAME, APP_VERSION};
use clawd_store::{firstrun, prefs};
use serde_json::json;
use tauri::{AppHandle, Manager, RunEvent};
use tokio::sync::{mpsc, watch};

use crate::dialogs;
use crate::engine::{
    another_daemon_running, guard, sleep_or_stop, spawn_supervised, stopped, EngineConfig,
    EngineHandle,
};
use crate::state::{lock, AppState, JobState, Services, SettingsSession, ShellCmd};
use crate::tray::{self, TrayChange, TrayModel, TrayState};
use crate::windows;

/// `tray.REFRESH`.
pub const REFRESH: Duration = Duration::from_secs(3);
/// `tray.FEED_FIRST`: first firmware feed check after start (s).
pub const FEED_FIRST: Duration = Duration::from_secs(60);
/// `tray.FEED_RETRY`: after a failed check.
pub const FEED_RETRY: Duration = Duration::from_secs(1800);
/// `tray.AGENT_SCAN`: look for newly installed coding agents this often.
pub const AGENT_SCAN: Duration = Duration::from_secs(600);
/// The first agent scan comes this soon.
pub const AGENT_FIRST: Duration = Duration::from_secs(2);
/// The working set is trimmed this long after start-up (`spike-decision.md`).
pub const STARTUP_TRIM_DELAY: Duration = Duration::from_secs(30);
/// Capacity of the settings -> shell channel.
const SHELL_QUEUE: usize = 32;
/// The notice of a second launch (`tray.py:541`).
pub const ALREADY_RUNNING: &str =
    "Clawd đang chạy rồi (xem biểu tượng trên thanh menu / khay hệ thống).";
/// What `Clawd run` says when the event port is taken (`cli.py:25`).
pub const HEADLESS_ALREADY_RUNNING: &str =
    "Another Clawd daemon is already running (event port in use).";

fn now_epoch() -> f64 {
    use clawd_contracts::Clock;
    clawd_contracts::SystemClock.now()
}

// =============================================================================
// Platform helpers
// =============================================================================

/// After a settings window is destroyed: give the pages back to Windows
/// (`SetProcessWorkingSetSize(-1, -1)`; `spike-decision.md`: it only helps the Task Manager
/// figure, at the price of page faults later). Nothing elsewhere.
pub fn trim_working_set() {
    #[cfg(windows)]
    {
        #[link(name = "kernel32")]
        extern "system" {
            fn GetCurrentProcess() -> isize;
            fn SetProcessWorkingSetSize(process: isize, min: usize, max: usize) -> i32;
        }
        // SAFETY: (SIZE_T)-1 for both limits asks Windows to trim the working set now.
        unsafe {
            SetProcessWorkingSetSize(GetCurrentProcess(), usize::MAX, usize::MAX);
        }
    }
}

/// Windows GUI executable: let a subcommand print into the terminal that started it
/// (`AttachConsole(ATTACH_PARENT_PROCESS)`). A redirected stdout/stderr is left as it is.
pub fn attach_parent_console() {
    #[cfg(windows)]
    {
        #[link(name = "kernel32")]
        extern "system" {
            fn AttachConsole(process_id: u32) -> i32;
            fn GetStdHandle(which: u32) -> isize;
            fn SetStdHandle(which: u32, handle: isize) -> i32;
            fn CreateFileW(
                name: *const u16,
                access: u32,
                share: u32,
                security: *const u8,
                disposition: u32,
                flags: u32,
                template: isize,
            ) -> isize;
        }
        const ATTACH_PARENT_PROCESS: u32 = u32::MAX;
        const STD_OUTPUT_HANDLE: u32 = -11i32 as u32;
        const STD_ERROR_HANDLE: u32 = -12i32 as u32;
        const GENERIC_WRITE: u32 = 0x4000_0000;
        const SHARE_READ_WRITE: u32 = 3;
        const OPEN_EXISTING: u32 = 3;
        let console: Vec<u16> = "CONOUT$".encode_utf16().chain(Some(0)).collect();
        // SAFETY: plain Win32 calls with valid arguments; `console` is NUL-terminated.
        unsafe {
            if AttachConsole(ATTACH_PARENT_PROCESS) == 0 {
                return; // no parent console (started from the Start menu, a service, ...)
            }
            for which in [STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
                let current = GetStdHandle(which);
                if current == 0 || current == -1 {
                    let out = CreateFileW(
                        console.as_ptr(),
                        GENERIC_WRITE,
                        SHARE_READ_WRITE,
                        std::ptr::null(),
                        OPEN_EXISTING,
                        0,
                        0,
                    );
                    if out != -1 {
                        SetStdHandle(which, out);
                    }
                }
            }
        }
    }
}

/// Panic hook: the message and the stack go to the log (`clawd-desktop.log`) before anything
/// else happens, so a panic inside a supervised loop leaves a trace (red team #12).
pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let stack = std::backtrace::Backtrace::force_capture();
        tracing::error!("panic: {info}\n{stack}");
        previous(info);
    }));
}

/// Set once by Ctrl+C / SIGTERM (`Clawd run`).
static INTERRUPTED: AtomicBool = AtomicBool::new(false);

/// Make Ctrl+C and SIGTERM set [`INTERRUPTED`] (no extra crate: two small FFI declarations).
fn install_interrupt_flag() {
    #[cfg(windows)]
    {
        #[link(name = "kernel32")]
        extern "system" {
            fn SetConsoleCtrlHandler(handler: extern "system" fn(u32) -> i32, add: i32) -> i32;
        }
        extern "system" fn on_ctrl(_kind: u32) -> i32 {
            INTERRUPTED.store(true, Ordering::SeqCst);
            1
        }
        // SAFETY: `on_ctrl` only stores to an atomic.
        unsafe {
            SetConsoleCtrlHandler(on_ctrl, 1);
        }
    }
    #[cfg(unix)]
    {
        extern "C" {
            fn signal(signum: i32, handler: extern "C" fn(i32)) -> usize;
        }
        extern "C" fn on_signal(_signum: i32) {
            INTERRUPTED.store(true, Ordering::SeqCst);
        }
        const SIGINT: i32 = 2;
        const SIGTERM: i32 = 15;
        // SAFETY: `on_signal` only stores to an atomic (async-signal-safe).
        unsafe {
            signal(SIGINT, on_signal);
            signal(SIGTERM, on_signal);
        }
    }
}

// =============================================================================
// Start-up
// =============================================================================

/// `tray.first_run`: autostart on, once; plugin copies older than this app refreshed and the hook
/// script rewritten, so an app update reaches the agents already connected. Nothing of any agent
/// is connected here: the setup wizard asks first. Never keeps the app from starting.
pub fn first_run(cfg: &EngineConfig, registry: &Registry) {
    let paths = &cfg.paths;
    let done = firstrun::load(paths)
        .map(|p| p.get_bool("first_run_done").unwrap_or(false))
        .unwrap_or(false);
    if !done {
        let autostart = clawd_store::autostart::Autostart::new(paths, cfg.names.clone());
        if autostart.supported() {
            match autostart.enable() {
                Ok(_) => tracing::info!("Autostart enabled"),
                Err(e) => tracing::info!("Autostart not enabled: {e}"),
            }
        }
        let marked = prefs::update_prefs(paths, |p| {
            let mut migrated = firstrun::migrate(p);
            migrated.0.insert("first_run_done".to_owned(), json!(true));
            *p = migrated;
        });
        if let Err(e) = marked {
            tracing::warn!("First run not recorded: {e}");
        }
    }
    guard("agent refresh", || {
        if !cfg!(windows) && paths.hook_script().exists() {
            if let Err(e) = clawd_agents::hooks::write_script(paths, &cfg.env, None, None) {
                tracing::warn!("Agent refresh: {e}");
            }
        }
        for id in clawd_agents::refresh_plugins(registry) {
            tracing::info!("Updated the {id} plugin");
        }
    });
}

/// The services every window needs, built once.
pub fn build_services(cfg: &EngineConfig) -> Arc<Services> {
    let updater = Arc::new(clawd_appupdate::updater(&cfg.paths, &cfg.env));
    let registry = Arc::new(Registry::new(cfg.agents.clone()));
    let feed = clawd_firmware::feed::from_settings(&cfg.paths, &cfg.env).map(Arc::new);
    let bundle = clawd_firmware::firmware::load_bundle(&cfg.env, None);
    Arc::new(Services::new(
        cfg.paths.clone(),
        cfg.names.clone(),
        updater,
        registry,
        feed,
        bundle,
        Arc::clone(&cfg.runner),
    ))
}

/// Everything built before the Tauri app exists.
struct Parts {
    engine: EngineHandle,
    services: Arc<Services>,
    state: AppState,
    shell_rx: mpsc::Receiver<ShellCmd>,
}

fn build_parts(cfg: EngineConfig) -> Parts {
    let services = build_services(&cfg);
    let (engine, status_rx) = EngineHandle::new(cfg);
    let (shell_tx, shell_rx) = mpsc::channel(SHELL_QUEUE);
    let state = AppState::new(
        engine.clone(),
        shell_tx,
        Arc::new(JobState::default()),
        status_rx,
        Arc::clone(&services),
    );
    Parts {
        engine,
        services,
        state,
        shell_rx,
    }
}

fn start_logging(cfg: &EngineConfig) {
    if let Err(e) = clawd_store::log::init_logging(&cfg.paths) {
        eprintln!("Could not open the log file: {e}");
    }
    install_panic_hook();
}

// =============================================================================
// What a ShellCmd can make the shell do
// =============================================================================

/// What a `ShellCmd` (and the menu) can make the shell do. Production: [`Shell`]; tests: a fake.
pub trait ShellBackend: Send + Sync {
    /// `finder.set_manual(text)`: true when it was a valid address.
    fn set_manual(&self, raw: &str) -> bool;
    /// `finder.retry()`: the same address, again now.
    fn retry_clock(&self);
    /// `finder.use(clock)`.
    fn use_clock(&self, clock: ClockTarget);
    /// `engine.poll_now()`.
    fn poll_now(&self);
    /// `Tray.refresh`.
    fn refresh(&self);
    /// `Tray._restart_for_update`.
    fn restart_for_update(&self);
    /// `Tray._rescan`.
    fn rescan_setup(&self);
}

/// `Tray._settings_cmds`: what the settings window asked for (`tray.py:476-505`).
pub fn apply_shell_cmd(backend: &dyn ShellBackend, cmd: ShellCmd) {
    match cmd {
        ShellCmd::Refresh => backend.refresh(),
        ShellCmd::AppUpdateApply => backend.restart_for_update(),
        ShellCmd::SetClock { host } => {
            if host.is_empty() {
                backend.retry_clock(); // "Tìm lại": same address, again now
            } else {
                backend.set_manual(&host);
            }
            backend.refresh();
        }
        ShellCmd::UseClock {
            name,
            host,
            port,
            version,
        } => {
            // Picked in the setup wizard (mDNS).
            let mut clock = ClockTarget::new(host, ClockSource::Mdns);
            clock.port = if port == 0 { 80 } else { port };
            clock.name = name;
            clock.version = version;
            backend.use_clock(clock);
            backend.refresh();
        }
        ShellCmd::UsageSource => backend.poll_now(),
        ShellCmd::Setup => backend.rescan_setup(),
    }
}

/// `Channel.send` without a tray (`settings_app.py:46-75`): write clock.json for the next tray
/// start to pick up. Anything else needs a tray and is dropped.
pub fn apply_standalone_cmd(store: &ClockStore, cmd: ShellCmd) {
    match cmd {
        ShellCmd::UseClock {
            name,
            host,
            port,
            version,
        } => store.save(&json!({"name": name, "host": host, "port": port, "version": version})),
        ShellCmd::SetClock { host } if !host.is_empty() => {
            store.save(&json!({"manual_host": host}));
        }
        ShellCmd::SetClock { .. } => {
            // "Tìm lại": keep the address, let mDNS replace it.
            let _ = store.unpin();
        }
        _ => {}
    }
}

// =============================================================================
// The tray shell
// =============================================================================

/// The tray process's state: the tray's attributes of Python's `Tray`, the engine and services.
pub struct Shell {
    app: AppHandle,
    engine: EngineHandle,
    services: Arc<Services>,
    env: Env,
    view: Mutex<TrayState>,
    stop_tx: watch::Sender<bool>,
}

type LoopFuture = Pin<Box<dyn Future<Output = ()> + Send>>;

impl Shell {
    fn new(app: AppHandle, engine: EngineHandle, services: Arc<Services>, env: Env) -> Arc<Self> {
        let (stop_tx, _) = clawd_contracts::new_stop();
        Arc::new(Self {
            app,
            engine,
            services,
            env,
            view: Mutex::new(TrayState::new()),
            stop_tx,
        })
    }

    fn stop_rx(&self) -> StopRx {
        self.stop_tx.subscribe()
    }

    /// Redraw what `change` says (main thread).
    fn redraw(&self, change: TrayChange, model: TrayModel) {
        if change.is_empty() {
            return;
        }
        let app = self.app.clone();
        let _ = self.app.run_on_main_thread(move || {
            let Some(tray_icon) = app.tray_by_id("clawd") else {
                return;
            };
            if let Some(dot) = change.icon {
                if let Ok(image) = tray::dot_image(dot) {
                    let _ = tray_icon.set_icon(Some(image));
                }
            }
            if change.tooltip {
                let _ = tray_icon.set_tooltip(Some(tray::tooltip(&model.head, &model.detail)));
            }
            if change.menu {
                if let Ok(menu) = tray::build_menu(&app, &model) {
                    let _ = tray_icon.set_menu(Some(menu));
                }
            }
        });
    }

    /// Change one attribute of the menu and redraw it when something differs.
    fn update_menu(&self, change: impl FnOnce(&mut TrayModel)) {
        let (changed, model) = {
            let mut view = lock(&self.view);
            let changed = view.update(change);
            (changed, view.model().clone())
        };
        self.redraw(
            TrayChange {
                menu: changed,
                ..TrayChange::default()
            },
            model,
        );
    }

    /// `Tray._sync_app_update`: the menu item follows the folder, so a download made by the
    /// settings window shows too.
    fn sync_app_update(&self) {
        let updater: &Updater = &self.services.updater;
        let Some(state) = guard("app update state", || updater.state()) else {
            return;
        };
        let new = if state.status == "ready" && state.can_apply {
            state.version
        } else {
            String::new()
        };
        let mut changed = false;
        self.update_menu(|m| {
            if m.app_new != new {
                changed = true;
                m.app_new = new.clone();
            }
        });
        if changed {
            if new.is_empty() {
                tracing::info!("No app update waiting");
            } else {
                tracing::info!("App update ready: {new}");
            }
        }
    }

    /// The agents' names for the "Tìm thấy …" item.
    fn agent_names(&self, ids: &[String]) -> String {
        ids.iter()
            .filter_map(|id| self.services.registry.get(id).map(|a| a.name()))
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// `Tray._open_window`.
    fn open_window(&self, tab: &str) {
        tauri::async_runtime::spawn(windows::open_settings(self.app.clone(), tab.to_owned()));
    }

    /// `Tray.open_agents`: offered once, the item goes whatever the answer.
    fn open_agents(&self) {
        let ids: Vec<String> = lock(&self.view).model().new_agents.clone();
        let refs: Vec<&str> = ids.iter().map(String::as_str).collect();
        if let Err(e) = firstrun::mark_seen(&self.services.paths, &refs) {
            tracing::warn!("Setup state: {e}");
        }
        self.update_menu(|m| {
            m.new_agents.clear();
            m.new_agent_names.clear();
        });
        self.open_window("agents");
    }

    /// `Tray._maybe_open_setup`: the wizard opens by itself once per app version while steps
    /// are left.
    fn maybe_open_setup(&self) {
        let paths = &self.services.paths;
        if !firstrun::should_open(paths, None) {
            return;
        }
        if let Err(e) = firstrun::mark_shown(paths) {
            tracing::warn!("Setup state: {e}");
            return;
        }
        tracing::info!("Opening the setup wizard");
        self.open_window("setup");
    }

    /// `Tray.manual`: the typed-address dialog (never opens by itself).
    fn manual(self: &Arc<Self>) {
        let shell = Arc::clone(self);
        std::thread::Builder::new()
            .name("clawd-ask".to_owned())
            .spawn(move || {
                guard("address dialog", || {
                    dialogs::manual(
                        |prompt| dialogs::ask_text(prompt, ""),
                        |text| shell.set_manual(text),
                        dialogs::alert,
                    );
                    shell.refresh();
                });
            })
            .ok();
    }

    /// A choice in the tray menu. Returns at once: the work runs elsewhere.
    fn on_menu(self: &Arc<Self>, id: &str) {
        match id {
            tray::id::SETTINGS => self.open_window(""),
            tray::id::OPEN_SETUP => self.open_window("setup"),
            tray::id::OPEN_UPDATE => self.open_window("update"),
            tray::id::OPEN_AGENTS => self.open_agents(),
            tray::id::MANUAL => self.manual(),
            tray::id::RESTART_UPDATE => {
                let shell = Arc::clone(self);
                std::thread::Builder::new()
                    .name("clawd-restart".to_owned())
                    .spawn(move || {
                        guard("restart for update", || shell.restart_for_update());
                    })
                    .ok();
            }
            tray::id::QUIT => self.quit(),
            _ => {}
        }
    }

    /// Left click on the icon: the panel under the icon (macOS), else the default item
    /// "Cài đặt…".
    fn on_left_click(&self) {
        #[cfg(target_os = "macos")]
        crate::popover::toggle(&self.app);
        #[cfg(not(target_os = "macos"))]
        self.open_window("");
    }

    /// `Tray.quit`: stop the loops and the engine, then leave. The engine's `stop()` blocks, so it
    /// runs on its own thread.
    pub fn quit(&self) {
        self.stop_tx.send_replace(true);
        let (engine, app) = (self.engine.clone(), self.app.clone());
        std::thread::Builder::new()
            .name("clawd-quit".to_owned())
            .spawn(move || {
                engine.stop();
                app.exit(0);
            })
            .ok();
    }

    /// The loops of `Tray.run`'s setup, each supervised, plus the `ShellCmd` handler.
    fn spawn_loops(self: &Arc<Self>, mut shell_rx: mpsc::Receiver<ShellCmd>) {
        let shell = Arc::clone(self);
        // Inside the runtime: `spawn_supervised` uses `tokio::spawn`.
        tauri::async_runtime::spawn(async move {
            let mut stop = shell.stop_rx();
            spawn_loop(&shell, "tray refresh", &stop, refresh_loop);
            spawn_loop(&shell, "firmware feed", &stop, feed_loop);
            spawn_loop(&shell, "app update", &stop, app_update_loop);
            spawn_loop(&shell, "agent scan", &stop, agent_loop);
            loop {
                tokio::select! {
                    cmd = shell_rx.recv() => {
                        let Some(cmd) = cmd else { return };
                        let shell = Arc::clone(&shell);
                        // A command may touch files and the keychain: off the async threads.
                        drop(tokio::task::spawn_blocking(move || {
                            guard("shell command", || apply_shell_cmd(&*shell, cmd));
                        }));
                    }
                    _ = stopped(&mut stop) => return,
                }
            }
        });
    }
}

impl ShellBackend for Shell {
    fn set_manual(&self, raw: &str) -> bool {
        self.engine.call(|finder| finder.set_manual(raw)).is_some()
    }

    fn retry_clock(&self) {
        self.engine.call(|finder| finder.retry());
    }

    fn use_clock(&self, clock: ClockTarget) {
        self.engine.call(|finder| finder.use_clock(&clock));
    }

    fn poll_now(&self) {
        self.engine.poll_now();
    }

    /// `Tray.refresh`: the status text and dot, the update item, the open panel.
    fn refresh(&self) {
        let snapshot = self.engine.snapshot();
        let now = now_epoch();
        let (dot, head, detail) = tray::status(&snapshot, now);
        let lost = tray::lost(&snapshot, now);
        let (change, model) = {
            let mut view = lock(&self.view);
            let change = view.set_status(dot, head, detail, lost);
            (change, view.model().clone())
        };
        self.redraw(change, model);
        self.sync_app_update();
        #[cfg(target_os = "macos")]
        crate::popover::push(&self.app);
    }

    /// `Tray._restart_for_update`: apply the downloaded update, then leave.
    fn restart_for_update(&self) {
        let (ok, why) = self.services.updater.apply(Some(std::process::id()));
        if !ok {
            dialogs::alert(&why);
            self.sync_app_update();
            return;
        }
        tracing::info!("Restarting to install the update");
        self.quit();
    }

    /// `Tray._rescan`: a wizard step or an agent changed.
    fn rescan_setup(&self) {
        let paths = &self.services.paths;
        let left = firstrun::pending(paths, None);
        let (setup_left, new_agents) = {
            let view = lock(&self.view);
            let m = view.model();
            (m.setup_left.clone(), m.new_agents.clone())
        };
        if left != setup_left || (!left.iter().any(|s| s == "agents") && !new_agents.is_empty()) {
            let seen = firstrun::seen(paths);
            let remaining: Vec<String> = new_agents
                .into_iter()
                .filter(|id| !seen.contains(id))
                .collect();
            let names = self.agent_names(&remaining);
            self.update_menu(|m| {
                m.setup_left = left.clone();
                m.new_agents = remaining.clone();
                m.new_agent_names = names.clone();
            });
        }
    }
}

// ---- the loops ---------------------------------------------------------------

fn spawn_loop(
    shell: &Arc<Shell>,
    name: &'static str,
    stop: &StopRx,
    body: fn(Arc<Shell>, StopRx) -> LoopFuture,
) {
    let (shell, stop) = (Arc::clone(shell), stop.clone());
    spawn_supervised(name, stop.clone(), move || {
        body(Arc::clone(&shell), stop.clone())
    });
}

/// A loop that has nothing to do (no feed, updates off) still waits for the stop signal: a
/// supervised loop that simply ended would be started again.
async fn wait_for_stop(mut stop: StopRx) {
    stopped(&mut stop).await;
}

/// `Tray._loop`: every [`REFRESH`], redraw what changed.
fn refresh_loop(shell: Arc<Shell>, mut stop: StopRx) -> LoopFuture {
    Box::pin(async move {
        while !sleep_or_stop(&mut stop, REFRESH).await {
            let shell = Arc::clone(&shell);
            let _ = tokio::task::spawn_blocking(move || {
                guard("tray refresh", || shell.refresh());
            })
            .await;
        }
    })
}

/// `Tray._feed_loop`: ask the firmware feed now and then whether the clock has something to
/// install.
fn feed_loop(shell: Arc<Shell>, mut stop: StopRx) -> LoopFuture {
    Box::pin(async move {
        let Some(feed) = shell.services.feed.clone() else {
            return wait_for_stop(stop).await;
        };
        tracing::info!("Update feed: on");
        let mut wait = FEED_FIRST;
        loop {
            if sleep_or_stop(&mut stop, wait).await {
                return;
            }
            wait = Duration::from_secs_f64(clawd_firmware::feed::CHECK_EVERY);
            let Some(clock) = shell.engine.snapshot().clock else {
                wait = FEED_FIRST; // try again soon once a clock is found
                continue;
            };
            let manifest = match feed.latest().await {
                Ok(m) => m,
                Err(e) => {
                    tracing::info!("Update check: {e}");
                    wait = FEED_RETRY;
                    continue;
                }
            };
            let heap = match ClockApi::new(&clock.base(), None).heap().await {
                Ok(h) => h,
                Err(e) => {
                    tracing::info!("Update check: {e}");
                    wait = FEED_RETRY;
                    continue;
                }
            };
            let new = clawd_firmware::feed::offer(
                &heap,
                manifest.as_ref(),
                shell.services.bundle.as_ref(),
            );
            let mut changed = false;
            shell.update_menu(|m| {
                if m.fw_new != new {
                    changed = true;
                    m.fw_new = new;
                }
            });
            if changed {
                if new != 0 {
                    tracing::info!("Update for the clock: #{new}");
                } else {
                    tracing::info!("Clock is up to date");
                }
            }
        }
    })
}

/// `Tray._app_update_loop`: look for a newer app now and then (GitHub Releases) and download it
/// in the background. Applying it is the user's choice: the menu item / the Update tab's button.
fn app_update_loop(shell: Arc<Shell>, mut stop: StopRx) -> LoopFuture {
    Box::pin(async move {
        let paths = shell.services.paths.clone();
        let env = shell.env.clone();
        let on = tokio::task::spawn_blocking(move || {
            clawd_appupdate::auto_enabled(&paths)
                && clawd_appupdate::from_settings(&paths, &env).is_ok()
        })
        .await
        .unwrap_or(false);
        if !on {
            tracing::info!("App update check: off");
            return wait_for_stop(stop).await;
        }
        tracing::info!("App update check: on");
        let mut wait = Duration::from_secs_f64(clawd_appupdate::source::FIRST_CHECK);
        loop {
            if sleep_or_stop(&mut stop, wait).await {
                return;
            }
            wait = Duration::from_secs_f64(clawd_appupdate::source::CHECK_EVERY);
            let state = shell.services.updater.check(true).await;
            if state.status == "error" {
                wait = Duration::from_secs_f64(clawd_appupdate::source::RETRY);
            }
            let shell = Arc::clone(&shell);
            let _ = tokio::task::spawn_blocking(move || shell.sync_app_update()).await;
        }
    })
}

/// What `Tray._agent_loop` finds: the wizard steps left and the agents installed since last
/// offered.
fn scan_agents(services: &Services) -> (Vec<String>, Vec<String>) {
    let left = firstrun::pending(&services.paths, None);
    let mut new = Vec::new();
    if !left.iter().any(|s| s == "agents") {
        // The wizard's Agent step offers them itself while it is pending.
        let registry = &services.registry;
        let installed: Vec<String> = registry
            .iter()
            .filter(|a| a.is_installed().unwrap_or(false))
            .map(|a| a.id().to_owned())
            .collect();
        new = firstrun::new_agents(
            &services.paths,
            &clawd_agents::found_ids(registry),
            &installed,
        );
    }
    (left, new)
}

/// `Tray._agent_loop`: keep the wizard item and the "new agent found" offer current.
fn agent_loop(shell: Arc<Shell>, mut stop: StopRx) -> LoopFuture {
    Box::pin(async move {
        let mut wait = AGENT_FIRST;
        loop {
            if sleep_or_stop(&mut stop, wait).await {
                return;
            }
            wait = AGENT_SCAN;
            let services = Arc::clone(&shell.services);
            let scanned = tokio::task::spawn_blocking(move || scan_agents(&services)).await;
            let Ok((left, new)) = scanned else {
                tracing::info!("Agent scan failed");
                continue;
            };
            let names = shell.agent_names(&new);
            let (old_left, old_new) = {
                let view = lock(&shell.view);
                (
                    view.model().setup_left.clone(),
                    view.model().new_agents.clone(),
                )
            };
            if (&left, &new) != (&old_left, &old_new) {
                if new != old_new && !new.is_empty() {
                    tracing::info!("New agents found: {}", new.join(", "));
                }
                shell.update_menu(|m| {
                    m.setup_left = left.clone();
                    m.new_agents = new.clone();
                    m.new_agent_names = names.clone();
                });
            }
        }
    })
}

// =============================================================================
// Entry points
// =============================================================================

fn run_events(event: &RunEvent, keep_running: bool) {
    // No window left is not "quit": only app.exit(code) ends the tray process.
    if let RunEvent::ExitRequested { code, api, .. } = event {
        if keep_running && code.is_none() {
            api.prevent_exit();
        }
    }
}

/// `Clawd` / `Clawd tray` (`tray.main`). Returns the exit code.
pub fn run_tray(context: tauri::Context<tauri::Wry>) -> i32 {
    let cfg = EngineConfig::system();
    start_logging(&cfg);
    tracing::info!(
        "{APP_NAME} {APP_VERSION} starting ({})",
        std::env::consts::OS
    );
    if tauri::async_runtime::block_on(another_daemon_running(&cfg.paths, &cfg.env)) {
        tracing::info!("Another Clawd daemon already owns the event port; exiting");
        dialogs::alert(ALREADY_RUNNING);
        return 0;
    }
    let parts = build_parts(cfg.clone());
    first_run(&cfg, &parts.services.registry);
    let Parts {
        engine,
        services,
        state,
        shell_rx,
    } = parts;

    let engine_for_setup = engine.clone();
    let env = cfg.env.clone();
    let builder = crate::commands::register(tauri::Builder::default()).manage(state);
    #[cfg(target_os = "macos")]
    let builder = builder.plugin(tauri_plugin_positioner::init());
    // The settings bridge's backup export/import call `app.dialog()` from Rust; without this
    // plugin that panics (and the page just sees a cancelled dialog).
    let builder = builder.plugin(tauri_plugin_dialog::init());
    let built = builder
        .setup(move |app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            let shell = Shell::new(
                app.handle().clone(),
                engine_for_setup.clone(),
                services,
                env,
            );
            let on_menu = Arc::clone(&shell);
            let on_click = Arc::clone(&shell);
            tray::build_tray(
                app.handle(),
                move |_, id| on_menu.on_menu(id),
                move |_| on_click.on_left_click(),
            )?;
            // The engine starts after the tray is on screen (tray <= 1 s).
            if let Err(e) = engine_for_setup.start() {
                tracing::error!("Engine did not start: {e}");
            }
            shell.spawn_loops(shell_rx);
            // The spike showed a trimmed working set only helps the Task Manager figure: do it once
            // the start-up allocations are done.
            tauri::async_runtime::spawn(async {
                tokio::time::sleep(STARTUP_TRIM_DELAY).await;
                trim_working_set();
            });
            let first = Arc::clone(&shell);
            tauri::async_runtime::spawn_blocking(move || {
                guard("first refresh", || first.refresh());
                guard("setup wizard", || first.maybe_open_setup());
            });
            app.manage(shell);
            Ok(())
        })
        .build(context);
    let app = match built {
        Ok(app) => app,
        Err(e) => {
            tracing::error!("Could not start the app: {e}");
            engine.stop();
            return 1;
        }
    };
    let code = app.run_return(|_, event| run_events(&event, true));
    engine.stop();
    tracing::info!("Bye");
    code
}

/// `Clawd settings`: the settings window alone, as the separate process of the Python app was:
/// no tray, no engine, no event server. A `set_clock` / `use_clock` from the page edits
/// clock.json for the next tray start (`settings_app.Channel.send` without a pipe). Returns the
/// exit code.
pub fn run_settings_only(
    context: tauri::Context<tauri::Wry>,
    clock: String,
    key: String,
    tab: String,
) -> i32 {
    let cfg = EngineConfig::system();
    start_logging(&cfg);
    let Parts {
        mut shell_rx,
        state,
        ..
    } = build_parts(cfg.clone());
    let (clock_base, clock_key) = if clock.is_empty() {
        crate::cli::remembered_clock(&ClockStore::new(cfg.paths.clock_file()).load())
    } else {
        (clock, key)
    };
    let key = if clock_key.is_empty() {
        clock_base.clone()
    } else {
        clock_key
    };
    tracing::info!(
        "[settings] window for {}",
        if clock_base.is_empty() {
            "no clock"
        } else {
            &clock_base
        }
    );
    let store_path = cfg.paths.clock_file();
    let built = crate::commands::register(tauri::Builder::default())
        .manage(state)
        .setup(move |app| {
            tauri::async_runtime::spawn(async move {
                let store = Arc::new(ClockStore::new(store_path));
                while let Some(cmd) = shell_rx.recv().await {
                    let store = Arc::clone(&store);
                    let _ = tokio::task::spawn_blocking(move || {
                        guard("shell command", || apply_standalone_cmd(&store, cmd));
                    })
                    .await;
                }
            });
            let session = SettingsSession::new(clock_base, key, windows::known_tab(&tab));
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                // The window is all this mode is: no window, nothing left to do.
                if !windows::show_settings(handle.clone(), session).await {
                    handle.exit(1);
                }
            });
            Ok(())
        })
        .build(context);
    let app = match built {
        Ok(app) => app,
        Err(e) => {
            tracing::error!("Could not start the settings window: {e}");
            return 1;
        }
    };
    let code = app.run_return(|_, event| run_events(&event, false));
    tracing::info!("[settings] window closed");
    code
}

/// `Clawd run` (`cli._run_headless`): the engine without a tray, until Ctrl+C. Returns the exit
/// code.
pub fn run_headless(cfg: EngineConfig) -> i32 {
    start_logging(&cfg);
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("Could not start the runtime: {e}");
            return 1;
        }
    };
    if runtime.block_on(another_daemon_running(&cfg.paths, &cfg.env)) {
        println!("{HEADLESS_ALREADY_RUNNING}");
        return 1;
    }
    install_interrupt_flag();
    let (engine, _status) = EngineHandle::new(cfg);
    let (stop_tx, stop_rx) = clawd_contracts::new_stop();
    runtime.block_on(async {
        let waiter = tokio::spawn(async move {
            while !INTERRUPTED.load(Ordering::SeqCst) {
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
            stop_tx.send_replace(true);
        });
        engine.run_until(stop_rx).await;
        waiter.abort();
    });
    0
}
