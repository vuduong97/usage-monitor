//! The small panel under the menu-bar icon (`popover.py`, macOS): Clawd and two usage bars.
//!
//! Design A of the port: a frameless `popover` webview window created when the icon is clicked and
//! destroyed when it loses focus (nothing stays hidden, RAM first), placed under the icon by the
//! positioner plugin (`TrayBottomCenter`). The page is `ui/popover.html`; it asks for things with
//! `invoke('popover_msg', {msg})` (which lands in [`handle`]) and is fed by the events
//! `clawd://popover-update` (the [`view`]), `clawd://popover-pak` (Clawd's sprites, base64) and
//! `clawd://popover-visible`.
//!
//! Everything but the window itself is pure and unit-tested; the window code compiles on every
//! platform but only the macOS tray calls [`toggle`].

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use clawd_clock::ClockApi;
use clawd_contracts::{
    OsStoreNames, EVENT_POPOVER_PAK, EVENT_POPOVER_UPDATE, EVENT_POPOVER_VISIBLE, WINDOW_POPOVER,
};
use clawd_firmware::preview::{pak_count, PakCache, PAK_MAX};
use serde::Serialize;
use serde_json::Value;
use tauri::{
    AppHandle, Emitter, LogicalSize, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
    WindowEvent,
};
use tauri_plugin_positioner::{Position, WindowExt};

use crate::state::{lock, AppState, Status};
use crate::tray;

/// `popover.WIDTH`.
pub const WIDTH: f64 = 264.0;
/// The height before the page reports its own (`NSMakeRect(0, 0, WIDTH, 92)`).
pub const INITIAL_HEIGHT: f64 = 92.0;
/// `popover.STALE`: usage older than this says when it was read.
pub const STALE: f64 = 600.0;
/// `popover.PAK_RETRY_S`: after a failed read of `/clawd.pak`.
pub const PAK_RETRY_S: f64 = 300.0;
/// What a `size` message may ask for (`40 <= h <= 400`).
pub const HEIGHT_RANGE: std::ops::RangeInclusive<f64> = 40.0..=400.0;
/// A click on the icon this soon after the panel lost focus is the click that closes it.
const REOPEN_GUARD: Duration = Duration::from_millis(400);

// =============================================================================
// What the page shows (pure)
// =============================================================================

/// `popover.left`: time until a reset, short: `45p`, `1g05`, `4n 20g`.
pub fn left(minutes: f64) -> String {
    let m = minutes.max(0.0) as u64;
    if m < 60 {
        format!("{m}p")
    } else if m < 1440 {
        format!("{}g{:02}", m / 60, m % 60)
    } else {
        format!("{}n {}g", m / 1440, m % 1440 / 60)
    }
}

/// One bar of the panel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Row {
    pub name: String,
    pub pct: u32,
    pub left: String,
}

/// `popover.view`: what `ui/popover.html` draws.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PopoverView {
    pub state: String,
    pub label: String,
    pub rows: Vec<Row>,
}

fn number(v: Option<&Value>) -> Option<f64> {
    v.and_then(Value::as_f64)
}

fn row(name: &str, pct: Option<f64>, rem: Option<f64>, when: &str, gone: f64) -> Row {
    let pct = pct.map_or(0, |p| p.round_ties_even().clamp(0.0, 100.0) as u32);
    let left_text = if !when.is_empty() {
        when.to_owned()
    } else {
        match rem {
            Some(r) if r >= 0.0 => left(r - gone),
            _ => String::new(),
        }
    };
    Row {
        name: name.to_owned(),
        pct,
        left: left_text,
    }
}

/// `popover.view(snap, now)`: `label` is the tray's headline when something needs the user,
/// otherwise "Đã kết nối"; usage comes from Claude, so the bars show even without a clock. Reset
/// times count down from when the payload was read.
pub fn view(snap: &Status, now: f64) -> PopoverView {
    let (dot, head, _detail) = tray::status(snap, now);
    let mut label = if dot == tray::Dot::Ok {
        "Đã kết nối".to_owned()
    } else {
        head
    };
    let mut rows = Vec::new();
    if let (Some(p), Some(at)) = (&snap.payload, snap.payload_at) {
        let shows = number(p.get("s")).is_some() && p.ok() != Some(false) && at != 0.0;
        if shows {
            let gone = ((now - at) / 60.0).max(0.0);
            if p.acct() == Some("ent") {
                // One spending limit per billing month.
                let when = p.rd().unwrap_or("");
                rows.push(row(
                    "Tháng",
                    number(p.get("s")),
                    number(p.get("sr")),
                    when,
                    gone,
                ));
            } else {
                rows.push(row(
                    "Phiên",
                    number(p.get("s")),
                    number(p.get("sr")),
                    "",
                    gone,
                ));
                let week = p.get("w").map_or(Some(0.0), |v| v.as_f64());
                rows.push(row("Tuần", week, number(p.get("wr")), "", gone));
            }
            if now - at >= STALE {
                label.push_str(&format!(" · số liệu lúc {}", tray::local_hhmm(at)));
            }
        }
    }
    PopoverView {
        state: dot.as_str().to_owned(),
        label,
        rows,
    }
}

