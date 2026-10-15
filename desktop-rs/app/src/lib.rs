//! `clawd-app`: the Tauri shell around the engine (phases 12 and 13 fill it in).
//!
//! Phase 02 provides only the frozen seams: [`state`] (shared state and the settings->shell
//! channel), [`commands`] (the single invoke handler and its allowlist) and `build.rs`.
//! `engine` and `popover` are stubs owned by phase 12; `commands/<group>.rs` are stubs owned by
//! phase 13.
#![allow(unused_variables, dead_code)] // stub modules: phases 12/13 remove this

pub mod commands;
pub mod engine;
pub mod popover;
pub mod state;
