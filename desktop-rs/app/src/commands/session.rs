//! A settings window's session: its mutable state (`SettingsSession.data`) and `Bridge`, the
//! handle every command works through. It is the part of `SettingsApi.__init__` and the private
//! helpers of `settings_api.py:133-420` that more than one command shares: the clock client of
//! this window (`_connect`, `_follow_store`), the feed release (`_check_feed`, `_kick_feed`), the
//! firmware facts (`_fw_info`), the app facts (`_app_info`) and the style helpers.
//!
//! No lock is held across an `await`: commands copy what they need out of `SessionData`, ask the
//! clock, then lock again to store the answer.

use std::collections::BTreeMap;
use std::sync::{Arc, MutexGuard};
use std::time::{Duration, Instant};

use clawd_agents::Registry;
use clawd_clock::{stored_clock, ClockApi, ClockStore, MdnsSource};
use clawd_contracts::{AppPaths, Clock, ClockError, ClockTarget, Env, SystemClock, APP_VERSION};
use clawd_firmware::feed::{self, Feed, Release};
use clawd_firmware::firmware::{self, Bundle};
use clawd_firmware::preview::{self, FrameCache, Renderer, CLASSIC};
use clawd_store::{autostart::Autostart, creds};
use serde_json::{json, Value};
use tokio::sync::mpsc;

use super::CommandCtx;
use crate::state::{lock, JobState, Services, SettingsSession, ShellCmd};

mod schema;

pub use schema::{bad_value, base64, clean, clock_view, err, fail, merged, truthy};

