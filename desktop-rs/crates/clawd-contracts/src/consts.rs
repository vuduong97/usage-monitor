//! Frozen constants (plan.md "Frozen contracts"). Each cites its Python origin.

// ---- identity -------------------------------------------------------------------
/// `clawd_desktop.APP_NAME`. Also `mainBinaryName` / `productName` in `tauri.conf.json`.
pub const APP_NAME: &str = "Clawd";
/// Version of this build (workspace version). The release tag must equal it (red team #2).
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
/// `appupdate.BUNDLE_ID`, `autostart.LABEL`, Tauri `identifier`.
pub const BUNDLE_ID: &str = "vn.clawd.desktop";
/// Windows executable name (frozen: autostart Run value, Inno `Clawd.exe cleanup`, updater).
pub const BIN_NAME_WINDOWS: &str = "Clawd.exe";
/// Inside the macOS bundle: `Clawd.app/Contents/MacOS/Clawd`.
pub const BIN_PATH_MACOS: &str = "Clawd.app/Contents/MacOS/Clawd";
/// Inno Setup `AppId` (`packaging/windows/clawd.iss`).
pub const INNO_APP_ID: &str = "{6C9B4E2A-1F7D-4C3B-9A51-C1A7D0E5B9F3}";
/// `appupdate.REPO` (env `CLAWD_UPDATE_REPO` / config `update_repo` override it).
pub const UPDATE_REPO: &str = "vuduong97/usage-monitor";

// ---- config dir (`paths.py:15-27`) ----------------------------------------------
/// `~/.config/<this>/` on every OS (shared with the older usage daemon).
pub const CONFIG_DIR_NAME: &str = "claude-usage-monitor";
pub const CONFIG_FILE_NAME: &str = "config";
pub const CLOCK_FILE_NAME: &str = "clock.json";
pub const PREFS_FILE_NAME: &str = "desktop.json";
/// Lock file next to `desktop.json` (Python: `PREFS_FILE.with_suffix(".lock")`).
pub const PREFS_LOCK_FILE_NAME: &str = "desktop.lock";
pub const PAYLOAD_FILE_NAME: &str = "usage_payload.json";
pub const LOG_FILE_NAME: &str = "clawd-desktop.log";
/// The rotated log (`LOG_FILE.with_suffix(".log.1")`).
pub const LOG_ROTATED_FILE_NAME: &str = "clawd-desktop.log.1";
pub const PAK_FILE_NAME: &str = "clawd.pak";
pub const RENDERER_DIR_NAME: &str = "renderer";
pub const HOOK_SCRIPT_NAME: &str = "clawd-event.sh";
pub const OTA_BUSY_FILE_NAME: &str = "ota.busy";
/// `pause.PAUSE_FILE` (the engine's push lease; NOT `ota.busy`).
pub const PAUSE_FILE_NAME: &str = "pause.json";
pub const PET_FILE_NAME: &str = "pet.json";
pub const ATTENTION_FILE_NAME: &str = "attention";
pub const FEED_CACHE_DIR_NAME: &str = "feed-cache";
pub const APP_UPDATE_DIR_NAME: &str = "app-update";

/// `paths.LOG_MAX`: the log rotates once it passes this many bytes.
pub const LOG_MAX: u64 = 1_000_000;
/// `paths.PREFS_IO_WAIT` (seconds): how long `desktop.json` may stay locked or busy.
pub const PREFS_IO_WAIT: f64 = 5.0;
/// `paths.OTA_BUSY_STALE` (seconds): `ota.busy` older than this no longer counts.
/// Read only by the app updater. Different from the pause lease below.
pub const OTA_BUSY_STALE: f64 = 900.0;
/// `pause.MAX_LEASE` (seconds): never trust a `pause.json` `until` further out than this.
pub const MAX_LEASE: f64 = 300.0;
/// `paths.SCREEN_USAGE`: the clock's `usage.screen` value meaning "Usage" (0 auto, 1 usage, 2 clawd).
pub const SCREEN_USAGE: i64 = 1;
/// `paths.PAYLOAD_KEYS`: the keys the clock's `/api/usage` reads. Alerts (`m`, `x`) and the
/// beep (`b`) are one-shot and never stored.
pub const PAYLOAD_KEYS: [&str; 11] = [
    "s", "sr", "w", "wr", "st", "ok", "acct", "tp", "rd", "t", "tf",
];

