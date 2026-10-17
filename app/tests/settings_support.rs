//! What the settings tests share (`mod settings_support;`): a fake clock firmware on loopback
//! (the `FakeApi` of `tests/test_settings_api.py`, one level lower: real HTTP, so the real
//! `ClockApi` runs), a `Bridge` over a temp home with an in-memory keyring, and bundles.
//! Nothing here touches the real HOME, keyring, registry or network.
#![allow(dead_code)]

#[path = "common/mod.rs"]
mod common;

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use clawd_agents::Registry;
use clawd_app::commands::session::{clock_view, Bridge};
use clawd_app::state::{JobState, Services, SettingsSession, ShellCmd};
use clawd_appupdate::{Updater, UpdaterOptions};
use clawd_contracts::{AppPaths, OsStoreNames, Platform};
use clawd_firmware::crc32::{crc32, crc32_hex};
use clawd_firmware::firmware::{load_bundle, sha256_hex, Bundle};
use clawd_firmware::packs::StyleSource;
use clawd_store::creds::Keyring;
use clawd_testkit::clock::DigestGate;
use clawd_testkit::config::FakeRunner;
use clawd_testkit::fw::make_style_pack;
use clawd_testkit::store::MemKeyring;
use clawd_testkit::{FakeHttpServer, FakeResponse, RecordedRequest, TempConfig};
use serde_json::{json, Value};
use tokio::sync::mpsc;

/// The clock's config (`CFG`).
pub fn cfg() -> Value {
    json!({"hostname": "clawd", "wifi": [{"ssid": "Home", "passSet": true}, {"ssid": "Phone", "passSet": true}],
        "brightness": 45, "autoBrightness": false, "rotation": 0, "mode": "usage",
        "usage": {"usageUrl": "", "pollSec": 60, "screen": 0, "anim": 9, "layout": 0},
        "clock": {"tz": "", "tzPosix": "UTC0", "nightEnabled": false, "nightStart": "22:00",
                  "nightEnd": "07:00", "nightLevel": 0},
        "auth": {"enabled": false, "user": "admin", "passSet": false}, "features": {"usage": true}})
}

/// `OLD_HEAP`: before `fwSeq` existed.
pub fn old_heap() -> Value {
    json!({"version": "2.16.0", "pak": 17, "desk": 31, "uptime": 900})
}

/// `NEW_HEAP`.
pub fn new_heap() -> Value {
    json!({"version": "2.16.0", "variant": "esp8266-clawd", "fwSeq": 1, "layouts": 3, "pak": 17,
        "desk": 31, "uptime": 5})
}

/// `STYLED_HEAP`: runs firmware that draws style packs.
pub fn styled_heap() -> Value {
    let mut h = new_heap();
    h["fwSeq"] = json!(2);
    h["layouts"] = json!(1);
    h["styleApi"] = json!(1);
    h["style"] = json!("");
    h
}

/// `{**base, **over}`.
pub fn with(base: Value, over: Value) -> Value {
    let mut out = base;
    for (k, v) in over.as_object().expect("an object") {
        out[k] = v.clone();
    }
    out
}

// ---- the fake clock ---------------------------------------------------------------------

struct ClockState {
    heap: Mutex<Value>,
    config: Mutex<Value>,
    status: Mutex<Value>,
    after: Mutex<Value>,
    saved: Mutex<Vec<Value>>,
    styles: Mutex<Vec<Vec<u8>>>,
    packs: Mutex<Vec<String>>,
    style_crc: Mutex<String>,
    pak: Mutex<Option<Vec<u8>>>,
    uploads: AtomicUsize,
    pak_reads: AtomicUsize,
    heap_down: AtomicBool,
    status_down: AtomicBool,
    gate: Option<DigestGate>,
}

/// A fake clock on `127.0.0.1:<port>`.
pub struct FakeClock {
    pub server: FakeHttpServer,
    st: Arc<ClockState>,
}

/// The bytes of the multipart part named `file`.
fn multipart_file(body: &[u8]) -> Option<Vec<u8>> {
    let find = |hay: &[u8], needle: &[u8], from: usize| {
        hay[from..]
            .windows(needle.len())
            .position(|w| w == needle)
            .map(|p| p + from)
    };
    let name = find(body, b"name=\"file\"", 0)?;
    let start = find(body, b"\r\n\r\n", name)? + 4;
    let end = body[start..]
        .windows(4)
        .rposition(|w| w == b"\r\n--")
        .map(|p| p + start)?;
    Some(body[start..end].to_vec())
}

