//! A settings window's session: its mutable state (`SettingsSession.data`) and `Bridge`, the
//! handle every command works through. It is the part of `SettingsApi.__init__` and the private
//! helpers of `settings_api.py:133-420` that more than one command shares: the clock client of
//! this window (`_connect`, `_follow_store`), the remote style packs (`check_styles`,
//! `kick_styles`), the firmware facts (`_fw_info`), the app facts (`_app_info`) and the style
//! helpers.
//!
//! No lock is held across an `await`: commands copy what they need out of `SessionData`, ask the
//! clock, then lock again to store the answer.

use std::collections::BTreeMap;
use std::sync::{Arc, MutexGuard};
use std::time::{Duration, Instant};

use clawd_agents::Registry;
use clawd_clock::{stored_clock, ClockApi, ClockStore, MdnsSource};
use clawd_contracts::{AppPaths, Clock, ClockError, ClockTarget, Env, SystemClock, APP_VERSION};
use clawd_firmware::firmware::{self, Bundle};
use clawd_firmware::packs::{self, RemoteStyles, StyleSource};
use clawd_firmware::preview::{FrameCache, Renderer, CLASSIC};
use clawd_store::autostart::Autostart;
use clawd_store::creds::{self, Keyring, OsKeyring};
use serde_json::{json, Value};
use tokio::sync::mpsc;

use super::CommandCtx;
use crate::state::{lock, JobState, Services, SettingsSession, ShellCmd};

mod schema;

pub use schema::{bad_value, base64, clean, clock_view, err, fail, merged, truthy};

/// "Tìm lại" waits this long: the saved address again, or an mDNS answer.
pub const SEARCH_WAIT: Duration = Duration::from_secs(15);
/// The window asks for remote style packs again after this long.
pub const STYLES_RECHECK: Duration = Duration::from_secs(600);
/// `TYPE_IP`: what to do when no clock can be found.
pub const TYPE_IP: &str =
    "Nhập IP hiện trên màn hình đồng hồ (lúc khởi động) vào ô Nhập địa chỉ rồi bấm Dùng.";
/// The most frames the preview cache keeps (`preview.CACHE_MAX`).
pub const FRAME_CACHE_SIZE: usize = 32;

/// Makes the mDNS browser of the setup wizard (`mdns_factory`); tests pass a fake.
pub type MdnsFactory = Arc<dyn Fn() -> Arc<dyn MdnsSource> + Send + Sync>;

/// Per-window state; `Default` is an empty session (nothing connected, nothing loaded).
pub struct SessionData {
    /// The clock client for this window (`SettingsApi._api`); `None` until first use.
    pub api: Option<Arc<ClockApi>>,
    /// The clock's address (`_base`) and the keychain key of its web password (`_key`).
    pub base: String,
    pub key: String,
    /// Last `/api/heap` (`_heap`).
    pub heap: Value,
    /// Last `/api/config` (`_cfg`).
    pub cfg: Value,
    /// The opening tab was already handed to the page (`_tab` is read once).
    pub tab_taken: bool,
    /// The bundled firmware as the window shows it: its own style packs, or the remote ones
    /// ([`packs::choose`]); and those remote packs when they are in use.
    pub view: Option<Arc<Bundle>>,
    pub remote: Option<Arc<RemoteStyles>>,
    /// Last remote packs check, its error text and whether a background check runs.
    pub styles_at: Option<Instant>,
    pub styles_error: String,
    pub styles_pending: bool,
    /// One remote packs check at a time.
    pub styles_lock: Arc<tokio::sync::Mutex<()>>,
    /// The renderer that draws the clock's screen, whether the bundled one was looked for, the
    /// frames it made and which renderer they belong to (`_renderer`, `_frames`).
    pub renderer: Option<Arc<Renderer>>,
    pub renderer_loaded: bool,
    pub frames: Option<FrameCache>,
    pub renderer_generation: u64,
    /// When reading `/clawd.pak` off the clock last failed (`_pak_failed`), epoch seconds.
    pub pak_failed: Option<f64>,
    /// The wizard's mDNS browser and the clocks it resolved, by instance name.
    pub mdns: Option<Arc<dyn MdnsSource>>,
    pub mdns_seen: BTreeMap<String, ClockTarget>,
    pub mdns_factory: Option<MdnsFactory>,
    /// What `now` means and where environment variables come from (tests replace both).
    pub clock: Arc<dyn Clock>,
    pub env: Option<Env>,
    /// Where the clock's web password is kept: the OS store, or an in-memory one in tests.
    pub keyring: Arc<dyn Keyring>,
    /// How long "Tìm lại" waits for an answer (`SEARCH_WAIT`), and the pause before a firmware
    /// upload (`ota.SETTLE`; `None`: the firmware crate's own).
    pub search_wait: Duration,
    pub ota_settle: Option<Duration>,
}

