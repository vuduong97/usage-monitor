//! `discovery.ClockFinder`: find the clock so nobody types an IP more than once.
//!
//! Order of preference, re-evaluated every couple of seconds:
//! 1. A manual address: `http_targets` in the config file, or one typed in the tray / settings
//!    window. Used as-is.
//! 2. The remembered clock (`clock.json`), looked up by its mDNS name so a new IP from the
//!    router is followed; its last address is tried right away at startup.
//! 3. The first clock that answers mDNS; it becomes the remembered one.
//!
//! Push results feed back through [`ClockFinder::report`]: a clock that fails `FAIL_LIMIT` (3)
//! pushes in a row is dropped for `BAD_HOLD` (60 s), then its address is tried again.
//! Every method is `&self` (lock-protected inside): the engine, tray and settings call it
//! from different tasks.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use clawd_contracts::{
    Clock, ClockSource, ClockTarget, MonotonicClock, StopRx, SystemClock, TargetsFn,
};
use serde_json::{json, Map, Value};

use crate::clock::manual_clock;
use crate::mdns::{MdnsBrowser, MdnsSource};
use crate::pyval;
use crate::store::{stored_clock, ClockStore};

/// `discovery.FAIL_LIMIT`.
pub const FAIL_LIMIT: u32 = 3;
/// `discovery.BAD_HOLD` (seconds).
pub const BAD_HOLD: f64 = 60.0;
/// `discovery.TICK` (seconds).
pub const TICK: f64 = 2.0;

/// `ClockFinder.describe()`: the snapshot for the tray menu and the engine's status.
#[derive(Debug, Clone, PartialEq)]
pub struct FinderSnapshot {
    /// `http_targets` is set in the config file.
    pub manual_config: bool,
    /// The clock in use; `None` when `manual_config` or nothing is known yet.
    pub clock: Option<ClockTarget>,
    pub config_targets: Vec<String>,
    /// Wall time (epoch s) of the last accepted push.
    pub last_ok: Option<f64>,
    /// `FAIL_LIMIT` failed pushes in a row, or no clock and no manual config.
    pub failing: bool,
    /// `ClockFinder.searching`: nothing found yet, mDNS still being tried.
    pub searching: bool,
}

/// The mutable part of `ClockFinder` (`self._lock` guards it).
struct State {
    current: Option<ClockTarget>,
    failures: u32,
    last_ok: Option<f64>,
    searching: bool,
    /// host -> retry after (monotonic seconds).
    bad: HashMap<String, f64>,
    seen_change: Option<u64>,
    /// instance name -> resolved clock, in name order (`names()` is sorted).
    mdns: BTreeMap<String, ClockTarget>,
}

/// `discovery.ClockFinder`.
pub struct ClockFinder {
    store: ClockStore,
    manual: TargetsFn,
    browser: Arc<dyn MdnsSource>,
    clock: Arc<dyn Clock>,
    /// `time.time()` for `last_ok`.
    wall: Arc<dyn Clock>,
    state: Mutex<State>,
}

impl ClockFinder {
    /// `ClockFinder(store, manual, browser, clock)`: `manual` returns the config's
    /// `http_targets`; `clock` is the monotonic clock for `BAD_HOLD`.
    pub fn new(
        store: ClockStore,
        manual: TargetsFn,
        browser: Arc<dyn MdnsSource>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        // Tried at once (mDNS confirms or corrects it shortly): typed address, else the
        // remembered one.
        let current = stored_clock(&store.load());
        Self {
            store,
            manual,
            browser,
            clock,
            wall: Arc::new(SystemClock),
            state: Mutex::new(State {
                current,
                failures: 0,
                last_ok: None,
                searching: true,
                bad: HashMap::new(),
                seen_change: None,
                mdns: BTreeMap::new(),
            }),
        }
    }

    /// Production wiring: the real [`crate::MdnsBrowser`] and a monotonic system clock.
    pub fn with_defaults(store: ClockStore, manual: TargetsFn) -> Self {
        Self::new(
            store,
            manual,
            Arc::new(MdnsBrowser::new()),
            Arc::new(MonotonicClock::new()),
        )
    }

