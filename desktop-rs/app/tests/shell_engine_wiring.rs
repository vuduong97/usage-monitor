//! `test_engine.py` end to end over the real crates: a temp home, loopback servers, a hand-driven
//! mDNS and fake clocks. Nothing outside the temp home is touched.
mod common;

use clawd_app::engine::another_daemon_running;
use clawd_clock::ClockStore;
use clawd_store::pause;
use clawd_testkit::store::put_config_file;
use clawd_testkit::usage::FakeClockSink;
use clawd_testkit::{FakeClock, TempConfig};
use common::*;
use serde_json::json;

fn block_on<F: std::future::Future>(f: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime")
        .block_on(f)
}

#[test]
fn engine_handle_targets_follow_the_pause_lease() {
    let tc = TempConfig::new();
    ClockStore::new(tc.paths().clock_file())
        .save(&json!({"host": "10.0.0.7", "name": "clawd", "port": 80, "version": ""}));
    let now = FakeClock::new(1000.0);
    let (engine, _rx) = engine_with_fakes(engine_config(&tc, free_port()), &now);
    assert_eq!(engine.targets(), ["http://10.0.0.7/api/usage"]);
    assert_eq!(engine.snapshot().paused, None);

    pause::hold_at(tc.paths(), 60.0, "firmware update", 1000.0).expect("hold");
    assert!(
        engine.targets().is_empty(),
        "no push competes with the upload"
    );
    assert_eq!(engine.snapshot().paused.as_deref(), Some("firmware update"));

    now.advance(61.0); // the window crashed: the lease runs out
    assert_eq!(engine.targets(), ["http://10.0.0.7/api/usage"]);
    assert_eq!(engine.snapshot().paused, None);
}

#[test]
fn engine_run_pushes_usage_to_the_found_clock() {
    let tc = TempConfig::new();
    let sink = FakeClockSink::start();
    put_config_file(
        tc.paths(),
        "config",
        Some(&format!("http_targets={}\n", sink.server.url())),
    );
    put_config_file(
        tc.paths(),
        "desktop.json",
        Some(r#"{"usage_source": "off"}"#),
    );
    let cfg = engine_config(&tc, free_port());
    let (engine, _rx) = engine_with_fakes(cfg.clone(), &FakeClock::new(1000.0));

    assert!(
        !block_on(another_daemon_running(&cfg.paths, &cfg.env)),
        "the event port is free before start"
    );
    engine.start().expect("start");
    assert!(
        wait_until(15, || !sink.payloads().is_empty()),
        "the usage payload reaches the clock"
    );
    assert_eq!(sink.payloads()[0]["src"], "off");
    assert!(
        wait_until(5, || engine.snapshot().last_ok.is_some()),
        "the finder saw the push succeed"
    );
    let status = engine.snapshot();
    assert!(status.manual_config, "http_targets from the config file");
    assert_eq!(status.payload.as_ref().and_then(|p| p.src()), Some("off"));
    assert!(
        wait_until(5, || block_on(another_daemon_running(&cfg.paths, &cfg.env))),
        "daemon_running: the event port answers /state"
    );
    assert!(engine.snapshot().events_on, "the event server is up");

    engine.stop();
    assert!(
        wait_until(5, || !block_on(another_daemon_running(
            &cfg.paths, &cfg.env
        ))),
        "the port is free again after stop"
    );
}

#[test]
fn stop_twice_and_poll_now_after_stop_are_harmless() {
    let tc = TempConfig::new();
    let (engine, _rx) = engine_with_fakes(engine_config(&tc, free_port()), &FakeClock::new(1000.0));
    engine.start().expect("start");
    engine.start().expect("a second start does nothing");
    engine.stop();
    engine.stop(); // Tray.quit() then main()'s cleanup
    engine.poll_now(); // a late settings-window command
    let _ = engine.snapshot();
}
