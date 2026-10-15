//! `clawd-events`: agent events, pet moods and pokes, decided into the one state the clock's
//! desk view should show, plus the loopback server hooks post to (phase 04 implements it).
//!
//! STUB (phase 02): real signatures and docs, `todo!()` bodies. Python origin: `events.py`
//! (see `desktop-rs/PORT-MAP.md`; symbols keep their Python module position, red team #6).
//! This crate does **not** depend on `clawd-agents`: the server takes the normalizer as a
//! [`NormalizeFn`] and the app passes `clawd_agents::normalize` (breaks the Python
//! `events <-> agents` cycle).
//!
//! Layers, highest first: poke moment > agent activity > focus timer > pet moment > idle pick >
//! nothing (the clock shows its usage screens).
#![allow(
    unused_variables,
    dead_code,
    clippy::new_without_default,
    clippy::too_many_arguments,
    clippy::type_complexity
)] // stub crate: the owning phase removes this when it implements the bodies

pub mod brain;
pub mod config;
pub mod idle;
pub mod pet;
pub mod pokes;
pub mod probe;
pub mod pusher;
pub mod server;
pub mod source_events;
pub mod tracker;
pub mod vocab;

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