    /// Replace the wall clock behind `last_ok` (default [`SystemClock`]); for tests.
    pub fn with_wall_clock(mut self, wall: Arc<dyn Clock>) -> Self {
        self.wall = wall;
        self
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// `ClockFinder.store`: the remembered-clock file (the tray's typed-address default reads it).
    pub fn store(&self) -> &ClockStore {
        &self.store
    }

    /// `ClockFinder.current`: the clock tried now.
    pub fn current(&self) -> Option<ClockTarget> {
        self.lock().current.clone()
    }

    /// `ClockFinder.targets()`: the `/api/usage` URLs to push to: the config's `http_targets`
    /// when set, else the current clock's.
    pub fn targets(&self) -> Vec<String> {
        let configured = (self.manual)();
        if !configured.is_empty() {
            return configured;
        }
        self.lock()
            .current
            .iter()
            .map(ClockTarget::usage_url)
            .collect()
    }

    /// `ClockFinder.describe()`.
    pub fn describe(&self) -> FinderSnapshot {
        let configured = (self.manual)();
        let st = self.lock();
        let manual_config = !configured.is_empty();
        FinderSnapshot {
            manual_config,
            clock: if manual_config {
                None
            } else {
                st.current.clone()
            },
            failing: st.failures >= FAIL_LIMIT || (st.current.is_none() && !manual_config),
            config_targets: configured,
            last_ok: st.last_ok,
            searching: st.searching,
        }
    }

    /// `ClockFinder.report(url, ok)`: feedback from a usage push (fed by
    /// `clawd_usage::ResultHook`).
    pub fn report(&self, url: &str, ok: bool) {
        let mut st = self.lock();
        let Some(cur) = st.current.clone().filter(|c| c.usage_url() == url) else {
            if ok {
                st.last_ok = Some(self.wall.now());
            }
            return;
        };
        if ok {
            st.failures = 0;
            st.last_ok = Some(self.wall.now());
            if cur.source == ClockSource::Saved {
                self.remember(&cur);
            }
            return;
        }
        st.failures += 1;
        if st.failures >= FAIL_LIMIT && cur.source != ClockSource::Manual {
            tracing::info!(
                "Clock {} not answering; trying its address again in {BAD_HOLD:.0} s",
                cur.label()
            );
            st.bad.insert(cur.host, self.clock.now() + BAD_HOLD);
            st.current = None;
            st.failures = 0;
        }
    }

    // ---- tray actions ----------------------------------------------------------------

    /// `ClockFinder.set_manual(raw)`: the user typed an address (tray "Nhập địa chỉ đồng hồ…",
    /// settings); pins it in `clock.json` (`manual_host`). `None` when `raw` is not an address.
    pub fn set_manual(&self, raw: &str) -> Option<ClockTarget> {
        let c = manual_clock(raw)?;
        {
            let mut st = self.lock();
            let mut data = self.store.load();
            if let Some(map) = data.as_object_mut() {
                map.insert("manual_host".into(), Value::String(raw.trim().to_owned()));
            }
            self.store.save(&data);
            st.current = Some(c.clone());
            st.failures = 0;
        }
        tracing::info!("Clock set by hand: {}", c.label());
        Some(c)
    }

    /// `ClockFinder.use(c)` (`use` is a Rust keyword): the clock the user picked from the ones
    /// mDNS sees (setup wizard): remembered by name, not pinned, so a new DHCP address follows it.
    pub fn use_clock(&self, c: &ClockTarget) {
        {
            let mut st = self.lock();
            self.store.save(&remembered_json(c));
            st.current = Some(c.clone());
            st.failures = 0;
            st.bad.remove(&c.host);
        }
        tracing::info!("Clock picked: {}", c.label());
    }

    /// `ClockFinder.retry()` ("Tìm lại"): keep the address, try it again now and let mDNS replace
    /// it; a typed address loses its pin, not its value.
    pub fn retry(&self) {
        let c = {
            let mut st = self.lock();
            let c = self.store.unpin();
            st.current = c.clone();
            st.failures = 0;
            st.bad.clear();
            st.mdns.clear();
            st.seen_change = None;
            c
        };
        match c {
            Some(c) => tracing::info!("Trying the clock again: {}", c.label()),
            None => tracing::info!("Looking for a clock (mDNS only, no address yet)"),
        }
    }

    // ---- the loop --------------------------------------------------------------------

    /// `ClockFinder._remember`.
    fn remember(&self, c: &ClockTarget) {
        let data = self.store.load();
        if data.get("manual_host").is_some_and(pyval::truthy) {
            return;
        }
        // A clock without a name (typed once) keeps the old name only for the same address.
        let name = if !c.name.is_empty() {
            c.name.clone()
        } else if data
            .get("host")
            .is_some_and(|h| matches!(h, Value::String(s) if *s == c.host))
        {
            pyval::str_or_empty(data.get("name"))
        } else {
            String::new()
        };
        let new = json!({"name": name, "host": c.host, "port": c.port, "version": c.version});
        let Some(new_map) = new.as_object() else {
            return;
        };
        let differs = new_map
            .iter()
            .any(|(k, v)| !pyval::eq(data.get(k).unwrap_or(&Value::Null), v));
        let extra_keys = data
            .as_object()
            .is_some_and(|d| d.keys().any(|k| !new_map.contains_key(k)));
        if differs || extra_keys {
            // also drops old keys
            self.store.save(&new);
        }
    }

    /// `ClockFinder._adopt`.
    fn adopt(&self, st: &mut State, c: &ClockTarget, why: &str) {
        if st.current.as_ref() != Some(c) {
            let version = if c.version.is_empty() {
                String::new()
            } else {
                format!(" v{}", c.version)
            };
            tracing::info!("Clock {why}: {}{version}", c.label());
        }
        st.current = Some(c.clone());
        st.failures = 0;
        self.remember(c);
    }

    /// `ClockFinder._refresh_mdns`: resolve the names mDNS sees (blocking, off the runtime).
    async fn refresh_mdns(&self) {
        let names = self.browser.names();
        let change = self.browser.changed();
        {
            let st = self.lock();
            if st.seen_change == Some(change) && names.iter().all(|n| st.mdns.contains_key(n)) {
                return;
            }
        }
        self.lock().seen_change = Some(change);
        let mut fresh = BTreeMap::new();
        for name in names {
            let browser = Arc::clone(&self.browser);
            let asked = name.clone();
            let resolved = tokio::task::spawn_blocking(move || browser.resolve(&asked))
                .await
                .ok()
                .flatten();
            if let Some(c) = resolved {
                fresh.insert(name, c);
            }
        }
        let mut st = self.lock();
        // A fresh mDNS answer clears a hold on that address.
        for c in fresh.values() {
            st.bad.remove(&c.host);
        }
        st.mdns = fresh;
    }

    /// `ClockFinder._choose`: pick a clock from what mDNS knows. True when one is in use.
    fn choose(&self, now: f64) -> bool {
        let mut st = self.lock();
        let cur = st.current.clone();
        if cur
            .as_ref()
            .is_some_and(|c| c.source == ClockSource::Manual)
        {
            return true;
        }
        let saved = self.store.load();
        let mut want = pyval::str_or_empty(saved.get("name"));
        if want.is_empty() {
            want = cur.as_ref().map(|c| c.name.clone()).unwrap_or_default();
        }
        let held = |st: &State, host: &str| st.bad.get(host).copied().unwrap_or(0.0) > now;
        let mdns: Vec<ClockTarget> = st
            .mdns
            .values()
            .filter(|c| !held(&st, &c.host))
            .cloned()
            .collect();
        if !want.is_empty() {
            if let Some(m) = mdns.iter().find(|c| c.name == want) {
                let same = cur.as_ref().is_some_and(|c| {
                    (&c.host, c.port, c.source) == (&m.host, m.port, ClockSource::Mdns)
                });
                if !same {
                    let why = if cur.is_none() {
                        "found"
                    } else {
                        "address updated"
                    };
                    self.adopt(&mut st, m, why);
                }
                return true;
            }
        } else if let Some(c) = cur.as_ref().filter(|c| c.source == ClockSource::Saved) {
            // An address without a name: take the mDNS name once it shows up there.
            if let Some(m) = mdns.iter().find(|m| m.host == c.host) {
                self.adopt(&mut st, m, "named");
            }
            return true;
        }
        if cur.is_some() {
            return true;
        }
        let saved_host = pyval::str_or_empty(saved.get("host"));
        if !saved_host.is_empty() && !held(&st, &saved_host) {
            let mut c = ClockTarget::new(saved_host, ClockSource::Saved);
            c.port = match saved.get("port") {
                Some(p) if pyval::truthy(p) => pyval::to_port(p).unwrap_or(80),
                _ => 80,
            };
            c.name = pyval::str_or_empty(saved.get("name"));
            self.adopt(&mut st, &c, "retrying");
            return true;
        }
        if want.is_empty() {
            if let Some(first) = mdns.iter().min_by(|a, b| a.name.cmp(&b.name)) {
                self.adopt(&mut st, first, "found");
                return true;
            }
        }
        false
    }

    /// `ClockFinder.step()`: one evaluation (mDNS names -> resolve -> choose/adopt/remember).
    pub async fn step(&self) {
        if !(self.manual)().is_empty() {
            self.lock().searching = false;
            return;
        }
        self.refresh_mdns().await;
        let found = self.choose(self.clock.now());
        self.lock().searching = !found;
    }

    /// `ClockFinder.run(stop)`: `step` every [`TICK`] until stopped; failures are logged.
    pub async fn run(&self, mut stop: StopRx) {
        let browser = Arc::clone(&self.browser);
        if let Ok(false) = tokio::task::spawn_blocking(move || browser.start()).await {
            tracing::info!("mDNS discovery off: using the remembered and typed addresses only");
        }
        while !*stop.borrow() {
            self.step().await;
            tokio::select! {
                stopped = stop.wait_for(|s| *s) => {
                    if stopped.is_err() {
                        break; // the sender is gone: nobody can ask for more
                    }
                }
                _ = tokio::time::sleep(Duration::from_secs_f64(TICK)) => {}
            }
        }
        let browser = Arc::clone(&self.browser);
        let _ = tokio::task::spawn_blocking(move || browser.stop()).await;
    }
}

/// The `clock.json` of a remembered (not pinned) clock.
fn remembered_json(c: &ClockTarget) -> Value {
    let mut map = Map::new();
    map.insert("name".into(), json!(c.name));
    map.insert("host".into(), json!(c.host));
    map.insert("port".into(), json!(c.port));
    map.insert("version".into(), json!(c.version));
    Value::Object(map)
}