impl Default for SessionData {
    fn default() -> Self {
        Self {
            api: None,
            base: String::new(),
            key: String::new(),
            heap: json!({}),
            cfg: json!({}),
            tab_taken: false,
            view: None,
            remote: None,
            styles_at: None,
            styles_error: String::new(),
            styles_pending: false,
            styles_lock: Arc::default(),
            renderer: None,
            renderer_loaded: false,
            frames: None,
            renderer_generation: 0,
            pak_failed: None,
            mdns: None,
            mdns_seen: BTreeMap::new(),
            mdns_factory: None,
            clock: Arc::new(SystemClock),
            env: None,
            keyring: Arc::new(OsKeyring),
            search_wait: SEARCH_WAIT,
            ota_settle: None,
        }
    }
}

/// `close_mdns`: stop the wizard's browser (the window manager calls this when the window goes).
pub fn close_mdns(session: &SettingsSession) {
    let mdns = lock(&session.data).mdns.take();
    if let Some(mdns) = mdns {
        mdns.stop();
    }
}

/// `_diff(edited)`: what Save would send (`Err`: the schema refused a value).
pub fn diff(b: &Bridge, edited: &Value) -> Result<Value, String> {
    let cfg = b.data().cfg.clone();
    Ok(clawd_clock::config_diff(&clock_view(&cfg), &clean(edited)?))
}

/// The positional JS argument `i`, `null` when the page sent fewer.
pub fn arg(args: &[Value], i: usize) -> &Value {
    static NULL: Value = Value::Null;
    args.get(i).unwrap_or(&NULL)
}

