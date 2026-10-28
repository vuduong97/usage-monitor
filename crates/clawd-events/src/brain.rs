//! `events.Brain` (`events.py:836-892`): owns every layer and decides the one state the clock
//! should show.

use std::sync::{Arc, Mutex};

use clawd_contracts::{DesiredState, UsagePayload, View};
use serde_json::{json, Value};
use tokio::sync::Notify;

use crate::pet::Pet;
use crate::pokes::Pokes;
use crate::skill_rules::{RulesWatch, SkillRules};
use crate::tracker::{make_view, Tracker};

/// The Brain is shared by the event server (ingests), the state pusher (reads `desired`) and the
/// engine (publishes `usage`): one lock, held briefly, never across an `.await`. To wake the
/// pusher without holding the lock, clone [`Brain::wake`] out first.
pub type SharedBrain = Arc<Mutex<Brain>>;

/// `events.MIN_DISPLAY` / `MIN_DISPLAY_WORKING`: how long an agent animation stays up before
/// anything but a poke replaces it (clawd-on-desk's minDisplay), seconds.
pub fn min_display(anim: &str) -> f64 {
    match anim {
        "notification" | "question" | "error" | "dizzy" => 5.0,
        "sweeping" => 5.5,
        "happy" => 4.0,
        _ => 1.0,
    }
}

/// `events.FOCUS_KEEPS_TEXT`: agent animations whose text a running focus timer leaves alone.
const FOCUS_KEEPS_TEXT: [&str; 3] = ["notification", "question", "error"];

/// `events.Brain`.
pub struct Brain {
    pub pet: Pet,
    pub tracker: Tracker,
    pub pokes: Pokes,
    /// `Brain.wake` (`asyncio.Event` in Python): notified when an event or poke changed
    /// something; the pusher waits on it. Cloneable out of the lock.
    pub wake: Arc<Notify>,
    /// Latest usage payload (set by the engine from the Publisher's `on_payload`).
    pub usage: Option<UsagePayload>,
    usage_only: Box<dyn Fn() -> bool + Send + Sync>,
    /// Turn starts the Tracker reported and the Pet has not counted yet (the Tracker and the
    /// Pet live in the same struct, so the callback cannot borrow the Pet).
    turn_queue: Arc<Mutex<Vec<f64>>>,
    /// Last view handed out, for minDisplay.
    shown: Option<View>,
    shown_since: f64,
    /// The skill animation rules file, re-read when it changes (`None`: rules only come from
    /// [`Brain::set_skill_rules`], as in tests).
    rules_watch: Option<RulesWatch>,
}

impl Brain {
    /// `Brain(pet, usage_only)`: `usage_only()` is true while the clock's main screen is Usage
    /// (then only a poke or an agent that needs you interrupts it).
    pub fn new(pet: Pet, usage_only: Box<dyn Fn() -> bool + Send + Sync>) -> Self {
        Self::with_tracker(pet, usage_only, None)
    }

    /// Same with a Tracker built by the caller from `make(on_turn)` (tests pin its random
    /// draws); `None` builds the default one.
    pub fn with_tracker(
        pet: Pet,
        usage_only: Box<dyn Fn() -> bool + Send + Sync>,
        make_tracker: Option<Box<dyn FnOnce(crate::tracker::OnTurn) -> Tracker>>,
    ) -> Self {
        let turn_queue: Arc<Mutex<Vec<f64>>> = Arc::new(Mutex::new(Vec::new()));
        let queue = Arc::clone(&turn_queue);
        let on_turn: crate::tracker::OnTurn = Box::new(move |now| {
            if let Ok(mut q) = queue.lock() {
                q.push(now);
            }
        });
        let tracker = match make_tracker {
            Some(make) => make(on_turn),
            None => Tracker::new(Some(on_turn)),
        };
        Self {
            pet,
            tracker,
            pokes: Pokes::new(),
            wake: Arc::new(Notify::new()),
            usage: None,
            usage_only,
            turn_queue,
            shown: None,
            shown_since: 0.0,
            rules_watch: None,
        }
    }

    /// Follow the skill animation rules file: read now, then again whenever it changes, so the
    /// settings window's edits apply without a restart.
    pub fn watch_skill_rules(&mut self, path: std::path::PathBuf, now: f64) {
        let mut watch = RulesWatch::new(path);
        if let Some(rules) = watch.poll(now) {
            self.tracker.rules = rules;
        }
        self.rules_watch = Some(watch);
    }

    /// Replace the skill animation rules.
    pub fn set_skill_rules(&mut self, rules: SkillRules) {
        self.tracker.rules = rules;
    }

