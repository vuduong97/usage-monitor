//! Golden tests: replay `fixtures/agents/*.json` (written by `python-0.1.x:tools/golden/export_agents.py` from
//! the real Python adapters) against the Rust adapters, in a temp home.
//!
//! Cases flagged `needs_other_phases` call `clawd-events` / `clawd-config` code that is still an
//! (formerly ignored until phases 03/04/06 merged; they now run by default)
//! `cargo test -p clawd-agents -- --ignored` after those phases land).

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use clawd_agents::{hooks, Agent, AgentCtx, Registry};
use clawd_contracts::{Env, Platform};
use clawd_testkit::agents::{expand_home, normalize_home, snapshot_file, write_initial};
use clawd_testkit::{load_fixture_value, TempConfig};
use serde_json::{json, Value};

fn platform(name: &str) -> Platform {
    match name {
        "win32" => Platform::Win32,
        "darwin" => Platform::Darwin,
        other => panic!("platform {other}"),
    }
}

fn ctx_for(tc: &TempConfig, platform_name: &str, port: u64, env: &Value) -> AgentCtx {
    let vars: HashMap<String, String> = env
        .as_object()
        .map(|o| {
            o.iter()
                .map(|(k, v)| (k.clone(), v.as_str().unwrap_or_default().to_owned()))
                .collect()
        })
        .unwrap_or_default();
    let vars = expand_home(tc.home(), &json!(vars));
    let vars: HashMap<String, String> = serde_json::from_value(vars).expect("env map");
    AgentCtx {
        paths: tc.paths().clone(),
        platform: platform(platform_name),
        port: Some(u16::try_from(port).expect("port")),
        script: None,
        which: Arc::new(|_| None),
        env: Env::from_map(vars),
    }
}

/// Order-sensitive comparison (key order is part of the contract).
fn assert_same(actual: &Value, expected: &Value, what: &str) {
    assert_eq!(
        serde_json::to_string(actual).unwrap(),
        serde_json::to_string(expected).unwrap(),
        "{what}"
    );
}

fn flag(case: &Value) -> bool {
    case["needs_other_phases"].as_bool().unwrap_or(false)
}

fn run_step(agent: &dyn Agent, step: &str) -> Value {
    match step {
        "refresh" => json!({"refreshed": agent.refresh()}),
        "install" => match agent.install() {
            Ok(m) => json!({"message": m}),
            Err(e) => json!({"error": e.to_string()}),
        },
        "uninstall" => match agent.uninstall() {
            Ok(m) => json!({"message": m}),
            Err(e) => json!({"error": e.to_string()}),
        },
        other => panic!("step {other}"),
    }
}

fn run_configs(include_other_phases: bool) {
    let fx = load_fixture_value("agents", "configs");
    let (input, output) = (&fx["input"], &fx["output"]);
    let port = input["port"].as_u64().unwrap();
    assert_eq!(
        input["cases"].as_array().unwrap().len(),
        output["cases"].as_array().unwrap().len(),
        "configs: cases"
    );
    for (case, expected) in input["cases"]
        .as_array()
        .unwrap()
        .iter()
        .zip(output["cases"].as_array().unwrap())
    {
        if flag(case) && !include_other_phases {
            continue;
        }
        let name = case["name"].as_str().unwrap();
        let tc = TempConfig::new();
        write_initial(tc.home(), &case["initial"]);
        let ctx = ctx_for(&tc, case["platform"].as_str().unwrap(), port, &case["env"]);
        let reg = Registry::new(ctx);
        let agent = reg.get(case["agent"].as_str().unwrap()).unwrap();
        let watched = case["watched"].as_array().unwrap();
        assert_eq!(
            case["steps"].as_array().unwrap().len(),
            expected["steps"].as_array().unwrap().len(),
            "{name}: steps"
        );
        for (step_name, want) in case["steps"]
            .as_array()
            .unwrap()
            .iter()
            .zip(expected["steps"].as_array().unwrap())
        {
            let step_name = step_name.as_str().unwrap();
            let got = run_step(agent, step_name);
            let label = format!("{name}/{step_name}");
            let got = normalize_home(tc.home(), &got);
            for key in ["message", "error", "refreshed"] {
                assert_eq!(got.get(key), want.get(key), "{label}: {key}");
            }
            let installed = match agent.is_installed() {
                Ok(v) => json!(v),
                Err(e) => json!({"error": e.to_string()}),
            };
            assert_same(
                &normalize_home(tc.home(), &installed),
                &want["installed"],
                &format!("{label}: installed"),
            );
            for rel in watched {
                let rel = rel.as_str().unwrap();
                let want_file = &want["files"][rel];
                let expect_text = want_file.get("text").is_some();
                let got_file = snapshot_file(tc.home(), rel, expect_text);
                assert_same(&got_file, want_file, &format!("{label}: file {rel}"));
            }
            if let Some(left) = want.get("stale_backup_left") {
                let dir = tc.home().join(".copilot").join("hooks");
                let any = std::fs::read_dir(dir)
                    .unwrap()
                    .flatten()
                    .any(|e| e.file_name().to_string_lossy().contains(".bak-clawd-"));
                assert_eq!(&json!(any), left, "{label}: stale backup");
            }
        }
        let info = serde_json::to_value(agent.info()).unwrap();
        assert_same(
            &normalize_home(tc.home(), &info),
            &expected["info"],
            &format!("{name}: info"),
        );
    }
}

#[test]
fn configs_match_python() {
    run_configs(false);
}

