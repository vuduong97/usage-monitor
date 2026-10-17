//! The tray icon (`tray.py`, `icons.py`): the status text (pure, tested), the menu as a list of
//! entries copied from `tray.py:192-208` (pure, tested) and the Tauri tray built from them.
//!
//! The icons are three small PNGs (green / yellow / red dot, `icons.status_icon`) drawn once
//! from `icons.py` and committed in `app/icons/`.

use clawd_contracts::APP_NAME;
use tauri::image::Image;
use tauri::menu::{IsMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::AppHandle;

use crate::engine::Status;

/// `tray.FRESH`: a push this recent counts as "connected".
pub const FRESH: f64 = 180.0;
/// `tray.LOGIN_HINT`.
pub const LOGIN_HINT: &str = "Đăng nhập Claude hết hạn: mở Claude Code hoặc gõ claude login";
/// Windows cuts a tooltip at 127 characters; Python kept 120.
pub const TOOLTIP_MAX: usize = 120;
/// What the head line says before the first refresh.
pub const STARTING_HEAD: &str = "Đang khởi động…";

/// The dot colour of the tray icon (`icons.status_icon`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Dot {
    Ok,
    #[default]
    Busy,
    Error,
}

impl Dot {
    pub fn as_str(self) -> &'static str {
        match self {
            Dot::Ok => "ok",
            Dot::Busy => "busy",
            Dot::Error => "error",
        }
    }

    /// The committed PNG of this dot.
    pub fn png(self) -> &'static [u8] {
        match self {
            Dot::Ok => include_bytes!("../icons/tray-ok.png"),
            Dot::Busy => include_bytes!("../icons/tray-busy.png"),
            Dot::Error => include_bytes!("../icons/tray-error.png"),
        }
    }
}

// =============================================================================
// Status text (`tray.status`, `tray.lost`)
// =============================================================================

/// `HH:MM` of an epoch time in the local time zone (`time.strftime("%H:%M", time.localtime(t))`).
pub fn local_hhmm(epoch: f64) -> String {
    let (h, m, _) = local_hms(epoch);
    format!("{h:02}:{m:02}")
}

/// `HH:MM:SS` of an epoch time in the local time zone (the log and `cleanup` timestamps).
pub fn local_hhmmss(epoch: f64) -> String {
    let (h, m, s) = local_hms(epoch);
    format!("{h:02}:{m:02}:{s:02}")
}

#[cfg(windows)]
fn local_hms(epoch: f64) -> (u32, u32, u32) {
    #[repr(C)]
    #[derive(Default)]
    struct SystemTime {
        year: u16,
        month: u16,
        day_of_week: u16,
        day: u16,
        hour: u16,
        minute: u16,
        second: u16,
        milliseconds: u16,
    }
    #[repr(C)]
    #[derive(Default)]
    struct FileTime {
        low: u32,
        high: u32,
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn FileTimeToSystemTime(file: *const FileTime, out: *mut SystemTime) -> i32;
        fn SystemTimeToTzSpecificLocalTime(
            zone: *const u8,
            utc: *const SystemTime,
            local: *mut SystemTime,
        ) -> i32;
    }
    // FILETIME counts 100 ns from 1601-01-01; the Unix epoch is 11_644_473_600 s later.
    let ticks = ((epoch + 11_644_473_600.0).max(0.0) * 10_000_000.0) as u64;
    let utc = FileTime {
        low: ticks as u32,
        high: (ticks >> 32) as u32,
    };
    let (mut utc_parts, mut out) = (SystemTime::default(), SystemTime::default());
    // SAFETY: plain out-parameters of the right layout. A null zone means the current time zone;
    // the conversion uses the zone rules of that date (daylight saving), not today's offset.
    let ok = unsafe {
        FileTimeToSystemTime(&utc, &mut utc_parts) != 0
            && SystemTimeToTzSpecificLocalTime(std::ptr::null(), &utc_parts, &mut out) != 0
    };
    if ok {
        (
            u32::from(out.hour),
            u32::from(out.minute),
            u32::from(out.second),
        )
    } else {
        let secs = epoch.max(0.0) as u64 % 86_400;
        (
            (secs / 3600) as u32,
            (secs % 3600 / 60) as u32,
            (secs % 60) as u32,
        )
    }
}

