//! Hook payloads of the agents events.py knows itself -> tracker events (`events.py:487-553`).
//! Events are JSON objects (`serde_json::Value`) exactly like the Python dicts: keys `src`,
//! `sid`, `ev`, `tool`, `cmd`, `msg`, `sub`, `debug`.

use serde_json::{json, Map, Value};

use std::sync::LazyLock;

use regex::Regex;

use crate::pyval::{py_str, text_or_empty, truthy};
use crate::skill_rules::normalize_skill;
use crate::vocab::{is_debug_prompt, is_debug_skill_load, tool_action, EVENTS};

/// First `n` characters (Python `s[:n]`).
pub(crate) fn char_prefix(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

/// `events._tool_summary(tool, inp)`: `"<tool>: <first of command, file_path, path, url,
/// pattern, description>"`, or just `tool`. Public here because `clawd-agents` (Codex) uses it
/// (`agents.base.tool_summary`).
pub fn tool_summary(tool: &str, input: &Value) -> String {
    let Some(map) = input.as_object() else {
        return tool.to_owned();
    };
    for key in [
        "command",
        "file_path",
        "path",
        "url",
        "pattern",
        "description",
    ] {
        if let Some(Value::String(v)) = map.get(key) {
            if !v.trim().is_empty() {
                return format!("{tool}: {}", v.trim());
            }
        }
    }
    tool.to_owned()
}

/// `_CLAUDE_SIMPLE`: hooks that map one to one.
fn claude_simple(name: &str) -> Option<&'static str> {
    Some(match name {
        "SessionStart" => "start",
        "UserPromptSubmit" => "prompt",
        "PostToolUse" => "tool_done",
        "PostToolUseFailure" => "tool_error",
        "SubagentStart" => "subagent_start",
        "SubagentStop" => "subagent_stop",
        "Stop" => "stop",
        "StopFailure" => "stop_error",
        "PreCompact" => "compact",
        "PostCompact" => "compact_done",
        "SessionEnd" => "end",
        _ => return None,
    })
}

/// `events.claude_event(p)`: map one Claude Code hook payload (its stdin JSON) to a tracker
/// event, or `None` (not a Claude payload, Cursor running Claude hooks, idle notifications...).
pub fn claude_event(p: &Value) -> Option<Value> {
    let map = p.as_object()?;
    if map.contains_key("cursor_version") {
        return None; // Cursor running Claude Code's hooks: its own adapter reports it
    }
    let name = map.get("hook_event_name")?.as_str()?;
    let sid = map.get("session_id")?.as_str().filter(|s| !s.is_empty())?;
    let tool = map
        .get("tool_name")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let mut ev = Map::new();
    ev.insert("src".into(), json!("claude"));
    ev.insert("sid".into(), json!(format!("claude:{sid}")));
    ev.insert("tool".into(), json!(tool));
    let input = map.get("tool_input");
    if let Some(Value::String(command)) = input.and_then(|i| i.get("command")) {
        ev.insert("cmd".into(), json!(char_prefix(command, 300)));
    }
    if name == "UserPromptSubmit" && is_debug_prompt(map.get("prompt").and_then(Value::as_str)) {
        ev.insert("debug".into(), json!(true)); // only the verdict travels, never the prompt
    }
    if name == "PreToolUse" && is_debug_skill_load(tool, input.unwrap_or(&Value::Null)) {
        ev.insert("debug".into(), json!(true));
    }
    let finish = |mut ev: Map<String, Value>, kind: &str, msg: Option<String>| {
        ev.insert("ev".into(), json!(kind));
        if let Some(msg) = msg {
            ev.insert("msg".into(), json!(msg));
        }
        Some(Value::Object(ev))
    };
    if let Some(kind) = claude_simple(name) {
        return finish(ev, kind, None);
    }
    match name {
        "PreToolUse" => {
            // the question text stays on this machine
            let kind = if tool == "AskUserQuestion" {
                "attention"
            } else {
                "tool"
            };
            finish(ev, kind, None)
        }
        "PermissionRequest" => {
            let summary = tool_summary(tool, input.unwrap_or(&Value::Null));
            finish(ev, "attention", Some(format!("Allow {summary}")))
        }
        "Notification" => {
            let kind = text_or_empty(map.get("notification_type"));
            let msg = text_or_empty(map.get("message"));
            if kind == "idle_prompt"
                || (kind.is_empty() && msg.to_lowercase().contains("waiting for your input"))
            {
                return None; // the turn already ended; nothing to answer
            }
            let msg = if msg.is_empty() {
                "Claude needs you".to_owned()
            } else {
                msg
            };
            finish(ev, "attention", Some(msg))
        }
        _ => None,
    }
}

