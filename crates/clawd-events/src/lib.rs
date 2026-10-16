//! `clawd-events`: agent events, pet moods and pokes, decided into the one state the clock's
//! desk view should show, plus the loopback server hooks post to (phase 04 implements it).
//!
//! Python origin: `events.py`
//! (see `plans/261015-0726-rust-tauri-port/PORT-MAP.md`; symbols keep their Python module position, red team #6).
//! This crate does **not** depend on `clawd-agents`: the server takes the normalizer as a
//! [`NormalizeFn`] and the app passes `clawd_agents::normalize` (breaks the Python
//! `events <-> agents` cycle).
//!
//! Layers, highest first: poke moment > agent activity > focus timer > pet moment > idle pick >
//! nothing (the clock shows its usage screens).

pub mod brain;
pub mod config;
pub mod idle;
pub mod localtime;
pub mod pet;
pub mod pokes;
pub mod probe;
pub mod pusher;
pub mod rng;
pub mod server;
pub mod source_events;
pub mod tracker;
pub mod vocab;

mod httpc;
pub mod pyjson;
mod pyval;

pub use brain::{Brain, SharedBrain};
pub use config::{read_event_port, read_pet_config};
pub use idle::read_hid_idle;
pub use pet::Pet;
pub use pokes::Pokes;
pub use probe::daemon_running;
pub use pusher::{heap_url, state_url, wire, StatePusher};
pub use server::{EventServer, NormalizeFn};
pub use source_events::{claude_event, pi_event, tool_summary};
pub use tracker::{Moment, Sess, Tracker};
pub use vocab::{
    ascii_text, is_debug_prompt, is_debug_skill_load, is_question_tool, is_test_command,
    tool_action, CLAUDE_HOOK_EVENTS, EVENTS,
};

/// `events.View`, `events.PetConfig` live in `clawd-contracts` (wire types) and keep their
/// Python names here.
pub use clawd_contracts::{DesiredState, PetConfig, View, WireState};

/// Resolves once the stop signal is `true` (or its sender is gone). A helper, not an inline
/// `wait_for`, so the watch guard is dropped before the caller's next `.await` and the future stays `Send`.
pub(crate) async fn stop_requested(stop: &mut tokio::sync::watch::Receiver<bool>) {
    let _ = stop.wait_for(|stopped| *stopped).await;
}
