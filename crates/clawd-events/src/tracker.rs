//! Per-session agent state resolved into one desk animation (`events.py:232-486`).
//! Crate-internal in practice (only the Brain and the server use it); public for golden tests.

use clawd_contracts::View;
use indexmap::IndexMap;
use serde_json::{json, Value};

use crate::pyval::{text_or_empty, truthy};
use crate::rng::{Rng, SystemRng};
use crate::skill_rules::{normalize_skill, SkillRule, SkillRules};
use crate::source_events::char_prefix;
use crate::vocab::{
    action_text, ascii_text, is_question_tool, is_test_command, tool_action, variants_of,
    DEBUG_TEXT, EVENTS, QUESTION_SEQ, QUESTION_TEXTS, READING_ACTIONS, SESSION_SHOWS_THROUGH,
    WIZARD_TEXT, WORKING_VARIANTS,
};

/// `events.SESSION_TTL`: a working/thinking session silent this long went away.
pub const SESSION_TTL: f64 = 600.0;
/// `events.ATTENTION_TTL`: how long "needs you" stays up without an answer.
pub const ATTENTION_TTL: f64 = 1800.0;
/// `events.IDLE_FORGET`: drop idle sessions after this.
pub const IDLE_FORGET: f64 = 3600.0;
/// `events.ERROR_SHOW`.
pub const ERROR_SHOW: f64 = 5.0;
/// `events.DONE_SHOW`: the "Done!" happy, then the usage screens.
pub const DONE_SHOW: f64 = 7.0;
/// `events.DIZZY_SHOW`.
pub const DIZZY_SHOW: f64 = 6.0;
/// `events.ULTRATHINK_AFTER`: this long with no tool running.
pub const ULTRATHINK_AFTER: f64 = 30.0;
/// `events.EDIT_STREAK`: this many edit tools in a row: "typing boss".
pub const EDIT_STREAK: u32 = 10;
/// `events.WIZARD_AFTER`: a turn running this long: "wizard".
pub const WIZARD_AFTER: f64 = 1800.0;
/// `events.DIZZY_ERRORS`, `events.DIZZY_WINDOW`.
pub const DIZZY_ERRORS: usize = 3;
pub const DIZZY_WINDOW: f64 = 120.0;
/// Skills an agent loaded in one turn that are remembered for skill rules.
pub const NESTED_SKILLS_MAX: usize = 8;

/// `events.Moment`: a timed animation queued by the pet or a poke.
#[derive(Debug, Clone, PartialEq)]
pub struct Moment {
    pub anim: String,
    pub text: String,
    pub secs: f64,
    /// 0 until first played.
    pub start: f64,
    pub seq: Vec<String>,
    pub urgent: bool,
}

/// `events.Sess`: one agent session.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Sess {
    pub sid: String,
    pub src: String,
    /// A child agent process (e.g. `pi -p` run by a subagent).
    pub sub: bool,
    /// `idle | thinking | working | attention | waiting | sweeping`.
    pub state: String,
    /// Raw tool name, for `clawd status`.
    pub tool: String,
    /// Its `ACTION_TEXT` key, for the clock.
    pub action: String,
    pub msg: String,
    /// The attention is a question tool (react left/right).
    pub asking: bool,
    pub ask_text: String,
    pub subs: u32,
    pub last: f64,
    pub error_until: f64,
    pub error_text: String,
    pub done_until: f64,
    pub since: f64,
    pub turn_start: f64,
    pub running: u32,
    pub quiet_since: f64,
    pub testing: bool,
    pub debugging: bool,
    pub debug_next: bool,
    pub edits: u32,
    /// The skill the user invoked in this turn's prompt (normalized; skill animation rules).
    pub skill: String,
    /// Skills the agent loaded during this turn, oldest first (`Skill` tool, `SKILL.md` reads).
    pub nested: Vec<String>,
}

/// Called with `now` when a turn starts (the Pet counts turns per day).
pub type OnTurn = Box<dyn FnMut(f64) + Send>;

