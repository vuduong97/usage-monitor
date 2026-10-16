//! Adapter safety rules that need no other phase: an unparsable config is never touched, and an
//! agent never claims another agent's hooks (`test_broken_config_is_never_touched`,
//! `test_an_agent_does_not_claim_another_agents_hooks`).

use std::collections::HashMap;
use std::fs;
use std::sync::Arc;

use clawd_agents::{AgentCtx, Registry};
use clawd_contracts::{Env, Platform};
use clawd_testkit::TempConfig;
use serde_json::{json, Value};

const PORT: u16 = 47913;

fn registry(tc: &TempConfig, platform: Platform) -> Registry {
    Registry::new(AgentCtx {
        paths: tc.paths().clone(),
        platform,
        port: Some(PORT),
        script: None,
        which: Arc::new(|_| None),
        env: Env::from_map(HashMap::new()),
    })
}

fn leftovers(dir: &std::path::Path) -> Vec<String> {
    fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.contains(".bak-clawd-") || n.ends_with(".clawd-tmp"))
        .collect()
}

#[test]
fn an_unparsable_config_is_never_touched() {
    const BROKEN: &str = "{ // comments";
    for (agent_id, rel) in [
        ("gemini", ".gemini/settings.json"),
        ("codex", ".codex/hooks.json"),
        ("cursor", ".cursor/hooks.json"),
    ] {
        for platform in [Platform::Darwin, Platform::Win32] {
            let tc = TempConfig::new();
            let file = tc.write_home(rel, BROKEN);
            let reg = registry(&tc, platform);
            let agent = reg.get(agent_id).unwrap();
            let label = format!("{agent_id}/{platform:?}");

            for step in ["install", "uninstall"] {
                let result = if step == "install" {
                    agent.install()
                } else {
                    agent.uninstall()
                };
                let err = result.expect_err(&label).to_string();
                assert!(
                    err.contains(file.file_name().unwrap().to_str().unwrap())
                        && err.contains("không đọc được"),
                    "{label}/{step}: {err}"
                );
                assert_eq!(fs::read_to_string(&file).unwrap(), BROKEN, "{label}/{step}");
                assert!(
                    leftovers(file.parent().unwrap()).is_empty(),
                    "{label}/{step}"
                );
            }
            assert!(agent.is_installed().is_err(), "{label}: is_installed");
            let info = agent.info();
            assert!(!info.error.is_empty() && !info.installed, "{label}: info");
        }
    }
}

/// The hook entry an agent wrote: first handler of the first event in its config file.
fn first_handler(tc: &TempConfig, rel: &str) -> Value {
    let text = fs::read_to_string(tc.home().join(rel)).unwrap();
    let data: Value = serde_json::from_str(&text).unwrap();
    let (_, entries) = data["hooks"].as_object().unwrap().iter().next().unwrap();
    let entry = &entries[0];
    entry
        .get("hooks")
        .map_or_else(|| entry.clone(), |hs| hs[0].clone())
}

#[test]
fn an_agent_does_not_claim_another_agents_hooks() {
    let tc = TempConfig::new();
    let reg = registry(&tc, Platform::Darwin);
    for id in ["codex", "gemini", "cursor"] {
        reg.get(id).unwrap().install().unwrap();
    }
    let hooks = [
        ("codex", first_handler(&tc, ".codex/hooks.json")),
        ("gemini", first_handler(&tc, ".gemini/settings.json")),
        ("cursor", first_handler(&tc, ".cursor/hooks.json")),
    ];
    for (owner, hook) in &hooks {
        for agent in reg.iter() {
            assert_eq!(
                agent.ours(hook),
                agent.id() == *owner,
                "{} hook seen by {}",
                owner,
                agent.id()
            );
        }
    }
    let foreign = json!({"type": "command", "command": "node /x/codex-helper.cjs"});
    assert!(reg.iter().all(|a| !a.ours(&foreign)));
}