/// A message of the page (`popover.py:262-280`).
#[derive(Debug, Clone, PartialEq)]
pub enum PopoverMsg {
    Ready,
    /// A content height inside [`HEIGHT_RANGE`].
    Size(f64),
    Settings,
    Close,
}

/// Read the `{cmd, ...}` object of `popover_msg`; anything unknown or out of range is `None`
/// (ignored, like Python).
pub fn parse_message(msg: &Value) -> Option<PopoverMsg> {
    match msg.get("cmd")?.as_str()? {
        "ready" => Some(PopoverMsg::Ready),
        "size" => {
            let h = msg.get("h")?.as_f64()?;
            HEIGHT_RANGE
                .contains(&h)
                .then_some(PopoverMsg::Size(h.trunc()))
        }
        "settings" => Some(PopoverMsg::Settings),
        "close" => Some(PopoverMsg::Close),
        _ => None,
    }
}

/// Standard base64 with padding (the pak travels to the page as text).
pub fn base64(data: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |acc, (i, b)| acc | u32::from(*b) << (16 - 8 * i));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

// =============================================================================
// Clawd's sprites: the clock's /clawd.pak, cached like the settings window does
// =============================================================================

/// After a failed read, no new attempt for [`PAK_RETRY_S`] (`_pak_failed`).
pub fn pak_retry_allowed(failed_at: f64, now: f64) -> bool {
    now - failed_at >= PAK_RETRY_S
}

static PAK_FAILED_AT: Mutex<f64> = Mutex::new(0.0);

/// `popover.load_pak`: the cached pack, else read it off the clock (login from the keychain when
/// the clock has one). `None` when there is none to be had; the page then draws the app icon.
pub async fn load_pak(
    base: &str,
    key: &str,
    names: &OsStoreNames,
    cache_path: PathBuf,
    now: f64,
) -> Option<Vec<u8>> {
    let cached = tokio::task::spawn_blocking({
        let path = cache_path.clone();
        move || PakCache::new(path).load()
    })
    .await
    .ok()
    .flatten();
    if let Some((data, _)) = cached {
        return Some(data);
    }
    if base.is_empty() || !pak_retry_allowed(*lock(&PAK_FAILED_AT), now) {
        return None;
    }
    let auth = if key.is_empty() {
        None
    } else {
        let (names, key) = (names.clone(), key.to_owned());
        tokio::task::spawn_blocking(move || clawd_store::creds::get(&names, &key))
            .await
            .ok()
            .flatten()
    };
    let api = ClockApi::new(base, auth);
    let fetched = async {
        let want = api
            .heap()
            .await
            .map_err(|e| e.to_string())?
            .get("pak")
            .and_then(Value::as_u64)
            .filter(|n| *n > 0)
            .ok_or_else(|| "the clock has no clawd.pak".to_owned())?;
        let data = api
            .get_file("/clawd.pak", PAK_MAX)
            .await
            .map_err(|e| e.to_string())?;
        pak_count(&data).map_err(|e| e.to_string())?;
        Ok::<_, String>((want, data))
    }
    .await;
    match fetched {
        Ok((want, data)) => {
            let to_save = data.clone();
            let _ = tokio::task::spawn_blocking(move || {
                let _ = PakCache::new(cache_path).save(&to_save, want as u32);
            })
            .await;
            Some(data)
        }
        Err(e) => {
            *lock(&PAK_FAILED_AT) = now;
            tracing::info!("[popover] no clawd.pak from {base}: {e}");
            None
        }
    }
}

// =============================================================================
// The window
// =============================================================================

/// What the page and the app have told each other about the current window.
#[derive(Debug, Default)]
struct PanelFlags {
    ready: bool,
    pak_sent: bool,
    pak_loading: bool,
    /// When the panel last lost focus (the click on the icon that follows closes it).
    blurred_at: Option<Instant>,
}

static PANEL: Mutex<PanelFlags> = Mutex::new(PanelFlags {
    ready: false,
    pak_sent: false,
    pak_loading: false,
    blurred_at: None,
});

fn panel(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window(WINDOW_POPOVER)
}

fn now_epoch() -> f64 {
    use clawd_contracts::Clock;
    clawd_contracts::SystemClock.now()
}

