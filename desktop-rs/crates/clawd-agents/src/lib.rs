//! `clawd-agents`: the coding agents Clawd listens to, one adapter each .
//!
//! Python origin: `agents/*.py` and `hooks.py` (see `desktop-rs/PORT-MAP.md`).
//!
//! DAG: depends on `clawd-events` (the shared vocabulary: `EVENTS`, `CLAUDE_HOOK_EVENTS`,
//! `claude_event`, `pi_event`, `is_question_tool`, `is_debug_prompt`, `ascii_text`,
//! `read_event_port`), never the reverse. [`normalize`] has the shape of
//! `clawd_events::NormalizeFn`; the app hands it to the event server.
//!
//! An adapter connects one agent to `http://127.0.0.1:<event port>/hook/<agent id>`: `install` /
//! `uninstall` add or remove only our own entries in the agent's config (merged, backed up,
//! written atomically, never touching a file that does not parse), `normalize` turns one hook
//! payload into tracker events. No test ever writes the user's agent config: every path derives
//! from the injected [`base::AgentCtx`].

pub mod assets;
pub mod base;
pub mod claude;
pub mod codex;
pub mod copilot;
pub mod cursor;
pub mod gemini;
pub mod hooks;
pub mod json_edit;
pub mod normalize;
pub mod opencode;
pub mod pi;
pub mod plugin;
pub mod registry;

pub use base::{Agent, AgentCtx, AgentInfo, HTTP_MARK, SCRIPT_MARK};
pub use clawd_contracts::AgentError;
pub use normalize::normalize;
pub use registry::{found_ids, infos, refresh_plugins, uninstall_all, Registry, IDS};

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    /// The DAG contract: `normalize` fits `clawd_events::NormalizeFn` (red team #6).
    #[test]
    fn normalize_fits_the_event_servers_alias() {
        let _f: clawd_events::NormalizeFn = Arc::new(super::normalize);
    }
}
