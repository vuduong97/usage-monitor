//! The engine's graph built from the real crates over a temp home, with the two things the
//! engine cannot take from outside swapped for loopback fakes:
//!
//! * the Anthropic usage endpoint (`UsageEnv::usage_url`, which `EngineHandle::run` always sets
//!   to the production URL), and
//! * the loop supervision, so a test can put a panic into one loop.
//!
//! The wiring mirrors `clawd_app::engine::run`: one `Publisher` (usage -> clock `/api/usage`),
//! one `Brain` + `EventServer` + `StatePusher` (hook -> clock `/api/state`). Nothing here
//! touches the real home, keychain, registry, port 47913 or the internet.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use clawd_agents::normalize;
use clawd_contracts::{new_stop, Env, StopRx, SystemClock, TargetsFn};
use clawd_events::server::Limits;
use clawd_events::{
    read_pet_config, Brain, EventServer, NormalizeFn, Pet, SharedBrain, StatePusher,
};
use clawd_testkit::clock::FakeClockDevice;
use clawd_testkit::config::FakeRunner;
use clawd_testkit::usage::{oauth_usage_max, FakeAnthropic};
use clawd_testkit::{FakeClock, FakeResponse, TempConfig};
use clawd_usage::{HttpSinks, PollFn, Publisher, UsageEnv};
use serde_json::{json, Value};
use tokio::runtime::Runtime;
use tokio::sync::watch;
use tokio::task::JoinHandle;

/// 2026-10-11 09:40 UTC: the `now` of the golden usage fixtures.
pub const NOW: f64 = 1_791_711_600.0;
/// A token the fake Anthropic accepts (a fake: no real token is ever committed).
pub const TOKEN: &str = "sk-ant-test-e2e";

/// Poll `cond` every 20 ms until it holds or `secs` pass.
pub fn wait_until(secs: f64, mut cond: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs_f64(secs);
    while Instant::now() < deadline {
        if cond() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    cond()
}

/// A loopback port nobody holds right now.
pub fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .expect("bind")
        .local_addr()
        .expect("addr")
        .port()
}

/// A fake clock firmware that also takes `POST /api/usage` and `POST /api/state`.
pub fn clock_device() -> FakeClockDevice {
    let device = FakeClockDevice::start(None);
    for path in ["/api/usage", "/api/state"] {
        device
            .server()
            .route("POST", path, FakeResponse::json(200, &json!({"ok": true})));
    }
    device
}

/// Every `POST /api/state` body the clock got, parsed.
pub fn state_bodies(device: &FakeClockDevice) -> Vec<Value> {
    device
        .server()
        .requests_to("/api/state")
        .iter()
        .filter_map(|r| r.body_json())
        .collect()
}

/// Every `POST /api/usage` body the clock got, as the raw text.
pub fn usage_bodies(device: &FakeClockDevice) -> Vec<String> {
    device
        .server()
        .requests_to("/api/usage")
        .iter()
        .map(|r| String::from_utf8_lossy(&r.body).into_owned())
        .collect()
}

/// A client without system proxy settings: loopback must never go through one.
pub fn direct_client() -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .build()
        .expect("client")
}

/// What a scenario can change before the rig starts.
#[derive(Default)]
pub struct RigOptions {
    /// Replace the usage poll (a panicking or counting one); default: the real Anthropic poll
    /// against the fake server.
    pub poller: Option<PollFn>,
    /// Event server limits; default: the production ones.
    pub limits: Option<Limits>,
}

/// Fake Anthropic + fake clock + temp home + the engine's loops on their own runtime.
pub struct Rig {
    pub tc: TempConfig,
    /// The clock the usage poll reads (fixed at [`NOW`] so payloads are byte-exact).
    pub usage_clock: FakeClock,
    pub anthropic: FakeAnthropic,
    pub device: FakeClockDevice,
    pub brain: SharedBrain,
    pub publisher: Arc<Publisher>,
    pub pusher: Arc<StatePusher>,
    pub port: u16,
    pub stop_tx: watch::Sender<bool>,
    pub stop: StopRx,
    pub rt: Runtime,
    pub targets: TargetsFn,
    server: Option<EventServer>,
    tasks: Vec<JoinHandle<()>>,
}