    /// Wrap into the shared handle the server and pusher take.
    pub fn shared(self) -> SharedBrain {
        Arc::new(Mutex::new(self))
    }

    /// Hand the turn starts the Tracker reported to the Pet (Python calls `pet.on_turn` from
    /// inside `ingest`; here it follows right after, before anything reads the Pet).
    fn sync_turns(&mut self) {
        let pending: Vec<f64> = match self.turn_queue.lock() {
            Ok(mut q) => std::mem::take(&mut *q),
            Err(_) => return,
        };
        for now in pending {
            self.pet.on_turn(now);
        }
    }

    /// `Brain.tracker.ingest(ev, now)` + the Pet's turn count: true when anything changed.
    pub fn ingest(&mut self, ev: &Value, now: f64) -> bool {
        let changed = self.tracker.ingest(ev, now);
        self.sync_turns();
        changed
    }

    /// `Brain._hold(v, now)`: an agent animation stays up at least `min_display(anim)` seconds
    /// before anything but a poke replaces it.
    fn hold(&mut self, v: Option<View>, now: f64) -> Option<View> {
        if let Some(shown) = &self.shown {
            let v_layer_is_poke = v.as_ref().is_some_and(|v| v.layer == "poke");
            let same_names = v.as_ref().is_some_and(|v| v.names() == shown.names());
            if shown.layer == "agent"
                && !v_layer_is_poke
                && !same_names
                && now - self.shown_since < min_display(&shown.anim)
            {
                return Some(shown.clone());
            }
        }
        let changed = match (&v, &self.shown) {
            (Some(v), Some(shown)) => {
                v.names() != shown.names() || v.rep != shown.rep || v.variants != shown.variants
            }
            _ => true,
        };
        if changed {
            self.shown_since = now;
        }
        self.shown = v.clone();
        v
    }

    /// `Brain.desired(now)`: `{a, seq, rep, v, u, txt, cd, dim, focus_until}` for this instant
    /// (poke moment > agent > focus > pet moment > idle pick), honouring the minDisplay holds.
    pub fn desired(&mut self, now: f64) -> DesiredState {
        self.sync_turns();
        if let Some(rules) = self.rules_watch.as_mut().and_then(|w| w.poll(now)) {
            self.tracker.rules = rules;
        }
        self.tracker.desk_new = self.pet.desk_new;
        let mut v = match self.pokes.moment(now) {
            Some(poke) => Some(poke),
            None => self.tracker.view(now),
        };
        let only = (self.usage_only)();
        if only
            && v.as_ref()
                .is_some_and(|v| v.layer != "poke" && !(v.layer == "agent" && v.urgent))
        {
            v = None; // Usage screen: only a poke or an agent that needs you interrupts it
        }
        let focus = self.pokes.focus_left(now);
        let mut cd = 0;
        let focus_takes_over = v
            .as_ref()
            .is_none_or(|v| v.layer == "agent" && !FOCUS_KEEPS_TEXT.contains(&v.anim.as_str()));
        if focus != 0 && focus_takes_over {
            v = Some(match v {
                Some(v) => View {
                    text: "Focus".into(),
                    ..v
                },
                None => make_view("reading", "Focus", "focus"),
            });
            cd = focus;
        }
        if v.is_none() && !only {
            v = self.pet.moment(now);
        }
        if v.is_none() && !only {
            v = self.pet.continuous(now);
        }
        let v = self.hold(v, now);
        if v.as_ref().is_some_and(|v| v.layer != "idle") {
            self.pet.hold_off(now); // the next idle pick waits a full gap
        }
        let mut state = DesiredState::idle();
        if let Some(v) = &v {
            state.a = v.anim.clone();
            state.seq = v.names().iter().map(|s| (*s).to_owned()).collect();
            state.rep = v.rep;
            state.v = v.variants.clone();
            state.u = v.urgent;
            // the clock text always starts with a capital
            let mut chars = v.text.chars();
            state.txt = match chars.next() {
                Some(first) => first.to_uppercase().chain(chars).collect(),
                None => String::new(),
            };
        }
        state.cd = cd;
        state.dim = self.pet.dim(now);
        state.focus_until = if cd != 0 {
            self.pokes.focus_until as i64
        } else {
            0
        };
        state
    }

    /// The `GET /state` body: `{desired, sessions, turns, away, focus_left}`.
    pub fn state_body(&mut self, now: f64) -> Value {
        let desired = self.desired(now);
        json!({
            "desired": desired,
            "sessions": self.tracker.snapshot(now),
            "turns": self.pet.turns,
            "away": self.pet.away,
            "focus_left": self.pokes.focus_left(now),
        })
    }
}