#[cfg(unix)]
fn local_hms(epoch: f64) -> (u32, u32, u32) {
    #[repr(C)]
    struct Tm {
        sec: i32,
        min: i32,
        hour: i32,
        mday: i32,
        mon: i32,
        year: i32,
        wday: i32,
        yday: i32,
        isdst: i32,
        gmtoff: i64,
        zone: *const u8,
    }
    extern "C" {
        fn localtime_r(time: *const i64, out: *mut Tm) -> *mut Tm;
    }
    let time = epoch as i64;
    let mut tm = Tm {
        sec: 0,
        min: 0,
        hour: 0,
        mday: 0,
        mon: 0,
        year: 0,
        wday: 0,
        yday: 0,
        isdst: 0,
        gmtoff: 0,
        zone: std::ptr::null(),
    };
    // SAFETY: `tm` has the glibc/macOS `struct tm` layout and `time` is a valid `time_t`.
    let done = unsafe { !localtime_r(&time, &mut tm).is_null() };
    if done {
        (
            tm.hour.max(0) as u32,
            tm.min.max(0) as u32,
            tm.sec.clamp(0, 59) as u32,
        )
    } else {
        let secs = epoch.max(0.0) as u64 % 86_400;
        (
            (secs / 3600) as u32,
            (secs % 3600 / 60) as u32,
            (secs % 60) as u32,
        )
    }
}

/// `tray.status`: (dot, headline, detail line) for the menu and the tooltip.
pub fn status(snap: &Status, now: f64) -> (Dot, String, String) {
    let connected = snap.last_ok.is_some_and(|t| t != 0.0 && now - t < FRESH);
    let mut detail = String::new();
    if let (Some(p), Some(at)) = (&snap.payload, snap.payload_at) {
        let has_bars = p.contains_key("s") || p.contains_key("w");
        if p.ok() != Some(false) && has_bars && at != 0.0 {
            if p.src() == Some("codex") {
                detail.push_str("Codex · ");
            }
            detail.push_str(&format!(
                "Phiên {}% · Tuần {}%",
                p.s().unwrap_or(0),
                p.w().unwrap_or(0)
            ));
            if now - at >= FRESH {
                // Only an old reading needs its time.
                detail.push_str(&format!(" · lúc {}", local_hhmm(at)));
            }
        }
    }

    if snap.token == Some(false) {
        // The clock may well be reached: say so, or an expired login reads as a lost clock.
        let where_ = match (&snap.clock, connected) {
            (Some(c), true) => format!(
                "Đã kết nối {} · ",
                if c.name.is_empty() { &c.host } else { &c.name }
            ),
            _ => String::new(),
        };
        return (Dot::Error, format!("{where_}{LOGIN_HINT}"), detail);
    }
    if snap.manual_config {
        let targets = snap.config_targets.join(", ");
        let head = if connected {
            format!("Đã kết nối · {targets}")
        } else {
            format!("Đang gửi tới · {targets}")
        };
        return (if connected { Dot::Ok } else { Dot::Busy }, head, detail);
    }
    let Some(clock) = &snap.clock else {
        // Nothing to try: no address saved and mDNS silent (there is no scan).
        return (
            Dot::Error,
            "Không thấy đồng hồ: nhập IP hiện trên màn hình đồng hồ (Nhập địa chỉ đồng hồ…)"
                .to_owned(),
            detail,
        );
    };
    let name = if clock.name.is_empty() {
        &clock.host
    } else {
        &clock.name
    };
    if connected {
        (Dot::Ok, format!("Đã kết nối · {name}"), detail)
    } else if snap.failing {
        (
            Dot::Error,
            format!("Đồng hồ {name} không trả lời: đổi IP thì nhập IP mới (Nhập địa chỉ đồng hồ…)"),
            detail,
        )
    } else {
        (Dot::Busy, format!("Đang kết nối {name}…"), detail)
    }
}

