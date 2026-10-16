//! Golden replay: `fixtures/config/*.json` come from the real Python readers
//! (`python-0.1.x:tools/golden/export_config.py`); every case must come out identical here.

use std::fs;
use std::path::Path;

use clawd_config::cliproxy::read_cliproxy_key_on;
use clawd_config::tokens::{
    decode_keychain_blob, extract_access_token, read_token_for_on, read_token_store,
};
use clawd_config::{
    detect_hour_format, normalize_http_target, read_chime_setting, read_cliproxy_url,
    read_clock_setting, read_config_dirs, read_config_value, read_http_targets, read_onoff_setting,
    read_usage_source, Host,
};
use clawd_testkit::config::{FakeReply, FakeRunner};
use clawd_testkit::{load_fixture_value, TempConfig};
use serde_json::{json, Value};

const USER: &str = "clawd-test-user";

fn str_of(v: &Value) -> &str {
    v.as_str().expect("string")
}

fn opt(v: Option<String>) -> Value {
    v.map_or(Value::Null, Value::String)
}

/// `<home>/...` with forward slashes, the way the exporter writes paths.
fn home_rel(cfg: &TempConfig, path: &Path) -> String {
    let home = cfg.home().to_string_lossy().into_owned();
    path.to_string_lossy()
        .replace(&home, "<home>")
        .replace('\\', "/")
}