/// A view with the defaults of `events.View(anim, text, layer)`.
pub(crate) fn make_view(anim: &str, text: &str, layer: &str) -> View {
    View {
        anim: anim.to_owned(),
        text: text.to_owned(),
        layer: layer.to_owned(),
        ..View::default()
    }
}

fn owned(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| (*s).to_owned()).collect()
}

/// `max(xs, key=last)`: the first of equal maxima, like Python.
fn latest<'a>(sessions: impl IntoIterator<Item = &'a Sess>) -> &'a Sess {
    let mut best: Option<&Sess> = None;
    for s in sessions {
        if best.is_none_or(|b| s.last > b.last) {
            best = Some(s);
        }
    }
    best.expect("latest() of a non-empty set")
}

/// `events.Tracker`.
pub struct Tracker {
    /// Insertion-ordered like the Python dict: it decides `/state` order and `max` ties.
    pub sessions: IndexMap<String, Sess>,
    on_turn: Option<OnTurn>,
    rng: Box<dyn Rng>,
    /// No phrase twice in a row.
    last_ask_text: String,
    /// Recent failures (dizzy).
    errors: Vec<f64>,
    dizzy_until: f64,
    /// Skill animation rules (empty = the plain behaviour).
    pub rules: SkillRules,
    /// Every clock has the newer desk.pak (set by the Brain from the Pet's probe result).
    pub desk_new: bool,
}

impl Tracker {
    /// `Tracker(on_turn)`.
    pub fn new(on_turn: Option<OnTurn>) -> Self {
        Self::with_rng(on_turn, Box::new(SystemRng::new()))
    }

    /// `Tracker(on_turn, rng)`.
    pub fn with_rng(on_turn: Option<OnTurn>, rng: Box<dyn Rng>) -> Self {
        Self {
            sessions: IndexMap::new(),
            on_turn,
            rng,
            last_ask_text: String::new(),
            errors: Vec::new(),
            dizzy_until: 0.0,
            rules: SkillRules::default(),
            desk_new: false,
        }
    }

    fn error(&mut self, now: f64) {
        self.errors.retain(|t| now - t < DIZZY_WINDOW);
        self.errors.push(now);
        if self.errors.len() >= DIZZY_ERRORS {
            self.errors.clear();
            self.dizzy_until = now + DIZZY_SHOW;
        }
    }

    /// `Tracker.ingest(ev, now)`: apply one tracker event (see [`crate::EVENTS`]); true when it
    /// changed anything (the server then wakes the Brain).
    pub fn ingest(&mut self, ev: &Value, now: f64) -> bool {
        let kind = text_or_empty(ev.get("ev"));
        let sid = char_prefix(&text_or_empty(ev.get("sid")), 200);
        if !EVENTS.contains(&kind.as_str()) || sid.is_empty() {
            return false;
        }
        if kind == "end" {
            return self.sessions.shift_remove(&sid).is_some();
        }
        // Work on a copy and `insert` it back: an existing key keeps its slot, like a dict.
        let mut s = match self.sessions.get(&sid) {
            Some(existing) => existing.clone(),
            None => Sess {
                sid: sid.clone(),
                src: text_or_empty(ev.get("src")),
                sub: truthy(ev.get("sub")),
                state: "idle".to_owned(),
                since: now,
                quiet_since: now,
                ..Sess::default()
            },
        };
        s.last = now;
        let before = s.state.clone();
        self.apply(&mut s, &kind, ev, now);
        if s.state != before {
            s.since = now;
        }
        self.sessions.insert(sid, s);
        true
    }

