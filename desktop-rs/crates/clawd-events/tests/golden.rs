//! Golden tests: replay `desktop-rs/fixtures/events/*.json`, recorded from the real Python
//! `clawd_desktop.events` by `tools/golden/export_events.py`. A mismatch is a bug in the Rust
//! port: fix the Rust, never the fixture.

use clawd_contracts::{DesiredState, Env, PetConfig, UsagePayload};
use clawd_events::rng::FirstRng;
use clawd_events::tracker::Tracker;
use clawd_events::{
    ascii_text, claude_event, heap_url, is_debug_prompt, is_debug_skill_load, is_question_tool,
    is_test_command, pi_event, read_event_port, read_pet_config, state_url, tool_action,
    tool_summary, wire, Brain, Pet, CLAUDE_HOOK_EVENTS, EVENTS,
};
use clawd_testkit::load_fixture_value;
use serde_json::{json, Value};

const T0: f64 = 1_760_000_000.0;

fn cases<'a>(output: &'a Value, name: &str) -> &'a Vec<Value> {
    output[name]
        .as_array()
        .unwrap_or_else(|| panic!("fixture has no `{name}` cases"))
}

/// Same JSON bytes (object key order included).
fn same_bytes(actual: &Value, expected: &Value) -> bool {
    serde_json::to_string(actual).unwrap() == serde_json::to_string(expected).unwrap()
}

#[test]
fn vocab_helpers_match_python() {
    let fixture = load_fixture_value("events", "helpers");
    let out = &fixture["output"];
    for case in cases(out, "ascii_text") {
        let limit = case[1].as_u64().map_or(90, |n| n as usize);
        assert_eq!(
            ascii_text(case[0].as_str().unwrap(), limit),
            case[2].as_str().unwrap(),
            "ascii_text {case}"
        );
    }
    for case in cases(out, "is_test_command") {
        assert_eq!(
            is_test_command(case[0].as_str()),
            case[1].as_bool().unwrap(),
            "is_test_command {case}"
        );
    }
    for case in cases(out, "is_debug_prompt") {
        assert_eq!(
            is_debug_prompt(case[0].as_str()),
            case[1].as_bool().unwrap(),
            "is_debug_prompt {case}"
        );
    }
    for case in cases(out, "is_debug_skill_load") {
        assert_eq!(
            is_debug_skill_load(case[0].as_str().unwrap(), &case[1]),
            case[2].as_bool().unwrap(),
            "is_debug_skill_load {case}"
        );
    }
    for case in cases(out, "is_question_tool") {
        assert_eq!(
            is_question_tool(case[0].as_str().unwrap()),
            case[1].as_bool().unwrap(),
            "is_question_tool {case}"
        );
    }
    for case in cases(out, "tool_action") {
        assert_eq!(
            tool_action(case[0].as_str().unwrap()),
            case[1].as_str().unwrap(),
            "tool_action {case}"
        );
    }
    for case in cases(out, "tool_summary") {
        assert_eq!(
            tool_summary(case[0].as_str().unwrap(), &case[1]),
            case[2].as_str().unwrap(),
            "tool_summary {case}"
        );
    }
    assert_eq!(
        json!(EVENTS
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>()),
        out["constants"]["EVENTS"]
    );
    assert_eq!(
        json!(CLAUDE_HOOK_EVENTS),
        out["constants"]["CLAUDE_HOOK_EVENTS"]
    );
}

#[test]
fn claude_and_pi_normalisers_match_python_bytes() {
    let fixture = load_fixture_value("events", "helpers");
    let out = &fixture["output"];
    for case in cases(out, "claude_event") {
        let actual = claude_event(&case[0]).unwrap_or(Value::Null);
        assert!(
            same_bytes(&actual, &case[1]),
            "claude_event {}\n actual {actual}\n expected {}",
            case[0],
            case[1]
        );
    }
    for case in cases(out, "pi_event") {
        let actual = pi_event(&case[0]).unwrap_or(Value::Null);
        assert!(
            same_bytes(&actual, &case[1]),
            "pi_event {}\n actual {actual}\n expected {}",
            case[0],
            case[1]
        );
    }
}

