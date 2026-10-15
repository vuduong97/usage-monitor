//! Codex CLI: command hooks in `$CODEX_HOME|~/.codex/hooks.json` (Claude-like payloads,
//! `async` hooks so they never hold up a tool call).

use std::path::PathBuf;

use clawd_contracts::{AgentError, Platform};
use serde_json::Value;

use crate::base::{Agent, AgentCtx};

/// The `codex` adapter (`agents/codex.py`).
pub struct Codex {
    ctx: AgentCtx,
}

impl Codex {
    pub fn new(ctx: AgentCtx) -> Self {
        Self { ctx }
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
        "Mở Codex, gõ /hooks và chọn tin cậy các hook của Clawd (một lần)."
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
