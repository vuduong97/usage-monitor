//! The real `EngineHandle` over a temp home, a hand-driven mDNS and a fake clock firmware, the
//! way `app/tests/shell_engine_wiring.rs` builds it (phase 12), for the scenarios that need the
//! engine's own wiring (`run`): event server on a free port, pusher, publisher with
//! `usage_source = off` (the engine cannot be pointed at a fake Anthropic).

use std::collections::HashMap;
use std::sync::Arc;

use clawd_agents::AgentCtx;
use clawd_app::engine::{EngineConfig, EngineHandle, Status};
use clawd_clock::{ClockFinder, ClockStore};
use clawd_config::read_http_targets;
use clawd_contracts::{Env, Platform, TargetsFn};
use clawd_store::pause::PauseReader;
use clawd_testkit::clock::{FakeClockDevice, FakeMdns};
use clawd_testkit::config::FakeRunner;
use clawd_testkit::store::put_config_file;
use clawd_testkit::{FakeClock, TempConfig};
use tokio::sync::watch;

use crate::rig::{clock_device, env_with_port, free_port};

pub struct EngineRig {
    pub tc: TempConfig,
    pub device: FakeClockDevice,
    pub engine: EngineHandle,
    pub status: watch::Receiver<Status>,
    pub cfg: EngineConfig,
    pub port: u16,
    pub now: FakeClock,
}

impl EngineRig {
    /// `extra_config` is appended to the config file (e.g. `events = off`). The engine is built,
    /// not started.
    pub fn new(extra_config: &str) -> Self {
        Self::on_port(extra_config, free_port())
    }

    /// Same, with the event server on `port` (e.g. one another engine already holds).
    pub fn on_port(extra_config: &str, port: u16) -> Self {
        let tc = TempConfig::new();
        let device = clock_device();
        put_config_file(
            tc.paths(),
            "config",
            Some(&format!("http_targets={}\n{extra_config}", device.url())),
        );
        put_config_file(
            tc.paths(),
            "desktop.json",
            Some(r#"{"usage_source": "off"}"#),
        );
        let cfg = EngineConfig {
            paths: tc.paths().clone(),
            env: env_with_port(port),
            names: tc.os_names().clone(),
            runner: Arc::new(FakeRunner::new()),
            agents: AgentCtx {
                paths: tc.paths().clone(),
                platform: Platform::Win32,
                port: Some(port),
                script: None,
                which: Arc::new(|_| None),
                env: Env::from_map(HashMap::new()),
            },
        };
        let now = FakeClock::new(1_800_000_000.0);
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
        let (engine, status) = EngineHandle::with_parts(cfg.clone(), finder, pause);
        Self {
            tc,
            device,
            engine,
            status,
            cfg,
            port,
            now,
        }
    }
}

impl Drop for EngineRig {
    fn drop(&mut self) {
        self.engine.stop();
    }
}
