//! The engine (`engine.py`): finds the clock, polls usage, runs the agent event server and pushes
//! everything to the clock, on its own tokio runtime so the tray owns the main thread.
//!
//! * One multi-thread runtime (2 workers, thread name `clawd-engine`) built by
//!   [`EngineHandle::start`]; [`EngineHandle::stop`] ends it (idempotent, commit `03928c8`).
//! * Every loop runs under [`supervise`]: a panic is caught at the task boundary (tokio turns it
//!   into a `JoinError`, the `catch_unwind` of red team #12), logged and the loop starts again
//!   after a backoff (`tray.py:248,297,436`, `gather(return_exceptions=True)` `engine.py:185`).
//!   Sync callbacks on other threads use [`guard`], which is `std::panic::catch_unwind`.
//! * [`EngineHandle::targets`] is empty while a pause lease holds (`engine.py:71-82`), so nothing
//!   competes with a firmware upload.
//!
//! The pure parts (`EngineState`, [`gated_targets`], [`screen_of`], the token and screen-sync
//! loops, [`supervise`]) are unit-tested here; the wiring in `run` needs the real crates
//! (`#[ignore = "needs 03-10"]` tests in `tests/shell_engine.rs`).

use std::any::Any;
use std::future::Future;
use std::io;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use clawd_agents::normalize;
use clawd_clock::{BootWatch, ClockApi, ClockFinder, ClockStore, FinderSnapshot};
use clawd_config::{
    has_claude_token, read_config_value, read_http_targets, read_onoff_setting, read_usage_source,
    CommandRunner, SystemRunner,
};
use clawd_contracts::{
    new_stop, AppPaths, BoxFuture, ClockTarget, Env, MonotonicClock, OsStoreNames, StopRx,
    TargetsFn, UsagePayload, SCREEN_USAGE,
};
use clawd_events::{
    read_event_port, read_pet_config, Brain, EventServer, NormalizeFn, Pet, SharedBrain,
    StatePusher,
};
use clawd_store::pause::PauseReader;
use clawd_usage::{HttpSinks, Publisher, UsageEnv};
use serde_json::Value;
use tokio::sync::watch;
use tokio::task::JoinHandle;

use crate::state::lock;

/// `engine.TOKEN_CHECK_EVERY`.
pub const TOKEN_CHECK_EVERY: Duration = Duration::from_secs(60);
/// `engine.SCREEN_SYNC_EVERY`.
pub const SCREEN_SYNC_EVERY: Duration = Duration::from_secs(300);
/// `engine.SCREEN_SYNC_RETRY`: after a failed sync (desktop.json busy in the settings window).
pub const SCREEN_SYNC_RETRY: Duration = Duration::from_secs(15);
/// How long `stop()` waits for the loops before it aborts them.
pub const STOP_TIMEOUT: Duration = Duration::from_secs(8);

// =============================================================================
// Panic containment
// =============================================================================

/// Restart schedule of a supervised loop.
#[derive(Debug, Clone, Copy)]
pub struct Backoff {
    /// Wait before the first restart; doubles per quick failure.
    pub first: Duration,
    pub max: Duration,
    /// A run that lasted this long counts as healthy: the next wait starts over at `first`.
    pub healthy_after: Duration,
}

/// What the engine and tray loops use.
pub const BACKOFF: Backoff = Backoff {
    first: Duration::from_secs(1),
    max: Duration::from_secs(60),
    healthy_after: Duration::from_secs(60),
};

/// The text of a panic payload (`&str` or `String`, else a placeholder).
pub fn panic_message(payload: &(dyn Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_owned()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "non-string panic payload".to_owned()
    }
}

/// Run a synchronous callback so that a panic cannot cross into the OS event loop: it is logged
/// (the panic hook has already written the stack) and `None` returned.
pub fn guard<R>(what: &str, f: impl FnOnce() -> R) -> Option<R> {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(v) => Some(v),
        Err(p) => {
            tracing::error!("{what} panicked: {}", panic_message(&*p));
            None
        }
    }
}

/// Resolve when `stop` turns true (or its sender is gone).
pub async fn stopped(stop: &mut StopRx) {
    let _ = stop.wait_for(|s| *s).await;
}

/// Sleep for `dur` unless stopped first; true when the stop signal fired.
pub async fn sleep_or_stop(stop: &mut StopRx, dur: Duration) -> bool {
    if *stop.borrow() {
        return true;
    }
    tokio::select! {
        _ = tokio::time::sleep(dur) => false,
        _ = stopped(stop) => true,
    }
}

