//! `Clawd cleanup` (what the uninstaller runs) over a temp home: hooks gone from every agent,
//! the autostart entry gone, foreign settings untouched. Library level: the CLI itself builds
//! `EngineConfig::system()` (real home, production Run value), so it is never run here; the test
//! drives the same `cleanup_with` the CLI does, with sandbox names and a temp home.
use std::sync::Arc;

use clawd_agents::{hooks, uninstall_all, AgentCtx, Registry};
use clawd_app::cli::cleanup_with;
use clawd_contracts::{Env, Platform};
use clawd_testkit::store::SandboxAutostart;
use clawd_testkit::TempConfig;
use serde_json::{json, Value};

fn registry(tc: &TempConfig, platform: Platform) -> Registry {
    Registry::new(AgentCtx {
        paths: tc.paths().clone(),
        platform,
        port: Some(47913),
        script: None,
        which: Arc::new(|_| None),
        env: Env::empty(),
    })
}

fn read_json(path: &std::path::Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).expect("read")).expect("json")
}

/// Hooks on (the `Clawd hooks on` path), every agent installable on a bare temp home installed,
/// the autostart entry written; then cleanup.
fn cleanup_scenario(platform: Platform) {
    let tc = TempConfig::new();
    let foreign = json!({
        "model": "opus",
        "hooks": {"PreToolUse": [{"matcher": "Bash", "hooks": [{"type": "command", "command": "my-own-linter"}]}]}
    });
    let settings = tc.write_home(".claude/settings.json", foreign.to_string());

    let lines = hooks::apply(tc.paths(), &Env::empty(), true, None, platform, None);
    assert!(!lines.is_empty(), "hooks on says something");
    let installed: Value = read_json(&settings);
    assert!(
        installed.to_string().contains("/hook/claude")
            || installed.to_string().contains("clawd-event"),
        "our hook is in settings.json: {installed}"
    );
    assert!(installed.to_string().contains("my-own-linter"));

    let registry = registry(&tc, platform);
    let mut connected = Vec::new();
    for agent in registry.iter() {
        if agent.install().is_ok() && agent.is_installed().unwrap_or(false) {
            connected.push(agent.id().to_owned());
        }
    }
    assert!(
        connected.iter().any(|id| id == "claude"),
        "claude connects on a temp home: {connected:?}"
    );

    let sandbox = SandboxAutostart::new(
        tc.paths(),
        tc.os_names(),
        platform,
        vec!["C:/clawd-e2e/Clawd.exe".to_owned()],
    );
    assert!(
        sandbox.autostart.enable().expect("enable"),
        "autostart supported"
    );
    assert!(sandbox.autostart.is_enabled());

    let mut out = Vec::new();
    cleanup_with(
        || sandbox.autostart.disable(),
        || uninstall_all(&registry),
        || "12:00:00".to_owned(),
        &mut out,
    );
    let text = String::from_utf8(out).expect("utf8");

    assert!(text.ends_with("[12:00:00] cleanup done\n"), "{text}");
    for id in &connected {
        assert!(
            text.contains(&format!("[12:00:00] {id}: ")),
            "a line for {id}: {text}"
        );
        assert!(
            !registry
                .get(id)
                .expect("agent")
                .is_installed()
                .unwrap_or(false),
            "{id} still connected"
        );
    }
    assert!(!sandbox.autostart.is_enabled(), "autostart entry gone");
    let after = read_json(&settings).to_string();
    assert!(
        !after.contains("/hook/claude") && !after.contains("clawd-event"),
        "no Clawd hook left: {after}"
    );
    assert!(
        after.contains("my-own-linter") && after.contains("opus"),
        "the user's own settings survive: {after}"
    );

    // the uninstaller may run twice (repair, then uninstall): the second run is harmless
    let mut again = Vec::new();
    cleanup_with(
        || sandbox.autostart.disable(),
        || uninstall_all(&registry),
        || "12:00:01".to_owned(),
        &mut again,
    );
    assert!(String::from_utf8(again)
        .expect("utf8")
        .ends_with("cleanup done\n"));
    assert_eq!(
        read_json(&settings).to_string(),
        after,
        "a second cleanup changes nothing"
    );
}

#[test]
fn cleanup_removes_hooks_and_the_launch_agent_on_macos_layout() {
    cleanup_scenario(Platform::Darwin);
}

#[cfg(windows)]
#[test]
fn cleanup_removes_hooks_and_the_run_value_sandbox_on_windows() {
    cleanup_scenario(Platform::Win32);
}

#[test]
fn cleanup_keeps_going_when_autostart_fails() {
    let tc = TempConfig::new();
    tc.write_home(".claude/settings.json", "{}");
    let registry = registry(&tc, Platform::Win32);
    registry
        .get("claude")
        .expect("claude")
        .install()
        .expect("install");
    let mut out = Vec::new();
    cleanup_with(
        || Err(std::io::Error::other("registry locked")),
        || uninstall_all(&registry),
        || "t".to_owned(),
        &mut out,
    );
    let text = String::from_utf8(out).expect("utf8");
    assert!(text.contains("autostart: registry locked"), "{text}");
    assert!(!registry.get("claude").unwrap().is_installed().unwrap());
}