// ---- event server (`events.py:46-54`, scout §3) ---------------------------------
pub const EVENT_HOST: &str = "127.0.0.1";
/// Frozen port. Override order: env [`EVENT_PORT_ENV`] -> config `event_port` -> this.
pub const EVENT_PORT: u16 = 47913;
pub const EVENT_PORT_ENV: &str = "CLAWD_EVENT_PORT";
/// Clock desk-view paths.
pub const STATE_PATH: &str = "/api/state";
pub const HEAP_PATH: &str = "/api/heap";
/// `events.STATE_TTL` (seconds): how long the clock keeps a pushed state without a resend.
pub const STATE_TTL: u32 = 60;
/// `events.MAX_BODY`: a hook/poke body over this answers 413.
pub const MAX_BODY: usize = 4 * 1024 * 1024;
/// `usage.HTTP_PUSH_PATH`: appended to a bare `http_targets` host.
pub const HTTP_PUSH_PATH: &str = "/api/usage";

// ---- OS stores (frozen; tests inject [`crate::OsStoreNames`] instead) -------------
/// `creds.SERVICE`: keyring service. Windows: TargetName = this (fallback `user@this`).
pub const KEYRING_SERVICE: &str = "vn.clawd.desktop";
/// `creds.DEFAULT_USER`.
pub const DEFAULT_CLOCK_USER: &str = "admin";
/// WinVault `Comment` written by python-keyring (kept byte-for-byte).
pub const WINVAULT_COMMENT: &str = "Stored using python-keyring";
/// `autostart.RUN_KEY` (HKCU).
pub const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
/// `autostart.RUN_VALUE`: its data is `"<exe>"`, no arguments.
pub const RUN_VALUE: &str = "Clawd";
/// `autostart.LABEL`: `~/Library/LaunchAgents/<label>.plist`.
pub const LAUNCH_AGENT_LABEL: &str = "vn.clawd.desktop";

// ---- app update assets (`appupdate.py:9-10,100-110`) ------------------------------
/// `{version}` is `X.Y.Z`.
pub const ASSET_WINDOWS_SETUP: &str = "Clawd-{version}-windows-setup.exe";
/// `{version}` is `X.Y.Z`, `{arch}` is `arm64` or `x86_64`.
pub const ASSET_MAC_ZIP: &str = "Clawd-{version}-mac-{arch}.zip";
pub const ASSET_MAC_DMG: &str = "Clawd-{version}-mac-{arch}.dmg";
pub const SUMS_NAME: &str = "SHA256SUMS";
/// New in the port (Q2): minisign signature of [`SUMS_NAME`].
pub const SUMS_SIG_NAME: &str = "SHA256SUMS.minisig";

/// Placeholder until phase 11 creates the production key (enters through a Contract CR).
/// It is not a valid minisign key, so verification fails closed.
pub const MINISIGN_PUBKEY_PLACEHOLDER: &str = "PLACEHOLDER-MINISIGN-PUBLIC-KEY-REPLACE-IN-PHASE-11";
/// minisign public key (base64 line) that signs `SHA256SUMS`. Build-time override
/// `CLAWD_MINISIGN_PUBKEY` is for test builds on a fork only (phase 14).
pub const MINISIGN_PUBKEY: &str = match option_env!("CLAWD_MINISIGN_PUBKEY") {
    Some(key) => key,
    None => MINISIGN_PUBKEY_PLACEHOLDER,
};

// ---- build-time feed defaults (replace `_feed_build.py`; secrets never in the repo) ---
pub const BUILD_FEED_URL: Option<&str> = option_env!("CLAWD_FEED_URL");
pub const BUILD_FEED_TOKEN: Option<&str> = option_env!("CLAWD_FEED_TOKEN");

// ---- clock firmware ---------------------------------------------------------------
/// `firmware.CLAWD_VARIANT`.
pub const CLAWD_VARIANT: &str = "esp8266-clawd";
/// `discovery.SERVICE`.
pub const MDNS_SERVICE: &str = "_clawd._tcp.local.";

// ---- Tauri window labels and events (phase 02 "Event contract") --------------------
pub const WINDOW_SETTINGS: &str = "settings";
pub const WINDOW_POPOVER: &str = "popover";
/// JS -> Rust: the page painted, show the window.
pub const EVENT_READY: &str = "clawd://ready";
/// Rust -> JS: open this tab (`window.clawdShowTab`).
pub const EVENT_SHOW_TAB: &str = "clawd://show-tab";
/// Rust -> JS: close attempted during OTA (`window.clawdBusyClose`).
pub const EVENT_BUSY_CLOSE: &str = "clawd://busy-close";
/// Rust -> popover JS: `clawdUpdate(view)`.
pub const EVENT_POPOVER_UPDATE: &str = "clawd://popover-update";
/// Rust -> popover JS: `clawdPak(base64)`.
pub const EVENT_POPOVER_PAK: &str = "clawd://popover-pak";
/// Rust -> popover JS: `clawdVisible(bool)`.
pub const EVENT_POPOVER_VISIBLE: &str = "clawd://popover-visible";
