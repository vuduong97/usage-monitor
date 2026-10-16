//! Cursor (the editor's agent and the cursor-agent CLI): `~/.cursor/hooks.json`
//! (`agents/cursor.py`).
//!
//! Format and events: <https://cursor.com/docs/hooks>. Only observing hooks are used. On
//! Cursor's permission hooks (`preToolUse`, `beforeShellExecution`, `beforeReadFile`,
//! `beforeMCPExecution`, `subagentStart`) an empty or non-JSON answer BLOCKS the action, so a
//! hook that just reports would stop the agent whenever Clawd is not running. Hence no "tool
//! started": a tool shows once it is done (`postToolUse`), and no subagent count.
//!
//! Cursor also runs Claude Code's hooks from `~/.claude/settings.json` (Third-Party Imports, on by
//! default); `clawd_events::claude_event` drops those Cursor payloads, so a Cursor chat never
//! counts twice.

use std::path::PathBuf;

use clawd_contracts::{AgentError, Platform};
use serde_json::{json, Value};

use crate::base::{Agent, AgentCtx};
use crate::json_edit::{
    base_event, edit_json, first_str, has_ours, load, merge_flat, prompt_event, str_or, truthy,
    with_ev,
};

const EVENTS: [&str; 7] = [
    "sessionStart",
    "beforeSubmitPrompt",
    "postToolUse",
    "postToolUseFailure",
    "preCompact",
    "stop",
    "sessionEnd",
];

/// `STOP[status]`, default `stop`.
fn stop_kind(status: &str) -> &'static str {
    match status {
        "aborted" => "abort",
        "error" => "stop_error",
        _ => "stop",
    }
}

/// `_tool(raw)`: the tool name without the `MCP:` prefix, `""` when not a string.
fn tool_name(raw: Option<&Value>) -> String {
    let t = raw.and_then(Value::as_str).unwrap_or_default();
    t.strip_prefix("MCP:").unwrap_or(t).to_owned()
}

/// The `cursor` adapter.
pub struct Cursor {
    ctx: AgentCtx,
}

impl Cursor {
    pub fn new(ctx: AgentCtx) -> Self {
        Self { ctx }
    }

    fn file(&self) -> PathBuf {
        self.config_dir().join("hooks.json")
    }

    fn handler(&self, event: &str) -> Value {
        json!({"command": self.command(event), "timeout": 5})
    }
}

impl Agent for Cursor {
    fn id(&self) -> &'static str {
        "cursor"
    }
    fn name(&self) -> &'static str {
        "Cursor"
    }
    fn signals(&self) -> &'static [&'static str] {
        &["think", "tool", "error"]
    }
    fn tested(&self) -> &'static [Platform] {
        &[]
    }
    fn note(&self) -> &'static str {
        ""
    }
    fn ctx(&self) -> &AgentCtx {
        &self.ctx
    }

    fn config_dir(&self) -> PathBuf {
        self.ctx.paths.home().join(".cursor")
    }
    fn is_installed(&self) -> Result<bool, AgentError> {
        let file = self.file();
        if !file.exists() {
            return Ok(false);
        }
        Ok(has_ours(Some(&load(&file)?), &|h| self.ours(h)))
    }
    fn install(&self) -> Result<String, AgentError> {
        crate::base::prepare_for(self)?;
        let add: Vec<(String, Value)> = EVENTS
            .iter()
            .map(|e| ((*e).to_owned(), self.handler(e)))
            .collect();
        edit_json(
            &self.file(),
            &|d| {
                let mut out = merge_flat(d, Some(&add), &|h| self.ours(h));
                if let Some(obj) = out.as_object_mut() {
                    if !obj.contains_key("version") {
                        obj.insert("version".to_owned(), json!(1));
                    }
                }
                out
            },
            true,
        )?;
        Ok("Đã nối.".to_owned())
    }
    fn uninstall(&self) -> Result<String, AgentError> {
        edit_json(
            &self.file(),
            &|d| merge_flat(d, None, &|h| self.ours(h)),
            false,
        )?;
        Ok("Đã ngắt.".to_owned())
    }
    fn normalize(&self, p: &Value, hint: &str) -> Vec<Value> {
        if !p.is_object() {
            return Vec::new();
        }
        let mut name = first_str(p, &["hook_event_name"]);
        if name.is_empty() {
            name = hint.to_owned();
        }
        let sid = first_str(p, &["conversation_id", "session_id"]);
        if sid.is_empty() {
            return Vec::new();
        }
        let tool = Value::String(tool_name(p.get("tool_name")));
        let input = p.get("tool_input").unwrap_or(&Value::Null);
        let ev = base_event("cursor", &sid, &tool, input);
        match name.as_str() {
            "sessionStart" => {
                let mut out = with_ev(&ev, "start");
                out["sub"] = Value::Bool(p.get("is_background_agent").is_some_and(truthy));
                vec![out]
            }
            "beforeSubmitPrompt" => vec![prompt_event(&ev, p)],
            "postToolUse" => vec![with_ev(&ev, "tool"), with_ev(&ev, "tool_done")],
            "postToolUseFailure" => {
                let interrupted = p.get("is_interrupt").is_some_and(truthy);
                vec![with_ev(
                    &ev,
                    if interrupted { "abort" } else { "tool_error" },
                )]
            }
            "preCompact" => vec![with_ev(&ev, "compact")],
            "stop" => vec![with_ev(&ev, stop_kind(&str_or(p, "status", "completed")))],
            "sessionEnd" => vec![with_ev(&ev, "end")],
            _ => Vec::new(),
        }
    }
}
