//! The clock's settings API (`clockapi.py`): read and save settings (`/api/config`; a POST
//! carries only what changed), scan WiFi, reboot, factory reset, backup/restore, push a firmware
//! image (`/update`), install or clear the usage screen's style pack (`/api/style`).
//!
//! The web password is HTTP **Digest** auth (`diqwest`): one auth state lives as long as the
//! client, so after the first challenge every request (the 0.5 MB upload too) is signed up
//! front. Requests are serialised per client (the ESP8266 serves one client at a time).
//! Failure mapping: 401 -> `ClockError::AuthNeeded`; >= 400 -> `ClockError::Http` (body cut to
//! 200 chars); connect/timeout -> retries then `ClockError::NoAnswer`; a malformed host ->
//! `ClockError::Other`, never a panic.

use std::time::Duration;

use clawd_contracts::{Clock, ClockError};
use serde_json::Value;

/// `clockapi.TIMEOUT`: the clock answers slowly for a few minutes after a boot.
pub const TIMEOUT: Duration = Duration::from_secs(20);
/// `clockapi.UPLOAD_TIMEOUT`.
pub const UPLOAD_TIMEOUT: Duration = Duration::from_secs(180);
/// `clockapi.RETRIES`: extra tries for a GET that timed out or could not connect.
pub const RETRIES: u32 = 2;
/// `clockapi.RETRY_GAP`.
pub const RETRY_GAP: Duration = Duration::from_millis(1500);
/// `clockapi.OTA_MAX`: the clawd -> clawd OTA budget.
pub const OTA_MAX: usize = 524_288;
/// `clockapi.STYLE_MAX`: the largest style pack the firmware takes.
pub const STYLE_MAX: usize = 32_768;
/// `clockapi.PACK_MAX`: `/desk.pak` and `/clawd.pak`: what the filesystem can hold.
pub const PACK_MAX: usize = 3_000_000;
/// `clockapi.PACK_TIMEOUT`: about 2 MB over the clock's WiFi.
pub const PACK_TIMEOUT: Duration = Duration::from_secs(600);

/// `clockapi.ClockApi`. Responses stay untyped JSON (`serde_json::Value`): the firmware owns
/// their schema. Share it behind an `Arc`; every method takes `&self`.
pub struct ClockApi {
    _private: (),
}

impl ClockApi {
    /// `ClockApi(base, auth)`: `base` like `http://10.0.0.5` (a trailing `/` is dropped);
    /// `auth` is `(user, password)` for the Digest login.
    pub fn new(base: &str, auth: Option<(String, String)>) -> Self {
        todo!("phase 08")
    }

    /// The base URL.
    pub fn base(&self) -> &str {
        todo!("phase 08")
    }

    /// `ClockApi.set_auth(auth)`.
    pub fn set_auth(&self, auth: Option<(String, String)>) {
        todo!("phase 08")
    }

    /// `GET /api/heap`: no login; uptime, version, fwSeq, layouts, packs. Object or
    /// `ClockError::Other("odd /api/heap answer")`.
    pub async fn heap(&self) -> Result<Value, ClockError> {
        todo!("phase 08")
    }

    /// `GET /api/config` (login when set).
    pub async fn config(&self) -> Result<Value, ClockError> {
        todo!("phase 08")
    }

    /// `POST /api/config` with only `changes`: `{"ok": true, "reboot": <WiFi changed>}`; empty
    /// changes return `{"ok": true, "reboot": false, "unchanged": true}` without a request.
    pub async fn save(&self, changes: &Value) -> Result<Value, ClockError> {
        todo!("phase 08")
    }

    /// `GET /api/status` (login): mode, connected, ssid, ip, rssi, uptime...
    pub async fn status(&self) -> Result<Value, ClockError> {
        todo!("phase 08")
    }

    /// `GET /api/scan`: nearby WiFi `[{ssid, rssi, enc}]`; the clock blocks about 3 s scanning.
    pub async fn scan(&self) -> Result<Vec<Value>, ClockError> {
        todo!("phase 08")
    }

    /// `POST /api/reboot`.
    pub async fn reboot(&self) -> Result<(), ClockError> {
        todo!("phase 08")
    }

    /// `POST /api/factory`.
    pub async fn factory_reset(&self) -> Result<(), ClockError> {
        todo!("phase 08")
    }

    /// `GET /api/export`: the saved `config.json`, WiFi passwords included (it is a backup).
    pub async fn export(&self) -> Result<Value, ClockError> {
        todo!("phase 08")
    }

    /// `POST /api/import`: apply a backup; the clock reboots afterwards.
    pub async fn restore(&self, data: &Value) -> Result<(), ClockError> {
        todo!("phase 08")
    }

    /// `GET /api/fs/get?path=`: one file from the clock's filesystem; `ClockError` when it is
    /// missing or bigger than `max_bytes`.
    pub async fn get_file(&self, path: &str, max_bytes: usize) -> Result<Vec<u8>, ClockError> {
        todo!("phase 08")
    }

    /// `POST /update` (multipart `firmware`): size must be 1..=[`OTA_MAX`]; collects the digest
    /// challenge with a small request first; the clock reboots about 1 s after the `OK`.
    pub async fn upload_firmware(&self, image: &[u8]) -> Result<(), ClockError> {
        todo!("phase 08")
    }

    /// `POST /api/clawdpak` (`kind = "clawdpak"`) or `/api/deskpak` (`"deskpak"`): replace
    /// `/clawd.pak` / `/desk.pak`; `{ok, anims, crc?}`. Size 1..=[`PACK_MAX`].
    pub async fn upload_pack(&self, kind: &str, data: &[u8]) -> Result<Value, ClockError> {
        todo!("phase 08")
    }

    /// `GET /api/style` (no login): `{api, active, slot, id?, name?, size?, crc?, error?}`.
    pub async fn style(&self) -> Result<Value, ClockError> {
        todo!("phase 08")
    }

    /// `POST /api/style` (multipart `file`): send a style pack; the clock shows it at once.
    /// Size 1..=[`STYLE_MAX`]. Returns its `/api/style` view.
    pub async fn install_style(&self, pack: &[u8]) -> Result<Value, ClockError> {
        todo!("phase 08")
    }

    /// `DELETE /api/style`: back to the classic screen.
    pub async fn clear_style(&self) -> Result<Value, ClockError> {
        todo!("phase 08")
    }

    /// `ClockApi.wait_back(booted_before, deadline, clock, tick)`: poll `/api/heap` until the
    /// clock answers with an uptime that began after `booted_before` (a monotonic time from
    /// `clock`); returns that heap. `tick` runs every poll (the caller renews its push pause
    /// there). `deadline` default 150 s; `ClockError::Other` when the clock does not come back.
    pub async fn wait_back(
        &self,
        booted_before: f64,
        deadline: Option<Duration>,
        clock: &dyn Clock,
        tick: Option<&(dyn Fn() + Send + Sync)>,
    ) -> Result<Value, ClockError> {
        todo!("phase 08")
    }
}
