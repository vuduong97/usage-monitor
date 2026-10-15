//! `firstrun.py`: the setup wizard's bookkeeping, kept in `desktop.json`.
//!
//! * `setup_done`: wizard steps finished or skipped ([`STEPS`]). A fresh install has none, so
//!   it sees every step. 0.1.x had only `first_run_done` (its users already found the clock),
//!   so an upgrade sees only the new steps.
//! * `setup_shown`: the app version that last opened the wizard by itself, so it opens once per
//!   version while steps are left.
//! * `agents_seen`: agents already offered to the user; a newly installed agent is offered
//!   once, never again whatever the answer.

use std::collections::BTreeSet;
use std::io;

use clawd_contracts::{AppPaths, Prefs};

/// `firstrun.STEPS`.
pub const STEPS: [&str; 3] = ["clock", "agents", "usage"];

/// `firstrun.migrate(prefs)`: `prefs` with `setup_done` (from `first_run_done` when that is all
/// there is). Returns a copy; never changes other keys.
pub fn migrate(prefs: &Prefs) -> Prefs {
    todo!("phase 03")
}

/// `firstrun.load()`: prefs after persisting the migration (one `update_prefs` when needed).
pub fn load(paths: &AppPaths) -> io::Result<Prefs> {
    todo!("phase 03")
}

/// `firstrun.pending(prefs)`: the steps not done yet, in [`STEPS`] order. `None` = read the file.
pub fn pending(paths: &AppPaths, prefs: Option<&Prefs>) -> Vec<String> {
    todo!("phase 03")
}

/// `firstrun.mark_done(steps)`: record steps (unknown ones ignored); returns what is pending.
pub fn mark_done(paths: &AppPaths, steps: &[&str]) -> io::Result<Vec<String>> {
    todo!("phase 03")
}

/// `firstrun.should_open(prefs)`: steps left and not yet shown by this version
/// ([`clawd_contracts::APP_VERSION`]).
pub fn should_open(paths: &AppPaths, prefs: Option<&Prefs>) -> bool {
    todo!("phase 03")
}

/// `firstrun.mark_shown()`: `setup_shown = APP_VERSION`.
pub fn mark_shown(paths: &AppPaths) -> io::Result<()> {
    todo!("phase 03")
}

/// `firstrun.seen()`: agents already offered.
pub fn seen(paths: &AppPaths) -> BTreeSet<String> {
    todo!("phase 03")
}

/// `firstrun.mark_seen(ids)`: add to `agents_seen` (sorted, deduplicated; no write when nothing
/// new).
pub fn mark_seen(paths: &AppPaths, ids: &[&str]) -> io::Result<()> {
    todo!("phase 03")
}

/// `firstrun.new_agents(found, installed)`: agents on this machine never offered before and
/// not connected yet.
pub fn new_agents(paths: &AppPaths, found: &[String], installed: &[String]) -> Vec<String> {
    todo!("phase 03")
}
