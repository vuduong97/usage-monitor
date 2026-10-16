//! Clawd's moods (`events.py:555-728`): quiet hours, away dozing, low-usage worry, yawning,
//! turn milestones, the idle picks between usage screens.

use std::path::PathBuf;

use clawd_contracts::{PetConfig, UsagePayload, View};
use serde_json::Value;

use crate::localtime::{system_local_time, LocalTimeFn};
use crate::pyval::py_str;
use crate::rng::{Rng, SystemRng};
use crate::tracker::{make_view, Moment};
use crate::vocab::ascii_text;

/// `events.IDLE_GAP`: seconds of usage screens between two picks.
pub const IDLE_GAP: (f64, f64) = (180.0, 360.0);
/// `events.IDLE_PICKS`: what Clawd slips in between the usage screens, one pass each.
pub const IDLE_PICKS: [&[&str]; 7] = [
    &["idle"],
    &["look"],
    &["bubble"],
    &["coffee hand"],
    &["shy"],
    &["outlaw"],
    &["yawn", "sleeping", "wake"],
];
/// `events.IDLE_PICKS_NEW`: picks only a newer desk.pak has (31+ animations).
pub const IDLE_PICKS_NEW: [&str; 3] = ["coffee hand", "shy", "outlaw"];
/// `events.DESK_NEW_MIN`.
pub const DESK_NEW_MIN: i64 = 31;
/// `events.WAKE_SECS`.
pub const WAKE_SECS: f64 = 1.5;

/// `events.PASS_SECS`: one pass of each idle animation on the clock, seconds.
pub fn pass_secs(name: &str) -> f64 {
    match name {
        "idle" => 16.08,
        "look" => 10.05,
        "bubble" => 4.5,
        "coffee hand" => 4.4,
        "shy" => 3.21,
        "outlaw" => 16.0,
        "yawn" => 7.0,
        "sleeping" => 10.05,
        "wake" => 1.5,
        _ => panic!("no PASS_SECS entry for idle animation {name:?}"),
    }
}

/// `events._in_window`.
fn in_window(minute: u32, start: u32, end: u32) -> bool {
    if start == end {
        return false;
    }
    if start < end {
        start <= minute && minute < end
    } else {
        minute >= start || minute < end
    }
}

/// `events._play`: the first queued moment still running, dropping the finished ones.
pub(crate) fn play(queue: &mut Vec<Moment>, now: f64, layer: &str) -> Option<View> {
    while let Some(m) = queue.first_mut() {
        if m.start == 0.0 {
            m.start = now;
        }
        if now - m.start < m.secs {
            return Some(View {
                seq: m.seq.clone(),
                urgent: m.urgent,
                ..make_view(&m.anim, &m.text, layer)
            });
        }
        queue.remove(0);
    }
    None
}

/// `events.Pet`.
pub struct Pet {
    /// The user is away (HID idle past `away_after`); shown in `GET /state` as `away`.
    pub away: bool,
    /// Every clock has the newer idle-pick animations (`DESK_NEW_MIN`).
    pub desk_new: bool,
    /// Turns counted today; shown in `GET /state` as `turns`.
    pub turns: u32,
    /// The idle pick on screen.
    pub pick: Option<Moment>,
    /// When the next idle pick starts (0: not scheduled).
    pub next_pick: f64,
    cfg: PetConfig,
    store: Option<PathBuf>,
    localtime: LocalTimeFn,
    rng: Box<dyn Rng>,
    moments: Vec<Moment>,
    last_pick: Vec<String>,
    last_low: f64,
    last_yawn: f64,
    last_pct: Option<f64>,
    last_reset_min: Option<i64>,
    day: String,
    greeted: String,
}

impl Pet {
    /// `Pet(cfg, store)`: `store` is `pet.json` (turns + greeting per day) or `None`.
    pub fn new(cfg: PetConfig, store: Option<PathBuf>) -> Self {
        Self::with_parts(
            cfg,
            store,
            Box::new(system_local_time),
            Box::new(SystemRng::new()),
        )
    }

    /// `Pet(cfg, store, localtime, rng)`: tests pin the hour and the random draws.
    pub fn with_parts(
        cfg: PetConfig,
        store: Option<PathBuf>,
        localtime: LocalTimeFn,
        rng: Box<dyn Rng>,
    ) -> Self {
        let mut pet = Self {
            away: false,
            desk_new: false,
            turns: 0,
            pick: None,
            next_pick: 0.0,
            cfg,
            store,
            localtime,
            rng,
            moments: Vec::new(),
            last_pick: Vec::new(),
            last_low: 0.0,
            last_yawn: 0.0,
            last_pct: None,
            last_reset_min: None,
            day: String::new(),
            greeted: String::new(),
        };
        pet.load();
        pet
    }