/// `tray.lost`: the clock is not reached (none known, or no push got through lately): the menu
/// offers to type its address. Not for `http_targets` from the config file.
pub fn lost(snap: &Status, now: f64) -> bool {
    let reached = snap.last_ok.is_some_and(|t| t != 0.0 && now - t < FRESH);
    !snap.manual_config && !reached
}

/// The tooltip: `Clawd: head` and the detail on a second line, cut to [`TOOLTIP_MAX`] characters.
pub fn tooltip(head: &str, detail: &str) -> String {
    let full = if detail.is_empty() {
        format!("{APP_NAME}: {head}")
    } else {
        format!("{APP_NAME}: {head}\n{detail}")
    };
    full.chars().take(TOOLTIP_MAX).collect()
}

// =============================================================================
// The menu (`tray.py:192-208`)
// =============================================================================

/// Item ids of the menu (what `on_menu_event` receives).
pub mod id {
    pub const HEAD: &str = "head";
    pub const DETAIL: &str = "detail";
    pub const RESTART_UPDATE: &str = "restart_update";
    pub const OPEN_UPDATE: &str = "open_update";
    pub const OPEN_SETUP: &str = "open_setup";
    pub const OPEN_AGENTS: &str = "open_agents";
    pub const SETTINGS: &str = "settings";
    pub const MANUAL: &str = "manual";
    pub const QUIT: &str = "quit";
}

/// Everything the menu shows: the tray's attributes in Python (`head`, `detail`, `app_new`,
/// `fw_new`, `setup_left`, `new_agents`) and `lost`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TrayModel {
    pub head: String,
    pub detail: String,
    /// A newer app is downloaded and checked (version), `""`: none.
    pub app_new: String,
    /// The bundled firmware for the clock, when it lacks it or its packs (`packs::offer`), 0: none.
    pub fw_new: u64,
    /// Wizard steps not done yet.
    pub setup_left: Vec<String>,
    /// Agents installed since last offered and not connected (ids).
    pub new_agents: Vec<String>,
    /// Their names, comma separated.
    pub new_agent_names: String,
    /// `tray.lost`: offer to type the clock's address.
    pub lost: bool,
}

/// One row of the menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuEntry {
    Item {
        id: &'static str,
        text: String,
        enabled: bool,
    },
    Separator,
}

/// The menu exactly as `tray.py:192-208` builds it, hidden items left out. Nothing else may be
/// added.
pub fn menu_entries(m: &TrayModel) -> Vec<MenuEntry> {
    let item = |id, text: String, enabled| MenuEntry::Item { id, text, enabled };
    let mut out = vec![item(id::HEAD, m.head.clone(), false)];
    if !m.detail.is_empty() {
        out.push(item(id::DETAIL, m.detail.clone(), false));
    }
    out.push(MenuEntry::Separator);
    if !m.app_new.is_empty() {
        out.push(item(
            id::RESTART_UPDATE,
            format!("● Khởi động lại để cập nhật {APP_NAME} {}", m.app_new),
            true,
        ));
    }
    if m.fw_new != 0 {
        out.push(item(
            id::OPEN_UPDATE,
            format!("● Có firmware mới (#{})", m.fw_new),
            true,
        ));
    }
    if !m.setup_left.is_empty() {
        out.push(item(
            id::OPEN_SETUP,
            "● Hoàn tất thiết lập…".to_owned(),
            true,
        ));
    }
    if !m.new_agents.is_empty() {
        out.push(item(
            id::OPEN_AGENTS,
            format!("● Tìm thấy {}: nối với {APP_NAME}…", m.new_agent_names),
            true,
        ));
    }
    out.push(item(id::SETTINGS, "Cài đặt…".to_owned(), true));
    if m.lost {
        out.push(item(id::MANUAL, "Nhập địa chỉ đồng hồ…".to_owned(), true));
    }
    out.push(MenuEntry::Separator);
    out.push(item(id::QUIT, format!("Thoát {APP_NAME}"), true));
    out
}

