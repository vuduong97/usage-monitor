//! Shared builders of the shell tests: pure values, and the real crates over a temp home.
#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use clawd_agents::AgentCtx;
use clawd_app::engine::{EngineConfig, EngineHandle, Status};
use clawd_clock::{ClockFinder, ClockStore};
use clawd_config::read_http_targets;
use clawd_contracts::{
    ClockSource, ClockTarget, Env, Platform, TargetsFn, UsagePayload, EVENT_PORT_ENV,
};
use clawd_store::pause::PauseReader;
use clawd_testkit::clock::FakeMdns;
use clawd_testkit::config::FakeRunner;
use clawd_testkit::{FakeClock, TempConfig};
use serde_json::{json, Value};

pub const NOW: f64 = 1_800_000_000.0;

/// `A = Clock(host="10.0.0.7", name="clawd")`.
pub fn clock_a() -> ClockTarget {
    let mut c = ClockTarget::new("10.0.0.7", ClockSource::Saved);
    c.name = "clawd".to_owned();
    c
}

pub fn payload(v: Value) -> UsagePayload {
    serde_json::from_value(v).expect("payload")
}

/// The `snap()` helper of `test_tray.py`: a known clock is not assumed; `token` is `true`.
pub fn snap() -> Status {
    Status {
        token: Some(true),
        ..Status::default()
    }
}

/// `test_popover.snap()`: connected, 10 % / 41 %.
pub fn popover_snap() -> Status {
    Status {
        clock: Some(clock_a()),
        last_ok: Some(NOW - 5.0),
        token: Some(true),
        payload: Some(payload(
            json!({"s": 10, "sr": 105, "w": 41, "wr": 6965, "acct": "pro", "ok": true}),
        )),
        payload_at: Some(NOW),
        ..Status::default()
    }
}

// ---- the real crates over a temp home -------------------------------------------------

/// A loopback port nobody holds right now (the event server of a test engine listens on it).
pub fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .expect("bind")
        .local_addr()
        .expect("addr")
        .port()
}

/// The agents' surroundings over the temp home: no PATH, no environment.
pub fn agent_ctx(tc: &TempConfig, platform: Platform) -> AgentCtx {
    AgentCtx {
        paths: tc.paths().clone(),
        platform,
        port: Some(47913),
        script: None,
        which: Arc::new(|_| None),
        env: Env::from_map(HashMap::new()),
    }
}

/// An engine configuration that touches nothing outside the temp home: the event server listens
/// on `port`, commands are faked, OS-store names are the test ones.
pub fn engine_config(tc: &TempConfig, port: u16) -> EngineConfig {
    EngineConfig {
        paths: tc.paths().clone(),
        env: Env::from_map(HashMap::from([(
            EVENT_PORT_ENV.to_owned(),
            port.to_string(),
        )])),
        names: tc.os_names().clone(),
        runner: Arc::new(FakeRunner::new()),
        agents: agent_ctx(tc, Platform::Win32),
    }
}

/// `EngineHandle::new` with a hand-driven mDNS and `now` as both the finder's monotonic clock
/// and the clock behind the pause lease (never the real mDNS browser).
pub fn engine_with_fakes(
    cfg: EngineConfig,
    now: &FakeClock,
) -> (EngineHandle, tokio::sync::watch::Receiver<Status>) {
    let manual: TargetsFn = {
        let paths = cfg.paths.clone();
        Arc::new(move || read_http_targets(&paths))
    };
    let finder = Arc::new(ClockFinder::new(
        ClockStore::new(cfg.paths.clock_file()),
        manual,
        Arc::new(FakeMdns::new(&[])),
        now.arc(),
    ));
    let pause = PauseReader::with_clock(&cfg.paths, now.arc());
    EngineHandle::with_parts(cfg, finder, pause)
}

/// Poll `cond` until it holds or `secs` pass (the engine runs on its own threads).
pub fn wait_until(secs: u64, mut cond: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(secs);
    while Instant::now() < deadline {
        if cond() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    cond()
}