    /// `Pet._load`: any problem leaves the counters empty.
    fn load(&mut self) {
        let Some(store) = &self.store else {
            return;
        };
        let Ok(text) = std::fs::read_to_string(store) else {
            return;
        };
        let Ok(Value::Object(d)) = serde_json::from_str::<Value>(&text) else {
            return;
        };
        let day = d.get("day").map_or_else(String::new, py_str);
        let turns = match d.get("turns") {
            None => Some(0),
            Some(Value::Number(n)) => n.as_i64().or_else(|| n.as_f64().map(|f| f as i64)),
            Some(Value::String(s)) => s.trim().parse::<i64>().ok(),
            Some(Value::Bool(b)) => Some(i64::from(*b)),
            Some(_) => None,
        };
        let Some(turns) = turns else {
            return;
        };
        self.day = day;
        self.turns = turns.max(0) as u32;
        self.greeted = d.get("greeted").map_or_else(String::new, py_str);
    }

    /// `Pet._save`: write-then-rename; failures are ignored (the counters are cosmetic).
    fn save(&self) {
        let Some(store) = &self.store else {
            return;
        };
        let quote = |s: &str| serde_json::to_string(s).unwrap_or_else(|_| "\"\"".to_owned());
        let text = format!(
            "{{\"day\": {}, \"turns\": {}, \"greeted\": {}}}",
            quote(&self.day),
            self.turns,
            quote(&self.greeted)
        );
        if let Some(parent) = store.parent() {
            if std::fs::create_dir_all(parent).is_err() {
                return;
            }
        }
        let tmp = store.with_extension("tmp");
        if std::fs::write(&tmp, text).is_ok() {
            let _ = std::fs::rename(&tmp, store);
        }
    }

    fn today(&self, now: f64) -> String {
        (self.localtime)(now).date()
    }

    fn minute(&self, now: f64) -> u32 {
        (self.localtime)(now).minute_of_day()
    }

    /// `Pet.push(anim, text, secs)`: queue a moment.
    pub fn push(&mut self, anim: &str, text: &str, secs: f64) {
        self.moments.push(Moment {
            anim: anim.to_owned(),
            text: ascii_text(text, crate::vocab::TEXT_MAX),
            secs,
            start: 0.0,
            seq: Vec::new(),
            urgent: false,
        });
    }

    /// `Pet.observe(now, idle_s, usage)`: feed it the HID idle seconds (`None` = unknown) and the
    /// latest usage payload.
    pub fn observe(&mut self, now: f64, idle_s: Option<f64>, usage: Option<&UsagePayload>) {
        if let Some(idle) = idle_s {
            let away = idle >= f64::from(self.cfg.away_after);
            if away && !self.away {
                self.away = true; // only remembered for the greeting
                self.moments.clear();
            } else if !away && self.away {
                self.away = false;
                let today = self.today(now);
                let minute = self.minute(now);
                let text = if (5 * 60..11 * 60).contains(&minute) && self.greeted != today {
                    self.greeted = today;
                    self.save();
                    "Good morning!"
                } else {
                    "Welcome back!"
                };
                self.moments = vec![Moment {
                    anim: "wake".into(),
                    text: text.into(),
                    secs: WAKE_SECS + 3.0,
                    start: 0.0,
                    seq: vec!["wake".into(), "happy".into()],
                    urgent: true,
                }];
            }
        }
        if self.away {
            return;
        }
        let cfg = self.cfg.clone();
        if in_window(self.minute(now), cfg.yawn_from, cfg.quiet_start)
            && now - self.last_yawn >= f64::from(cfg.yawn_every)
            && self.moments.is_empty()
        {
            self.last_yawn = now;
            self.push("yawn", "So sleepy...", 4.0);
        }
        let Some(usage) = usage else {
            return;
        };
        if matches!(usage.get("ok"), Some(Value::Bool(false)))
            || matches!(usage.get("acct"), Some(Value::String(a)) if a == "ent")
        {
            return;
        }
        let Some(pct) = number_or_bool(usage.get("s")) else {
            return;
        };
        let sr = match usage.get("sr") {
            Some(Value::Number(n)) => n.as_i64(),
            Some(Value::Bool(b)) => Some(i64::from(*b)),
            _ => None,
        };
        let (prev, prev_sr) = (self.last_pct, self.last_reset_min);
        let reset = prev.is_some_and(|prev| {
            (prev >= 20.0 && pct <= prev - 30.0)
                || matches!((sr, prev_sr), (Some(sr), Some(prev_sr)) if sr > prev_sr + 120 && pct < prev)
        });
        if reset {
            self.push("double jump", "Fresh quota!", 5.0);
        } else if pct >= f64::from(cfg.low_pct)
            && now - self.last_low >= f64::from(cfg.low_every)
            && self.moments.is_empty()
        {
            self.last_low = now;
            self.push("low battery", &format!("Session {}%", pct as i64), 6.0);
        }
        self.last_pct = Some(pct);
        self.last_reset_min = sr;
    }

