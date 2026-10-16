//! Hook payloads of the agents events.py knows itself -> tracker events (`events.py:487-553`).
//! Events are JSON objects (`serde_json::Value`) exactly like the Python dicts: keys `src`,
//! `sid`, `ev`, `tool`, `cmd`, `msg`, `sub`, `debug`.

use serde_json::{json, Map, Value};

use crate::pyval::{py_str, text_or_empty, truthy};
use crate::vocab::{is_debug_prompt, is_debug_skill_load, EVENTS};

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
/// tracker's, filling `src = "pi"` and a prefixed `sid`.
pub fn pi_event(p: &Value) -> Option<Value> {
    let map = p.as_object()?;
    let kind = map.get("ev")?.as_str().filter(|k| EVENTS.contains(k))?;
    let sid = map.get("sid").filter(|s| truthy(Some(s)))?;
    let debug = matches!(map.get("debug"), Some(Value::Bool(true)));
    Some(json!({
        "src": "pi",
        "sid": format!("pi:{}", char_prefix(&py_str(sid), 160)),
        "ev": kind,
        "tool": text_or_empty(map.get("tool")),
        "msg": text_or_empty(map.get("msg")),
        "cmd": char_prefix(&text_or_empty(map.get("cmd")), 300),
        "sub": truthy(map.get("sub")),
        "debug": debug,
    }))
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
    fn tool_summary_picks_first_present_key() {
        assert_eq!(tool_summary("Read", &json!({"path": " /a "})), "Read: /a");
        assert_eq!(
            tool_summary("Read", &json!({"command": " ", "url": "u"})),
            "Read: u"
        );
        assert_eq!(tool_summary("Read", &json!("x")), "Read");
    }
}