impl ClockState {
    fn answer(&self, req: &RecordedRequest) -> FakeResponse {
        let path = req.path();
        if path == "/api/heap" {
            if self.heap_down.load(Ordering::SeqCst) {
                return FakeResponse::dropped();
            }
            return FakeResponse::json(200, &self.heap.lock().unwrap().clone());
        }
        if let Some(gate) = &self.gate {
            if path != "/api/scan" && !gate.authorized(req) {
                return gate.challenge();
            }
        }
        match (req.method.as_str(), path) {
            ("GET", "/api/config") => FakeResponse::json(200, &self.config.lock().unwrap().clone()),
            ("POST", "/api/config") => {
                let changes = req.body_json().unwrap_or(Value::Null);
                let mut cfg = self.config.lock().unwrap();
                for (k, v) in changes.as_object().cloned().unwrap_or_default() {
                    match (
                        v.as_object(),
                        cfg.get_mut(&k).and_then(Value::as_object_mut),
                    ) {
                        (Some(new), Some(old)) => old.extend(new.clone()),
                        _ => cfg[&k] = v,
                    }
                }
                let reboot = changes.get("wifi").is_some() || changes.get("hostname").is_some();
                self.saved.lock().unwrap().push(changes);
                FakeResponse::json(200, &json!({"ok": true, "reboot": reboot}))
            }
            ("GET", "/api/status") => {
                if self.status_down.load(Ordering::SeqCst) {
                    return FakeResponse::dropped();
                }
                FakeResponse::json(200, &self.status.lock().unwrap().clone())
            }
            ("GET", "/api/scan") => FakeResponse::json(
                200,
                &json!([{"ssid": "A", "rssi": -70, "enc": true}, {"ssid": "A", "rssi": -50, "enc": true},
                    {"ssid": "", "rssi": -40}, {"ssid": "B", "rssi": -60, "enc": false}]),
            ),
            ("POST", "/update") => {
                self.uploads.fetch_add(1, Ordering::SeqCst);
                *self.heap.lock().unwrap() = self.after.lock().unwrap().clone();
                FakeResponse::text(200, "OK")
            }
            ("POST", "/api/reboot" | "/api/factory" | "/api/import") => {
                FakeResponse::json(200, &json!({"ok": true}))
            }
            ("GET", "/api/export") => FakeResponse::json(200, &json!({"hostname": "clawd"})),
            ("GET", "/api/style") => {
                let heap = self.heap.lock().unwrap();
                let id = heap
                    .get("style")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_owned();
                FakeResponse::json(
                    200,
                    &json!({"api": 1, "active": !id.is_empty(), "id": id,
                        "crc": self.style_crc.lock().unwrap().clone()}),
                )
            }
            ("POST", "/api/style") => {
                let Some(pack) = multipart_file(&req.body) else {
                    return FakeResponse::text(400, "no file part");
                };
                let id: String = pack[16..32]
                    .iter()
                    .take_while(|b| **b != 0)
                    .map(|b| *b as char)
                    .collect();
                self.heap.lock().unwrap()["style"] = json!(id);
                self.styles.lock().unwrap().push(pack);
                FakeResponse::json(
                    200,
                    &json!({"ok": true, "style": {"api": 1, "active": true, "id": id}}),
                )
            }
            ("DELETE", "/api/style") => {
                self.heap.lock().unwrap()["style"] = json!("");
                self.styles.lock().unwrap().push(Vec::new());
                FakeResponse::json(
                    200,
                    &json!({"ok": true, "style": {"api": 1, "active": false}}),
                )
            }
            ("POST", "/api/clawdpak" | "/api/deskpak") => {
                let Some(data) = multipart_file(&req.body) else {
                    return FakeResponse::text(400, "no file part");
                };
                let clawd = path == "/api/clawdpak";
                let crc = crc32_hex(&data);
                {
                    let mut heap = self.heap.lock().unwrap();
                    heap[if clawd { "pakCrc" } else { "deskCrc" }] = json!(crc);
                    heap[if clawd { "pak" } else { "desk" }] = json!(17);
                }
                self.packs
                    .lock()
                    .unwrap()
                    .push(if clawd { "clawdpak" } else { "deskpak" }.into());
                FakeResponse::json(200, &json!({"ok": true, "anims": 1, "crc": crc}))
            }
            ("GET", "/api/fs/get") => {
                self.pak_reads.fetch_add(1, Ordering::SeqCst);
                match self.pak.lock().unwrap().clone() {
                    Some(bytes) => FakeResponse::bytes(200, bytes),
                    None => FakeResponse::status(404),
                }
            }
            _ => FakeResponse::status(404),
        }
    }
}