/// Run the future `make()` builds as its own task; when it panics (or ends before `stop`),
/// log it and run it again after `backoff`. Returns when `stop` fires. Must be awaited inside a
/// tokio runtime.
pub async fn supervise<F, Fut>(name: &'static str, mut stop: StopRx, backoff: Backoff, mut make: F)
where
    F: FnMut() -> Fut,
    Fut: Future<Output = ()> + Send + 'static,
{
    let mut delay = backoff.first;
    while !*stop.borrow() {
        let started = Instant::now();
        let outcome = tokio::spawn(make()).await;
        if *stop.borrow() {
            return;
        }
        match outcome {
            Ok(()) => tracing::warn!("{name} ended before stop; restarting"),
            Err(e) if e.is_panic() => {
                tracing::error!("{name} panicked: {}", panic_message(&*e.into_panic()))
            }
            Err(_) => return, // cancelled: the runtime is going away
        }
        if started.elapsed() >= backoff.healthy_after {
            delay = backoff.first;
        }
        tracing::info!("{name} restarts in {delay:?}");
        if sleep_or_stop(&mut stop, delay).await {
            return;
        }
        delay = (delay * 2).min(backoff.max);
    }
}

/// [`supervise`] as a spawned task (so `run` can join it).
pub fn spawn_supervised<F, Fut>(
    name: &'static str,
    stop: StopRx,
    make: F,
) -> JoinHandle<()>
where
    F: FnMut() -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    tokio::spawn(supervise(name, stop, BACKOFF, make))
}

// =============================================================================
// What the engine knows (pure)
// =============================================================================

/// `engine.snapshot()` as a value: the finder's `describe()` plus the last payload, `token`,
/// `events_on` and `paused`. Input of `tray::status` and `popover::view`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Status {
    /// The clock in use (`None` when `manual_config` or nothing is known).
    pub clock: Option<ClockTarget>,
    pub manual_config: bool,
    pub config_targets: Vec<String>,
    /// Epoch seconds of the last accepted push.
    pub last_ok: Option<f64>,
    pub failing: bool,
    pub payload: Option<UsagePayload>,
    pub payload_at: Option<f64>,
    /// `None` until checked; `Some(false)` = Claude login problem.
    pub token: Option<bool>,
    pub events_on: bool,
    /// Reason of the active pause lease, if any.
    pub paused: Option<String>,
}

/// The mutable facts `Engine` kept as attributes.
#[derive(Debug, Default)]
pub struct EngineState {
    pub payload: Option<UsagePayload>,
    pub payload_at: Option<f64>,
    /// `None` until checked.
    pub token: Option<bool>,
    pub events_on: bool,
    pub paused: Option<String>,
}

impl EngineState {
    /// `Engine._on_payload` (without the file write): keep the payload and read what it says
    /// about the Claude login.
    pub fn on_payload(&mut self, p: &UsagePayload, now: f64) {
        self.payload = Some(p.clone());
        self.payload_at = Some(now);
        if p.src().is_some_and(|s| !s.is_empty()) {
            // Codex usage, or none: says nothing about a Claude login.
            self.token = None;
        } else if p.ok() == Some(false) {
            self.token = Some(false);
        } else if p.contains_key("s") || p.contains_key("w") {
            self.token = Some(true);
        }
    }

    /// Remember the pause reason; true (and a log line) when it changed (`Engine.targets`).
    pub fn note_pause(&mut self, reason: Option<String>) -> bool {
        if reason == self.paused {
            return false;
        }
        match &reason {
            Some(r) => tracing::info!("Pushes paused: {r}"),
            None => tracing::info!("Pushes resumed"),
        }
        self.paused = reason;
        true
    }

    /// `Engine.snapshot()`.
    pub fn status(&self, finder: &FinderSnapshot) -> Status {
        Status {
            clock: finder.clock.clone(),
            manual_config: finder.manual_config,
            config_targets: finder.config_targets.clone(),
            last_ok: finder.last_ok,
            failing: finder.failing,
            payload: self.payload.clone(),
            payload_at: self.payload_at,
            token: self.token,
            events_on: self.events_on,
            paused: self.paused.clone(),
        }
    }
}

