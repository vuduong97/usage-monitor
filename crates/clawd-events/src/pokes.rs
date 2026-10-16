//! Pokes from the `clawd` CLI (`POST /poke`) and the focus timer (`events.py:766-835`).

use clawd_contracts::View;
use serde_json::{json, Value};

use crate::pet::play;
use crate::pyval::{py_repr, py_str, text_or_empty, to_float, truthy};
use crate::tracker::Moment;
use crate::vocab::{ascii_text, TEXT_MAX};

/// `events.POKE_SECS`: reactions.clickLeft / clickRight.
pub const POKE_SECS: f64 = 2.5;
/// `events.ANNOYED_SECS`: reactions.annoyed.
pub const ANNOYED_SECS: f64 = 3.5;
/// `events.ANNOYED_POKES`, `events.ANNOYED_WINDOW`: this many pokes within the window: annoyed.
pub const ANNOYED_POKES: usize = 3;
pub const ANNOYED_WINDOW: f64 = 5.0;

/// `events.Pokes`.
pub struct Pokes {
    pub focus_until: f64,
    moments: Vec<Moment>,
    poked: Vec<f64>,
    left: bool,
}

impl Default for Pokes {
    fn default() -> Self {
        Self::new()
    }
}

fn failure(error: &str) -> Value {
    json!({"ok": false, "error": error})
}

/// `float(cmd.get("secs") or default)` clamped to 1..=300; 6 when it is not a number.
fn seconds(value: Option<&Value>, default: f64) -> f64 {
    let chosen = if truthy(value) {
        value.and_then(to_float)
    } else {
        Some(default)
    };
    match chosen {
        None => 6.0,
        Some(f) if f.is_nan() => 1.0, // Python: max(1.0, min(nan, 300.0)) is 1.0
        Some(f) => f.clamp(1.0, 300.0),
    }
}

impl Pokes {
    pub fn new() -> Self {
        Self {
            focus_until: 0.0,
            moments: Vec::new(),
            poked: Vec::new(),
            left: false,
        }
    }

    /// `Pokes.handle(cmd, now)`: run one CLI command (`play`, `focus`, `poke`, ...); returns the
    /// JSON answer (`{"ok": true}` or `{"ok": false, "error": ...}`).
    pub fn handle(&mut self, cmd: &Value, now: f64) -> Value {
        let op = cmd.get("cmd");
        let text = || text_or_empty(cmd.get("text"));
        match op.and_then(Value::as_str) {
            Some("play") => {
                let anim = ascii_text(&text_or_empty(cmd.get("anim")), 15);
                if anim.is_empty() {
                    return failure("anim required");
                }
                self.one(&anim, &text(), seconds(cmd.get("secs"), 6.0), now);
            }
            Some("wave") => {
                let text = if truthy(cmd.get("text")) {
                    text()
                } else {
                    "Hi!".to_owned()
                };
                self.one("happy", &text, seconds(cmd.get("secs"), 4.0), now);
            }
            Some("say") => {
                let text = text();
                if text.trim().is_empty() {
                    return failure("text required");
                }
                let anim = if truthy(cmd.get("anim")) {
                    py_str(&cmd["anim"])
                } else {
                    "idle".to_owned()
                };
                self.one(&anim, &text, seconds(cmd.get("secs"), 10.0), now);
            }
            Some("focus") => {
                let mins = cmd.get("mins");
                let stop = match mins {
                    None | Some(Value::Null) | Some(Value::Bool(false)) => true,
                    Some(Value::Number(n)) => n.as_f64() == Some(0.0),
                    Some(Value::String(s)) => matches!(s.as_str(), "0" | "stop" | "off"),
                    Some(_) => false,
                };
                if stop {
                    self.focus_until = 0.0;
                } else {
                    let Some(m) = mins.and_then(to_float) else {
                        return failure("mins must be a number");
                    };
                    if !(m > 0.0 && m <= 600.0) {
                        return failure("mins must be 1..600");
                    }
                    self.focus_until = now + m * 60.0;
                }
            }
            Some("poke") => {
                // the desktop pet's clicks
                self.poked.retain(|t| now - t < ANNOYED_WINDOW);
                self.poked.push(now);
                if self.poked.len() >= ANNOYED_POKES {
                    self.poked.clear();
                    self.one("annoyed", &text(), ANNOYED_SECS, now);
                } else {
                    self.left = !self.left;
                    let anim = if self.left {
                        "react left"
                    } else {
                        "react right"
                    };
                    self.one(anim, &text(), POKE_SECS, now);
                }
            }
            Some("clear") => self.moments.clear(),
            _ => {
                let shown = op.map_or_else(|| "None".to_owned(), py_repr);
                return failure(&format!("unknown cmd {shown}"));
            }
        }
        json!({"ok": true})
    }

    fn one(&mut self, anim: &str, text: &str, secs: f64, now: f64) {
        self.moments = vec![Moment {
            anim: ascii_text(anim, 15),
            text: ascii_text(text, TEXT_MAX),
            secs,
            start: now,
            seq: Vec::new(),
            urgent: true,
        }];
    }

    /// `Pokes.moment(now)`: the poke moment playing now (layer `poke`), or `None`.
    pub fn moment(&mut self, now: f64) -> Option<View> {
        if self.focus_until != 0.0 && now >= self.focus_until {
            self.focus_until = 0.0;
            self.moments = vec![Moment {
                anim: "happy".into(),
                text: "Focus done!".into(),
                secs: 6.0,
                start: now,
                seq: Vec::new(),
                urgent: true,
            }];
        }
        play(&mut self.moments, now, "poke")
    }

    /// `Pokes.focus_left(now)`: whole seconds left on the focus timer, 0 when none.
    pub fn focus_left(&self, now: f64) -> i64 {
        if self.focus_until > now {
            (self.focus_until - now + 0.999) as i64
        } else {
            0
        }
    }
}
