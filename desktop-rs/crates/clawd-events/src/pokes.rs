//! Pokes from the `clawd` CLI (`POST /poke`) and the focus timer (`events.py:766-835`).

use clawd_contracts::View;
use serde_json::Value;

/// `events.Pokes`.
pub struct Pokes {
    pub focus_until: f64,
}

impl Pokes {
    pub fn new() -> Self {
        todo!("phase 04")
    }

    /// `Pokes.handle(cmd, now)`: run one CLI command (`play`, `focus`, `poke`, ...); returns the
    /// JSON answer (`{"ok": true}` or `{"ok": false, "error": ...}`).
    pub fn handle(&mut self, cmd: &Value, now: f64) -> Value {
        todo!("phase 04")
    }

    /// `Pokes.moment(now)`: the poke moment playing now (layer `poke`), or `None`.
    pub fn moment(&mut self, now: f64) -> Option<View> {
        todo!("phase 04")
    }

    /// `Pokes.focus_left(now)`: whole seconds left on the focus timer, 0 when none.
    pub fn focus_left(&self, now: f64) -> i64 {
        todo!("phase 04")
    }
}