/// `Engine.targets()`: no clocks while a pause lease holds, else the finder's.
pub fn gated_targets(
    state: &Mutex<EngineState>,
    pause_reason: Option<String>,
    finder_targets: impl FnOnce() -> Vec<String>,
) -> Vec<String> {
    let paused = pause_reason.is_some();
    lock(state).note_pause(pause_reason);
    if paused {
        Vec::new()
    } else {
        finder_targets()
    }
}

/// `usage.screen` of a clock's `/api/config`: an integer (a bool is not one), else `None`.
pub fn screen_of(config: &Value) -> Option<i64> {
    config.get("usage")?.get("screen")?.as_i64()
}

/// The keychain key of the clock's web password: its name survives a new IP.
pub fn credential_key(clock: &ClockTarget) -> &str {
    if clock.name.is_empty() {
        &clock.host
    } else {
        &clock.name
    }
}

// =============================================================================
// The loops that need no crate beyond closures (tested)
// =============================================================================

/// `Engine._watch_token`: only until the first usage payload (after that the payload says whether
/// a token works, and it also counts accounts this check cannot see, e.g. cliproxy). `check`
/// answers "is a Claude token on this machine".
pub async fn watch_token_loop<F, Fut>(
    mut stop: StopRx,
    state: Arc<Mutex<EngineState>>,
    every: Duration,
    check: F,
) where
    F: Fn() -> Fut,
    Fut: Future<Output = bool>,
{
    while !*stop.borrow() && lock(&state).payload.is_none() {
        let found = check().await;
        {
            let mut st = lock(&state);
            if st.payload.is_none() {
                // Missing locally is not proof: wait for the poll.
                st.token = found.then_some(true);
            }
        }
        if sleep_or_stop(&mut stop, every).await {
            return;
        }
    }
}

/// `Engine._sync_screen`: copy the clock's main screen into `desktop.json` now and then (the
/// settings window tells the engine when it saves; this covers a change made on the clock's web
/// page, and the first run after an update). A failing write retries sooner and never ends the
/// loop.
pub async fn sync_screen_loop<R, RFut>(
    mut stop: StopRx,
    every: Duration,
    retry: Duration,
    paused: impl Fn() -> bool,
    read: R,
    set: Arc<dyn Fn(i64) -> io::Result<()> + Send + Sync>,
) where
    R: Fn() -> RFut,
    RFut: Future<Output = Option<i64>>,
{
    while !*stop.borrow() {
        let mut wait = every;
        if !paused() {
            if let Some(screen) = read().await {
                let set = Arc::clone(&set);
                match tokio::task::spawn_blocking(move || set(screen)).await {
                    Ok(Ok(())) => {}
                    Ok(Err(e)) => {
                        tracing::warn!("Main screen sync failed: {e:?}");
                        wait = retry;
                    }
                    Err(e) => {
                        tracing::warn!("Main screen sync failed: {e:?}");
                        wait = retry;
                    }
                }
            }
        }
        if sleep_or_stop(&mut stop, wait).await {
            return;
        }
    }
}

// =============================================================================
// The handle
// =============================================================================

/// What the engine needs from its surroundings: injected, never read from globals.
#[derive(Clone)]
pub struct EngineConfig {
    pub paths: AppPaths,
    pub env: Env,
    pub names: OsStoreNames,
    pub runner: Arc<dyn CommandRunner>,
}

impl EngineConfig {
    /// This machine: the real home, process environment, OS-store names and command runner.
    pub fn system() -> Self {
        Self {
            paths: AppPaths::system(),
            env: Env::system(),
            names: OsStoreNames::production(),
            runner: Arc::new(SystemRunner),
        }
    }
}

struct Shared {
    cfg: EngineConfig,
    finder: Arc<ClockFinder>,
    pause: PauseReader,
    state: Arc<Mutex<EngineState>>,
    publisher: Mutex<Option<Arc<Publisher>>>,
    stop_tx: watch::Sender<bool>,
    status_tx: watch::Sender<Status>,
}

#[derive(Default)]
struct Running {
    runtime: Option<tokio::runtime::Runtime>,
    run: Option<JoinHandle<()>>,
}

/// Cloneable handle to the engine (`Engine` in Python), stored in `AppState`.
#[derive(Clone)]
pub struct EngineHandle {
    shared: Arc<Shared>,
    running: Arc<Mutex<Running>>,
}