fn write_text(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn make_store(root: &Path, case: &Value) {
    fs::create_dir_all(root).unwrap();
    if let Some(active) = case["active"].as_str() {
        fs::write(root.join("active_config"), active).unwrap();
    }
    for (profile, text) in case["profiles"].as_object().unwrap() {
        write_text(
            &root.join("credentials").join(format!("{profile}.json")),
            str_of(text),
        );
    }
}

fn reply_of(spec: &Value) -> FakeReply {
    match spec.get("raises").and_then(Value::as_str) {
        Some("FileNotFoundError") => FakeReply::not_found(),
        Some("TimeoutExpired") => FakeReply::timed_out(),
        Some(other) => panic!("unknown raise {other}"),
        None => {
            let rc = spec.get("rc").and_then(Value::as_i64).unwrap_or(0) as i32;
            if rc == 0 {
                FakeReply::ok(spec.get("stdout").and_then(Value::as_str).unwrap_or(""))
            } else {
                FakeReply::exit(rc, "")
            }
        }
    }
}

/// `[program, args...]` of every call, as Python's `cmd` lists.
fn commands(runner: &FakeRunner) -> Value {
    Value::Array(
        runner
            .calls()
            .into_iter()
            .map(|call| {
                let mut cmd = vec![call.program];
                cmd.extend(call.args);
                json!(cmd)
            })
            .collect(),
    )
}

#[test]
fn golden_config_file() {
    let fx = load_fixture_value("config", "config_file");
    let names: Vec<&str> = fx["input"]["names"]
        .as_array()
        .unwrap()
        .iter()
        .map(str_of)
        .collect();
    let onoff = fx["input"]["onoff"].as_array().unwrap();
    let cases = fx["input"]["cases"].as_array().unwrap();
    let outputs = fx["output"].as_array().unwrap();
    assert_eq!(cases.len(), outputs.len());
    for (case, want) in cases.iter().zip(outputs) {
        let label = str_of(&case["name"]);
        let cfg = TempConfig::new();
        if let Some(text) = case["config"].as_str() {
            cfg.write_config("config", text);
        }
        let p = cfg.paths();
        let values: serde_json::Map<String, Value> = names
            .iter()
            .map(|n| ((*n).to_owned(), opt(read_config_value(p, n))))
            .collect();
        assert_eq!(Value::Object(values), want["values"], "{label}: values");
        let onoff_got: Vec<Value> = onoff
            .iter()
            .map(|pair| json!(read_onoff_setting(p, str_of(&pair[0]), str_of(&pair[1]))))
            .collect();
        assert_eq!(json!(onoff_got), want["onoff"], "{label}: onoff");
        assert_eq!(
            json!(read_chime_setting(p)),
            want["chime"],
            "{label}: chime"
        );
        assert_eq!(
            json!(read_clock_setting(p)),
            want["clock"],
            "{label}: clock"
        );
        let dirs: Vec<String> = read_config_dirs(p)
            .iter()
            .map(|d| home_rel(&cfg, d))
            .collect();
        assert_eq!(json!(dirs), want["config_dirs"], "{label}: config_dirs");
        assert_eq!(
            json!(read_http_targets(p)),
            want["http_targets"],
            "{label}: http_targets"
        );
        assert_eq!(
            json!(read_cliproxy_url(p)),
            want["cliproxy_url"],
            "{label}: cliproxy_url"
        );
    }
}

#[test]
fn golden_usage_source() {
    let fx = load_fixture_value("config", "usage_source");
    let inputs = fx["input"].as_array().unwrap();
    let outputs = fx["output"].as_array().unwrap();
    for (input, want) in inputs.iter().zip(outputs) {
        let cfg = TempConfig::new();
        if let Some(text) = input.as_str() {
            cfg.write_config("desktop.json", text);
        }
        assert_eq!(json!(read_usage_source(cfg.paths())), *want, "{input}");
    }
}

#[test]
fn golden_normalize_http_target() {
    let fx = load_fixture_value("config", "normalize_http_target");
    for (input, want) in fx["input"]
        .as_array()
        .unwrap()
        .iter()
        .zip(fx["output"].as_array().unwrap())
    {
        // Python raises ValueError for an unbalanced `[`: the port answers None
        let want = if want.get("error").is_some() {
            &Value::Null
        } else {
            want
        };
        assert_eq!(opt(normalize_http_target(str_of(input))), *want, "{input}");
    }
}

#[test]
fn golden_token_store() {
    let fx = load_fixture_value("config", "token_store");
    let now = fx["input"]["now"].as_f64().unwrap();
    let cases = fx["input"]["cases"].as_array().unwrap();
    let outputs = fx["output"].as_array().unwrap();
    for (case, want) in cases.iter().zip(outputs) {
        let cfg = TempConfig::new();
        let root = cfg.root().join("store");
        make_store(&root, case);
        assert_eq!(
            opt(read_token_store(&root, Some(now))),
            *want,
            "{}",
            case["name"]
        );
    }
}

#[test]
fn golden_extract_access_token_and_decode() {
    let fx = load_fixture_value("config", "extract_access_token");
    for (blob, want) in fx["input"]
        .as_array()
        .unwrap()
        .iter()
        .zip(fx["output"].as_array().unwrap())
    {
        assert_eq!(opt(extract_access_token(str_of(blob))), *want, "{blob}");
    }
    let fx = load_fixture_value("config", "decode_keychain_blob");
    for (raw, want) in fx["input"]
        .as_array()
        .unwrap()
        .iter()
        .zip(fx["output"].as_array().unwrap())
    {
        assert_eq!(json!(decode_keychain_blob(str_of(raw))), *want, "{raw}");
    }
}

#[test]
fn golden_token_for() {
    let fx = load_fixture_value("config", "token_for");
    let user = str_of(&fx["input"]["user"]).to_owned();
    assert_eq!(user, USER);
    let cases = fx["input"]["cases"].as_array().unwrap();
    let outputs = fx["output"].as_array().unwrap();
    for (case, want) in cases.iter().zip(outputs) {
        let label = str_of(&case["name"]);
        let cfg = TempConfig::new();
        let dir = if case["dir"] == "default" {
            cfg.paths().default_claude_dir()
        } else {
            cfg.home().join(".claude-work")
        };
        fs::create_dir_all(&dir).unwrap();
        if let Some(files) = case["files"].as_object() {
            for (name, text) in files {
                write_text(&dir.join(name), str_of(text));
            }
        }
        if case.get("store").is_some() {
            make_store(&cfg.paths().anthropic_store_dir(), &case["store"]);
        }
        let runner = match case.get("keychain") {
            Some(spec) => FakeRunner::new().on_match(
                |program, args| {
                    program == "security" && args.first() == Some(&"find-generic-password")
                },
                reply_of(spec),
            ),
            None => FakeRunner::new(),
        };
        let host = Host::new(case["platform"] == "darwin", &user);
        let token = read_token_for_on(cfg.paths(), &runner, &dir, &host);
        assert_eq!(opt(token), want["token"], "{label}: token");
        assert_eq!(commands(&runner), want["commands"], "{label}: commands");
    }
}

#[test]
fn golden_cliproxy_key() {
    let fx = load_fixture_value("config", "cliproxy_key");
    let cases = fx["input"]["cases"].as_array().unwrap();
    let outputs = fx["output"].as_array().unwrap();
    for (case, want) in cases.iter().zip(outputs) {
        let label = str_of(&case["name"]);
        let mut runner = FakeRunner::new();
        for (service, spec) in case["replies"].as_object().unwrap() {
            runner = runner.keychain_service(service, reply_of(spec));
        }
        let host = Host::new(case["platform"] == "darwin", USER);
        assert_eq!(
            opt(read_cliproxy_key_on(&runner, &host)),
            want["key"],
            "{label}: key"
        );
        assert_eq!(commands(&runner), want["commands"], "{label}: commands");
    }
}

#[test]
fn golden_hour_format() {
    let fx = load_fixture_value("config", "hour_format");
    let cases = fx["input"]["cases"].as_array().unwrap();
    let outputs = fx["output"].as_array().unwrap();
    for (case, want) in cases.iter().zip(outputs) {
        let label = str_of(&case["name"]);
        let mut runner = FakeRunner::new();
        for (key, spec) in case["replies"].as_object().unwrap() {
            runner = runner.defaults(key, reply_of(spec));
        }
        assert_eq!(
            json!(detect_hour_format(&runner)),
            want["hour_format"],
            "{label}: hour_format"
        );
        assert_eq!(commands(&runner), want["commands"], "{label}: commands");
    }
}
