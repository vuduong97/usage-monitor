//! The real `EngineHandle` (`run`'s wiring) over a temp home: a hook POST reaches the clock,
//! `/state` and `/poke` answer, the production limits refuse hostile requests, stop frees the
//! port. The usage source is `off`: the engine has no hook to point it at a fake Anthropic (that
//! path is in `chain.rs`).
use std::io::Write;
use std::time::{Duration, Instant};

use clawd_app::engine::another_daemon_running;
use clawd_e2e::engine_rig::EngineRig;
use clawd_e2e::http::{self, connect, post_hook, Request};
use clawd_e2e::rig::{state_bodies, usage_bodies, wait_until};
use serde_json::json;

fn block_on<F: std::future::Future>(f: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime")
        .block_on(f)
}

fn daemon_up(rig: &EngineRig) -> bool {
    block_on(another_daemon_running(&rig.cfg.paths, &rig.cfg.env))
}

#[test]
fn engine_run_turns_a_hook_post_into_pushes_on_the_clock() {
    let rig = EngineRig::new("");
    rig.engine.start().expect("start");
    assert!(
        wait_until(15.0, || daemon_up(&rig)),
        "the event server answers /state"
    );
    assert!(rig.engine.snapshot().events_on);

    // usage: the engine pushed its first payload to the http_targets clock
    assert!(wait_until(15.0, || !usage_bodies(&rig.device).is_empty()));
    let usage: serde_json::Value = serde_json::from_str(&usage_bodies(&rig.device)[0]).unwrap();
    assert_eq!(usage["src"], "off");

    // a Claude hook, as the hook script posts it
    let tool = json!({
        "hook_event_name": "PreToolUse", "session_id": "eng-1",
        "tool_name": "Edit", "tool_input": {"file_path": "a.rs"}
    });
    let reply = post_hook(rig.port, "claude", "PreToolUse", &tool).expect("post");
    assert_eq!((reply.status, reply.json()), (200, json!({"ok": true})));
    assert!(
        wait_until(15.0, || state_bodies(&rig.device)
            .iter()
            .any(|b| b["a"] != json!(""))),
        "the clock never got an active state: {:?}",
        state_bodies(&rig.device)
    );
    let state = http::get_state(rig.port).expect("state").json();
    assert!(
        state["sessions"].to_string().contains("claude:eng-1"),
        "{state}"
    );

    // /poke says something on the clock
    let poke =
        http::post_poke(rig.port, &json!({"cmd": "say", "text": "From the engine"})).expect("poke");
    assert_eq!(poke.status, 200);
    assert!(wait_until(15.0, || state_bodies(&rig.device)
        .iter()
        .any(|b| b["txt"] == json!("From the engine"))));

    rig.engine.stop();
    assert!(
        wait_until(8.0, || !daemon_up(&rig)),
        "the port is free again after stop"
    );
}

#[test]
fn the_engine_server_refuses_hostile_requests_with_the_production_limits() {
    let rig = EngineRig::new("");
    rig.engine.start().expect("start");
    assert!(wait_until(15.0, || daemon_up(&rig)));
    let port = rig.port;

    let rebind = http::send(
        port,
        &Request::get(port, "/state").host(Some("evil.example")),
    )
    .unwrap();
    assert_eq!(rebind.status, 403);
    let browser = http::send(
        port,
        &Request::post(port, "/poke", b"{}").header("Origin", "http://evil.example"),
    )
    .unwrap();
    assert_eq!(browser.status, 403);
    let huge = http::send(
        port,
        &Request::post(port, "/hook/claude?e=Stop", b"{}")
            .declared_length(clawd_contracts::MAX_BODY as u64 + 1),
    )
    .unwrap();
    assert_eq!(huge.status, 413);

    // Slowloris within the production limits: stalled heads hold a slot each, the 64-slot cap is
    // far away, so a normal client is served at once while they hang.
    let mut stalled = Vec::new();
    for _ in 0..8 {
        let mut s = connect(port).expect("connect");
        s.write_all(b"POST /hook/claude?e=Stop HTTP/1.1\r\nHost: 127.0.0.1")
            .expect("write");
        stalled.push(s);
    }
    let started = Instant::now();
    assert_eq!(http::get_state(port).expect("state").status, 200);
    assert!(started.elapsed() < Duration::from_secs(2));
    drop(stalled);
    assert_eq!(http::get_state(port).expect("state").status, 200);
    // the refused requests changed nothing: no session, no active state on the clock
    let state = http::get_state(port).expect("state").json();
    assert_eq!(state["sessions"], json!([]), "{state}");
    assert!(state_bodies(&rig.device)
        .iter()
        .all(|b| b["a"] == json!("")));
}

#[test]
fn events_off_in_the_config_means_no_event_server() {
    let rig = EngineRig::new("events = off\n");
    rig.engine.start().expect("start");
    assert!(
        wait_until(15.0, || !usage_bodies(&rig.device).is_empty()),
        "usage still flows"
    );
    assert!(!rig.engine.snapshot().events_on);
    assert!(!daemon_up(&rig), "nothing listens on the event port");
    assert!(http::get_state(rig.port).is_err());
}

#[test]
fn a_second_engine_on_a_taken_port_runs_without_agent_events() {
    let first = EngineRig::new("");
    first.engine.start().expect("start");
    assert!(wait_until(15.0, || daemon_up(&first)));

    // Same port, other temp home: the bind fails, the engine carries on (usage only).
    let second = EngineRig::on_port("", first.port);
    second.engine.start().expect("start");
    assert!(wait_until(15.0, || !usage_bodies(&second.device).is_empty()));
    assert!(!second.engine.snapshot().events_on, "the port was taken");
    assert!(
        post_hook(first.port, "claude", "Stop", &json!({})).is_ok(),
        "the first engine still owns the port"
    );
}