/// `events.pi_event(p)`: validate an event a pi/opencode extension already shaped like the
/// tracker's, filling `src = "pi"` and a prefixed `sid`. The skill names the pi extension sends
/// (`skill`: typed in the prompt, `skill_nested`: loaded by the agent) pass through normalized,
/// and only when present.
pub fn pi_event(p: &Value) -> Option<Value> {
    let map = p.as_object()?;
    let kind = map.get("ev")?.as_str().filter(|k| EVENTS.contains(k))?;
    let sid = map.get("sid").filter(|s| truthy(Some(s)))?;
    let debug = matches!(map.get("debug"), Some(Value::Bool(true)));
    let mut ev = json!({
        "src": "pi",
        "sid": format!("pi:{}", char_prefix(&py_str(sid), 160)),
        "ev": kind,
        "tool": text_or_empty(map.get("tool")),
        "msg": text_or_empty(map.get("msg")),
        "cmd": char_prefix(&text_or_empty(map.get("cmd")), 300),
        "sub": truthy(map.get("sub")),
        "debug": debug,
    });
    for key in ["skill", "skill_nested"] {
        let name = normalize_skill(&text_or_empty(map.get(key)));
        if !name.is_empty() {
            ev[key] = json!(name);
        }
    }
    Some(ev)
}

static SKILL_BLOCK_NAME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"<skill\s+name="([^"]+)""#).expect("SKILL_BLOCK_NAME"));
static SKILL_FILE_DIR: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:^|[/\\])([^/\\]+)[/\\]SKILL\.md\n?$").expect("SKILL_FILE_DIR")
});

/// The skill a prompt invokes, normalized: a typed `/name args` (Claude Code passes the line as
/// typed; pi's `/skill:name`), or the first `<skill name="...">` block pi expanded into it.
/// Empty when there is none. Only this name ever leaves the hook, never the prompt.
pub fn prompt_skill(prompt: &str) -> String {
    if let Some(caps) = SKILL_BLOCK_NAME.captures(prompt) {
        return normalize_skill(&caps[1]);
    }
    let line = prompt.trim_start();
    if !line.starts_with('/') {
        return String::new();
    }
    normalize_skill(line.split_whitespace().next().unwrap_or(""))
}

/// The skill a tool call loads, normalized: a `Skill` tool (`skill` or `command` input), or a
/// read of `<name>/SKILL.md`. Empty when the call loads no skill.
pub fn nested_skill(tool: &str, input: &Value) -> String {
    let Some(map) = input.as_object() else {
        return String::new();
    };
    if tool.trim().eq_ignore_ascii_case("skill") {
        let raw = [map.get("skill"), map.get("command")]
            .into_iter()
            .flatten()
            .find_map(Value::as_str)
            .unwrap_or_default();
        return normalize_skill(raw);
    }
    if tool_action(tool) == "read" {
        let path = [map.get("file_path"), map.get("path")]
            .into_iter()
            .flatten()
            .find_map(Value::as_str)
            .unwrap_or_default();
        if let Some(caps) = SKILL_FILE_DIR.captures(path) {
            return normalize_skill(&caps[1]);
        }
    }
    String::new()
}

