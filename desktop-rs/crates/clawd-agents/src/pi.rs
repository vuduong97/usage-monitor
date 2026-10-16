//! pi: the `clawd-home.ts` extension in pi's extensions directory
//! (`$PI_CODING_AGENT_DIR|~/.pi/agent/extensions`) (`agents/pi.py`). The extension shapes its
//! events like the tracker's own (`integrations/pi`), so `normalize` only validates them.

use std::path::PathBuf;

use clawd_contracts::{AgentError, Platform};
use serde_json::Value;

use crate::base::{Agent, AgentCtx};
use crate::json_edit::prepared;
use crate::plugin;

/// A line only our extension has.
pub const MARK: &str = "pi extension: report agent lifecycle to the clawd-home daemon";
const FILE_NAME: &str = "clawd-home.ts";
const BUNDLED: &str = "pi/clawd-home.ts";

/// The `pi` adapter.
pub struct Pi {
    ctx: AgentCtx,
}

impl Pi {
    pub fn new(ctx: AgentCtx) -> Self {
        Self { ctx }
    }

    pub fn plugin_dir(&self) -> PathBuf {
        self.config_dir().join("extensions")
    }

    fn target(&self) -> PathBuf {
        self.plugin_dir().join(FILE_NAME)
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
        self.env_dir(
            "PI_CODING_AGENT_DIR",
            self.ctx.paths.home().join(".pi").join("agent"),
        )
    }
    fn found(&self) -> bool {
        self.config_dir().is_dir() || (self.ctx.which)("pi").is_some()
    }
    fn is_installed(&self) -> Result<bool, AgentError> {
        Ok(plugin::is_installed(&self.target(), MARK))
    }
    fn install(&self) -> Result<String, AgentError> {
        plugin::install(&self.target(), MARK, BUNDLED)
    }
    fn uninstall(&self) -> Result<String, AgentError> {
        plugin::uninstall(&self.target(), MARK)
    }
    fn normalize(&self, p: &Value, _hint: &str) -> Vec<Value> {
        prepared("pi", p)
    }
    fn refresh(&self) -> bool {
        plugin::refresh(&self.target(), MARK, BUNDLED)
    }
}
