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

/// `test_agents.test_event_server_routes_every_agent` through the engine's own server: every
/// agent the registry knows answers a hook, an unknown id is 404, the sessions land in the brain.
#[test]
fn the_engine_server_takes_a_hook_from_every_agent() {
    let rig = EngineRig::new("");
    rig.engine.start().expect("start");
    assert!(wait_until(15.0, || daemon_up(&rig)));
    let port = rig.port;
    let hooks = [
        ("codex", "UserPromptSubmit", json!({"session_id": "t1"})),
        (
            "gemini",
            "",
            json!({"hook_event_name": "BeforeTool", "session_id": "g", "tool_name": "read_file"}),
        ),
        ("opencode", "", json!({"ev": "prompt", "sid": "o"})),
        ("pi", "", json!({"ev": "prompt", "sid": "p"})),
        (
            "claude",
            "",
            json!({"hook_event_name": "UserPromptSubmit", "session_id": "c"}),
        ),
    ];
    for (agent, hint, payload) in &hooks {
        let reply = post_hook(port, agent, hint, payload).expect("post");
        assert_eq!(reply.status, 200, "{agent}: {}", reply.text());
    }
    assert_eq!(
        post_hook(port, "nobody", "", &json!({}))
            .expect("post")
            .status,
        404
    );
    let state = http::get_state(port).expect("state").json().to_string();
    for sid in ["codex:t1", "gemini:g", "opencode:o", "pi:p", "claude:c"] {
        assert!(state.contains(sid), "{sid} missing from {state}");
    }
}

/// `test_preview.test_engine_keeps_every_payload_for_the_settings_window`: whatever the engine
/// pushes is also kept in the payload file the settings window draws the clock from.
#[test]
fn the_engine_keeps_the_pushed_payload_for_the_settings_window() {
    let rig = EngineRig::new("");
    assert!(clawd_store::paths::usage_payload(&rig.cfg.paths).is_none());
    rig.engine.start().expect("start");
    // INTEGRATION BUG (see the phase 14 report): `run` installs the payload hook after it has
    // started the publisher, so the very first payload can miss it. Ask for more polls.
    assert!(
        wait_until(15.0, || {
            rig.engine.poll_now();
            clawd_store::paths::usage_payload(&rig.cfg.paths).is_some()
        }),
        "no payload file; config dir: {:?}; status {:?}",
        std::fs::read_dir(rig.cfg.paths.config_dir())
            .map(|d| d.flatten().map(|e| e.file_name()).collect::<Vec<_>>()),
        rig.engine.snapshot().payload
    );
    let (kept, _at) = clawd_store::paths::usage_payload(&rig.cfg.paths).expect("kept");
    // `src` is not one of the screen keys the file keeps (PAYLOAD_KEYS)
    assert!(kept.src().is_none(), "{kept:?}");
}

/// `test_preview.test_engine_survives_an_unwritable_payload_file`: a payload file that cannot be
/// written (a directory sits in its place) costs the settings window its copy, nothing else.
#[test]
fn an_unwritable_payload_file_does_not_stop_the_pushes() {
    let rig = EngineRig::new("");
    std::fs::create_dir_all(rig.cfg.paths.payload_file()).expect("a directory in the way");
    rig.engine.start().expect("start");
    assert!(
        wait_until(15.0, || !usage_bodies(&rig.device).is_empty()),
        "the clock still gets its usage"
    );
    assert!(
        wait_until(15.0, || {
            rig.engine.poll_now(); // the first payload can miss the hook (see above)
            rig.engine.snapshot().payload.as_ref().and_then(|p| p.src()) == Some("off")
        }),
        "the engine keeps the payload"
    );
    let reply = post_hook(rig.port, "claude", "", &json!({})).expect("post");
    assert_eq!(reply.status, 200, "the server is up");
}

/// REPRO of an integration bug (owner: phase 12, `app/src/engine.rs::run`): the publisher task is
/// spawned before `set_on_payload` is called (the event server is started in between), so the
/// first payload can be pushed before the hook exists. `Engine.status().payload` and the payload
/// file then stay empty until the next poll (`POLL_INTERVAL` = 60 s). Run it with
/// `cargo test -p clawd-e2e --test engine -- --ignored`; it fails now and fixes itself once the
/// hook is installed before the loops start.
#[test]
#[ignore = "integration bug: first payload misses the on_payload hook (engine.rs run order)"]
fn the_first_payload_reaches_the_on_payload_hook_without_a_second_poll() {
    let mut misses = 0;
    for _ in 0..5 {
        let rig = EngineRig::new("");
        rig.engine.start().expect("start");
        assert!(wait_until(15.0, || !usage_bodies(&rig.device).is_empty()));
        if !wait_until(3.0, || rig.engine.snapshot().payload.is_some()) {
            misses += 1;
        }
    }
    assert_eq!(
        misses, 0,
        "{misses}/5 engines never saw their first payload"
    );
}