    fn apply(&mut self, s: &mut Sess, kind: &str, ev: &Value, now: f64) {
        let raw = text_or_empty(ev.get("tool"));
        let tool = ascii_text(&raw, 24);
        if kind == "tool_done" || kind == "tool_error" {
            s.running = s.running.saturating_sub(1);
            if s.running == 0 {
                s.quiet_since = now;
            }
        }
        match kind {
            "prompt" => {
                s.state = "thinking".into();
                s.msg.clear();
                s.turn_start = now;
                s.edits = 0;
                s.testing = false;
                s.running = 0;
                s.quiet_since = now;
                s.debugging = s.debug_next || truthy(ev.get("debug"));
                s.debug_next = false;
                s.skill = normalize_skill(&text_or_empty(ev.get("skill")));
                s.nested.clear();
            }
            "tool" => {
                s.state = "working".into();
                s.msg.clear();
                if s.turn_start == 0.0 {
                    s.turn_start = now;
                }
                s.running = (s.running + 1).min(16);
                if !tool.is_empty() {
                    s.action = tool_action(&raw).to_owned();
                    s.tool = tool;
                    s.edits = if s.action == "edit" { s.edits + 1 } else { 0 };
                }
                s.testing = is_test_command(ev.get("cmd").and_then(Value::as_str));
                if truthy(ev.get("debug")) {
                    s.debugging = true;
                }
                let nested = normalize_skill(&text_or_empty(ev.get("skill_nested")));
                if !nested.is_empty() {
                    s.nested.retain(|n| *n != nested);
                    if s.nested.len() >= NESTED_SKILLS_MAX {
                        s.nested.remove(0);
                    }
                    s.nested.push(nested);
                }
            }
            "tool_done" => {
                s.state = "working".into();
                s.msg.clear();
                s.testing = false;
            }
            "attention_done" => {
                if s.state == "attention" || s.state == "waiting" {
                    s.state = "thinking".into();
                    s.msg.clear();
                    s.asking = false;
                    s.quiet_since = now;
                }
            }
            "attention_seen" => {
                // the user went to that session: stop calling
                if s.state == "attention" {
                    s.state = "waiting".into();
                    s.msg.clear();
                }
            }
            "tool_error" => {
                if s.state != "attention" && s.state != "waiting" {
                    s.state = "working".into();
                }
                let action = if tool.is_empty() {
                    s.action.clone()
                } else {
                    tool_action(&raw).to_owned()
                };
                s.error_until = now + ERROR_SHOW;
                s.error_text = action_text(&action).1.to_owned();
                s.testing = false;
                s.debugging = true;
                s.debug_next = true;
                self.error(now);
            }
            "subagent_start" => s.subs = (s.subs + 1).min(8),
            "subagent_stop" => s.subs = s.subs.saturating_sub(1),
            "attention" => {
                let open_question = s.asking && (s.state == "attention" || s.state == "waiting");
                let mut asking = is_question_tool(&raw);
                if raw.is_empty() && open_question {
                    asking = true; // a generic nudge while a question is open
                }
                if asking && !open_question {
                    // a new question: pick its phrase once
                    let pool: Vec<&str> = QUESTION_TEXTS
                        .iter()
                        .copied()
                        .filter(|t| *t != self.last_ask_text)
                        .collect();
                    let pick = pool[self.rng.below(pool.len())].to_owned();
                    s.ask_text = pick.clone();
                    self.last_ask_text = pick;
                }
                s.msg = if asking {
                    String::new()
                } else {
                    ascii_text(&text_or_empty(ev.get("msg")), crate::vocab::TEXT_MAX)
                };
                s.state = "attention".into();
                s.asking = asking;
            }
            "compact" => s.state = "sweeping".into(),
            "compact_done" => {
                s.state = "thinking".into();
                s.quiet_since = now;
            }
            "stop" => {
                s.state = "idle".into();
                s.subs = 0;
                s.msg.clear();
                s.done_until = now + DONE_SHOW;
                if !s.sub {
                    if let Some(on_turn) = self.on_turn.as_mut() {
                        on_turn(now);
                    }
                }
            }
            "abort" => {
                // the user cancelled: back to idle, quietly
                s.state = "idle".into();
                s.subs = 0;
                s.msg.clear();
            }
            "stop_error" => {
                s.state = "idle".into();
                s.subs = 0;
                s.msg.clear();
                s.error_until = now + ERROR_SHOW;
                s.error_text = "Error".into();
                s.debug_next = true;
                self.error(now);
            }
            _ => {}
        }
        if matches!(kind, "stop" | "abort" | "stop_error") {
            s.turn_start = 0.0;
            s.debugging = false;
            s.edits = 0;
            s.testing = false;
            s.running = 0;
            s.skill.clear();
            s.nested.clear();
        }
        if s.state != "attention" && s.state != "waiting" {
            s.asking = false;
        }
    }

