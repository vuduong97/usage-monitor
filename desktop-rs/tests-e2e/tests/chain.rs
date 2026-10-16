//! Fake Anthropic -> usage publisher -> fake clock, and a hook POST -> event server -> brain ->
//! state pusher -> fake clock, with the payloads compared to the golden fixtures of the Python
//! app (key order included).
use clawd_e2e::http::{self, post_hook};
use clawd_e2e::rig::{state_bodies, usage_bodies, wait_until, Rig, RigOptions};
use clawd_testkit::load_fixture_value;
use serde_json::{json, Value};

/// Canonical key order of `POST /api/state` (`events.wire`).
const WIRE_ORDER: [&str; 8] = ["a", "ttl", "rep", "v", "u", "txt", "cd", "dim"];

fn is_subsequence(keys: &[String], order: &[&str]) -> bool {
    let mut it = order.iter();
    keys.iter().all(|k| it.any(|o| o == k))
}

fn keys_of_compact(text: &str) -> Vec<String> {
    let v: Value = serde_json::from_str(text).expect("json");
    v.as_object().expect("object").keys().cloned().collect()
}

#[test]
fn usage_poll_reaches_the_clock_byte_for_byte_like_the_python_golden() {
    let mut rig = Rig::new(RigOptions::default());
    rig.spawn_loops();
    assert!(
        wait_until(15.0, || !usage_bodies(&rig.device).is_empty()),
        "the clock got no /api/usage push"
    );

    // golden: oauth_mapping.json "max account" (now = 1791711600)
    let golden = load_fixture_value("usage", "oauth_mapping");
    let case = golden["output"]
        .as_array()
        .expect("cases")
        .iter()
        .find(|c| c["name"] == "max account")
        .expect("max account case");
    let expected = serde_json::to_string(&case["result"]["value"]).expect("json");
    assert_eq!(usage_bodies(&rig.device)[0], expected);

    // the fake Anthropic saw the documented request, nothing went anywhere else
    let req = &rig.anthropic.server.requests_to("/api/oauth/usage")[0];
    assert_eq!(req.header("authorization"), Some("Bearer sk-ant-test-e2e"));
    assert_eq!(req.header("anthropic-beta"), Some("oauth-2025-04-20"));
    assert!(rig.anthropic.server.requests_to("/v1/messages").is_empty());
    let sent = &rig.device.server().requests_to("/api/usage")[0];
    assert_eq!(sent.header("content-type"), Some("application/json"));
    rig.shutdown();
}

#[test]
fn a_claude_hook_post_becomes_a_state_push_in_wire_order() {
    let mut rig = Rig::new(RigOptions::default());
    rig.spawn_loops();
    let port = rig.port();

    let prompt = json!({"hook_event_name": "UserPromptSubmit", "session_id": "e2e-1"});
    let reply = post_hook(port, "claude", "UserPromptSubmit", &prompt).expect("post");
    assert_eq!((reply.status, reply.json()), (200, json!({"ok": true})));
    let tool = json!({
        "hook_event_name": "PreToolUse", "session_id": "e2e-1",
        "tool_name": "Bash", "tool_input": {"command": "cargo test"}
    });
    assert_eq!(
        post_hook(port, "claude", "PreToolUse", &tool)
            .expect("post")
            .status,
        200
    );

    assert!(
        wait_until(15.0, || state_bodies(&rig.device)
            .iter()
            .any(|b| b["a"] != json!(""))),
        "no active state reached the clock: {:?}",
        state_bodies(&rig.device)
    );

    // golden: every body of events.wire is a subsequence of the canonical order, ttl 60
    let golden = load_fixture_value("events", "helpers");
    for case in golden["output"]["wire"].as_array().expect("wire cases") {
        let keys = keys_of_compact(case[1].as_str().expect("text"));
        assert!(is_subsequence(&keys, &WIRE_ORDER), "fixture {keys:?}");
    }
    let raws: Vec<String> = rig
        .device
        .server()
        .requests_to("/api/state")
        .iter()
        .map(|r| String::from_utf8_lossy(&r.body).into_owned())
        .collect();
    assert!(!raws.is_empty());
    for raw in &raws {
        assert!(is_subsequence(&keys_of_compact(raw), &WIRE_ORDER), "{raw}");
        assert!(
            !raw.contains(": ") && !raw.contains(", "),
            "compact JSON: {raw}"
        );
        let body: Value = serde_json::from_str(raw).expect("json");
        assert_eq!(body["ttl"], 60, "{raw}");
    }
    rig.shutdown();
}

#[test]
fn state_and_poke_work_end_to_end() {
    let mut rig = Rig::new(RigOptions::default());
    rig.spawn_loops();
    let port = rig.port();

    let state = http::get_state(port).expect("state");
    assert_eq!(state.status, 200);
    assert_eq!(state.header("content-type"), Some("application/json"));
    let keys: Vec<String> = state
        .json()
        .as_object()
        .expect("object")
        .keys()
        .cloned()
        .collect();
    assert_eq!(keys, ["desired", "sessions", "turns", "away", "focus_left"]);

    let poke = http::post_poke(port, &json!({"cmd": "say", "text": "Hello e2e", "secs": 5}))
        .expect("poke");
    assert_eq!((poke.status, poke.json()), (200, json!({"ok": true})));
    assert!(
        wait_until(15.0, || state_bodies(&rig.device)
            .iter()
            .any(|b| b["txt"] == json!("Hello e2e"))),
        "the poke text never reached the clock: {:?}",
        state_bodies(&rig.device)
    );

    let bad = http::post_poke(port, &json!({"cmd": "say", "text": "  "})).expect("poke");
    assert_eq!(bad.status, 400);
    assert_eq!(bad.json(), json!({"ok": false, "error": "text required"}));
    rig.shutdown();
}
