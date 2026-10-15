//! Hook payloads of the agents events.py knows itself -> tracker events (`events.py:487-553`).
//! Events are JSON objects (`serde_json::Value`) exactly like the Python dicts: keys `src`,
//! `sid`, `ev`, `tool`, `cmd`, `msg`, `sub`, `debug`.

use serde_json::Value;

/// `events._tool_summary(tool, inp)`: `"<tool>: <first of command, file_path, path, url,
/// pattern, description>"`, or just `tool`. Public here because `clawd-agents` (Codex) uses it
/// (`agents.base.tool_summary`).
pub fn tool_summary(tool: &str, input: &Value) -> String {
    todo!("phase 04")
}

/// `events.claude_event(p)`: map one Claude Code hook payload (its stdin JSON) to a tracker
/// event, or `None` (not a Claude payload, Cursor running Claude hooks, idle notifications...).
pub fn claude_event(p: &Value) -> Option<Value> {
    todo!("phase 04")
}

/// `events.pi_event(p)`: validate an event a pi/opencode extension already shaped like the
/// tracker's, filling `src = "pi"` and a prefixed `sid`.
pub fn pi_event(p: &Value) -> Option<Value> {
    todo!("phase 04")
}
