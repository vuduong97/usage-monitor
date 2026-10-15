//! Cursor: flat hooks in `~/.cursor/hooks.json`.

use std::path::PathBuf;

use clawd_contracts::{AgentError, Platform};
use serde_json::Value;

use crate::base::{Agent, AgentCtx};

/// The `cursor` adapter (`agents/cursor.py`).
pub struct Cursor {
    ctx: AgentCtx,
}

impl Cursor {
    pub fn new(ctx: AgentCtx) -> Self {
        Self { ctx }
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
