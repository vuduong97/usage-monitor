//! Claude Code: command hooks in `settings.json` of every configured Claude config dir.
//! The edit itself lives in [`crate::hooks`] (older versions installed it from there and
//! `Clawd hooks on|off` still does); this adapter puts it behind the common face.
//! Payloads map through `clawd_events::claude_event`.

use std::path::PathBuf;

use clawd_contracts::{AgentError, Platform};
use serde_json::Value;

use crate::base::{Agent, AgentCtx};

/// The `claude` adapter (`agents/claude.py`).
pub struct Claude {
    ctx: AgentCtx,
}

impl Claude {
    pub fn new(ctx: AgentCtx) -> Self {
        Self { ctx }
    }
}

impl Agent for Claude {
    fn id(&self) -> &'static str {
        "claude"
    }
    fn name(&self) -> &'static str {
        "Claude Code"
    }
    fn signals(&self) -> &'static [&'static str] {
        &["think", "tool", "subagent", "error", "attention"]
    }
    fn tested(&self) -> &'static [Platform] {
        &[Platform::Darwin, Platform::Win32]
    }
    fn note(&self) -> &'static str {
        ""
    }
    fn ctx(&self) -> &AgentCtx {
        &self.ctx
    }

    fn config_dir(&self) -> PathBuf {
        todo!("phase 05")
    }
    fn is_installed(&self) -> Result<bool, AgentError> {
        todo!("phase 05")
    }
    fn install(&self) -> Result<String, AgentError> {
        todo!("phase 05")
    }
    fn uninstall(&self) -> Result<String, AgentError> {
        todo!("phase 05")
    }
    fn normalize(&self, p: &Value, hint: &str) -> Vec<Value> {
        todo!("phase 05")
    }
}