/// A Tauri menu with the entries of [`menu_entries`].
pub fn build_menu(app: &AppHandle, model: &TrayModel) -> tauri::Result<Menu<tauri::Wry>> {
    let mut items: Vec<Box<dyn IsMenuItem<tauri::Wry>>> = Vec::new();
    for entry in menu_entries(model) {
        match entry {
            MenuEntry::Item { id, text, enabled } => {
                items.push(Box::new(MenuItem::with_id(
                    app,
                    id,
                    text,
                    enabled,
                    None::<&str>,
                )?));
            }
            MenuEntry::Separator => items.push(Box::new(PredefinedMenuItem::separator(app)?)),
        }
    }
    let refs: Vec<&dyn IsMenuItem<tauri::Wry>> = items.iter().map(|b| b.as_ref()).collect();
    Menu::with_items(app, &refs)
}

/// The icon image of a dot.
pub fn dot_image(dot: Dot) -> tauri::Result<Image<'static>> {
    Image::from_bytes(dot.png())
}

/// Create the tray: the busy dot, the starting menu, left click -> `on_left_click`, menu choices
/// -> `on_menu`. Both callbacks run on the main thread and must return at once (spawn the work).
pub fn build_tray(
    app: &AppHandle,
    on_menu: impl Fn(&AppHandle, &str) + Send + Sync + 'static,
    on_left_click: impl Fn(&AppHandle) + Send + Sync + 'static,
) -> tauri::Result<TrayIcon> {
    let model = TrayModel {
        head: STARTING_HEAD.to_owned(),
        ..TrayModel::default()
    };
    TrayIconBuilder::with_id("clawd")
        .icon(dot_image(Dot::Busy)?)
        .tooltip(APP_NAME)
        .menu(&build_menu(app, &model)?)
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, event| on_menu(app, event.id.as_ref()))
        .on_tray_icon_event(move |tray, event| {
            // The positioner needs every tray event to know where the icon is (popover, macOS).
            #[cfg(target_os = "macos")]
            tauri_plugin_positioner::on_tray_event(tray.app_handle(), &event);
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                on_left_click(tray.app_handle());
            }
        })
        .build(app)
}

// =============================================================================
// What changed since the last refresh
// =============================================================================

/// What a refresh has to redraw.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TrayChange {
    /// The new dot, only when it differs from the one shown.
    pub icon: Option<Dot>,
    pub tooltip: bool,
    pub menu: bool,
}

impl TrayChange {
    pub fn is_empty(&self) -> bool {
        self.icon.is_none() && !self.tooltip && !self.menu
    }
}

/// The tray's attributes in Python (`state`, `head`, `detail`, ...): only a difference is redrawn
/// (`tray.refresh`: "only change the icon when it differs").
#[derive(Debug, Default)]
pub struct TrayState {
    dot: Option<Dot>,
    model: TrayModel,
}

impl TrayState {
    pub fn new() -> Self {
        Self {
            dot: None,
            model: TrayModel {
                head: STARTING_HEAD.to_owned(),
                ..TrayModel::default()
            },
        }
    }

    pub fn model(&self) -> &TrayModel {
        &self.model
    }

    /// `Tray.refresh`: take the engine's status text. The menu is also rebuilt when `lost`
    /// flips (Python evaluated `visible` when the menu opened).
    pub fn set_status(&mut self, dot: Dot, head: String, detail: String, lost: bool) -> TrayChange {
        let text_changed = head != self.model.head || detail != self.model.detail;
        let menu = text_changed || lost != self.model.lost;
        self.model.head = head;
        self.model.detail = detail;
        self.model.lost = lost;
        let icon = (self.dot != Some(dot)).then_some(dot);
        if icon.is_some() {
            self.dot = Some(dot);
        }
        TrayChange {
            icon,
            tooltip: text_changed,
            menu,
        }
    }

    /// Change one of the other attributes; true when the menu must be rebuilt.
    pub fn update(&mut self, change: impl FnOnce(&mut TrayModel)) -> bool {
        let before = self.model.clone();
        change(&mut self.model);
        self.model != before
    }
}
