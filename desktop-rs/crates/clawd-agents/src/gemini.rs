//! Gemini CLI: command hooks in `~/.gemini/settings.json` (`agents/gemini.py`).
//!
//! Format and events: <https://geminicli.com/docs/hooks/reference/>. Every payload has
//! `session_id` and `hook_event_name`. Hooks run in line, so ours give up within 0.3 s and print
//! nothing (empty stdout = allow). `~/.gemini` alone is not enough to say Gemini CLI is here:
//! the Antigravity IDE makes that folder too.

use std::path::PathBuf;

use clawd_contracts::{AgentError, Platform};
use serde_json::{json, Value};

use crate::base::{Agent, AgentCtx};
use crate::json_edit::{
    base_event, edit_json, first_str, has_ours, load, merge_groups, pre_tool_kind, prompt_event,
    str_or, truthy, with_ev,
};

const SIMPLE: [(&str, &str); 4] = [
    ("SessionStart", "start"),
    ("AfterAgent", "stop"),
    ("PreCompress", "compact"),
    ("SessionEnd", "end"),
];
const EVENTS: [&str; 8] = [
    "SessionStart",
    "BeforeAgent",
    "BeforeTool",
    "AfterTool",
    "AfterAgent",
    "PreCompress",
    "Notification",
    "SessionEnd",
];
const NOTE: &str = "Khởi động lại Gemini CLI sau khi nối.";

/// The `gemini` adapter.
pub struct Gemini {
    ctx: AgentCtx,
}

impl Gemini {
    pub fn new(ctx: AgentCtx) -> Self {
        Self { ctx }
    }

    fn file(&self) -> PathBuf {
        self.config_dir().join("settings.json")
    }

    fn handler(&self, event: &str) -> Value {
        json!({
            "name": format!("clawd-{event}"),
            "type": "command",
            "command": self.command(event),
            "timeout": 2000,
        })
    }
}

impl Agent for Gemini {
    fn id(&self) -> &'static str {
        "gemini"
    }
    fn name(&self) -> &'static str {
        "Gemini CLI"
    }
    fn signals(&self) -> &'static [&'static str] {
        &["think", "tool", "error", "attention"]
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
        self.ctx.paths.home().join(".gemini")
    }
    fn found(&self) -> bool {
        let dir = self.config_dir();
        (self.ctx.which)("gemini").is_some()
            || dir.join("settings.json").exists()
            || dir.join("oauth_creds.json").exists()
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
        // no matcher: every tool / event
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
        let ev = base_event("gemini", &sid, tool, input);
        if name == "BeforeAgent" {
            return vec![prompt_event(&ev, p)];
        }
        if let Some((_, kind)) = SIMPLE.iter().find(|(n, _)| *n == name) {
            return vec![with_ev(&ev, kind)];
        }
        match name.as_str() {
            "BeforeTool" => vec![with_ev(&ev, pre_tool_kind(tool))],
            "AfterTool" => {
                let failed = p
                    .get("tool_response")
                    .and_then(Value::as_object)
                    .is_some_and(|r| r.get("error").is_some_and(truthy));
                vec![with_ev(
                    &ev,
                    if failed { "tool_error" } else { "tool_done" },
                )]
            }
            "Notification" => {
                if str_or(p, "notification_type", "") != "ToolPermission" {
                    return Vec::new();
                }
                let mut out = with_ev(&ev, "attention");
                out["msg"] = Value::String(str_or(p, "message", "Gemini needs you"));
                vec![out]
            }
            _ => Vec::new(),
        }
    }
}
