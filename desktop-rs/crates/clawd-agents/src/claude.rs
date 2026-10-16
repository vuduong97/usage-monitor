//! Claude Code: command hooks in `settings.json` of every configured Claude config dir.
//! The edit itself lives in [`crate::hooks`] (older versions installed it from there and
//! `Clawd hooks on|off` still does); this adapter puts it behind the common face.
//! Payloads map through `clawd_events::claude_event`.

use std::path::PathBuf;

use clawd_contracts::{AgentError, Platform};
use serde_json::Value;

use crate::base::{Agent, AgentCtx};
use crate::hooks;

/// The `claude` adapter (`agents/claude.py`).
pub struct Claude {
    ctx: AgentCtx,
}

impl Claude {
    pub fn new(ctx: AgentCtx) -> Self {
        Self { ctx }
    }

    /// `Claude.dirs()`: the config dirs behind [`hooks::settings_files`].
    fn dirs(&self) -> Vec<PathBuf> {
        hooks::settings_files(&self.ctx.paths)
            .iter()
            .filter_map(|f| f.parent().map(PathBuf::from))
            .collect()
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
        let dirs = self.dirs();
        dirs.iter()
            .find(|d| d.is_dir())
            .or_else(|| dirs.first())
            .cloned()
            .unwrap_or_else(|| self.ctx.paths.home().join(".claude"))
    }
    fn found(&self) -> bool {
        self.dirs().iter().any(|d| d.is_dir())
    }
    fn where_(&self) -> String {
        let shown: Vec<String> = self
            .dirs()
            .iter()
            .filter(|d| d.is_dir())
            .map(|d| d.display().to_string())
            .collect();
        if shown.is_empty() {
            self.config_dir().display().to_string()
        } else {
            shown.join(", ")
        }
    }
    fn is_installed(&self) -> Result<bool, AgentError> {
        Ok(hooks::is_installed(&self.ctx.paths, None))
    }
    fn install(&self) -> Result<String, AgentError> {
        let ctx = &self.ctx;
        hooks::apply(&ctx.paths, &ctx.env, true, None, ctx.platform, None);
        Ok(if hooks::is_installed(&ctx.paths, None) {
            "Đã nối.".to_owned()
        } else {
            "Không ghi được settings.json của Claude Code.".to_owned()
        })
    }
    fn uninstall(&self) -> Result<String, AgentError> {
        let ctx = &self.ctx;
        hooks::apply(&ctx.paths, &ctx.env, false, None, ctx.platform, None);
        Ok("Đã ngắt.".to_owned())
    }
    fn normalize(&self, p: &Value, _hint: &str) -> Vec<Value> {
        clawd_events::claude_event(p).into_iter().collect()
    }
}
