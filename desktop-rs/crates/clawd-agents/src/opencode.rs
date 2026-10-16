//! opencode: a plugin (`clawd.js`) in `$XDG_CONFIG_HOME|~/.config/opencode/plugins`
//! (`agents/opencode.py`). opencode loads every `.js`/`.ts` file there at startup
//! (<https://opencode.ai/docs/plugins>). The plugin shapes its events like the tracker's own
//! (`json_edit::prepared`), so `normalize` only validates them.

use std::path::PathBuf;

use clawd_contracts::{AgentError, Platform};
use serde_json::Value;

use crate::base::{expand_user, Agent, AgentCtx};
use crate::json_edit::prepared;
use crate::plugin;

/// A line only our plugin has.
pub const MARK: &str = "opencode plugin: report agent lifecycle to the Clawd desktop app";
const FILE_NAME: &str = "clawd.js";
const BUNDLED: &str = "opencode/clawd.js";

/// The `opencode` adapter.
pub struct OpenCode {
    ctx: AgentCtx,
}

impl OpenCode {
    pub fn new(ctx: AgentCtx) -> Self {
        Self { ctx }
    }

    pub fn plugin_dir(&self) -> PathBuf {
        self.config_dir().join("plugins")
    }

    fn target(&self) -> PathBuf {
        self.plugin_dir().join(FILE_NAME)
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
        let base = match self
            .ctx
            .env
            .get("XDG_CONFIG_HOME")
            .filter(|v| !v.is_empty())
        {
            Some(xdg) => expand_user(&xdg, self.ctx.paths.home()),
            None => self.ctx.paths.home().join(".config"),
        };
        base.join("opencode")
    }
    fn found(&self) -> bool {
        self.config_dir().is_dir() || (self.ctx.which)("opencode").is_some()
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
        prepared("opencode", p)
    }
    fn refresh(&self) -> bool {
        plugin::refresh(&self.target(), MARK, BUNDLED)
    }
}
