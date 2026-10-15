//! Gemini CLI: hooks in `~/.gemini/settings.json` (groups without a matcher).

use std::path::PathBuf;

use clawd_contracts::{AgentError, Platform};
use serde_json::Value;

use crate::base::{Agent, AgentCtx};

/// The `gemini` adapter (`agents/gemini.py`).
pub struct Gemini {
    ctx: AgentCtx,
}

impl Gemini {
    pub fn new(ctx: AgentCtx) -> Self {
        Self { ctx }
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
        "Khởi động lại Gemini CLI sau khi nối."
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
