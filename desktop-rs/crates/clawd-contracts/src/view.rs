use serde::{Deserialize, Serialize};

/// `events.View`: what the clock's full-screen Clawd should show, decided by the Brain.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct View {
    /// What it is called (logs, `clawd status`, holds).
    pub anim: String,
    pub text: String,
    /// `poke | agent | focus | pet | idle`, or empty.
    pub layer: String,
    /// The clock plays these in order (default: just `anim`).
    pub seq: Vec<String>,
    /// Cycle `seq` instead of looping its last animation.
    pub rep: bool,
    /// Replace what plays now instead of at its pass end.
    pub urgent: bool,
    /// The clock mixes one in every 3 passes (`v`).
    pub variants: Vec<String>,
    /// A debugging session (shows through many agents).
    pub debug: bool,
}

impl View {
    /// `View.names`: `seq`, or just `anim` when `seq` is empty.
    pub fn names(&self) -> Vec<&str> {
        if self.seq.is_empty() {
            vec![self.anim.as_str()]
        } else {
            self.seq.iter().map(String::as_str).collect()
        }
    }
}

/// `Brain.desired(now)`: the one state the clock should show. Serialises to exactly the
/// `desired` object of `GET /state` (same keys, same order as the Python dict).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DesiredState {
    /// Animation name; empty = show the usage screens.
    pub a: String,
    pub seq: Vec<String>,
    pub rep: bool,
    /// Variants.
    pub v: Vec<String>,
    /// Urgent.
    pub u: bool,
    /// Text line (first letter capital).
    pub txt: String,
    /// Countdown seconds (focus timer).
    pub cd: i64,
    /// Brightness cap in quiet hours, -1 = none.
    pub dim: i64,
    pub focus_until: i64,
}

impl DesiredState {
    /// Nothing to show: the clock keeps its usage screens (`a` empty, no quiet-hours cap).
    pub fn idle() -> Self {
        Self {
            a: String::new(),
            seq: Vec::new(),
            rep: false,
            v: Vec::new(),
            u: false,
            txt: String::new(),
            cd: 0,
            dim: -1,
            focus_until: 0,
        }
    }
}

/// The `a` of a wire state: one name, or the list when the view is a sequence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum WireAnim {
    One(String),
    Many(Vec<String>),
}

/// `events.wire(d)`: the body of `POST /api/state` on the clock. Optional keys are omitted
/// when empty; field order is the Python key order (a, ttl, rep, v, u, txt, cd, dim), so
/// `serde_json::to_string` is byte-identical to `json.dumps(..., separators=(",", ":"))`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireState {
    pub a: WireAnim,
    /// `events.STATE_TTL`.
    pub ttl: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rep: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub v: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub u: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub txt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cd: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dim: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_fall_back_to_anim() {
        let v = View {
            anim: "idle".into(),
            ..View::default()
        };
        assert_eq!(v.names(), vec!["idle"]);
        let v = View {
            anim: "yawn".into(),
            seq: vec!["yawn".into(), "sleeping".into()],
            ..View::default()
        };
        assert_eq!(v.names(), vec!["yawn", "sleeping"]);
    }

    #[test]
    fn wire_state_serialises_like_python() {
        let w = WireState {
            a: WireAnim::Many(vec!["a".into(), "b".into()]),
            ttl: 60,
            rep: Some(true),
            v: None,
            u: None,
            txt: Some("Hi".into()),
            cd: None,
            dim: Some(15),
        };
        assert_eq!(
            serde_json::to_string(&w).unwrap(),
            r#"{"a":["a","b"],"ttl":60,"rep":true,"txt":"Hi","dim":15}"#
        );
    }

    #[test]
    fn desired_keys_match_the_state_route() {
        let json = serde_json::to_string(&DesiredState::idle()).unwrap();
        assert_eq!(
            json,
            r#"{"a":"","seq":[],"rep":false,"v":[],"u":false,"txt":"","cd":0,"dim":-1,"focus_until":0}"#
        );
    }
}
