//! The registry of adapters (`agents/__init__.py`). Adding an agent = one module with an
//! [`Agent`] implementation, listed in [`IDS`] and [`Registry::new`].

use crate::base::{Agent, AgentCtx, AgentInfo};
use crate::claude::Claude;
use crate::codex::Codex;
use crate::copilot::Copilot;
use crate::cursor::Cursor;
use crate::gemini::Gemini;
use crate::opencode::OpenCode;
use crate::pi::Pi;

/// `agents.IDS`: ids in display order (`ORDER`: Claude, Pi, Codex, Gemini, Cursor, OpenCode,
/// Copilot).
pub const IDS: [&str; 7] = [
    "claude", "pi", "codex", "gemini", "cursor", "opencode", "copilot",
];

/// `agents.registry(...)`: id -> adapter, in display order. Build it with
/// [`AgentCtx::system`] for the real machine or an injected ctx for tests.
pub struct Registry {
    agents: Vec<Box<dyn Agent>>,
}

impl Registry {
    /// One adapter per id of [`IDS`], all sharing `ctx`.
    pub fn new(ctx: AgentCtx) -> Self {
        let agents: Vec<Box<dyn Agent>> = vec![
            Box::new(Claude::new(ctx.clone())),
            Box::new(Pi::new(ctx.clone())),
            Box::new(Codex::new(ctx.clone())),
            Box::new(Gemini::new(ctx.clone())),
            Box::new(Cursor::new(ctx.clone())),
            Box::new(OpenCode::new(ctx.clone())),
            Box::new(Copilot::new(ctx)),
        ];
        Self { agents }
    }

    /// The adapter with this id.
    pub fn get(&self, id: &str) -> Option<&dyn Agent> {
        self.agents
            .iter()
            .find(|a| a.id() == id)
            .map(|a| a.as_ref())
    }

    /// Adapters in display order.
    pub fn iter(&self) -> std::slice::Iter<'_, Box<dyn Agent>> {
        self.agents.iter()
    }
}

/// `agents.infos(reg)`: one [`AgentInfo`] per adapter.
pub fn infos(reg: &Registry) -> Vec<AgentInfo> {
    reg.iter().map(|a| a.info()).collect()
}

/// `agents.found_ids(reg)`: ids of the agents present on this machine.
pub fn found_ids(reg: &Registry) -> Vec<String> {
    reg.iter()
        .filter(|a| a.found())
        .map(|a| a.id().to_owned())
        .collect()
}

/// `agents.uninstall_all(reg)`: remove every hook / plugin Clawd added; one `"<id>: <message>"`
/// per agent (errors become the message).
pub fn uninstall_all(reg: &Registry) -> Vec<String> {
    reg.iter()
        .map(|a| match a.uninstall() {
            Ok(msg) => format!("{}: {msg}", a.id()),
            Err(e) => format!("{}: {e}", a.id()),
        })
        .collect()
}

/// `agents.refresh_plugins(reg)`: replace plugin copies older than the app's (pi, opencode);
/// the ids that were updated.
pub fn refresh_plugins(reg: &Registry) -> Vec<String> {
    reg.iter()
        .filter(|a| a.refresh())
        .map(|a| a.id().to_owned())
        .collect()
}