impl FakeClock {
    pub fn start(heap: Value, password: Option<&str>) -> Self {
        let st = Arc::new(ClockState {
            heap: Mutex::new(heap),
            config: Mutex::new(cfg()),
            status: Mutex::new(
                json!({"mode": "sta", "connected": true, "ssid": "Home", "rssi": -58}),
            ),
            after: Mutex::new(new_heap()),
            saved: Mutex::default(),
            styles: Mutex::default(),
            packs: Mutex::default(),
            style_crc: Mutex::default(),
            pak: Mutex::new(None),
            uploads: AtomicUsize::new(0),
            pak_reads: AtomicUsize::new(0),
            heap_down: AtomicBool::new(false),
            status_down: AtomicBool::new(false),
            gate: password.map(|p| DigestGate::new("admin", p)),
        });
        let server = FakeHttpServer::start().expect("fake clock");
        for path in [
            "/api/heap",
            "/api/config",
            "/api/status",
            "/api/scan",
            "/update",
            "/api/reboot",
            "/api/factory",
            "/api/import",
            "/api/export",
            "/api/style",
            "/api/clawdpak",
            "/api/deskpak",
            "/api/fs/get",
        ] {
            let st = Arc::clone(&st);
            server.route_fn("", path, Arc::new(move |req| st.answer(req)));
        }
        Self { server, st }
    }

    pub fn url(&self) -> String {
        self.server.url()
    }

    pub fn port(&self) -> u16 {
        self.server.addr().port()
    }

    pub fn host(&self) -> String {
        format!("127.0.0.1:{}", self.port())
    }

    pub fn set_heap(&self, heap: Value) {
        *self.st.heap.lock().unwrap() = heap;
    }

    pub fn heap(&self) -> Value {
        self.st.heap.lock().unwrap().clone()
    }

    /// What the clock runs after an update (`after`).
    pub fn set_after(&self, heap: Value) {
        *self.st.after.lock().unwrap() = heap;
    }

    pub fn set_config(&self, cfg: Value) {
        *self.st.config.lock().unwrap() = cfg;
    }

    pub fn set_status(&self, status: Value) {
        *self.st.status.lock().unwrap() = status;
    }

    pub fn heap_down(&self, down: bool) {
        self.st.heap_down.store(down, Ordering::SeqCst);
    }

    pub fn status_down(&self, down: bool) {
        self.st.status_down.store(down, Ordering::SeqCst);
    }

    pub fn set_pak(&self, pak: Option<Vec<u8>>) {
        *self.st.pak.lock().unwrap() = pak;
    }

    pub fn set_style_crc(&self, crc: &str) {
        *self.st.style_crc.lock().unwrap() = crc.to_owned();
    }

    /// The bodies of `POST /api/config`.
    pub fn saved(&self) -> Vec<Value> {
        self.st.saved.lock().unwrap().clone()
    }

    /// The style packs sent, `[]` for a clear.
    pub fn styles(&self) -> Vec<Vec<u8>> {
        self.st.styles.lock().unwrap().clone()
    }

    /// The pack kinds sent.
    pub fn packs(&self) -> Vec<String> {
        self.st.packs.lock().unwrap().clone()
    }

    pub fn uploads(&self) -> usize {
        self.st.uploads.load(Ordering::SeqCst)
    }

    pub fn pak_reads(&self) -> usize {
        self.st.pak_reads.load(Ordering::SeqCst)
    }

    /// Requests to this clock so far.
    pub fn requests(&self) -> usize {
        self.server.requests().len()
    }
}

/// A loopback port nobody listens on (a clock that is gone).
pub fn dead_port() -> u16 {
    common::free_port()
}

// ---- bundles --------------------------------------------------------------------------------

/// `write_styles(folder, ids, api)`.
pub fn write_styles(folder: &std::path::Path, ids: &[&str]) {
    std::fs::create_dir_all(folder).unwrap();
    let mut rows = Vec::new();
    for sid in ids {
        let data = make_style_pack(sid);
        std::fs::write(folder.join(format!("{sid}.pak")), &data).unwrap();
        let name: String = sid
            .chars()
            .enumerate()
            .map(|(i, c)| if i == 0 { c.to_ascii_uppercase() } else { c })
            .collect();
        rows.push(
            json!({"id": sid, "name": name, "file": format!("{sid}.pak"), "size": data.len(),
            "crc": format!("{:08x}", crc32(&data[16..])), "light": *sid == "paper"}),
        );
    }
    std::fs::write(
        folder.join("index.json"),
        json!({"api": 1, "styles": rows}).to_string(),
    )
    .unwrap();
}