impl EngineHandle {
    /// The production wiring: the finder over `clock.json`, `http_targets` from the config file,
    /// the real mDNS browser, the pause reader over `pause.json`. Returns the handle and the
    /// receiver `AppState.status` keeps.
    pub fn new(cfg: EngineConfig) -> (Self, watch::Receiver<Status>) {
        let manual: TargetsFn = {
            let paths = cfg.paths.clone();
            Arc::new(move || read_http_targets(&paths))
        };
        let finder = Arc::new(ClockFinder::with_defaults(
            ClockStore::new(cfg.paths.clock_file()),
            manual,
        ));
        let pause = PauseReader::new(&cfg.paths);
        Self::with_parts(cfg, finder, pause)
    }

    /// [`Self::new`] with the finder and pause reader supplied (tests).
    pub fn with_parts(
        cfg: EngineConfig,
        finder: Arc<ClockFinder>,
        pause: PauseReader,
    ) -> (Self, watch::Receiver<Status>) {
        let (status_tx, status_rx) = watch::channel(Status::default());
        let (stop_tx, _) = new_stop();
        let shared = Arc::new(Shared {
            cfg,
            finder,
            pause,
            state: Arc::new(Mutex::new(EngineState::default())),
            publisher: Mutex::new(None),
            stop_tx,
            status_tx,
        });
        let handle = Self {
            shared,
            running: Arc::new(Mutex::new(Running::default())),
        };
        (handle, status_rx)
    }

    /// The clock finder (`engine.finder`: the tray's typed address, the settings window's
    /// "Tìm lại").
    pub fn finder(&self) -> &ClockFinder {
        &self.shared.finder
    }

    /// `Engine.call(fn)`: run a finder action; the finder is lock-protected, so this is a direct
    /// call from any thread.
    pub fn call<R>(&self, f: impl FnOnce(&ClockFinder) -> R) -> R {
        f(&self.shared.finder)
    }

    /// `Engine.targets()`: the clocks to push to; none while a pause lease holds.
    pub fn targets(&self) -> Vec<String> {
        self.shared.targets()
    }

    /// `Engine.start()`: build the runtime and run the engine on it. A second call does nothing.
    pub fn start(&self) -> io::Result<()> {
        let mut running = lock(&self.running);
        if running.runtime.is_some() {
            return Ok(());
        }
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("clawd-engine")
            .enable_all()
            .build()?;
        let stop = self.shared.stop_tx.subscribe();
        running.run = Some(runtime.spawn(run(Arc::clone(&self.shared), stop)));
        running.runtime = Some(runtime);
        Ok(())
    }

    /// Run until `stop` fires on the *calling* runtime (`clawd run`).
    pub async fn run_until(&self, stop: StopRx) {
        run(Arc::clone(&self.shared), stop).await;
    }

    /// `Engine.poll_now()`: ask for usage and send it now (the usage source changed). Any thread.
    pub fn poll_now(&self) {
        if let Some(publisher) = lock(&self.shared.publisher).clone() {
            publisher.request_poll();
        }
    }

    /// `Engine.snapshot()`: what the tray and popover show. Also publishes it on the `status`
    /// watch of `AppState`.
    pub fn snapshot(&self) -> Status {
        let status = self.shared.status();
        self.shared.status_tx.send_replace(status.clone());
        status
    }

    /// `Engine.read_clock_screen()`: the clock's main screen (`usage.screen` in its config, read
    /// with the web password from the keychain when it has one); `None` when it can't be read.
    pub async fn read_clock_screen(&self) -> Option<i64> {
        self.shared.read_clock_screen().await
    }

    /// `Engine.stop()`: idempotent (the tray's quit and `main`'s cleanup both call it). Blocks up
    /// to [`STOP_TIMEOUT`]: call it from a thread that is not a runtime worker.
    pub fn stop(&self) {
        self.shared.stop_tx.send_replace(true);
        let (runtime, run) = {
            let mut running = lock(&self.running);
            (running.runtime.take(), running.run.take())
        };
        let Some(runtime) = runtime else { return };
        let finish = move || {
            if let Some(run) = run {
                if runtime
                    .block_on(tokio::time::timeout(STOP_TIMEOUT, run))
                    .is_err()
                {
                    tracing::warn!("Engine did not stop in {STOP_TIMEOUT:?}; abandoning it");
                }
            }
            runtime.shutdown_timeout(Duration::from_secs(2));
        };
        if tokio::runtime::Handle::try_current().is_ok() {
            // Called from async code: a runtime cannot be blocked on from inside one.
            let _ = std::thread::spawn(finish).join();
        } else {
            finish();
        }
    }
}

impl Shared {
    fn targets(&self) -> Vec<String> {
        gated_targets(&self.state, self.pause.reason(), || self.finder.targets())
    }