impl Rig {
    /// Build everything; the event server listens on a free loopback port. Loops are not
    /// running yet: [`Self::spawn`] them (so a test can supervise them its own way).
    pub fn new(options: RigOptions) -> Self {
        let tc = TempConfig::new();
        let usage_clock = FakeClock::new(NOW);
        let anthropic = FakeAnthropic::start();
        anthropic.usage_ok(&oauth_usage_max());
        let device = clock_device();

        // A Claude config dir holding a token; the usage source is the default (claude).
        let claude_dir = tc.root().join("claude-a");
        std::fs::create_dir_all(&claude_dir).expect("mkdir");
        std::fs::write(
            claude_dir.join(".credentials.json"),
            json!({"claudeAiOauth": {"accessToken": TOKEN}}).to_string(),
        )
        .expect("credentials");
        tc.write_config(
            "config",
            format!("config_dirs = {}\nclock = off\n", claude_dir.display()),
        );

        let usage_url = device.usage_url();
        let targets: TargetsFn = Arc::new(move || vec![usage_url.clone()]);

        let mut usage_env = UsageEnv::new(tc.paths().clone(), Arc::new(FakeRunner::new()));
        usage_env.api_url = anthropic.api_url();
        usage_env.usage_url = anthropic.usage_url();
        usage_env.clock = usage_clock.arc();
        usage_env.env = Env::empty();
        usage_env.client = direct_client();
        usage_env.cliproxy_key_reader = |_| None;
        let sinks = HttpSinks::with_client(direct_client(), None);
        let mut publisher = Publisher::new(usage_env, sinks, Some(Arc::clone(&targets)));
        if let Some(poll) = options.poller {
            publisher = publisher.with_poller(poll);
        }

        let brain = Brain::new(
            Pet::new(read_pet_config(&|_| None), Some(tc.paths().pet_file())),
            Box::new(|| false),
        )
        .shared();
        let normalize_fn: NormalizeFn = Arc::new(normalize);
        let mut server = EventServer::new(Arc::clone(&brain), normalize_fn).with_port(0);
        if let Some(limits) = options.limits {
            server = server.with_limits(limits);
        }
        // Idle 0 s: the machine running the test is never "away".
        let pusher = Arc::new(StatePusher::with_clock(
            Arc::clone(&brain),
            Arc::clone(&targets),
            Arc::new(SystemClock),
            Arc::new(|| Some(0.0)),
        ));

        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(4)
            .enable_all()
            .build()
            .expect("runtime");
        rt.block_on(server.start()).expect("event server starts");
        let (stop_tx, stop) = new_stop();
        Self {
            port: server.port(),
            tc,
            usage_clock,
            anthropic,
            device,
            brain,
            publisher: Arc::new(publisher),
            pusher,
            stop_tx,
            stop,
            rt,
            targets,
            server: Some(server),
            tasks: Vec::new(),
        }
    }

    /// Run `future` on the rig's runtime; it is joined by [`Self::shutdown`].
    pub fn spawn(&mut self, future: impl std::future::Future<Output = ()> + Send + 'static) {
        self.tasks.push(self.rt.spawn(future));
    }

    /// The usage publisher and the state pusher, each in its own task, unsupervised.
    pub fn spawn_loops(&mut self) {
        let (publisher, stop) = (Arc::clone(&self.publisher), self.stop.clone());
        self.spawn(async move { publisher.run(stop).await });
        let (pusher, stop) = (Arc::clone(&self.pusher), self.stop.clone());
        self.spawn(async move { pusher.run(stop).await });
    }

    /// The hook port as the hook script would use it.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Stop the loops, join them and close the server. Safe to call twice.
    pub fn shutdown(&mut self) {
        self.stop_tx.send_replace(true);
        let tasks = std::mem::take(&mut self.tasks);
        let server = self.server.take();
        self.rt.block_on(async move {
            for task in tasks {
                let _ = tokio::time::timeout(Duration::from_secs(8), task).await;
            }
            if let Some(mut server) = server {
                server.close().await;
            }
        });
    }
}

impl Drop for Rig {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// A poller that counts its calls and panics on the first `panics` of them, then answers
/// `payload`.
pub fn flaky_poller(panics: usize, payload: Value) -> (PollFn, Arc<AtomicUsize>) {
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&calls);
    let poll: PollFn = Arc::new(move || {
        let n = counter.fetch_add(1, Ordering::SeqCst);
        let payload = payload.clone();
        Box::pin(async move {
            if n < panics {
                panic!("injected poll failure #{n}");
            }
            let payload = serde_json::from_value(payload).expect("payload");
            (Some(payload), false)
        })
    });
    (poll, calls)
}

/// `Env` with the event port set, like the engine tests do.
pub fn env_with_port(port: u16) -> Env {
    Env::from_map(HashMap::from([(
        clawd_contracts::EVENT_PORT_ENV.to_owned(),
        port.to_string(),
    )]))
}