    /// `Tracker.expire(now)`: stale attention/working sessions go idle, old idle ones are
    /// forgotten.
    pub fn expire(&mut self, now: f64) {
        for s in self.sessions.values_mut() {
            let age = now - s.last;
            if (s.state == "attention" || s.state == "waiting") && age > ATTENTION_TTL {
                s.state = "idle".into();
                s.msg.clear();
                s.asking = false;
            } else if matches!(s.state.as_str(), "thinking" | "working" | "sweeping")
                && age > SESSION_TTL
            {
                s.state = "idle".into();
                s.subs = 0;
            }
        }
        self.sessions
            .retain(|_, s| !(s.state == "idle" && now - s.last > IDLE_FORGET));
    }

    /// `Tracker.view(now)`: all sessions resolved into one agent-layer view, if any.
    pub fn view(&mut self, now: f64) -> Option<View> {
        self.expire(now);
        let main: Vec<&Sess> = self.sessions.values().filter(|s| !s.sub).collect();

        let attn: Vec<&Sess> = main
            .iter()
            .copied()
            .filter(|s| s.state == "attention")
            .collect();
        if !attn.is_empty() {
            let a = latest(attn);
            if a.asking {
                let text = if a.ask_text.is_empty() {
                    QUESTION_TEXTS[0]
                } else {
                    &a.ask_text
                };
                return Some(View {
                    seq: owned(&QUESTION_SEQ),
                    rep: true,
                    urgent: true,
                    ..make_view("question", text, "agent")
                });
            }
            let text = if a.msg.is_empty() {
                "Needs you"
            } else {
                &a.msg
            };
            return Some(View {
                urgent: true,
                ..make_view("notification", text, "agent")
            });
        }
        if self.dizzy_until > now {
            return Some(View {
                urgent: true,
                ..make_view("dizzy", "Too many errors", "agent")
            });
        }
        let errs: Vec<&Sess> = main
            .iter()
            .copied()
            .filter(|s| s.error_until > now)
            .collect();
        if !errs.is_empty() {
            let text = match latest(errs).error_text.as_str() {
                "" => "Error",
                other => other,
            };
            return Some(View {
                urgent: true,
                ..make_view("error", text, "agent")
            });
        }
        if main.iter().any(|s| s.state == "sweeping") {
            return Some(make_view("sweeping", "Compacting", "agent"));
        }
        // Agents: running main sessions + their subagents + live child processes.
        let active: Vec<&Sess> = main
            .iter()
            .copied()
            .filter(|s| s.state == "thinking" || s.state == "working")
            .collect();
        let child_processes = self
            .sessions
            .values()
            .filter(|s| s.sub && (s.state == "thinking" || s.state == "working"))
            .count();
        let agents =
            active.len() + main.iter().map(|s| s.subs as usize).sum::<usize>() + child_processes;
        if agents == 1 && !active.is_empty() {
            return Some(self.session_view(active[0], now).0);
        }
        if agents > 0 {
            // The most recent session's reading/debugger/ultrathink shows through,
            // counted; anything plainer (a skill rule too) leaves the many-agents animation.
            if !active.is_empty() {
                let (v, from_rule) = self.session_view(latest(active.iter().copied()), now);
                if !from_rule && (SESSION_SHOWS_THROUGH.contains(&v.anim.as_str()) || v.debug) {
                    let text = format!("{} ({agents})", v.text);
                    return Some(View { text, ..v });
                }
            }
            // One extra agent beside a running session: headphones; more: juggling.
            // With no main session running, the agents left are all extra.
            let extra = if active.is_empty() {
                agents
            } else {
                agents - 1
            };
            let noun = if agents == 1 { "agent" } else { "agents" };
            let anim = if extra <= 1 { "headphones" } else { "juggling" };
            return Some(make_view(anim, &format!("{agents} {noun}"), "agent"));
        }
        if main.iter().any(|s| s.done_until > now) {
            return Some(make_view("happy", "Done!", "agent"));
        }
        None
    }

