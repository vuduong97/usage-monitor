//! opencode: a plugin (`clawd.js`) in `$XDG_CONFIG_HOME|~/.config/opencode/plugins`.
//! The plugin shapes its events like the tracker's own (`json_edit::prepared`).

use std::path::PathBuf;

use clawd_contracts::{AgentError, Platform};
use serde_json::Value;

use crate::base::{Agent, AgentCtx};

/// The `opencode` adapter (`agents/opencode.py`).
pub struct OpenCode {
    ctx: AgentCtx,
}

impl OpenCode {
    pub fn new(ctx: AgentCtx) -> Self {
        Self { ctx }
    }

    /// `PluginAgent.plugin_dir()`: where the plugin file goes.
    pub fn plugin_dir(&self) -> PathBuf {
        todo!("phase 05")
    }
}

impl Agent for OpenCode {
    fn id(&self) -> &'static str {
        "opencode"
    }
    fn name(&self) -> &'static str {
        "opencode"
    }
    fn signals(&self) -> &'static [&'static str] {
        &["think", "tool", "subagent", "error", "attention"]
    }
    fn tested(&self) -> &'static [Platform] {
        &[]
    }
    fn note(&self) -> &'static str {
        "Khởi động lại opencode sau khi nối."
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

    fn refresh(&self) -> bool {
        todo!("phase 05")
    }
}
