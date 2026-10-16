//! The settings commands against a fake clock, through `Bridge` (the commands minus the Tauri
//! context). They need the real implementations of phases 03 (store), 05 (agents), 08 (clock),
//! 09 (firmware) and 10 (appupdate), whose stubs still `todo!()`, so they are `#[ignore]`d until
//! those merge (phase 13 step B''). Run them then with `cargo test -p clawd-app -- --ignored`.

use std::collections::BTreeMap;
use std::sync::Arc;

use clawd_agents::{AgentCtx, Registry};
use clawd_app::commands::clock::set_clock_on;
use clawd_app::commands::session::Bridge;
use clawd_app::commands::setup::use_clock;
use clawd_app::state::{JobState, Services, SettingsSession, ShellCmd};
use clawd_appupdate::{Updater, UpdaterOptions};
use clawd_clock::MdnsSource;
use clawd_config::SystemRunner;
use clawd_contracts::{ClockSource, ClockTarget, Env, Platform};
use clawd_testkit::{FakeHttpServer, FakeResponse, TempConfig};
use serde_json::{json, Value};
use tokio::sync::mpsc;

/// An mDNS source that sees exactly the clocks it is given.
struct FakeMdns(BTreeMap<String, ClockTarget>);

impl MdnsSource for FakeMdns {
    fn start(&self) -> bool {
        true
    }
    fn names(&self) -> Vec<String> {
        self.0.keys().cloned().collect()
    }
    fn changed(&self) -> u64 {
        0
    }
    fn resolve(&self, name: &str) -> Option<ClockTarget> {
        self.0.get(name).cloned()
    }
    fn stop(&self) {}
}

struct Rig {
    bridge: Bridge,
    tray: mpsc::Receiver<ShellCmd>,
    _temp: TempConfig,
}

fn rig(base: &str, key: &str) -> Rig {
    let temp = TempConfig::new();
    let paths = temp.paths().clone();
    let runner = Arc::new(SystemRunner);
    let ctx = AgentCtx {
        paths: paths.clone(),
        platform: Platform::current(),
        port: None,
        script: None,
        which: Arc::new(|_| None),
        env: Env::empty(),
    };
    let services = Services::new(
        paths.clone(),
        temp.os_names().clone(),
        Arc::new(Updater::new(
            None,
            UpdaterOptions::new(paths, runner.clone()),
        )),
        Arc::new(Registry::new(ctx)),
        None,
        None,
        runner,
    );
    let (shell, tray) = mpsc::channel(8);
    let session = Arc::new(SettingsSession::new(base, key, ""));
    session.data.lock().unwrap().env = Some(Env::empty());
    Rig {
        bridge: Bridge {
            services: Arc::new(services),
            shell,
            jobs: Arc::new(JobState::default()),
            session,
        },
        tray,
        _temp: temp,
    }
}

fn fake_clock() -> FakeHttpServer {
    let clock = FakeHttpServer::start().expect("fake clock");
    clock.route(
        "GET",
        "/api/heap",
        FakeResponse::json(200, &json!({"uptime": 5, "variant": "esp8266-clawd"})),
    );
    clock.route(
        "GET",
        "/api/config",
        FakeResponse::json(200, &json!({"hostname": "clawd", "brightness": 40})),
    );
    clock.route(
        "GET",
        "/api/status",
        FakeResponse::json(200, &json!({"mode": "ap"})),
    );
    clock
}

#[tokio::test]
#[ignore = "needs 03, 05, 08, 09, 10"]
async fn state_reads_the_clock_and_brings_the_opening_tab_once() {
    let clock = fake_clock();
    let mut rig = rig(&clock.url(), "clawd");
    *rig.bridge.session.data.lock().unwrap() = Default::default();
    let first = rig.bridge.state().await;
    assert_eq!(first["ok"], true);
    assert_eq!(first["config"]["hostname"], "clawd");
    assert!(first["net"].is_null());
    assert!(rig.bridge.state().await.get("tab").is_none());
    assert!(rig.tray.try_recv().is_err());
}

#[tokio::test]
#[ignore = "needs 03, 05, 08, 09, 10"]
async fn choosing_a_wizard_clock_tells_the_engine() {
    // test_settings_api.py:303 (`..._tells_the_tray`): the pick travels as ShellCmd::UseClock.
    let clock = fake_clock();
    let mut rig = rig("", "");
    let found = ClockTarget {
        host: "127.0.0.1".into(),
        port: clock.addr().port(),
        name: "clawd-ab12".into(),
        version: "1.4".into(),
        source: ClockSource::Mdns,
    };
    {
        let mut data = rig.bridge.session.data.lock().unwrap();
        data.mdns_seen.insert("clawd-ab12".into(), found.clone());
    }
    let answer = use_clock(&rig.bridge, &[json!("clawd-ab12")])
        .await
        .unwrap();
    assert_eq!(answer["ok"], true);
    assert_eq!(answer["current"]["answers"], true);
    assert_eq!(
        rig.tray.try_recv().unwrap(),
        ShellCmd::UseClock {
            name: "clawd-ab12".into(),
            host: "127.0.0.1".into(),
            port: found.port,
            version: "1.4".into(),
        }
    );
    let gone = use_clock(&rig.bridge, &[json!("nobody")]).await.unwrap();
    assert_eq!(gone["code"], "gone");
    let _ = FakeMdns(BTreeMap::new());
}

#[tokio::test]
#[ignore = "needs 03, 05, 08, 09, 10"]
async fn a_typed_address_goes_to_the_tray_and_is_used() {
    let clock = fake_clock();
    let mut rig = rig("", "");
    let host = format!("127.0.0.1:{}", clock.addr().port());
    let answer: Value = set_clock_on(&rig.bridge, &[json!(format!("  {host} "))])
        .await
        .unwrap();
    assert_eq!(answer["ok"], true);
    assert_eq!(
        rig.tray.try_recv().unwrap(),
        ShellCmd::SetClock { host: host.clone() }
    );
    assert_eq!(rig.bridge.address().0, format!("http://{host}"));
    let bad = set_clock_on(&rig.bridge, &[json!("not an address!")])
        .await
        .unwrap();
    assert_eq!(bad["code"], "bad");
    assert!(
        rig.tray.try_recv().is_err(),
        "a refused address is not passed on"
    );
}