    /// The skill rule for a session's turn: the skill the user typed, else the most recent
    /// skill the agent loaded that has a rule (so `/ak-cook` keeps its rule when it loads
    /// `ak-journal`, and a skill the agent picked by itself still counts).
    fn rule_for(&self, s: &Sess) -> Option<&SkillRule> {
        if self.rules.is_empty() {
            return None;
        }
        if let Some(rule) = self.rules.get(&s.skill, self.desk_new) {
            return Some(rule);
        }
        s.nested
            .iter()
            .rev()
            .find_map(|skill| self.rules.get(skill, self.desk_new))
    }

    /// One thinking/working session: ultrathink > testing (debugger) >
    /// reading > skill rule > typing boss > wizard > typing (thinking before its first tool,
    /// unless a skill rule applies). The text always says what the animation shows: a
    /// debugging turn is the debugger ("Debugging"), a long turn the wizard ("Long task").
    /// The flag is true when a skill rule decided the view.
    fn session_view(&self, s: &Sess, now: f64) -> (View, bool) {
        let with_variants = |mut v: View, variants: &[&str]| {
            v.variants = owned(variants);
            v
        };
        let ruled = |rule: &SkillRule, default_text: &str| {
            let text = if rule.text.is_empty() {
                default_text
            } else {
                &rule.text
            };
            (make_view(&rule.anim, text, "agent"), true)
        };
        if s.running == 0 && now - s.quiet_since >= ULTRATHINK_AFTER {
            let v = make_view("ultrathink", "Thinking hard", "agent");
            return (with_variants(v, variants_of("ultrathink")), false);
        }
        if s.state == "thinking" {
            if let Some(rule) = self.rule_for(s) {
                return ruled(rule, "Thinking");
            }
            return (make_view("thinking", "Thinking", "agent"), false);
        }
        if s.testing {
            return (make_view("debugger", "Testing", "agent"), false);
        }
        if s.debugging {
            let v = View {
                debug: true,
                ..make_view("debugger", DEBUG_TEXT, "agent")
            };
            return (v, false);
        }
        let reading = READING_ACTIONS.contains(&s.action.as_str());
        if !reading {
            if let Some(rule) = self.rule_for(s) {
                return ruled(rule, action_text(&s.action).0);
            }
        }
        let anim = if reading {
            "reading"
        } else if s.edits >= EDIT_STREAK {
            "typing boss"
        } else if s.turn_start != 0.0 && now - s.turn_start >= WIZARD_AFTER {
            "wizard"
        } else {
            "typing"
        };
        if anim == "wizard" {
            let v = make_view(anim, WIZARD_TEXT, "agent");
            return (with_variants(v, variants_of("wizard")), false);
        }
        let variants: &[&str] = if anim == "typing" && s.action.is_empty() {
            &WORKING_VARIANTS
        } else {
            variants_of(anim)
        };
        let v = make_view(anim, action_text(&s.action).0, "agent");
        (with_variants(v, variants), false)
    }

    /// `Tracker.snapshot(now)`: the `sessions` array of `GET /state`
    /// (`sid src sub state tool subs running debug age`).
    pub fn snapshot(&self, now: f64) -> Vec<Value> {
        self.sessions
            .values()
            .map(|s| {
                json!({
                    "sid": s.sid, "src": s.src, "sub": s.sub, "state": s.state,
                    "tool": s.tool, "subs": s.subs, "running": s.running,
                    "debug": s.debugging, "age": py_round(now - s.last),
                })
            })
            .collect()
    }
}

/// Python's `round(x)`: half to even.
pub(crate) fn py_round(x: f64) -> i64 {
    let floor = x.floor();
    let diff = x - floor;
    let rounded = if diff < 0.5 {
        floor
    } else if diff > 0.5 {
        floor + 1.0
    } else if floor % 2.0 == 0.0 {
        floor
    } else {
        floor + 1.0
    };
    rounded as i64
}
