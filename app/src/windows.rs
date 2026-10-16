//! The settings window (`settings_app.py`, `tray.py:445-466`): created lazily when the tray asks
//! for it, destroyed when it closes (no hidden webview: RAM first, `spike-decision.md` Design A).
//!
//! * A fresh [`SettingsSession`] each time it opens: the clock it talks to and the tab it opens on.
//! * Built with `visible(false)` and `backgroundColor #141413`, then shown at once as a dark
//!   window (Goal 3 mitigation 1) and again on `clawd://ready`, which also takes the focus.
//! * Always a fixed `data_directory` under the app's local data, never beside the exe.
//! * Open already: bring it forward and send `clawd://show-tab`.
//! * Closing while a firmware job runs: refused, the page is told with `clawd://busy-close`.

use std::path::PathBuf;
use std::sync::atomic::Ordering;

use clawd_contracts::{
    ClockTarget, EVENT_BUSY_CLOSE, EVENT_READY, EVENT_SHOW_TAB, WINDOW_SETTINGS,
};
use tauri::window::Color;
use tauri::{
    AppHandle, Emitter, Listener, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
    WindowEvent,
};

use crate::state::{AppState, SettingsSession};

/// The window's background until the page paints (`#141413`).
pub const BACKGROUND: Color = Color(20, 20, 19, 255);
/// `settings_app.WIDTH`, `HEIGHT`.
pub const WIDTH: f64 = 820.0;
pub const HEIGHT: f64 = 600.0;
/// `min_size=(720, 540)`.
pub const MIN_WIDTH: f64 = 720.0;
pub const MIN_HEIGHT: f64 = 540.0;
/// Window title (`f"{APP_NAME} · Cài đặt"`).
pub const TITLE: &str = "Clawd · Cài đặt";
/// Tabs the tray can open the window on (`settings_app.TABS`).
pub const TABS: [&str; 3] = ["update", "agents", "setup"];
/// Environment variable with WebView2 browser arguments to try instead of Tauri's defaults (the
/// Goal 3 experiments, mitigation 3). Read in debug builds only: a release build ignores it.
pub const WEBVIEW_ARGS_ENV: &str = "CLAWD_WEBVIEW_ARGS";

/// `Tray._clock_key`: the keychain entry of the clock's web password; its name survives a new IP.
pub fn clock_key(clock: &ClockTarget, typed_host: &str) -> String {
    if !clock.name.is_empty() {
        clock.name.clone()
    } else if !typed_host.is_empty() {
        typed_host.to_owned()
    } else {
        clock.host.clone()
    }
}

/// The tab to hand the page: only the three it knows, anything else opens the default one.
pub fn known_tab(tab: &str) -> &str {
    if TABS.contains(&tab) {
        tab
    } else {
        ""
    }
}

/// `manual_host` of clock.json: the address the user typed, "" when none (blocking file read).
pub fn typed_host(engine: &crate::engine::EngineHandle) -> String {
    engine
        .finder()
        .store()
        .load()
        .get("manual_host")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_owned()
}

/// The session of a window opened now: `(base, key)` of the remembered clock, empty when none.
pub fn new_session(clock: Option<&ClockTarget>, typed_host: &str, tab: &str) -> SettingsSession {
    let (base, key) = match clock {
        Some(c) => (c.base(), clock_key(c, typed_host)),
        None => (String::new(), String::new()),
    };
    SettingsSession::new(base, key, known_tab(tab))
}

/// Where the WebView2 profile lives: `<app local data>/webview`, kept for good (a fresh profile
/// costs seconds on the first open).
pub fn data_directory(app: &AppHandle) -> Option<PathBuf> {
    app.path()
        .app_local_data_dir()
        .ok()
        .map(|d| d.join("webview"))
}

/// Open the settings window on `tab`, or bring the open one forward. Call it from an async task
/// (building a window from a menu callback deadlocks on Windows).
pub async fn open_settings(app: AppHandle, tab: String) {
    if let Some(window) = app.get_webview_window(WINDOW_SETTINGS) {
        bring_forward(&window, &tab);
        return;
    }
    let state = app.state::<AppState>();
    // The finder's own view of the clock (not the 3 s old cached status), read off the async threads.
    let (clock, typed) = {
        let engine = state.engine.clone();
        tokio::task::spawn_blocking(move || (engine.snapshot().clock, typed_host(&engine)))
            .await
            .unwrap_or_default()
    };
    show_settings(app, new_session(clock.as_ref(), &typed, &tab)).await;
}