/// Left click on the icon (macOS): open the panel, or close it when it is open (or has just lost
/// focus to this very click).
pub fn toggle(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Some(window) = panel(&app) {
            let _ = window.destroy();
            return;
        }
        let just_closed = lock(&PANEL)
            .blurred_at
            .is_some_and(|t| t.elapsed() < REOPEN_GUARD);
        if !just_closed {
            open(&app).await;
        }
    });
}

async fn open(app: &AppHandle) {
    {
        let mut flags = lock(&PANEL);
        *flags = PanelFlags::default();
    }
    let mut builder =
        WebviewWindowBuilder::new(app, WINDOW_POPOVER, WebviewUrl::App("popover.html".into()))
            .inner_size(WIDTH, INITIAL_HEIGHT)
            .decorations(false)
            .resizable(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .visible(false)
            .shadow(true)
            .background_color(crate::windows::BACKGROUND);
    if let Some(dir) = crate::windows::data_directory(app) {
        builder = builder.data_directory(dir);
    }
    let window = match builder.build() {
        Ok(w) => w,
        Err(e) => {
            tracing::error!("[popover] could not open: {e}");
            return;
        }
    };
    let (app_for_events, window_for_events) = (app.clone(), window.clone());
    window.on_window_event(move |event| {
        if let WindowEvent::Focused(false) = event {
            // Lost focus = destroy: nothing is kept hidden.
            lock(&PANEL).blurred_at = Some(Instant::now());
            let _ = window_for_events.destroy();
            let _ = emit_visible(&app_for_events, false);
        }
    });
    place(&window);
    let _ = window.show();
    let _ = window.set_focus();
}

fn place(window: &WebviewWindow) {
    // Fails until the tray has reported its position (the first click always has).
    if let Err(e) = window.move_window(Position::TrayBottomCenter) {
        tracing::debug!("[popover] not placed under the icon: {e}");
    }
}

fn emit_visible(app: &AppHandle, on: bool) -> tauri::Result<()> {
    app.emit_to(WINDOW_POPOVER, EVENT_POPOVER_VISIBLE, on)
}

/// Send the current numbers to the open panel (cheap; the tray calls it every few seconds).
pub fn push(app: &AppHandle) {
    if panel(app).is_none() || !lock(&PANEL).ready {
        return;
    }
    let status = app.state::<AppState>().status.borrow().clone();
    let _ = app.emit_to(
        WINDOW_POPOVER,
        EVENT_POPOVER_UPDATE,
        view(&status, now_epoch()),
    );
}

fn want_pak(app: &AppHandle) {
    {
        let mut flags = lock(&PANEL);
        if flags.pak_sent || flags.pak_loading {
            return;
        }
        flags.pak_loading = true;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        let clock = state.status.borrow().clone().clock;
        let (base, key) = match &clock {
            Some(c) => {
                let engine = state.engine.clone();
                let typed =
                    tokio::task::spawn_blocking(move || crate::windows::typed_host(&engine))
                        .await
                        .unwrap_or_default();
                (c.base(), crate::windows::clock_key(c, &typed))
            }
            None => (String::new(), String::new()),
        };
        let services = std::sync::Arc::clone(&state.services);
        let data = load_pak(
            &base,
            &key,
            &services.names,
            services.paths.pak_file(),
            now_epoch(),
        )
        .await;
        let sent = {
            let mut flags = lock(&PANEL);
            flags.pak_loading = false;
            let send = data.is_some() && flags.ready && panel(&app).is_some();
            flags.pak_sent = send;
            send
        };
        if let (true, Some(data)) = (sent, data) {
            let _ = app.emit_to(WINDOW_POPOVER, EVENT_POPOVER_PAK, base64(&data));
        }
    });
}

/// One message from `ui/popover.html` (`invoke('popover_msg', {msg})`), after
/// `commands::popover_msg` has checked that the caller is the `popover` window.
pub fn handle(app: &AppHandle, msg: Value) -> Result<(), String> {
    let Some(message) = parse_message(&msg) else {
        return Ok(());
    };
    match message {
        PopoverMsg::Ready => {
            lock(&PANEL).ready = true;
            push(app);
            let _ = emit_visible(app, true);
            want_pak(app);
        }
        PopoverMsg::Size(h) => {
            if let Some(window) = panel(app) {
                let _ = window.set_size(LogicalSize::new(WIDTH, h));
                place(&window);
            }
        }
        PopoverMsg::Settings => {
            if let Some(window) = panel(app) {
                let _ = window.destroy();
            }
            tauri::async_runtime::spawn(crate::windows::open_settings(app.clone(), String::new()));
        }
        PopoverMsg::Close => {
            if let Some(window) = panel(app) {
                let _ = window.destroy();
            }
        }
    }
    Ok(())
}