    fn status(&self) -> Status {
        lock(&self.state).status(&self.finder.describe())
    }

    async fn read_clock_screen(&self) -> Option<i64> {
        let clock = self.finder.current()?;
        let names = self.cfg.names.clone();
        let key = credential_key(&clock).to_owned();
        let auth = tokio::task::spawn_blocking(move || clawd_store::creds::get(&names, &key))
            .await
            .ok()
            .flatten();
        let api = ClockApi::new(&clock.base(), auth);
        screen_of(&api.config().await.ok()?)
    }
}

/// `daemon_running()`: another Clawd daemon (this app, or the old LaunchAgent) owns the event
/// port. The only single-instance check there is (no mutex, no plugin).
pub async fn another_daemon_running(paths: &AppPaths, env: &Env) -> bool {
    let (paths, env) = (paths.clone(), env.clone());
    let Ok(port) = tokio::task::spawn_blocking(move || {
        read_event_port(&env, &|k| read_config_value(&paths, k))
    })
    .await
    else {
        return false;
    };
    clawd_events::daemon_running(port, Duration::from_secs(1)).await
}

// =============================================================================
// run(): the wiring
// =============================================================================

/// What `run` reads from the config file once, on a blocking thread.
struct EventSettings {
    enabled: bool,
    pet: clawd_contracts::PetConfig,
    port: u16,
}

fn read_event_settings(paths: &AppPaths, env: &Env) -> EventSettings {
    let read = |k: &str| read_config_value(paths, k);
    EventSettings {
        enabled: read_onoff_setting(paths, "events", "on") == "on",
        pet: read_pet_config(&read),
        port: read_event_port(env, &read),
    }
}

