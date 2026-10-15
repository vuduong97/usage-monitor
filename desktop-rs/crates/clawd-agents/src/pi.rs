//! pi: an extension file (`clawd-home.ts`) in `$PI_CODING_AGENT_DIR|~/.pi/agent/extensions`.
//! The extension shapes its events like the tracker's own, so `normalize` only validates
//! them (`json_edit::prepared`).

use std::path::PathBuf;

use clawd_contracts::{AgentError, Platform};
use serde_json::Value;

use crate::base::{Agent, AgentCtx};

/// The `pi` adapter (`agents/pi.py`).
pub struct Pi {
    ctx: AgentCtx,
}

impl Pi {
    pub fn new(ctx: AgentCtx) -> Self {
        Self { ctx }
    }

    /// `PluginAgent.plugin_dir()`: where the plugin file goes.
    pub fn plugin_dir(&self) -> PathBuf {
        todo!("phase 05")
    }
}

impl Agent for Pi {
    fn id(&self) -> &'static str {
        "pi"
    }
    fn name(&self) -> &'static str {
        "pi"
    }
    fn signals(&self) -> &'static [&'static str] {
        &["think", "tool", "subagent", "error", "attention"]
    }
    fn tested(&self) -> &'static [Platform] {
        &[Platform::Darwin]
    }
    fn note(&self) -> &'static str {
        "Khởi động lại pi (hoặc gõ /reload) sau khi nối."
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
