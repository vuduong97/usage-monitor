//! Codex CLI: command hooks in `$CODEX_HOME|~/.codex/hooks.json` (Claude-like payloads,
//! `async` hooks so they never hold up a tool call). `agents/codex.py`.
//!
//! Format and events: <https://developers.openai.com/codex/hooks>. Subagent hooks carry the
//! parent's session id. Codex runs a new or changed hook only after the user trusts it in
//! `/hooks`, hence the note.

use std::path::PathBuf;

use clawd_contracts::{AgentError, Platform};
use serde_json::{json, Value};

use crate::base::{Agent, AgentCtx};
use crate::json_edit::{
    base_event, edit_json, first_str, has_ours, load, merge_groups, pre_tool_kind, prompt_event,
    with_ev,
};

const SIMPLE: [(&str, &str); 9] = [
    ("SessionStart", "start"),
    ("PostToolUse", "tool_done"),
    ("PreCompact", "compact"),
    ("PostCompact", "compact_done"),
    ("SubagentStart", "subagent_start"),
    ("SubagentStop", "subagent_stop"),
    ("Stop", "stop"),
    ("Interrupt", "abort"),
    ("SessionEnd", "end"),
];
const EVENTS: [&str; 12] = [
    "SessionStart",
    "UserPromptSubmit",
    "PreToolUse",
    "PermissionRequest",
    "PostToolUse",
    "PreCompact",
    "PostCompact",
    "SubagentStart",
    "SubagentStop",
    "Stop",
    "Interrupt",
    "SessionEnd",
];
/// Codex caps these at 1-3 s and runs `SessionEnd` in line.
const SHORT: [&str; 2] = ["SessionEnd", "Interrupt"];
const NOTE: &str = "Mở Codex, gõ /hooks và chọn tin cậy các hook của Clawd (một lần).";

/// The `codex` adapter.
pub struct Codex {
    ctx: AgentCtx,
}

impl Codex {
    pub fn new(ctx: AgentCtx) -> Self {
        Self { ctx }
    }

    fn file(&self) -> PathBuf {
        self.config_dir().join("hooks.json")
    }

    fn handler(&self, event: &str) -> Value {
        let mut h = json!({
            "type": "command",
            "command": self.posix_command(event),
            "commandWindows": self.windows_command(event),
            "timeout": if SHORT.contains(&event) { 2 } else { 5 },
        });
        if event != "SessionEnd" {
            h["async"] = Value::Bool(true);
        }
        h
    }
}

impl Agent for Codex {
    fn id(&self) -> &'static str {
        "codex"
    }
    fn name(&self) -> &'static str {
        "Codex CLI"
    }
    fn signals(&self) -> &'static [&'static str] {
        &["think", "tool", "subagent", "attention"]
    }
    fn tested(&self) -> &'static [Platform] {
        &[]
    }
    fn note(&self) -> &'static str {
        NOTE
    }
    fn ctx(&self) -> &AgentCtx {
        &self.ctx
    }

    fn config_dir(&self) -> PathBuf {
        self.env_dir("CODEX_HOME", self.ctx.paths.home().join(".codex"))
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
            &|d| merge_groups(d, Some(&add), &|h| self.ours(h), None),
            true,
        )?;
        Ok(format!("Đã nối. {NOTE}"))
    }
    fn uninstall(&self) -> Result<String, AgentError> {
        edit_json(
            &self.file(),
            &|d| merge_groups(d, None, &|h| self.ours(h), None),
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
        let sid = first_str(p, &["session_id"]);
        if sid.is_empty() {
            return Vec::new();
        }
        let tool = p.get("tool_name").unwrap_or(&Value::Null);
        let input = p.get("tool_input").unwrap_or(&Value::Null);
        let ev = base_event("codex", &sid, tool, input);
        if name == "UserPromptSubmit" {
            return vec![prompt_event(&ev, p)];
        }
        if let Some((_, kind)) = SIMPLE.iter().find(|(n, _)| *n == name) {
            return vec![with_ev(&ev, kind)];
        }
        match name.as_str() {
            "PreToolUse" => vec![with_ev(&ev, pre_tool_kind(tool))],
            "PermissionRequest" => {
                let mut out = with_ev(&ev, "attention");
                let tool_name = ev["tool"].as_str().unwrap_or_default();
                out["msg"] = Value::String(format!(
                    "Allow {}",
                    clawd_events::tool_summary(tool_name, input)
                ));
                vec![out]
            }
            _ => Vec::new(),
        }
    }
}
