//! GitHub Copilot CLI: our own hook file, `~/.copilot/hooks/clawd.json` (`COPILOT_HOME` moves
//! it) (`agents/copilot.py`).
//!
//! Format and events: <https://docs.github.com/en/copilot/reference/hooks-reference>. Copilot
//! reads every `*.json` in that folder, so ours is a file of its own: nothing of the user's is
//! ever edited. PascalCase event names select the payload format with snake_case fields
//! (`session_id`, `tool_name`, `tool_input`), like Claude Code's; `subagentStart` and
//! `notification` only exist in camelCase (`sessionId`). A command hook with no output falls
//! through to Copilot's normal behaviour.

use std::fs;
use std::path::PathBuf;

use clawd_contracts::{AgentError, Platform};
use serde_json::{json, Map, Value};

use crate::base::{Agent, AgentCtx};
use crate::json_edit::{
    base_event, first_str, has_ours, pre_tool_kind, prompt_event, read_json, str_or, truthy,
    with_ev, write_json,
};

const SIMPLE: [(&str, &str); 8] = [
    ("SessionStart", "start"),
    ("PostToolUse", "tool_done"),
    ("PostToolUseFailure", "tool_error"),
    ("Stop", "stop"),
    ("SubagentStop", "subagent_stop"),
    ("subagentStart", "subagent_start"),
    ("PreCompact", "compact"),
    ("SessionEnd", "end"),
];
const EVENTS: [&str; 12] = [
    "SessionStart",
    "UserPromptSubmit",
    "PreToolUse",
    "PostToolUse",
    "PostToolUseFailure",
    "Stop",
    "subagentStart",
    "SubagentStop",
    "PreCompact",
    "ErrorOccurred",
    "notification",
    "SessionEnd",
];

/// The `copilot` adapter.
pub struct Copilot {
    ctx: AgentCtx,
}

impl Copilot {
    pub fn new(ctx: AgentCtx) -> Self {
        Self { ctx }
    }

    fn file(&self) -> PathBuf {
        self.config_dir().join("hooks").join("clawd.json")
    }

    fn handler(&self, event: &str) -> Value {
        json!({
            "type": "command",
            "bash": self.posix_command(event),
            "powershell": self.windows_command(event),
            "timeoutSec": 5,
        })
    }
}

impl Agent for Copilot {
    fn id(&self) -> &'static str {
        "copilot"
    }
    fn name(&self) -> &'static str {
        "Copilot CLI"
    }
    fn signals(&self) -> &'static [&'static str] {
        &["think", "tool", "subagent", "error", "attention"]
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
        self.env_dir("COPILOT_HOME", self.ctx.paths.home().join(".copilot"))
    }
    fn found(&self) -> bool {
        self.config_dir().is_dir() || (self.ctx.which)("copilot").is_some()
    }
    fn is_installed(&self) -> Result<bool, AgentError> {
        let file = self.file();
        Ok(file.exists() && has_ours(read_json(&file).as_ref(), &|h| self.ours(h)))
    }
    fn install(&self) -> Result<String, AgentError> {
        crate::base::prepare_for(self)?;
        let file = self.file();
        if file.exists() && !self.is_installed()? {
            return Err(AgentError(format!(
                "{} đã có và không phải của Clawd, Clawd không ghi đè.",
                file.display()
            )));
        }
        let table: Map<String, Value> = EVENTS
            .iter()
            .map(|e| ((*e).to_owned(), Value::Array(vec![self.handler(e)])))
            .collect();
        let data = json!({"version": 1, "hooks": table});
        if read_json(&file).as_ref() != Some(&data) {
            write_json(&file, &data).map_err(|e| AgentError(format!("{}: {e}", file.display())))?;
        }
        Ok("Đã nối.".to_owned())
    }
    fn uninstall(&self) -> Result<String, AgentError> {
        let file = self.file();
        if !self.is_installed()? {
            return Ok("Chưa nối.".to_owned());
        }
        fs::remove_file(&file).map_err(|e| AgentError(format!("{}: {e}", file.display())))?;
        // Copilot loads every *.json: leave none behind.
        let prefix = format!(
            "{}.bak-clawd-",
            file.file_name().unwrap_or_default().to_string_lossy()
        );
        if let Some(dir) = file.parent() {
            for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
                if entry.file_name().to_string_lossy().starts_with(&prefix) {
                    let _ = fs::remove_file(entry.path());
                }
            }
        }
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
        let sid = first_str(p, &["session_id", "sessionId"]);
        if sid.is_empty() {
            return Vec::new();
        }
        let pick = |snake: &str, camel: &str| -> Value {
            p.get(snake)
                .or_else(|| p.get(camel))
                .cloned()
                .unwrap_or(Value::Null)
        };
        let tool = pick("tool_name", "toolName");
        let input = pick("tool_input", "toolArgs");
        let ev = base_event("copilot", &sid, &tool, &input);
        if name == "UserPromptSubmit" {
            return vec![prompt_event(&ev, p)];
        }
        if let Some((_, kind)) = SIMPLE.iter().find(|(n, _)| *n == name) {
            return vec![with_ev(&ev, kind)];
        }
        match name.as_str() {
            "PreToolUse" => vec![with_ev(&ev, pre_tool_kind(&tool))],
            "ErrorOccurred" => {
                if p.get("recoverable").is_none_or(truthy) {
                    Vec::new()
                } else {
                    vec![with_ev(&ev, "stop_error")]
                }
            }
            "notification" => {
                let mut kind = str_or(p, "notification_type", "");
                if kind.is_empty() {
                    kind = str_or(p, "notificationType", "");
                }
                let kind = kind.to_lowercase();
                if kind.contains("permission") || kind.contains("elicitation") {
                    let mut out = with_ev(&ev, "attention");
                    out["msg"] = Value::String(str_or(p, "message", "Copilot needs you"));
                    vec![out]
                } else {
                    Vec::new()
                }
            }
            _ => Vec::new(),
        }
    }
}