async fn run(shared: Arc<Shared>, mut stop: StopRx) {
    tracing::info!("=== Clawd desktop (usage + agent events over WiFi) ===");
    let cfg = shared.cfg.clone();
    let finder = Arc::clone(&shared.finder);
    let targets: TargetsFn = {
        let shared = Arc::clone(&shared);
        Arc::new(move || shared.targets())
    };

    // Usage: every push reports to the finder (`TrackingSinks`).
    let sinks = HttpSinks::new(Some({
        let finder = Arc::clone(&finder);
        Arc::new(move |url: &str, ok: bool| finder.report(url, ok))
    }));
    let mut usage_env = UsageEnv::new(cfg.paths.clone(), Arc::clone(&cfg.runner));
    usage_env.env = cfg.env.clone();
    let publisher = Arc::new(Publisher::new(usage_env, sinks, Some(Arc::clone(&targets))));
    *lock(&shared.publisher) = Some(Arc::clone(&publisher));

    let mut tasks: Vec<JoinHandle<()>> = Vec::new();
    tasks.push(spawn_supervised("clock finder", stop.clone(), {
        let (finder, stop) = (Arc::clone(&finder), stop.clone());
        move || {
            let (finder, stop) = (Arc::clone(&finder), stop.clone());
            async move { finder.run(stop).await }
        }
    }));
    tasks.push(spawn_supervised("usage publisher", stop.clone(), {
        let (publisher, stop) = (Arc::clone(&publisher), stop.clone());
        move || {
            let (publisher, stop) = (Arc::clone(&publisher), stop.clone());
            async move { publisher.run(stop).await }
        }
    }));
    tasks.push(spawn_supervised("token watch", stop.clone(), {
        let (shared, stop) = (Arc::clone(&shared), stop.clone());
        move || {
            let (shared, stop) = (Arc::clone(&shared), stop.clone());
            async move {
                let state = Arc::clone(&shared.state);
                watch_token_loop(stop, state, TOKEN_CHECK_EVERY, || {
                    let cfg = shared.cfg.clone();
                    async move {
                        tokio::task::spawn_blocking(move || {
                            read_usage_source(&cfg.paths).starts_with("claude")
                                && has_claude_token(&cfg.paths, cfg.runner.as_ref())
                        })
                        .await
                        .unwrap_or(false)
                    }
                })
                .await;
            }
        }
    }));
    tasks.push(spawn_supervised("screen sync", stop.clone(), {
        let (shared, stop) = (Arc::clone(&shared), stop.clone());
        move || {
            let (shared, stop) = (Arc::clone(&shared), stop.clone());
            async move {
                let paths = shared.cfg.paths.clone();
                let set: Arc<dyn Fn(i64) -> io::Result<()> + Send + Sync> =
                    Arc::new(move |s| clawd_store::prefs::set_clock_screen(&paths, s));
                sync_screen_loop(
                    stop,
                    SCREEN_SYNC_EVERY,
                    SCREEN_SYNC_RETRY,
                    || shared.pause.reason().is_some(),
                    || shared.read_clock_screen(),
                    set,
                )
                .await;
            }
        }
    }));

    // Agent events: the brain, the loopback server, the pusher.
    let settings = {
        let (paths, env) = (cfg.paths.clone(), cfg.env.clone());
        tokio::task::spawn_blocking(move || read_event_settings(&paths, &env)).await
    };
    let mut server: Option<EventServer> = None;
    let mut brain: Option<SharedBrain> = None;
    let mut pusher: Option<Arc<StatePusher>> = None;
    if let Ok(s) = settings {
        if s.enabled {
            let paths = cfg.paths.clone();
            let the_brain = Brain::new(
                Pet::new(s.pet, Some(paths.pet_file())),
                Box::new(move || {
                    clawd_store::prefs::clock_screen(&paths) == Some(SCREEN_USAGE)
                }),
            )
            .shared();
            let normalize_fn: NormalizeFn = Arc::new(normalize);
            let mut srv =
                EventServer::new(Arc::clone(&the_brain), normalize_fn).with_port(s.port);
            match srv.start().await {
                Ok(()) => {
                    lock(&shared.state).events_on = true;
                    let push = Arc::new(StatePusher::new(
                        Arc::clone(&the_brain),
                        Arc::clone(&targets),
                    ));
                    tasks.push(spawn_supervised("state pusher", stop.clone(), {
                        let (push, stop) = (Arc::clone(&push), stop.clone());
                        move || {
                            let (push, stop) = (Arc::clone(&push), stop.clone());
                            async move { push.run(stop).await }
                        }
                    }));
                    pusher = Some(push);
                    server = Some(srv);
                }
                Err(e) => tracing::warn!("Event server not started ({e}); agent events disabled"),
            }
            brain = Some(the_brain);
        }
    }

    // Every delivered payload: the status, the settings window's copy, the brain's usage.
    {
        let (shared, brain) = (Arc::clone(&shared), brain.clone());
        publisher.set_on_payload(Arc::new(move |p: &UsagePayload| {
            lock(&shared.state).on_payload(p, now_epoch());
            if let Some(brain) = &brain {
                lock(brain).usage = Some(p.clone());
            }
            let (paths, p) = (shared.cfg.paths.clone(), p.clone());
            // The settings window draws the clock from this file: keep it off the loop thread.
            tokio::task::spawn_blocking(move || {
                if let Err(e) = clawd_store::paths::set_usage_payload(&paths, &p, None) {
                    tracing::warn!("Could not keep the usage payload for the settings window: {e}");
                }
            });
        }));
    }

    // A restarted clock lost both the usage bars and Clawd's state.
    let on_restart: clawd_clock::OnRestart = {
        let (publisher, brain, pusher) = (Arc::clone(&publisher), brain.clone(), pusher.clone());
        Arc::new(move |url: String, _uptime: u64| -> BoxFuture<bool> {
            let (publisher, brain, pusher) = (Arc::clone(&publisher), brain.clone(), pusher.clone());
            Box::pin(async move {
                if let (Some(pusher), Some(brain)) = (&pusher, &brain) {
                    pusher.invalidate();
                    let wake = Arc::clone(&lock(brain).wake);
                    wake.notify_one();
                }
                publisher.resend(&url).await
            })
        })
    };
    tasks.push(spawn_supervised("boot watch", stop.clone(), {
        let (targets, stop) = (Arc::clone(&targets), stop.clone());
        move || {
            let watch = BootWatch::new(
                Arc::clone(&targets),
                Arc::clone(&on_restart),
                Arc::new(MonotonicClock::new()),
            );
            let stop = stop.clone();
            async move { watch.run(stop).await }
        }
    }));

    stopped(&mut stop).await;
    let deadline = tokio::time::Instant::now() + STOP_TIMEOUT;
    for task in tasks {
        let abort = task.abort_handle();
        if tokio::time::timeout_at(deadline, task).await.is_err() {
            abort.abort();
        }
    }
    if let Some(mut server) = server {
        server.close().await;
    }
    tracing::info!("Stopped");
}

fn now_epoch() -> f64 {
    use clawd_contracts::Clock;
    clawd_contracts::SystemClock.now()
}
