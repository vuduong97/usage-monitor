//! The pure shaping logic of `settings_api.py` against `fixtures/settings/shaping.json`
//! (written by `tools/golden/export_settings.py` from the real Python).

use std::path::Path;

use clawd_app::commands::clock::strongest_per_ssid;
use clawd_app::commands::jobs::public_job;
use clawd_app::commands::session::{clean, clock_view, err};
use clawd_contracts::ClockError;
use serde_json::{json, Value};

fn golden() -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../fixtures/settings/shaping.json");
    let text = std::fs::read_to_string(path).expect("fixture settings/shaping.json");
    let fixture: Value = serde_json::from_str(&text).expect("fixture is JSON");
    fixture["output"].clone()
}

fn rows(golden: &Value, key: &str) -> Vec<Value> {
    golden[key]
        .as_array()
        .unwrap_or_else(|| panic!("{key} is a list"))
        .clone()
}

#[test]
fn clean_matches_python() {
    for case in rows(&golden(), "clean") {
        let got = match clean(&case["input"]) {
            Ok(v) => json!({"ok": v}),
            Err(e) => json!({"error": e}),
        };
        assert_eq!(got, case["result"], "input {}", case["input"]);
    }
}

#[test]
fn clock_view_matches_python() {
    for case in rows(&golden(), "clock_view") {
        assert_eq!(
            clock_view(&case["cfg"]),
            case["view"],
            "cfg {}",
            case["cfg"]
        );
    }
}

#[test]
fn the_save_diff_matches_python() {
    for case in rows(&golden(), "diff") {
        let edited = clean(&case["edited"]).expect("these edits are valid");
        let changes = clawd_clock::config_diff(&clock_view(&case["cfg"]), &edited);
        assert_eq!(changes, case["changes"], "edited {}", case["edited"]);
    }
}

#[test]
fn clock_errors_match_python() {
    for case in rows(&golden(), "errors") {
        let error = match case["name"].as_str().unwrap() {
            "AuthNeeded" => ClockError::AuthNeeded,
            "NoAnswer" => ClockError::NoAnswer {
                base: "http://10.0.0.5".into(),
                kind: "ConnectError".into(),
            },
            "Http" => ClockError::Http {
                status: 500,
                body: "boom".into(),
            },
            other => panic!("unknown error case {other}"),
        };
        assert_eq!(err(&error), case["response"], "{}", case["name"]);
    }
}

#[test]
fn a_scan_matches_python() {
    let nets = vec![
        json!({"ssid": "Home", "rssi": -70, "enc": 3}),
        json!({"ssid": "Cafe", "rssi": -50}),
        json!({"ssid": "Home", "rssi": -60, "enc": 3}),
        json!({"ssid": "", "rssi": -10}),
        json!({"rssi": -10}),
        json!({"ssid": "Far"}),
    ];
    let golden = golden();
    assert_eq!(
        strongest_per_ssid(&nets),
        golden["scan"]["networks"].as_array().unwrap().clone()
    );
}

#[test]
fn an_idle_job_matches_python() {
    let golden = golden();
    let mut idle = public_job(&json!({"running": false}));
    idle["ok"] = json!(true);
    assert_eq!(idle, golden["job_idle"]);
}
