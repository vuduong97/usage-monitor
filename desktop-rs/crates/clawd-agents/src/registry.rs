//! The registry of adapters (`agents/__init__.py`). Adding an agent = one module with an
//! [`Agent`] implementation, listed in [`IDS`] and [`Registry::new`].

use crate::base::{Agent, AgentCtx, AgentInfo};

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
        todo!("phase 05")
    }

    /// `registry[id]`.
    pub fn get(&self, id: &str) -> Option<&dyn Agent> {
        todo!("phase 05")
    }

    /// Adapters in display order.
    pub fn iter(&self) -> std::slice::Iter<'_, Box<dyn Agent>> {
        self.agents.iter()
    }
}

/// `agents.infos(reg)`: `Agent.info()` of each, in order (the settings window's list).
pub fn infos(reg: &Registry) -> Vec<AgentInfo> {
    todo!("phase 05")
}

/// `agents.found_ids(reg)`: ids of the agents present on this machine.
pub fn found_ids(reg: &Registry) -> Vec<String> {
    todo!("phase 05")
}

/// `agents.uninstall_all(reg)`: remove every hook/plugin Clawd added (the uninstaller's
/// `Clawd.exe cleanup`, "Ngắt kết nối tất cả"); one `"<id>: <message or error>"` line each,
/// errors included, never aborting on the first.
pub fn uninstall_all(reg: &Registry) -> Vec<String> {
    todo!("phase 05")
}

/// `agents.refresh_plugins(reg)`: replace plugin copies older than the app's (pi, opencode);
/// the ids that were updated.
pub fn refresh_plugins(reg: &Registry) -> Vec<String> {
    todo!("phase 05")
}
