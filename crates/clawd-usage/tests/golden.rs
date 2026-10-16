//! Golden tests: the Rust port must reproduce what the real Python (`python-0.1.x:tools/golden/export_usage.py`)
//! recorded in `fixtures/usage/*.json`, key order included.
//!
//! Needs the phase-06 `clawd-config` readers (config file, hour format): until phase 06 is merged
//! they only pass with the local scratch implementation.

mod common;

use chrono::{Local, TimeZone};
use clawd_contracts::{AppPaths, Env};
use clawd_testkit::{load_fixture_value, FakeResponse};
use clawd_usage::codex;
use clawd_usage::payload::{encode_payload, take_attention_flag, to_ascii};
use clawd_usage::{poll_api, poll_usage_endpoint, usage_from_oauth_usage, PollError};
use common::{assert_same_json, set_mtime, Fx};
use serde_json::{json, Value};

fn local_offset(epoch: f64) -> i64 {
    Local
        .timestamp_opt(epoch.floor() as i64, 0)
        .single()
        .map_or(0, |dt| i64::from(dt.offset().local_minus_utc()))
}

/// The fixture's `t` was `int(now) + <the exporter machine's offset>`; re-base it on ours.
fn rebase_clock_field(entry: &Value, mut expected: Value, now: f64) -> Value {
    if let (Some(gmtoff), Some(t)) = (entry.get("tz_gmtoff"), expected.get("t").cloned()) {
        let rebased = t.as_i64().unwrap() - gmtoff.as_i64().unwrap() + local_offset(now);
        expected["t"] = json!(rebased);
    }
    expected
}

fn outcome_value<E: std::fmt::Display>(
    res: Result<Option<clawd_contracts::UsagePayload>, E>,
) -> Value {
    match res {
        Ok(Some(p)) => json!({"value": p}),
        Ok(None) => json!({"value": null}),
        Err(e) => json!({"error": e.to_string()}),
    }
}

/// The fixture's `{"value": ...}` / `{"error": "<Python class>"}` as ours.
fn expected_outcome(result: &Value) -> Value {
    match result.get("error").and_then(Value::as_str) {
        Some("TokenExpired") => json!({"error": "the access token expired or was refused"}),
        Some(_) => json!({"error": result["message"]}),
        None => result.clone(),
    }
}

#[test]
fn oauth_mapping_matches_python() {
    let fx = load_fixture_value("usage", "oauth_mapping");
    let now = fx["input"]["now"].as_f64().unwrap();
    let cases = fx["output"].as_array().unwrap();
    assert!(cases.len() > 30);
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let got = match usage_from_oauth_usage(&case["body"], Some(now)) {
            Ok(p) => json!({"value": p}),
            Err(e) => json!({"error": e.0}),
        };
        assert_same_json(&got, &expected_outcome(&case["result"]), name);
    }
}

#[tokio::test]
async fn usage_result_classification_matches_python() {
    let fx = load_fixture_value("usage", "usage_result");
    let now = fx["input"]["now"].as_f64().unwrap();
    for case in fx["output"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let mut env = Fx::new();
        env.config(case["config"].as_str().unwrap());
        env.anthropic.usage_status(
            case["status"].as_u64().unwrap() as u16,
            case["text"].as_str().unwrap(),
        );
        env.clock.set(now);
        let got = match poll_usage_endpoint(&env.env, "sk-ant-test-token").await {
            Ok(p) => json!({"value": p}),
            Err(PollError::TokenExpired(e)) => json!({"error": e.to_string()}),
            Err(PollError::NotApplicable(e)) => json!({"error": e.0}),
        };
        let expected = expected_outcome(&case["result"]);
        let expected = match expected.get("value") {
            Some(v) if v.is_object() => {
                json!({"value": rebase_clock_field(case, v.clone(), now)})
            }
            _ => expected,
        };
        assert_same_json(&got, &expected, name);
        let _ = &mut env;
    }
}

