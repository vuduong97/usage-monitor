//! `tests/test_setup_api.py`: the setup wizard and the Agent tab over a temp home (the real
//! agent registry, prefs and first-run files; a fake clock and a hand-driven mDNS). Test names
//! are the Python names without `test_`.

mod settings_support;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use clawd_app::commands::agents::{agents_on, disconnect_all_on, set_agent_on, set_agents_on};
use clawd_app::commands::session::close_mdns;
use clawd_app::commands::setup::{finish_setup_on, setup_clocks_on, setup_state_on, use_clock};
use clawd_app::commands::usage::{set_usage_source_on, usage_sources_on};
use clawd_app::state::ShellCmd;
use clawd_clock::MdnsSource;
use clawd_contracts::{ClockSource, ClockTarget};
use clawd_store::prefs::load_prefs;
use clawd_testkit::clock::FakeMdns;
use serde_json::{json, Value};
use settings_support::*;

fn home_dir(rig: &Rig, name: &str) -> PathBuf {
    let dir = rig.paths().home().join(name);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn installed(r: &Value) -> std::collections::BTreeMap<String, bool> {
    r["agents"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| (a["id"].as_str().unwrap().to_owned(), a["installed"] == true))
        .collect()
}

fn pathlike(p: &Path) -> String {
    p.components().collect::<PathBuf>().display().to_string()
}

#[tokio::test(flavor = "multi_thread")]
async fn agents_lists_found_and_marks_them_offered() {
    let rig = rig();
    home_dir(&rig, ".codex");
    let r = agents_on(&rig.bridge, &[]).await.unwrap();
    let by: std::collections::BTreeMap<&str, &Value> = r["agents"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| (a["id"].as_str().unwrap(), a))
        .collect();
    assert!(
        by["codex"]["found"] == true
            && by["codex"]["installed"] == false
            && by["codex"]["beta"] == true
    );
    assert!(by["gemini"]["found"] == false && by["codex"].get("signals").is_some());
    // `wsl` says whether wsl.exe is on this machine's PATH (the Python test pinned it)
    assert!(r["wsl"].is_boolean());
    assert_eq!(load_prefs(&rig.paths()).agents_seen(), ["codex"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn set_agent_and_errors_are_shown() {
    let mut rig = rig();
    home_dir(&rig, ".cursor");
    let r = set_agent_on(&rig.bridge, &[json!("cursor"), json!(true)])
        .await
        .unwrap();
    assert_eq!(r["ok"], true, "{r}");
    assert!(installed(&r)["cursor"]);
    assert!(rig.told().contains(&ShellCmd::Setup));
    let gemini = home_dir(&rig, ".gemini");
    std::fs::write(gemini.join("settings.json"), "{broken").unwrap();
    let r = set_agent_on(&rig.bridge, &[json!("gemini"), json!(true)])
        .await
        .unwrap();
    assert_eq!(r["ok"], false);
    assert!(r["error"].as_str().unwrap().contains("JSON"), "{r}");
    let nobody = set_agent_on(&rig.bridge, &[json!("nobody"), json!(true)])
        .await
        .unwrap();
    assert_eq!(nobody["code"], "bad");
}

#[tokio::test(flavor = "multi_thread")]
async fn set_agents_syncs_to_the_ticks() {
    let rig = rig();
    home_dir(&rig, ".codex");
    home_dir(&rig, ".cursor");
    set_agent_on(&rig.bridge, &[json!("cursor"), json!(true)])
        .await
        .unwrap();
    let r = set_agents_on(&rig.bridge, &[json!(["codex"])])
        .await
        .unwrap();
    let by = installed(&r);
    assert!(r["ok"] == true && by["codex"] && !by["cursor"], "{r}");
    let r = disconnect_all_on(&rig.bridge, &[]).await.unwrap();
    assert!(installed(&r).values().all(|on| !on));
}

#[tokio::test(flavor = "multi_thread")]
async fn usage_sources_and_choice() {
    let mut rig = rig();
    let claude = home_dir(&rig, ".claude");
    // Codex has left a session log without limits: offered, nothing to show yet
    let day = home_dir(&rig, ".codex/sessions/2026/01/01");
    std::fs::write(day.join("rollout.jsonl"), "{}\n").unwrap();
    let r = usage_sources_on(&rig.bridge, &[]).await.unwrap();
    let ids: Vec<&str> = r["options"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["claude", "codex", "off"]);
    assert_eq!(r["current"], "claude");
    assert!(r["options"][1]["detail"]
        .as_str()
        .unwrap()
        .contains("chưa có giới hạn"));
    let r = set_usage_source_on(&rig.bridge, &[json!("codex")])
        .await
        .unwrap();
    assert_eq!(r["current"], "codex");
    assert!(rig.told().contains(&ShellCmd::UsageSource));
    let bad = set_usage_source_on(&rig.bridge, &[json!("gemini")])
        .await
        .unwrap();
    assert_eq!(bad["code"], "bad");
    set_usage_source_on(&rig.bridge, &[json!("claude")])
        .await
        .unwrap();
    let prefs: Value =
        serde_json::from_str(&std::fs::read_to_string(rig.paths().prefs_file()).unwrap()).unwrap();
    assert!(prefs.get("usage_source").is_none());
    // a second Claude config folder: one entry for all, one per folder
    let work = home_dir(&rig, ".claude-work");
    std::fs::write(
        rig.paths().config_file(),
        format!("config_dirs={},{}\n", claude.display(), work.display()),
    )
    .unwrap();
    let r = usage_sources_on(&rig.bridge, &[]).await.unwrap();
    let ids: Vec<String> = r["options"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["id"].as_str().unwrap().to_owned())
        .collect();
    let want = [
        "claude".to_owned(),
        format!("claude:{}", pathlike(&claude)),
        format!("claude:{}", pathlike(&work)),
    ];
    assert_eq!(ids[..3], want);
    let chosen = set_usage_source_on(&rig.bridge, &[json!(want[2].clone())])
        .await
        .unwrap();
    assert_eq!(chosen["current"], json!(want[2]));
}

#[tokio::test(flavor = "multi_thread")]
async fn setup_state_and_finish() {
    let mut rig = rig();
    let s = setup_state_on(&rig.bridge, &[]).await.unwrap();
    assert_eq!(s["pending"], json!(["clock", "agents", "usage"]));
    assert_eq!(s["fresh"], true);
    let s = finish_setup_on(&rig.bridge, &[json!(["clock", "agents"])])
        .await
        .unwrap();
    assert_eq!(s["pending"], json!(["usage"]));
    assert_eq!(s["fresh"], false);
    assert!(rig.told().contains(&ShellCmd::Setup));
}

#[tokio::test(flavor = "multi_thread")]
async fn clock_step_lists_mdns_clocks_and_picks_one_by_name() {
    let mut rig = rig();
    let found = ClockTarget {
        host: "127.0.0.1".into(),
        port: rig.clock.port(),
        name: "clawd-desk".into(),
        version: "2.16.0".into(),
        source: ClockSource::Mdns,
    };
    let mdns = Arc::new(FakeMdns::new(std::slice::from_ref(&found)));
    let shared = Arc::clone(&mdns);
    rig.bridge.data().mdns_factory =
        Some(Arc::new(move || -> Arc<dyn MdnsSource> { shared.clone() }));
    let r = setup_clocks_on(&rig.bridge, &[]).await.unwrap();
    assert_eq!(mdns.start_count(), 1);
    assert_eq!(r["clocks"][0]["name"], "clawd-desk");
    assert!(r["current"].is_null());
    let r = use_clock(&rig.bridge, &[json!("clawd-desk")])
        .await
        .unwrap();
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(r["current"]["answers"], true);
    assert_eq!(
        rig.told().last(),
        Some(&ShellCmd::UseClock {
            name: "clawd-desk".into(),
            host: "127.0.0.1".into(),
            port: found.port,
            version: "2.16.0".into(),
        })
    );
    let gone = use_clock(&rig.bridge, &[json!("other")]).await.unwrap();
    assert_eq!(gone["code"], "gone");
    close_mdns(&rig.bridge.session);
    assert_eq!(mdns.stop_count(), 1);
}
