//! `clawd-app`: the Tauri shell around the engine (phases 12 and 13).
//!
//! Phase 12 (this phase) owns `lifecycle` (start-up, the tray shell, quitting), `engine`, `tray`
//! (status text, menu, icons), `dialogs` (native input box and alert), `popover` (macOS panel),
//! `windows` (the lazy settings window) and `cli`. `state` and `commands` are the frozen seams of
//! phase 02; `commands/<group>.rs` are filled in by phase 13.

pub mod cli;
// Phase 13's command stubs: phase 13 removes this allow.
#[allow(unused_variables, dead_code)]
pub mod commands;
pub mod dialogs;
pub mod engine;
pub mod lifecycle;
pub mod popover;
pub mod state;
pub mod tray;
pub mod windows;