/// The window asks the feed again after this long.
pub const FEED_RECHECK: Duration = Duration::from_secs(600);
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
    /// The bundled firmware as a release, and the feed's release when it is newer (`_release`).
    pub bundle_release: Option<Arc<Release>>,
    pub release: Option<Arc<Release>>,
    /// Last feed check (`_feed_at`), its error text and whether a background check runs.
    pub feed_at: Option<Instant>,
    pub feed_error: String,
    pub feed_pending: bool,
    /// One feed check at a time (`_feed_lock`).
    pub feed_lock: Arc<tokio::sync::Mutex<()>>,
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
            bundle_release: None,
            release: None,
            feed_at: None,
            feed_error: String::new(),
            feed_pending: false,
            feed_lock: Arc::default(),
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
            d.bundle_release = self
                .services
                .bundle
                .clone()
                .map(|b| Arc::new(Release::from_bundle(b)));
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
            creds::get(&self.services.names, &key)
        };
        d.api = Some(Arc::new(ClockApi::new(&base, auth)));
        d.base = base;
        d.key = key;
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

    // ---- firmware source and feed ---------------------------------------------------------

    /// `_src`: the feed's release when it is newer than the bundled firmware, else the bundle.
    pub fn src(&self) -> Option<Arc<Release>> {
        let mut d = self.data();
        self.ensure_connected(&mut d);
        d.release.clone().or_else(|| d.bundle_release.clone())
    }

    /// The bundled firmware (`_bundle`).
    pub fn bundle(&self) -> Option<&Bundle> {
        self.services.bundle.as_ref()
    }

    fn feed_due(&self) -> bool {
        let d = self.data();
        self.services.feed.is_some()
            && !self.job_running()
            && !d.feed_at.is_some_and(|at| at.elapsed() < FEED_RECHECK)
    }

    /// `_kick_feed()`: check the feed in the background; the page polls `fw` meanwhile.
    pub fn kick_feed(&self) {
        if self.data().feed_pending || !self.feed_due() {
            return;
        }
        self.data().feed_pending = true;
        let bridge = self.clone();
        tokio::spawn(async move {
            bridge.check_feed(false).await;
            bridge.data().feed_pending = false;
        });
    }

    /// `_check_feed(force)`: ask the feed for its newest release (at most every `FEED_RECHECK`)
    /// and keep it, with its style packs, when it is newer than the bundle. Failures only show
    /// as text.
    pub async fn check_feed(&self, force: bool) {
        let Some(feed) = self.services.feed.clone() else {
            return;
        };
        if self.job_running() {
            return;
        }
        let lock = Arc::clone(&self.data().feed_lock);
        let _one_at_a_time = if force {
            lock.lock_owned().await
        } else {
            match lock.try_lock_owned() {
                Ok(guard) => guard,
                Err(_) => return,
            }
        };
        self.check_feed_locked(&feed, force).await;
    }

    async fn check_feed_locked(&self, feed: &Feed, force: bool) {
        {
            let mut d = self.data();
            if !force && d.feed_at.is_some_and(|at| at.elapsed() < FEED_RECHECK) {
                return;
            }
            d.feed_at = Some(Instant::now());
        }
        let manifest = match feed.latest().await {
            Ok(m) => {
                self.data().feed_error.clear();
                m
            }
            Err(e) => {
                tracing::warn!("[settings] {e}");
                self.data().feed_error = e.to_string();
                return;
            }
        };
        let bundle_seq = self.bundle().map(|b| b.seq);
        let Some(m) = manifest.filter(|m| bundle_seq.is_none_or(|seq| m.seq > seq)) else {
            self.data().release = None;
            return;
        };
        if self
            .data()
            .release
            .as_ref()
            .is_some_and(|r| r.manifest.as_ref().is_some_and(|rm| rm.seq == m.seq))
        {
            return;
        }
        let mut rel = feed::make_release(&m, Default::default());
        // The gallery needs the packs; a few hundred KB.
        match feed.fetch(&rel, &feed::STYLE_KINDS, None).await {
            Ok(fetched) => rel = fetched,
            Err(e) => tracing::warn!("[settings] style packs from the feed: {e}"),
        }
        let rel = Arc::new(rel);
        self.data().release = Some(Arc::clone(&rel));
        self.use_feed_renderer(feed, &rel).await;
    }

    /// `_use_feed_renderer`: preview with the renderer of the release the gallery now shows,
    /// when it has one for this computer: the one unpacked before if it still checks out, else
    /// downloaded once. Without it the page keeps what it had.
    async fn use_feed_renderer(&self, feed: &Feed, rel: &Release) {
        let Some(m) = rel.manifest.as_ref() else {
            return;
        };
        let platform = feed::renderer_platform();
        let Some(file) = m.renderer(platform) else {
            return;
        };
        let commit = m.commit.trim_end_matches("-dirty").to_owned();
        let root = self.paths().renderer_dir();
        let mut renderer = preview::load_unpacked(&root, &file.sha256, &commit);
        if renderer.is_none() {
            match feed.fetch_renderer(rel, platform).await {
                Ok(Some((_, bytes))) => {
                    if let Err(e) = preview::unpack_renderer(&bytes, &root, &file.sha256) {
                        tracing::warn!("[settings] renderer from the feed: {e}");
                        return;
                    }
                }
                Ok(None) => return,
                Err(e) => {
                    tracing::warn!("[settings] renderer from the feed: {e}");
                    return;
                }
            }
            renderer = preview::load_unpacked(&root, &file.sha256, &commit);
            if renderer.is_none() {
                tracing::warn!(
                    "[settings] renderer from the feed (release {}) does not match it",
                    m.seq
                );
                return;
            }
        }
        // Both at once: a frame of the renderer before never lands in this one's cache.
        let mut d = self.data();
        d.frames = Some(FrameCache::new(FRAME_CACHE_SIZE));
        d.renderer = renderer.map(Arc::new);
        d.renderer_loaded = true;
        d.renderer_generation += 1;
    }

    /// `_bundle_info()`.
    pub fn bundle_info(&self) -> Value {
        let Some(src) = self.src() else {
            return Value::Null;
        };
        let from_feed = self.data().release.is_some();
        let mut out = json!({
            "seq": src.seq, "size": src.size, "commit": src.commit, "styleApi": src.style_api,
            "source": if from_feed { "feed" } else { "app" },
        });
        if let (true, Some(m)) = (from_feed, src.manifest.as_ref()) {
            let date: String = src.date().chars().take(10).collect();
            let packs: serde_json::Map<String, Value> = m
                .files
                .iter()
                .filter(|f| feed::PACK_KINDS.contains(&f.kind.as_str()))
                .map(|f| (f.kind.clone(), json!(f.size)))
                .collect();
            out["notes"] = json!(src.notes());
            out["date"] = json!(date);
            out["packs"] = Value::Object(packs);
        }
        out
    }

    /// `_fw_info()`: the firmware/packs facts the page shows.
    pub fn fw_info(&self) -> Value {
        let src = self.src();
        let (heap, feed_error, feed_busy) = {
            let d = self.data();
            (d.heap.clone(), d.feed_error.clone(), d.feed_pending)
        };
        let bundle = src.as_deref().map(|r| &r.bundle);
        let packs: Vec<Value> = bundle
            .map(|b| {
                b.styles
                    .iter()
                    .map(|p| json!({"id": p.id, "name": p.name, "light": p.light}))
                    .collect()
            })
            .unwrap_or_default();
        let mut state = firmware::update_state(&heap, bundle);
        let plan = feed::plan(&heap, src.as_deref());
        if state == "current"
            && feed::PACK_KINDS
                .iter()
                .any(|k| plan.get(*k).and_then(Value::as_str) == Some("need"))
        {
            state = "packs"; // same image, but a pack differs or is missing
        }
        let active = firmware::clock_style(&heap);
        json!({
            "update": state, "plan": plan, "feedError": feed_error, "feedBusy": feed_busy,
            "styles": {
                "state": firmware::style_state(&heap, bundle),
                "active": if active.is_empty() { CLASSIC.to_owned() } else { active },
                "packs": packs,
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
        self.follow_store(); // the tray's latest find first
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
                if matches!(e, ClockError::AuthNeeded) || !self.follow_store() {
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
        self.kick_feed(); // in the background: the page polls fw() meanwhile
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