/// Python's `str(x)` for the values a page sends as an id.
pub fn text_of(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// Why a style could not be shown on the clock.
pub enum StyleError {
    Clock(ClockError),
    /// The pack the app carries is unreadable (`OSError`/`ValueError` in Python).
    Pack(String),
}

impl StyleError {
    /// The failure dict `save` answers with.
    pub fn into_fail(self) -> Value {
        match self {
            StyleError::Clock(e) => err(&e),
            StyleError::Pack(e) => fail(
                "error",
                &format!("Gói kiểu màn hình đi kèm app bị hỏng ({e})."),
            ),
        }
    }
}

/// Clears `styles_pending` when the background check ends.
struct StylesPending(Bridge);

impl Drop for StylesPending {
    fn drop(&mut self) {
        self.0.data().styles_pending = false;
    }
}

/// Everything a command needs, owned so a spawned task can keep it.
#[derive(Clone)]
pub struct Bridge {
    pub services: Arc<Services>,
    pub shell: mpsc::Sender<ShellCmd>,
    pub jobs: Arc<JobState>,
    pub session: Arc<SettingsSession>,
}

impl Bridge {
    pub fn of(ctx: &CommandCtx<'_>) -> Self {
        Self {
            services: Arc::clone(&ctx.state.services),
            shell: ctx.state.shell_tx.clone(),
            jobs: Arc::clone(&ctx.state.jobs),
            session: Arc::clone(&ctx.session),
        }
    }

    pub fn paths(&self) -> &AppPaths {
        &self.services.paths
    }

    pub fn registry(&self) -> &Registry {
        &self.services.registry
    }

    /// `self._notify(msg)`: tell the tray; true when it was delivered.
    pub fn notify(&self, cmd: ShellCmd) -> bool {
        self.shell.try_send(cmd).is_ok()
    }

    pub fn data(&self) -> MutexGuard<'_, SessionData> {
        lock(&self.session.data)
    }

    pub fn now(&self) -> f64 {
        self.data().clock.now()
    }

    pub fn env(&self) -> Env {
        self.data().env.clone().unwrap_or_else(Env::system)
    }

    pub fn store(&self) -> ClockStore {
        ClockStore::new(self.paths().clock_file())
    }

    pub fn job_running(&self) -> bool {
        lock(&self.jobs.job).get("running").is_some_and(truthy)
    }

    /// The first call connects to the clock the window was opened for (`_connect` in `__init__`).
    fn ensure_connected(&self, d: &mut SessionData) {
        if d.api.is_none() {
            self.connect_locked(d, &self.session.base, &self.session.key);
            if d.view.is_none() {
                // remote packs may have been checked before the first connection
                d.view = self.services.bundle.clone().map(Arc::new);
            }
        }
    }

    fn connect_locked(&self, d: &mut SessionData, base: &str, key: &str) {
        let base = base.trim_end_matches('/').to_owned();
        let key = if key.is_empty() {
            base.clone()
        } else {
            key.to_owned()
        };
        let auth = if key.is_empty() {
            None
        } else {
            creds::get_with(&*d.keyring, &self.services.names, &key)
        };
        d.api = Some(Arc::new(ClockApi::new(&base, auth)));
        d.base = base;
        d.key = key;
    }

    /// Run `f` on the blocking pool: keyring, file and prefs work must not stall the async
    /// threads. A panic in `f` is re-raised as the command's panic.
    pub async fn blocking<T: Send + 'static>(
        &self,
        f: impl FnOnce(Bridge) -> T + Send + 'static,
    ) -> T {
        let bridge = self.clone();
        tokio::task::spawn_blocking(move || f(bridge))
            .await
            .unwrap_or_else(|e| std::panic::resume_unwind(e.into_panic()))
    }

    /// `_connect(base, key)`.
    pub fn connect(&self, base: &str, key: &str) {
        let mut d = self.data();
        self.ensure_connected(&mut d);
        self.connect_locked(&mut d, base, key);
    }

    /// The clock client of this window.
    pub fn api(&self) -> Arc<ClockApi> {
        let mut d = self.data();
        self.ensure_connected(&mut d);
        d.api.clone().expect("connected above")
    }

    /// `_base` and `_key`.
    pub fn address(&self) -> (String, String) {
        let mut d = self.data();
        self.ensure_connected(&mut d);
        (d.base.clone(), d.key.clone())
    }

    /// `_follow_store()`: the tray keeps looking for the clock and saves where it finds it in
    /// `clock.json`; use that address when it differs from ours. True if it switched.
    pub fn follow_store(&self) -> bool {
        let (base, key) = self.address();
        match stored_clock(&self.store().load()) {
            Some(c) if c.base().trim_end_matches('/') != base => {
                self.connect(&c.base(), if c.name.is_empty() { &key } else { &c.name });
                true
            }
            _ => false,
        }
    }

    // ---- firmware source and remote style packs ---------------------------------------------

    /// `_src`: the bundled firmware, with the style packs the window shows (the remote ones
    /// when they are newer, [`packs::choose`]).
    pub fn src(&self) -> Option<Arc<Bundle>> {
        let mut d = self.data();
        self.ensure_connected(&mut d);
        d.view.clone()
    }

    /// The bundled firmware (`_bundle`).
    pub fn bundle(&self) -> Option<&Bundle> {
        self.services.bundle.as_ref()
    }

    fn styles_source(&self) -> Option<(&StyleSource, u32)> {
        let api = self.bundle()?.style_api;
        Some((self.services.packs.as_ref()?, api)).filter(|_| api >= 1)
    }

    fn styles_due(&self) -> bool {
        let d = self.data();
        self.styles_source().is_some()
            && !self.job_running()
            && d.styles_at.is_none_or(|at| at.elapsed() >= STYLES_RECHECK)
    }

    /// Check for remote style packs in the background; the page polls `fw` meanwhile.
    pub fn kick_styles(&self) {
        if self.data().styles_pending || !self.styles_due() {
            return;
        }
        self.data().styles_pending = true;
        let bridge = self.clone();
        tokio::spawn(async move {
            // Cleared however the check ends, a panic included.
            let _pending = StylesPending(bridge.clone());
            bridge.check_styles(false).await;
        });
    }

    /// Ask for the newest remote style packs of the bundle's style API (at most every
    /// [`STYLES_RECHECK`] unless `force`) and show them when they are newer than the bundled
    /// ones. Failures only show as text; the bundled packs stay.
    pub async fn check_styles(&self, force: bool) {
        let Some((source, api)) = self.styles_source() else {
            return;
        };
        let source = source.clone();
        if self.job_running() {
            return;
        }
        let lock = Arc::clone(&self.data().styles_lock);
        let _one_at_a_time = if force {
            lock.lock_owned().await
        } else {
            match lock.try_lock_owned() {
                Ok(guard) => guard,
                Err(_) => return,
            }
        };
        {
            let mut d = self.data();
            if !force && d.styles_at.is_some_and(|at| at.elapsed() < STYLES_RECHECK) {
                return;
            }
            d.styles_at = Some(Instant::now());
        }
        match source.latest(api).await {
            Ok(remote) => {
                self.data().styles_error.clear();
                self.use_remote(remote);
            }
            Err(e) => {
                tracing::warn!("[settings] {e}");
                self.data().styles_error = e.to_string();
            }
        }
    }

    /// Show `remote` when it may replace the bundled packs, else the bundled ones; the preview
    /// renderer draws whichever is shown.
    fn use_remote(&self, remote: Option<RemoteStyles>) {
        let Some(bundle) = self.bundle() else {
            return;
        };
        let remote = remote
            .filter(|r| packs::uses_remote(bundle, r))
            .map(Arc::new);
        let mut d = self.data();
        if d.remote.as_deref() == remote.as_deref() {
            return;
        }
        if let Some(r) = &remote {
            tracing::info!(
                "[settings] style packs rev {} ({} packs)",
                r.rev,
                r.styles.len()
            );
        }
        d.view = Some(Arc::new(packs::choose(bundle, remote.as_deref())));
        d.remote = remote;
        // Both at once: a frame of the packs before never lands in this one's cache. The
        // renderer is reloaded on the next preview, with the packs now shown.
        d.frames = Some(FrameCache::new(FRAME_CACHE_SIZE));
        d.renderer = None;
        d.renderer_loaded = false;
        d.renderer_generation += 1;
    }

    /// `renderer` drawing the remote packs in use too (same ids replace the bundled ones).
    pub fn with_remote_styles(&self, renderer: Renderer) -> Renderer {
        let remote = self.data().remote.clone();
        match remote {
            Some(r) => renderer.with_styles(
                r.styles
                    .iter()
                    .filter(|p| p.path.is_file())
                    .map(|p| (p.id.clone(), p.path.clone())),
            ),
            None => renderer,
        }
    }

    /// `_bundle_info()`.
    pub fn bundle_info(&self) -> Value {
        let Some(src) = self.src() else {
            return Value::Null;
        };
        let remote = self.data().remote.is_some();
        let packs: serde_json::Map<String, Value> = src
            .packs
            .iter()
            .map(|p| (p.kind.clone(), json!(p.size)))
            .collect();
        json!({
            "seq": src.seq, "size": src.size, "commit": src.commit, "styleApi": src.style_api,
            "source": "app", "packs": packs,
            "stylesRev": src.style_rev, "stylesSource": if remote { "remote" } else { "app" },
        })
    }

    /// `_fw_info()`: the firmware/packs facts the page shows.
    pub fn fw_info(&self) -> Value {
        let src = self.src();
        let (heap, styles_error, styles_busy) = {
            let d = self.data();
            (d.heap.clone(), d.styles_error.clone(), d.styles_pending)
        };
        let bundle = src.as_deref();
        let list: Vec<Value> = bundle
            .map(|b| {
                b.styles
                    .iter()
                    .map(|p| json!({"id": p.id, "name": p.name, "light": p.light}))
                    .collect()
            })
            .unwrap_or_default();
        let mut state = firmware::update_state(&heap, bundle);
        let plan = packs::plan(&heap, bundle);
        if state == "current"
            && packs::PACK_KINDS
                .iter()
                .any(|k| plan.get(*k).and_then(Value::as_str) == Some("need"))
        {
            state = "packs"; // same image, but a pack differs or is missing
        }
        let active = firmware::clock_style(&heap);
        json!({
            "update": state, "plan": plan, "stylesError": styles_error, "stylesBusy": styles_busy,
            "styles": {
                "state": firmware::style_state(&heap, bundle),
                "active": if active.is_empty() { CLASSIC.to_owned() } else { active },
                "packs": list,
            },
            "seq": firmware::clock_seq(&heap), "bundle": self.bundle_info(),
        })
    }

    /// `_style_arg(style)`: the page's style choice, checked: `None` (leave it), `"classic"` or
    /// a bundled pack id. `Err` for anything else.
    pub fn style_arg(&self, style: &Value) -> Result<Option<String>, String> {
        if style.is_null() {
            return Ok(None);
        }
        let src = self.src();
        let known = style.as_str().filter(|s| {
            *s == CLASSIC
                || src
                    .as_ref()
                    .is_some_and(|r| r.styles.iter().any(|p| p.id == *s))
        });
        known
            .map(|s| Some(s.to_owned()))
            .ok_or_else(|| "unknown style".to_owned())
    }

    /// `_apply_style(style)`: show `style` on the clock: send its pack, or clear the pack for
    /// the classic screen.
    pub async fn apply_style(&self, style: &str) -> Result<(), StyleError> {
        let api = self.api();
        let view = if style == CLASSIC {
            api.clear_style().await.map_err(StyleError::Clock)?
        } else {
            let pack = self
                .src()
                .and_then(|r| r.styles.iter().find(|p| p.id == style).cloned())
                .ok_or_else(|| StyleError::Pack("unknown style".into()))?;
            let bytes = pack.read().map_err(StyleError::Pack)?;
            api.install_style(&bytes).await.map_err(StyleError::Clock)?
        };
        let id = view
            .get("id")
            .map(text_of)
            .filter(|s| s != "null")
            .unwrap_or_default();
        let mut d = self.data();
        if let Some(heap) = d.heap.as_object_mut() {
            heap.insert("style".into(), json!(id));
        }
        Ok(())
    }

    /// `_mirror_screen(cfg)`: the engine stops pushing animations while the main screen is
    /// Usage; hand it the value through the prefs file.
    pub fn mirror_screen(&self, cfg: &Value) {
        if let Some(screen) = cfg.pointer("/usage/screen").and_then(Value::as_i64) {
            let _ = clawd_store::prefs::set_clock_screen(self.paths(), screen);
        }
    }

    /// `_app_info()`: facts about the app itself.
    pub fn app_info(&self) -> Value {
        let (base, key) = self.address();
        let autostart = Autostart::new(self.paths(), self.services.names.clone());
        let manual = self
            .store()
            .load()
            .get("manual_host")
            .map(text_of)
            .unwrap_or_default();
        let manual = if manual == "null" {
            String::new()
        } else {
            manual
        };
        json!({
            "version": APP_VERSION,
            "autostart": autostart.is_enabled(),
            "autostartSupported": autostart.supported(),
            "hooks": clawd_agents::hooks::is_installed(self.paths(), None),
            "log": self.paths().log_file().display().to_string(),
            "clock": base, "key": key, "manual": manual,
            "update": serde_json::to_value(self.services.updater.state()).unwrap_or(Value::Null),
        })
    }

    /// One quick probe of one address (`answers`): no login needed, a 401 still means a clock
    /// is there. Never a scan.
    pub async fn answers(base: &str) -> bool {
        let api = ClockApi::new(base, None);
        matches!(
            tokio::time::timeout(Duration::from_secs(3), api.heap()).await,
            Ok(Ok(_) | Err(ClockError::AuthNeeded))
        )
    }

    /// `state()`: everything the page shows. The tab the window was opened on rides on the
    /// first answer, clock or not.
    pub async fn state(&self) -> Value {
        let r = self.state_inner().await;
        let tab = {
            let mut d = self.data();
            if std::mem::replace(&mut d.tab_taken, true) {
                String::new()
            } else {
                match self.session.tab.as_str() {
                    t @ ("update" | "agents" | "setup") => t.to_owned(),
                    _ => String::new(),
                }
            }
        };
        if tab.is_empty() {
            r
        } else {
            merged(r, json!({"tab": tab}))
        }
    }

    async fn state_inner(&self) -> Value {
        self.services.updater.check_async(false); // the app's own update: needs no clock
        self.blocking(|b| {
            b.follow_store(); // the tray's latest find first (the first call asks the keyring)
        })
        .await;
        let (base, _) = self.address();
        if base.is_empty() {
            return merged(
                fail("noclock", &format!("Chưa có đồng hồ. {TYPE_IP}")),
                json!({"app": self.app_info()}),
            );
        }
        let api = self.api();
        let heap = match api.heap().await {
            Ok(h) => h,
            Err(e) => {
                if matches!(e, ClockError::AuthNeeded) || !self.blocking(|b| b.follow_store()).await
                {
                    return merged(err(&e), json!({"app": self.app_info()}));
                }
                // The tray found it elsewhere while we waited: ask there.
                match self.api().heap().await {
                    Ok(h) => h,
                    Err(e2) => return merged(err(&e2), json!({"app": self.app_info()})),
                }
            }
        };
        self.data().heap = heap.clone();
        self.kick_styles(); // in the background: the page polls fw() meanwhile
        let cfg = match self.api().config().await {
            Ok(c) => c,
            Err(e) => {
                return merged(
                    err(&e),
                    json!({"heap": heap, "fw": self.fw_info(), "app": self.app_info()}),
                );
            }
        };
        self.data().cfg = cfg.clone();
        self.mirror_screen(&cfg);
        json!({
            "ok": true, "heap": heap, "config": cfg, "fw": self.fw_info(),
            "net": self.net().await, "app": self.app_info(),
        })
    }

    /// `_net()`: the network the clock is on now (`{ssid, rssi}`), for the WiFi tab's "in use"
    /// mark. Only decoration: `null` when the clock does not say.
    async fn net(&self) -> Value {
        let Ok(st) = self.api().status().await else {
            return Value::Null;
        };
        let ssid = st
            .get("ssid")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty());
        let connected = st.get("connected").is_some_and(truthy);
        match (st.get("mode").and_then(Value::as_str), connected, ssid) {
            (Some("sta"), true, Some(ssid)) => {
                let rssi = st
                    .get("rssi")
                    .filter(|r| r.is_i64() || r.is_u64())
                    .cloned()
                    .unwrap_or(Value::Null);
                json!({"ssid": ssid, "rssi": rssi})
            }
            _ => Value::Null,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arguments_default_to_null() {
        let args = vec![json!(1)];
        assert_eq!(arg(&args, 0), &json!(1));
        assert_eq!(arg(&args, 3), &Value::Null);
    }

    #[test]
    fn ids_are_stringified_like_python() {
        assert_eq!(text_of(&json!("claude")), "claude");
        assert_eq!(text_of(&json!(7)), "7");
    }

    #[test]
    fn a_new_session_data_is_empty_and_does_not_touch_the_stubs() {
        let d = SessionData::default();
        assert!(d.api.is_none() && d.frames.is_none() && d.mdns.is_none());
        assert_eq!(d.heap, json!({}));
    }
}