/// `make_bundle(dir, seq, style_api, styles)`: a bundled firmware on disk.
pub fn make_bundle(dir: &std::path::Path, seq: u64, style_api: u32, styles: &[&str]) -> Bundle {
    std::fs::create_dir_all(dir).unwrap();
    let image = [&[0xe9u8][..], &[0u8; 999]].concat();
    std::fs::write(dir.join("clawd-firmware.bin"), &image).unwrap();
    std::fs::write(
        dir.join("firmware.json"),
        json!({"file": "clawd-firmware.bin", "size": image.len(), "sha256": sha256_hex(&image),
            "variant": "esp8266-clawd", "fwSeq": seq, "layouts": 1, "styleApi": style_api,
            "commit": "abc1234"})
        .to_string(),
    )
    .unwrap();
    if !styles.is_empty() {
        write_styles(&dir.join("styles"), styles);
    }
    load_bundle(&clawd_contracts::Env::empty(), Some(dir)).expect("bundle")
}

// ---- the rig --------------------------------------------------------------------------------

pub struct Rig {
    pub bridge: Bridge,
    pub tray: mpsc::Receiver<ShellCmd>,
    pub tc: TempConfig,
    pub keys: Arc<MemKeyring>,
    pub clock: FakeClock,
}

/// What a test chooses; everything else is the default of `bridge(tmp_path)`.
pub struct Build {
    pub heap: Value,
    pub password: Option<String>,
    pub bundle: Option<Bundle>,
    pub packs: Option<StyleSource>,
    /// `None`: the fake clock; `Some("")`: no clock at all.
    pub base: Option<String>,
    pub key: String,
    pub tab: String,
    pub tc: Option<TempConfig>,
    /// A second window over the paths of a first one (same home, same keyring names).
    pub shared: Option<(AppPaths, OsStoreNames)>,
}

impl Default for Build {
    fn default() -> Self {
        Self::new()
    }
}

impl Build {
    pub fn new() -> Self {
        Self {
            heap: new_heap(),
            password: None,
            bundle: None,
            packs: None,
            base: None,
            key: "clawd".into(),
            tab: String::new(),
            tc: None,
            shared: None,
        }
    }

    pub fn same_home_as(mut self, rig: &Rig) -> Self {
        self.shared = Some((rig.paths(), rig.tc.os_names().clone()));
        self
    }

    pub fn heap(mut self, heap: Value) -> Self {
        self.heap = heap;
        self
    }

    pub fn password(mut self, p: &str) -> Self {
        self.password = Some(p.into());
        self
    }

    pub fn bundle(mut self, b: Bundle) -> Self {
        self.bundle = Some(b);
        self
    }

    pub fn packs(mut self, p: StyleSource) -> Self {
        self.packs = Some(p);
        self
    }

    pub fn base(mut self, base: &str, key: &str) -> Self {
        self.base = Some(base.into());
        self.key = key.into();
        self
    }

    pub fn tab(mut self, tab: &str) -> Self {
        self.tab = tab.into();
        self
    }

    pub fn temp(mut self, tc: TempConfig) -> Self {
        self.tc = Some(tc);
        self
    }

    pub fn build(self) -> Rig {
        let tc = self.tc.unwrap_or_default();
        let clock = FakeClock::start(self.heap, self.password.as_deref());
        let (paths, names) = self
            .shared
            .unwrap_or_else(|| (tc.paths().clone(), tc.os_names().clone()));
        let runner = Arc::new(FakeRunner::new());
        let services = Services::new(
            paths.clone(),
            names,
            Arc::new(Updater::new(
                None,
                UpdaterOptions::new(paths, runner.clone()),
            )),
            Arc::new(Registry::new(common::agent_ctx(&tc, Platform::Darwin))),
            self.packs,
            self.bundle,
            runner,
        );
        let base = self.base.unwrap_or_else(|| clock.url());
        let (shell, tray) = mpsc::channel(32);
        let session = Arc::new(SettingsSession::new(base, self.key, self.tab));
        let keys = Arc::new(MemKeyring::new());
        {
            let mut d = session.data.lock().unwrap();
            let keyring: Arc<dyn Keyring> = keys.clone();
            d.keyring = keyring;
            d.env = Some(clawd_contracts::Env::empty());
            d.search_wait = Duration::ZERO;
            d.ota_settle = Some(Duration::ZERO);
        }
        Rig {
            bridge: Bridge {
                services: Arc::new(services),
                shell,
                jobs: Arc::new(JobState::default()),
                session,
            },
            tray,
            tc,
            keys,
            clock,
        }
    }
}

