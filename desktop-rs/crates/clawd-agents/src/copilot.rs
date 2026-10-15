//! Copilot CLI: flat hooks in `$COPILOT_HOME|~/.copilot/hooks/clawd.json`.

use std::path::PathBuf;

use clawd_contracts::{AgentError, Platform};
use serde_json::Value;

use crate::base::{Agent, AgentCtx};

/// The `copilot` adapter (`agents/copilot.py`).
pub struct Copilot {
    ctx: AgentCtx,
}

impl Copilot {
    pub fn new(ctx: AgentCtx) -> Self {
        Self { ctx }
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