    /// `Pet.on_turn(now)`: a turn started (counts it, queues a milestone moment).
    pub fn on_turn(&mut self, now: f64) {
        let today = self.today(now);
        if today != self.day {
            self.day = today;
            self.turns = 0;
        }
        self.turns += 1;
        self.save();
        if self.cfg.milestone != 0 && self.turns.is_multiple_of(self.cfg.milestone) {
            let text = format!("{} turns today!", self.turns);
            self.push("heart eyes", &text, 4.0);
        }
    }

    /// `pet.moments.clear()`: drop every queued moment.
    pub fn clear_moments(&mut self) {
        self.moments.clear();
    }

    /// `Pet.moment(now)`: the queued moment playing now (layer `pet`).
    pub fn moment(&mut self, now: f64) -> Option<View> {
        play(&mut self.moments, now, "pet")
    }

    /// `Pet.hold_off(now)`: something else is on screen: drop the idle pick, start a new gap.
    pub fn hold_off(&mut self, now: f64) {
        self.pick = None;
        self.next_pick = now + self.rng.uniform(IDLE_GAP.0, IDLE_GAP.1);
    }

    /// `Pet.pick_ends()`: when the idle pick on screen ends.
    pub fn pick_ends(&self) -> Option<f64> {
        self.pick.as_ref().map(|p| p.start + p.secs)
    }

    /// `Pet.continuous(now)`: the idle pick due now (layer `idle`), or `None` (usage screens).
    pub fn continuous(&mut self, now: f64) -> Option<View> {
        if self.pick.as_ref().is_some_and(|p| now - p.start >= p.secs) {
            self.hold_off(now);
        }
        if self.pick.is_none() {
            if self.next_pick == 0.0 {
                self.hold_off(now);
            }
            if now < self.next_pick {
                return None;
            }
            let allowed: Vec<&[&str]> = IDLE_PICKS
                .iter()
                .copied()
                .filter(|p| self.desk_new || !p.iter().any(|n| IDLE_PICKS_NEW.contains(n)))
                .collect();
            let fresh: Vec<&[&str]> = allowed
                .iter()
                .copied()
                .filter(|p| {
                    !self
                        .last_pick
                        .iter()
                        .map(String::as_str)
                        .eq(p.iter().copied())
                })
                .collect();
            let choices = if fresh.is_empty() { allowed } else { fresh };
            let seq = choices[self.rng.below(choices.len())];
            self.last_pick = seq.iter().map(|s| (*s).to_owned()).collect();
            self.pick = Some(Moment {
                anim: seq[0].to_owned(),
                text: String::new(),
                secs: seq.iter().map(|n| pass_secs(n)).sum(),
                start: now,
                seq: if seq.len() > 1 {
                    self.last_pick.clone()
                } else {
                    Vec::new()
                },
                urgent: false,
            });
        }
        let m = self.pick.as_ref()?;
        Some(View {
            seq: m.seq.clone(),
            ..make_view(&m.anim, "", "idle")
        })
    }

    /// `Pet.dim(now)`: brightness cap in quiet hours, `-1` for none.
    pub fn dim(&self, now: f64) -> i64 {
        let c = &self.cfg;
        if in_window(self.minute(now), c.quiet_start, c.quiet_end) {
            i64::from(c.quiet_dim)
        } else {
            -1
        }
    }
}

/// `isinstance(x, (int, float))` (a bool is an int in Python) as an `f64`.
fn number_or_bool(value: Option<&Value>) -> Option<f64> {
    match value {
        Some(Value::Number(n)) => n.as_f64(),
        Some(Value::Bool(b)) => Some(f64::from(u8::from(*b))),
        _ => None,
    }
}