/// `bridge(tmp_path)`.
pub fn rig() -> Rig {
    Build::new().build()
}

impl Rig {
    /// The paths this window uses (those of `tc`, or of the window it shares a home with).
    pub fn paths(&self) -> AppPaths {
        self.bridge.services.paths.clone()
    }

    /// `edited(**over)`: the page's copy of the clock's settings with some changed (a dict
    /// value updates the nested settings, anything else replaces).
    pub fn edited(&self, over: Value) -> Value {
        let mut v = clock_view(&cfg());
        for (k, val) in over.as_object().expect("an object") {
            match (val.as_object(), v.get_mut(k).and_then(Value::as_object_mut)) {
                (Some(new), Some(old)) => old.extend(new.clone()),
                _ => v[k] = val.clone(),
            }
        }
        v
    }

    /// Everything the tray was told so far.
    pub fn told(&mut self) -> Vec<ShellCmd> {
        let mut out = Vec::new();
        while let Ok(cmd) = self.tray.try_recv() {
            out.push(cmd);
        }
        out
    }
}

/// Poll `cond` until it holds or `secs` pass.
pub async fn wait_for(secs: u64, mut cond: impl FnMut() -> bool) -> bool {
    let end = std::time::Instant::now() + Duration::from_secs(secs);
    while std::time::Instant::now() < end {
        if cond() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    cond()
}

// ---- the renderer, the pak and the zip of the preview tests ---------------------------------

/// The standard base64 alphabet, decoded (the page gets frames as data URLs).
pub fn b64_decode(text: &str) -> Vec<u8> {
    let val = |c: u8| match c {
        b'A'..=b'Z' => c - b'A',
        b'a'..=b'z' => c - b'a' + 26,
        b'0'..=b'9' => c - b'0' + 52,
        b'+' => 62,
        b'/' => 63,
        _ => 64,
    };
    let (mut out, mut acc, mut bits) = (Vec::new(), 0u32, 0u32);
    for c in text.bytes().filter(|c| *c != b'=') {
        acc = (acc << 6) | u32::from(val(c));
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    out
}

/// The program the fake renderer folders run: `fake_clawd_render` of `clawd-firmware`
/// (tests/support), built next to the test programs. A test of another package cannot name its
/// binary, so it is looked up in the target dir and built there when it is missing.
pub fn fake_renderer_exe() -> std::path::PathBuf {
    static EXE: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
    EXE.get_or_init(|| {
        let name = if cfg!(windows) {
            "fake_clawd_render.exe"
        } else {
            "fake_clawd_render"
        };
        let profile_dir = std::env::current_exe()
            .unwrap()
            .parent()
            .and_then(std::path::Path::parent)
            .unwrap()
            .to_path_buf();
        let exe = profile_dir.join(name);
        if !exe.is_file() {
            let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
            let status = std::process::Command::new(cargo)
                .args([
                    "build",
                    "-p",
                    "clawd-firmware",
                    "--bin",
                    "fake_clawd_render",
                ])
                .status()
                .expect("run cargo to build the fake renderer");
            assert!(
                status.success() && exe.is_file(),
                "fake_clawd_render is not built"
            );
        }
        exe
    })
    .clone()
}

pub const FRAME_BYTES: usize = 240 * 240 * 3;

/// A renderer folder as `tools/build_preview.py` writes it, around the fake program: `mode`
/// (read at every run) says what it does.
pub fn fake_renderer_folder(
    root: &std::path::Path,
    mode: &str,
    commit: &str,
    styles: &[&str],
) -> std::path::PathBuf {
    let d = root.join("renderer");
    std::fs::create_dir_all(&d).unwrap();
    write_styles(&d.join("styles"), styles);
    let exe_name = if cfg!(windows) {
        "clawd_render.exe"
    } else {
        "clawd_render"
    };
    std::fs::copy(fake_renderer_exe(), d.join(exe_name)).unwrap();
    std::fs::write(d.join("clawd.pak"), b"pak").unwrap();
    std::fs::write(d.join("mode"), mode).unwrap();
    std::fs::write(
        d.join("preview.json"),
        json!({"exe": exe_name, "pak": "clawd.pak", "commit": commit, "layouts": 1,
            "styleApi": 1, "fwSeq": 2})
        .to_string(),
    )
    .unwrap();
    d
}

/// What `clip` mode prints: a full frame, then a small patch.
pub fn write_clip(folder: &std::path::Path) {
    let frames: [(u16, u16, u16, u16, u16, Vec<u8>); 2] = [
        (400, 0, 0, 240, 240, vec![1u8; FRAME_BYTES]),
        (120, 10, 20, 2, 1, vec![1, 2, 3, 4, 5, 6]),
    ];
    let mut out = b"CLP1".to_vec();
    out.extend_from_slice(&(frames.len() as u16).to_le_bytes());
    for (d, x, y, w, h, rgb) in frames {
        for n in [d, x, y, w, h] {
            out.extend_from_slice(&n.to_le_bytes());
        }
        out.extend_from_slice(&rgb);
    }
    std::fs::write(folder.join("clip.bin"), out).unwrap();
}

/// `make_pak(count, w, h, frames)`: a `clawd.pak` the page's approximate preview accepts.
pub fn make_pak(count: u16, w: u8, h: u8, frames: u16) -> Vec<u8> {
    let fbytes = (usize::from(w) * usize::from(h)).div_ceil(2);
    let mut head = b"CLWD".to_vec();
    head.extend_from_slice(&1u16.to_le_bytes());
    head.extend_from_slice(&count.to_le_bytes());
    head.extend_from_slice(&[0; 4]);
    let (mut descs, mut body) = (Vec::new(), Vec::new());
    let base = 12 + 72 * usize::from(count);
    for i in 0..count {
        let frames_off = base + body.len();
        body.extend(std::iter::repeat_n(0x12u8, fbytes * usize::from(frames)));
        let holds_off = base + body.len();
        for _ in 0..frames {
            body.extend_from_slice(&100u16.to_le_bytes());
        }
        let mut name = format!("anim{i}").into_bytes();
        name.resize(16, 0);
        descs.extend_from_slice(&name);
        descs.extend_from_slice(&[w, h, 0, 0]);
        for n in [frames, 0, frames - 1, fbytes as u16] {
            descs.extend_from_slice(&n.to_le_bytes());
        }
        descs.extend_from_slice(&[0; 32]);
        descs.extend_from_slice(&(frames_off as u32).to_le_bytes());
        descs.extend_from_slice(&(holds_off as u32).to_le_bytes());
        descs.extend_from_slice(&[0; 4]);
    }
    [head, descs, body].concat()
}

/// A zip with stored (uncompressed) entries.
pub fn zip_stored(files: &[(&str, &[u8])]) -> Vec<u8> {
    let (mut out, mut central) = (Vec::new(), Vec::new());
    for (name, data) in files {
        let offset = out.len() as u32;
        let crc = crc32(data);
        let mut common = Vec::new();
        common.extend_from_slice(&20u16.to_le_bytes()); // version needed
        common.extend_from_slice(&0u16.to_le_bytes()); // flags
        common.extend_from_slice(&0u16.to_le_bytes()); // stored
        common.extend_from_slice(&[0; 4]); // time, date
        common.extend_from_slice(&crc.to_le_bytes());
        common.extend_from_slice(&(data.len() as u32).to_le_bytes());
        common.extend_from_slice(&(data.len() as u32).to_le_bytes());
        common.extend_from_slice(&(name.len() as u16).to_le_bytes());
        common.extend_from_slice(&0u16.to_le_bytes()); // extra
        out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        out.extend_from_slice(&common);
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(data);
        central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        central.extend_from_slice(&20u16.to_le_bytes()); // version made by
        central.extend_from_slice(&common);
        central.extend_from_slice(&[0; 2 + 2 + 2]); // comment, disk, internal attributes
        central.extend_from_slice(&(0o100_755u32 << 16).to_le_bytes()); // external attributes
        central.extend_from_slice(&offset.to_le_bytes());
        central.extend_from_slice(name.as_bytes());
    }
    let cd_offset = out.len() as u32;
    let cd_size = central.len() as u32;
    out.extend_from_slice(&central);
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&(files.len() as u16).to_le_bytes());
    out.extend_from_slice(&(files.len() as u16).to_le_bytes());
    out.extend_from_slice(&cd_size.to_le_bytes());
    out.extend_from_slice(&cd_offset.to_le_bytes());
    out.extend_from_slice(&[0; 2]);
    out
}