#[test]
fn wire_and_clock_urls_match_python_bytes() {
    let fixture = load_fixture_value("events", "helpers");
    let out = &fixture["output"];
    for case in cases(out, "wire") {
        let desired: DesiredState = serde_json::from_value(case[0].clone()).unwrap();
        assert_eq!(
            clawd_events::pyjson::to_py_json_compact(&wire(&desired)),
            case[1].as_str().unwrap(),
            "wire {}",
            case[0]
        );
    }
    for case in cases(out, "urls") {
        let target = case[0].as_str().unwrap();
        assert_eq!(
            state_url(target),
            case[1].as_str().unwrap(),
            "state_url {target:?}"
        );
        assert_eq!(
            heap_url(target),
            case[2].as_str().unwrap(),
            "heap_url {target:?}"
        );
    }
}

fn reader(config: &Value) -> impl Fn(&str) -> Option<String> + '_ {
    move |name| config.get(name).and_then(Value::as_str).map(str::to_owned)
}

#[test]
fn config_readers_match_python() {
    let fixture = load_fixture_value("events", "helpers");
    let out = &fixture["output"];
    for case in cases(out, "read_event_port") {
        let env = match case[0]["env"].as_str() {
            Some(value) => {
                Env::from_map([("CLAWD_EVENT_PORT".to_owned(), value.to_owned())].into())
            }
            None => Env::empty(),
        };
        let read = reader(&case[0]["config"]);
        assert_eq!(
            u64::from(read_event_port(&env, &read)),
            case[1].as_u64().unwrap(),
            "read_event_port {}",
            case[0]
        );
    }
    for case in cases(out, "read_pet_config") {
        let read = reader(&case[0]);
        let actual: PetConfig = read_pet_config(&read);
        assert_eq!(
            serde_json::to_value(actual).unwrap(),
            case[1],
            "read_pet_config {}",
            case[0]
        );
    }
}

fn pet_config(spec: &Value) -> PetConfig {
    let mut cfg = serde_json::to_value(PetConfig::default()).unwrap();
    if let Some(overrides) = spec["cfg"].as_object() {
        for (key, value) in overrides {
            cfg[key.as_str()] = value.clone();
        }
    }
    serde_json::from_value(cfg).unwrap()
}

fn view_json(view: Option<clawd_contracts::View>) -> Value {
    view.map_or(Value::Null, |v| serde_json::to_value(v).unwrap())
}