/// Only one settings window is ever being opened: two clicks in a row must not build two windows
/// (the second build fails on the label and would wipe the live window's session).
static OPENING: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Run `create` unless a window exists by the time `gate` is ours, in which case `forward` gets it
/// instead. The check is repeated after the gate is taken: that is the whole point.
pub async fn open_once<W, Fut>(
    gate: &tokio::sync::Mutex<()>,
    existing: impl Fn() -> Option<W>,
    forward: impl FnOnce(W),
    create: impl FnOnce() -> Fut,
) -> bool
where
    Fut: std::future::Future<Output = bool>,
{
    let _opening = gate.lock().await;
    match existing() {
        Some(window) => {
            forward(window);
            true
        }
        None => create().await,
    }
}

/// A failed build must not clear the session of a window that is open (it belongs to that window).
pub fn clear_session_after_failed_build(window_exists: bool, clear: impl FnOnce()) {
    if !window_exists {
        clear();
    }
}

/// Open the window for `session` (or bring the open one forward on its tab).
/// `false` when no window could be opened.
pub async fn show_settings(app: AppHandle, session: SettingsSession) -> bool {
    let tab = session.tab.clone();
    open_once(
        &OPENING,
        || app.get_webview_window(WINDOW_SETTINGS),
        |window| bring_forward(&window, &tab),
        || build_settings(&app, session),
    )
    .await
}

async fn build_settings(app: &AppHandle, session: SettingsSession) -> bool {
    let state = app.state::<AppState>();
    state.session.set(session);

    let mut builder =
        WebviewWindowBuilder::new(app, WINDOW_SETTINGS, WebviewUrl::App("index.html".into()))
            .title(TITLE)
            .inner_size(WIDTH, HEIGHT)
            .min_inner_size(MIN_WIDTH, MIN_HEIGHT)
            .visible(false)
            .background_color(BACKGROUND);
    if let Some(dir) = data_directory(app) {
        builder = builder.data_directory(dir);
    }
    #[cfg(windows)]
    if let Some(args) = webview_args(&clawd_contracts::Env::system()) {
        builder = builder.additional_browser_args(&args);
    }
    match builder.build() {
        Ok(window) => {
            hook_events(app, &window);
            // Goal 3: the dark window is on screen while WebView2 loads the page.
            let _ = window.show();
            true
        }
        Err(e) => {
            tracing::error!("Settings window did not open: {e}");
            clear_session_after_failed_build(
                app.get_webview_window(WINDOW_SETTINGS).is_some(),
                || state.session.clear(),
            );
            false
        }
    }
}

/// The WebView2 browser arguments to experiment with: debug builds only, a release build never
/// reads the variable (nothing in the environment can change how the shipped window starts).
pub fn webview_args(env: &clawd_contracts::Env) -> Option<String> {
    if cfg!(debug_assertions) {
        env.get(WEBVIEW_ARGS_ENV).filter(|a| !a.is_empty())
    } else {
        None
    }
}

fn bring_forward(window: &WebviewWindow, tab: &str) {
    let tab = known_tab(tab);
    if !tab.is_empty() {
        let _ = window.emit(EVENT_SHOW_TAB, tab);
    }
    let _ = window.unminimize();
    let _ = window.show();
    let _ = window.set_focus();
}

fn hook_events(app: &AppHandle, window: &WebviewWindow) {
    let shown = window.clone();
    window.once(EVENT_READY, move |_| {
        let _ = shown.show();
        let _ = shown.set_focus();
    });
    let (app, closing) = (app.clone(), window.clone());
    window.on_window_event(move |event| match event {
        WindowEvent::CloseRequested { api, .. } => {
            let busy = app.state::<AppState>().jobs.busy.load(Ordering::SeqCst);
            if busy {
                // Never leave a firmware upload half done.
                api.prevent_close();
                let _ = closing.emit(EVENT_BUSY_CLOSE, ());
            }
        }
        WindowEvent::Destroyed => {
            app.state::<AppState>().session.clear();
            crate::lifecycle::trim_working_set();
        }
        _ => {}
    });
}