/// [`claude_event`] plus the skill names skill animation rules need: `skill` on a prompt that
/// invokes one, `skill_nested` on a tool call that loads one. What the live Claude adapter uses;
/// `claude_event` itself stays the Python port the golden fixtures pin.
pub fn claude_event_with_skill(p: &Value) -> Option<Value> {
    let mut ev = claude_event(p)?;
    let (key, name) = match p.get("hook_event_name").and_then(Value::as_str) {
        Some("UserPromptSubmit") => (
            "skill",
            prompt_skill(p.get("prompt").and_then(Value::as_str).unwrap_or_default()),
        ),
        Some("PreToolUse") => (
            "skill_nested",
            nested_skill(
                p.get("tool_name")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
                p.get("tool_input").unwrap_or(&Value::Null),
            ),
        ),
        _ => return Some(ev),
    };
    if !name.is_empty() {
        ev[key] = json!(name);
    }
    Some(ev)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn claude(name: &str, extra: Value) -> Option<Value> {
        let mut p = json!({"hook_event_name": name, "session_id": "s1"});
        p.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        claude_event(&p)
    }

    #[test]
    fn claude_mapping_matches_python() {
        let ev = claude(
            "PreToolUse",
            json!({"tool_name": "Bash", "tool_input": {"command": "ls"}}),
        )
        .unwrap();
        assert_eq!(
            serde_json::to_string(&ev).unwrap(),
            r#"{"src":"claude","sid":"claude:s1","tool":"Bash","cmd":"ls","ev":"tool"}"#
        );
        let ev = claude(
            "PermissionRequest",
            json!({"tool_name": "Bash", "tool_input": {"command": " npm test "}}),
        )
        .unwrap();
        assert_eq!(ev["msg"], "Allow Bash: npm test");
        assert_eq!(
            claude("PreToolUse", json!({"tool_name": "AskUserQuestion"})).unwrap()["ev"],
            "attention"
        );
        assert!(claude("Notification", json!({"notification_type": "idle_prompt"})).is_none());
        assert!(claude(
            "Notification",
            json!({"message": "Claude is Waiting for your input"})
        )
        .is_none());
        assert_eq!(
            claude("Notification", json!({})).unwrap()["msg"],
            "Claude needs you"
        );
        assert!(claude("Whatever", json!({})).is_none());
        assert!(claude("Stop", json!({"cursor_version": "1"})).is_none());
        assert!(claude_event(&json!({"hook_event_name": "Stop", "session_id": ""})).is_none());
        assert_eq!(
            claude("UserPromptSubmit", json!({"prompt": "fix the bug"})).unwrap()["debug"],
            true
        );
    }

    #[test]
    fn pi_event_validates_and_prefixes() {
        assert!(pi_event(&json!({"ev": "nope", "sid": "a"})).is_none());
        assert!(pi_event(&json!({"ev": "tool", "sid": ""})).is_none());
        let ev = pi_event(&json!({"ev": "tool", "sid": "a", "tool": "bash", "sub": 1, "debug": 1}))
            .unwrap();
        assert_eq!(
            serde_json::to_string(&ev).unwrap(),
            r#"{"src":"pi","sid":"pi:a","ev":"tool","tool":"bash","msg":"","cmd":"","sub":true,"debug":false}"#
        );
    }

    #[test]
    fn skills_from_prompts_and_tool_calls() {
        assert_eq!(prompt_skill("/ak-cook dạng bảng"), "ak-cook");
        assert_eq!(prompt_skill("  /ak:cook"), "ak-cook");
        assert_eq!(prompt_skill("/skill:ak-advise x"), "ak-advise");
        assert_eq!(
            prompt_skill("<skill name=\"ak-plan\" location=\"x\">\nbody\n</skill>\n\ngo"),
            "ak-plan"
        );
        assert_eq!(prompt_skill("fix /ak-cook later"), "");
        assert_eq!(prompt_skill("/Users/me/file.txt explain"), "");
        assert_eq!(prompt_skill(""), "");
        assert_eq!(
            nested_skill("Skill", &json!({"skill": "ak:journal"})),
            "ak-journal"
        );
        assert_eq!(
            nested_skill("skill", &json!({"command": "/ak-plan"})),
            "ak-plan"
        );
        assert_eq!(
            nested_skill(
                "read",
                &json!({"path": "/u/.agents/skills/ak-cook/SKILL.md"})
            ),
            "ak-cook"
        );
        assert_eq!(
            nested_skill("Read", &json!({"file_path": "C:\\s\\ak-fix\\SKILL.md"})),
            "ak-fix"
        );
        assert_eq!(
            nested_skill("read", &json!({"path": "ak-cook/refs/x.md"})),
            ""
        );
        assert_eq!(
            nested_skill("edit", &json!({"path": "ak-cook/SKILL.md"})),
            ""
        );
        assert_eq!(nested_skill("Skill", &json!("x")), "");
    }

    #[test]
    fn claude_events_carry_skill_names_only_when_there_is_one() {
        let with = |name: &str, extra: Value| {
            let mut p = json!({"hook_event_name": name, "session_id": "s1"});
            p.as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            claude_event_with_skill(&p).unwrap()
        };
        let ev = with("UserPromptSubmit", json!({"prompt": "/ak-cook build it"}));
        assert_eq!(ev["skill"], "ak-cook");
        assert_eq!(ev["ev"], "prompt");
        let ev = with(
            "PreToolUse",
            json!({"tool_name": "Skill", "tool_input": {"skill": "ak:journal"}}),
        );
        assert_eq!(ev["skill_nested"], "ak-journal");
        // nothing to add: exactly what claude_event gives
        for (name, extra) in [
            ("UserPromptSubmit", json!({"prompt": "fix the bug"})),
            (
                "PreToolUse",
                json!({"tool_name": "Bash", "tool_input": {"command": "ls"}}),
            ),
            ("Stop", json!({})),
        ] {
            let mut p = json!({"hook_event_name": name, "session_id": "s1"});
            p.as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            assert_eq!(claude_event_with_skill(&p), claude_event(&p), "{name}");
        }
        assert!(claude_event_with_skill(&json!({"x": 1})).is_none());
    }

    #[test]
    fn pi_events_pass_skill_names_through_normalized() {
        let ev = pi_event(&json!({"ev": "prompt", "sid": "a", "skill": "ak:cook"})).unwrap();
        assert_eq!(ev["skill"], "ak-cook");
        assert!(ev.get("skill_nested").is_none());
        let ev = pi_event(&json!({"ev": "tool", "sid": "a", "skill_nested": "bad name"})).unwrap();
        assert!(ev.get("skill_nested").is_none());
    }

    #[test]
    fn tool_summary_picks_first_present_key() {
        assert_eq!(tool_summary("Read", &json!({"path": " /a "})), "Read: /a");
        assert_eq!(
            tool_summary("Read", &json!({"command": " ", "url": "u"})),
            "Read: u"
        );
        assert_eq!(tool_summary("Read", &json!("x")), "Read");
    }
}