/// One scenario, step by step, the way `export_events.run_scenario` runs it.
fn run_scenario(spec: &Value) -> Vec<Value> {
    use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
    use std::sync::Arc;

    let hour = Arc::new(AtomicI64::new(spec["hour"].as_i64().unwrap_or(14)));
    let minute = Arc::new(AtomicI64::new(spec["minute"].as_i64().unwrap_or(0)));
    let usage_only = Arc::new(AtomicBool::new(
        spec["usage_only"].as_bool().unwrap_or(false),
    ));
    let (h, m) = (Arc::clone(&hour), Arc::clone(&minute));
    let mut pet = Pet::with_parts(
        pet_config(spec),
        None,
        Box::new(move |_| clawd_events::localtime::LocalTime {
            year: 2026,
            month: 10,
            day: 11,
            hour: h.load(Ordering::SeqCst) as u32,
            minute: m.load(Ordering::SeqCst) as u32,
        }),
        Box::new(FirstRng),
    );
    pet.desk_new = spec["desk_new"].as_bool().unwrap_or(false);
    let only = Arc::clone(&usage_only);
    let mut brain = Brain::with_tracker(
        pet,
        Box::new(move || only.load(Ordering::SeqCst)),
        Some(Box::new(|on_turn| {
            Tracker::with_rng(Some(on_turn), Box::new(FirstRng))
        })),
    );

    let mut out = Vec::new();
    for step in spec["steps"].as_array().unwrap() {
        let t = T0 + step["t"].as_f64().unwrap_or(0.0);
        let op = step["op"].as_str().unwrap();
        let result = match op {
            "claude" | "pi" | "event" => {
                let payload = &step["payload"];
                let event = match op {
                    "claude" => claude_event(payload),
                    "pi" => pi_event(payload),
                    _ => Some(payload.clone()),
                };
                match event {
                    Some(e) => {
                        let changed = brain.ingest(&e, t);
                        json!({"event": e, "changed": changed})
                    }
                    None => json!({"event": null, "changed": null}),
                }
            }
            "poke" => brain.pokes.handle(&step["cmd"], t),
            "observe" => {
                let usage = step["usage"]
                    .as_object()
                    .map(|o| UsagePayload::from(o.clone()));
                brain.pet.observe(t, step["idle"].as_f64(), usage.as_ref());
                Value::Null
            }
            "view" => view_json(brain.tracker.view(t)),
            "desired" => serde_json::to_value(brain.desired(t)).unwrap(),
            "state" => brain.state_body(t),
            "sessions" => Value::Array(brain.tracker.snapshot(t)),
            "run_idle" => {
                let (secs, stride) = (
                    step["secs"].as_f64().unwrap(),
                    step["step"].as_f64().unwrap(),
                );
                let (mut changes, mut last, mut cur) = (Vec::new(), None, t);
                while cur < t + secs {
                    let d = brain.desired(cur);
                    let key = (d.a.clone(), d.seq.clone());
                    if last.as_ref() != Some(&key) {
                        changes.push(json!([cur - T0, d.a, d.seq]));
                        last = Some(key);
                    }
                    cur += stride;
                }
                Value::Array(changes)
            }
            "pet_moment" => view_json(brain.pet.moment(t)),
            "pet_continuous" => view_json(brain.pet.continuous(t)),
            "pet_dim" => json!(brain.pet.dim(t)),
            "pet_on_turn" => {
                brain.pet.on_turn(t);
                json!(brain.pet.turns)
            }
            "pet_clear" => {
                brain.pet.clear_moments();
                Value::Null
            }
            "pokes_moment" => view_json(brain.pokes.moment(t)),
            "pokes_focus_left" => json!(brain.pokes.focus_left(t)),
            "set_session" => {
                let sess = brain
                    .tracker
                    .sessions
                    .get_mut(step["sid"].as_str().unwrap())
                    .unwrap();
                match step["field"].as_str().unwrap() {
                    "action" => sess.action = step["value"].as_str().unwrap().to_owned(),
                    "error_until" => sess.error_until = step["value"].as_f64().unwrap(),
                    other => panic!("set_session: unsupported field {other}"),
                }
                Value::Null
            }
            "set" => {
                if let Some(v) = step["hour"].as_i64() {
                    hour.store(v, Ordering::SeqCst);
                }
                if let Some(v) = step["minute"].as_i64() {
                    minute.store(v, Ordering::SeqCst);
                }
                if let Some(v) = step["usage_only"].as_bool() {
                    usage_only.store(v, Ordering::SeqCst);
                }
                if let Some(v) = step["desk_new"].as_bool() {
                    brain.pet.desk_new = v;
                }
                Value::Null
            }
            other => panic!("unknown scenario op {other}"),
        };
        out.push(result);
    }
    out
}

#[test]
fn every_brain_scenario_matches_python() {
    let fixture = load_fixture_value("events", "scenarios");
    let inputs = fixture["input"].as_object().unwrap();
    let outputs = fixture["output"].as_object().unwrap();
    assert!(
        inputs.len() >= 80,
        "fixture shrank: {} scenarios",
        inputs.len()
    );
    let mut failures = Vec::new();
    for (name, spec) in inputs {
        let actual = run_scenario(spec);
        let expected = outputs[name].as_array().unwrap();
        assert_eq!(actual.len(), expected.len(), "{name}: step count");
        for (i, (a, e)) in actual.iter().zip(expected).enumerate() {
            // `desired` and `state` are what the server answers with: key order is part of it
            let ordered = matches!(spec["steps"][i]["op"].as_str(), Some("desired" | "state"));
            if a != e || (ordered && !same_bytes(a, e)) {
                failures.push(format!(
                    "{name} step {i} ({}):\n  rust   {a}\n  python {e}",
                    spec["steps"][i]
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} mismatches:\n{}",
        failures.len(),
        failures
            .iter()
            .take(8)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}
