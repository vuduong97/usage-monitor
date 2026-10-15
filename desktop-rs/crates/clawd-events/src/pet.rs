//! Clawd's moods (`events.py:555-728`): quiet hours, away dozing, low-usage worry, yawning,
//! turn milestones, the idle picks between usage screens.

use std::path::PathBuf;

use clawd_contracts::{PetConfig, UsagePayload, View};

/// `events.Pet`.
pub struct Pet {
    /// The user is away (HID idle past `away_after`); shown in `GET /state` as `away`.
    pub away: bool,
    /// Every clock has the newer idle-pick animations (`DESK_NEW_MIN`).
    pub desk_new: bool,
    /// Turns counted today; shown in `GET /state` as `turns`.
    pub turns: u32,
}

impl Pet {
    /// `Pet(cfg, store)`: `store` is `pet.json` (turns + greeting per day) or `None`.
    pub fn new(cfg: PetConfig, store: Option<PathBuf>) -> Self {
        todo!("phase 04")
    }

    /// `Pet.push(anim, text, secs)`: queue a moment.
    pub fn push(&mut self, anim: &str, text: &str, secs: f64) {
        todo!("phase 04")
    }

    /// `Pet.observe(now, idle_s, usage)`: feed it the HID idle seconds (`None` = unknown) and the
    /// latest usage payload.
    pub fn observe(&mut self, now: f64, idle_s: Option<f64>, usage: Option<&UsagePayload>) {
        todo!("phase 04")
    }

    /// `Pet.on_turn(now)`: a turn started (counts it, queues a milestone moment).
    pub fn on_turn(&mut self, now: f64) {
        todo!("phase 04")
    }

    /// `Pet.moment(now)`: the queued moment playing now (layer `pet`).
    pub fn moment(&mut self, now: f64) -> Option<View> {
        todo!("phase 04")
    }

    /// `Pet.hold_off(now)`: something else is on screen: drop the idle pick, start a new gap.
    pub fn hold_off(&mut self, now: f64) {
        todo!("phase 04")
    }

    /// `Pet.pick_ends()`: when the idle pick on screen ends.
    pub fn pick_ends(&self) -> Option<f64> {
        todo!("phase 04")
    }

    /// `Pet.continuous(now)`: the idle pick due now (layer `idle`), or `None` (usage screens).
    pub fn continuous(&mut self, now: f64) -> Option<View> {
        todo!("phase 04")
    }

    /// `Pet.dim(now)`: brightness cap in quiet hours, `-1` for none.
    pub fn dim(&self, now: f64) -> i64 {
        todo!("phase 04")
    }
}