#[test]
fn configs_match_python_all() {
    run_configs(true);
}

#[test]
fn bundled_assets_equal_the_integrations_python_ships() {
    let fx = load_fixture_value("agents", "configs");
    let checks = fx["output"]["assets_equal_integrations"]
        .as_object()
        .unwrap();
    assert_eq!(checks.len(), 2);
    assert!(checks.values().all(|v| v == &json!(true)));
    // Live check against the repo's integrations/ (this crate sits at <repo>/crates);
    // skipped when that tree is absent (a crate checked out on its own).
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for rel in ["pi/clawd-home.ts", "opencode/clawd.js"] {
        let source = repo.join("integrations").join(rel);
        let Ok(python_copy) = std::fs::read_to_string(&source) else {
            eprintln!("skipped: {} not found", source.display());
            continue;
        };
        let bundled = std::str::from_utf8(clawd_agents::assets::integration(rel).unwrap()).unwrap();
        assert_eq!(
            bundled.replace("\r\n", "\n"),
            python_copy.replace("\r\n", "\n"),
            "assets/{rel} drifted from integrations/{rel}"
        );
    }
}

fn run_normalize(include_other_phases: bool) {
    let fx = load_fixture_value("agents", "normalize");
    let cases = fx["input"]["cases"].as_array().unwrap();
    let wants = fx["output"]["events"].as_array().unwrap();
    assert_eq!(cases.len(), wants.len(), "normalize: cases");
    for (i, (case, want)) in cases.iter().zip(wants).enumerate() {
        if flag(case) && !include_other_phases {
            continue;
        }
        let got = clawd_agents::normalize(
            case["agent"].as_str().unwrap(),
            &case["payload"],
            case["hint"].as_str().unwrap(),
        );
        let got = got.map_or(Value::Null, Value::Array);
        assert_same(&got, want, &format!("normalize case {i}: {case}"));
    }
}

#[test]
fn normalize_matches_python() {
    run_normalize(false);
}

#[test]
fn normalize_matches_python_all() {
    run_normalize(true);
}

#[test]
fn hooks_match_python() {
    let fx = load_fixture_value("agents", "hooks");
    let (input, output) = (&fx["input"], &fx["output"]);
    let port = u16::try_from(input["port"].as_u64().unwrap()).unwrap();
    let tc = TempConfig::new();
    let env = Env::empty();
    let script = input["script"].as_str().unwrap();

    for p in ["darwin", "win32"] {
        let got = hooks::handler(
            tc.paths(),
            &env,
            platform(p),
            Some(port),
            Some(Path::new(script)),
        );
        assert_same(&got, &output["handlers"][p], &format!("handler {p}"));
    }
    assert_eq!(
        hooks::hook_script_text(port),
        output["script_text"].as_str().unwrap()
    );
    let marks: Vec<&str> = input["marks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(marks, hooks::MARKS);
    let events: Vec<&str> = input["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(events, hooks::EVENTS);

    assert_eq!(
        input["merges"].as_array().unwrap().len(),
        output["merges"].as_array().unwrap().len(),
        "hooks: merges"
    );
    assert_eq!(
        input["installed"].as_array().unwrap().len(),
        output["installed"].as_array().unwrap().len(),
        "hooks: installed"
    );
    for (i, (case, want)) in input["merges"]
        .as_array()
        .unwrap()
        .iter()
        .zip(output["merges"].as_array().unwrap())
        .enumerate()
    {
        let hook = case.get("hook").filter(|h| !h.is_null());
        let got = hooks::merge(&case["settings"], hook);
        assert_same(&got, want, &format!("merge case {i}"));
    }

    for (i, (case, want)) in input["installed"]
        .as_array()
        .unwrap()
        .iter()
        .zip(output["installed"].as_array().unwrap())
        .enumerate()
    {
        let file = tc.home().join(format!("probe{i}")).join("settings.json");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, serde_json::to_string(&case["settings"]).unwrap()).unwrap();
        let got = hooks::is_installed(tc.paths(), Some(&[file]));
        assert_eq!(&json!(got), want, "is_installed case {i}");
    }
}

#[test]
fn registry_matches_python() {
    let fx = load_fixture_value("agents", "registry");
    let (input, output) = (&fx["input"], &fx["output"]);
    let tc = TempConfig::new();
    for dir in input["dirs"].as_array().unwrap() {
        std::fs::create_dir_all(tc.home().join(dir.as_str().unwrap())).unwrap();
    }
    let ctx = ctx_for(
        &tc,
        input["platform"].as_str().unwrap(),
        input["port"].as_u64().unwrap(),
        &json!({}),
    );
    let reg = Registry::new(ctx);
    let ids: Vec<&str> = reg.iter().map(|a| a.id()).collect();
    assert_eq!(json!(ids), output["ids"]);
    assert_eq!(json!(clawd_agents::IDS), output["ids"]);
    // claude reads the config dirs of clawd-config (phase 03): compare the others
    let infos: Vec<Value> = reg
        .iter()
        .filter(|a| a.id() != "claude")
        .map(|a| normalize_home(tc.home(), &serde_json::to_value(a.info()).unwrap()))
        .collect();
    assert_same(&json!(infos), &output["infos"], "infos");
    let found: Vec<&str> = reg
        .iter()
        .filter(|a| a.id() != "claude" && a.found())
        .map(|a| a.id())
        .collect();
    let want: Vec<&str> = output["found"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .filter(|id| *id != "claude")
        .collect();
    assert_eq!(found, want);
}