#[tokio::test]
async fn probe_headers_match_python() {
    let fx = load_fixture_value("usage", "probe_headers");
    let now = fx["input"]["now"].as_f64().unwrap();
    let token = fx["input"]["token"].as_str().unwrap();
    for case in fx["output"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let fxr = Fx::new();
        fxr.config(case["config"].as_str().unwrap());
        fxr.clock.set(now);
        let mut response = FakeResponse::json(
            case["status"].as_u64().unwrap() as u16,
            &json!({"id": "msg_test"}),
        );
        for (k, v) in case["headers"].as_object().unwrap() {
            response = response.with_header(k, v.as_str().unwrap());
        }
        fxr.anthropic.server.route("POST", "/v1/messages", response);
        let got = outcome_value(poll_api(&fxr.env, token).await);
        let expected = expected_outcome(&case["result"]);
        let expected = match expected.get("value") {
            Some(v) if v.is_object() => {
                json!({"value": rebase_clock_field(case, v.clone(), now)})
            }
            _ => expected,
        };
        // 4xx/5xx are `Ok(None)` (`{"value": null}`) here as in Python.
        assert_same_json(&got, &expected, name);

        let req = &fxr.anthropic.server.requests_to("/v1/messages")[0];
        assert_eq!(
            req.header("authorization"),
            Some(format!("Bearer {token}").as_str())
        );
        assert_eq!(req.header("anthropic-version"), Some("2023-06-01"));
        assert_eq!(req.header("anthropic-beta"), Some("oauth-2025-04-20"));
        assert_eq!(req.header("user-agent"), Some("claude-code/2.1.5"));
        assert_eq!(
            req.body_json().unwrap(),
            json!({"model": "claude-haiku-4-5-20251001", "max_tokens": 1,
                   "messages": [{"role": "user", "content": "hi"}]})
        );
    }
}

#[test]
fn to_ascii_and_encode_payload_match_python() {
    let fx = load_fixture_value("usage", "payload_text");
    for case in fx["output"]["to_ascii"].as_array().unwrap() {
        let text = case["text"].as_str().unwrap();
        let limit = case["limit"].as_u64().unwrap() as usize;
        assert_eq!(
            to_ascii(text, limit),
            case["result"].as_str().unwrap(),
            "to_ascii({text:?}, {limit})"
        );
    }
    for case in fx["output"]["encode_payload"].as_array().unwrap() {
        let payload = serde_json::from_value(case["payload"].clone()).unwrap();
        let limit = case["limit"].as_u64().unwrap() as usize;
        let got = String::from_utf8(encode_payload(&payload, limit)).unwrap();
        assert_eq!(
            got,
            case["bytes"].as_str().unwrap(),
            "encode_payload limit {limit}"
        );
        assert!(got.len() <= limit || !got.contains("\"m\"") && got.len() <= limit);
    }
}

#[test]
fn attention_flag_matches_python() {
    let fx = load_fixture_value("usage", "attention");
    for case in fx["output"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let now = case["now"].as_f64().unwrap();
        let env = Fx::new();
        let flag = env
            .cfg
            .write_config("attention", case["text"].as_str().unwrap());
        set_mtime(&flag, now - case["age"].as_f64().unwrap());
        let got =
            take_attention_flag(&env.env, Some(now)).map(|a| json!({"m": a.m, "s": a.s, "t": a.t}));
        assert_same_json(&got.unwrap_or(Value::Null), &case["result"], name);
        assert_eq!(
            !flag.exists(),
            case["consumed"].as_bool().unwrap(),
            "{name}: consumed"
        );
    }
}

#[test]
fn codex_payload_from_matches_python() {
    let fx = load_fixture_value("usage", "codex");
    for case in fx["output"]["payload_from"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let got = codex::payload_from(
            &case["rate_limits"],
            case["written"].as_f64().unwrap(),
            Some(case["now"].as_f64().unwrap()),
        );
        let got = got.map_or(Value::Null, |p| json!(p));
        assert_same_json(&got, &case["result"], name);
    }
}

#[test]
fn codex_read_over_session_logs_matches_python() {
    let fx = load_fixture_value("usage", "codex");
    for case in fx["output"]["read"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let env = Fx::new();
        for file in case["files"].as_array().unwrap() {
            let rel = format!(".codex/sessions/{}", file["path"].as_str().unwrap());
            let lines: Vec<&str> = file["lines"]
                .as_array()
                .unwrap()
                .iter()
                .map(|l| l.as_str().unwrap())
                .collect();
            let path = env.cfg.write_home(&rel, lines.join("\n") + "\n");
            set_mtime(&path, file["mtime"].as_f64().unwrap());
        }
        let paths: &AppPaths = env.cfg.paths();
        let got = codex::read(paths, &Env::empty(), Some(case["now"].as_f64().unwrap()));
        assert_same_json(
            &got.map_or(Value::Null, |p| json!(p)),
            &case["result"],
            name,
        );
        assert_eq!(
            codex::available(paths, &Env::empty()),
            case["available"].as_bool().unwrap(),
            "{name}: available"
        );
    }
}
