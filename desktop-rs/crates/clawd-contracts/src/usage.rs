use std::ops::{Deref, DerefMut};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::consts::PAYLOAD_KEYS;

/// The usage payload pushed to the clock's `POST /api/usage` and shown in the settings preview.
///
/// A **map newtype** like [`crate::Prefs`]: the Python code builds a dict whose keys come and go
/// (`sr` only when known, `c`/`t`/`tf` only when configured, `src`/`why` for Codex and
/// "login expired") and the wire JSON must keep the Python key order
/// (`json.dumps(payload, separators=(",", ":"))`), which `preserve_order` gives.
///
/// Keys: `s` session %, `sr` minutes to its reset, `w` week %, `wr` its reset, `st` status
/// (`allowed|allowed_warning|rejected|unknown`), `ok`, `acct` (`pro|ent`), `tp` billing
/// progress %, `pd` period days, `rd` reset date text, `t` local epoch, `tf` 12/24, `c` chime
/// flag, `src` (`codex|off`), `why` (`login`); one-shot extras `b` beep, `m` alert text, `x` clear.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UsagePayload(pub Map<String, Value>);

impl UsagePayload {
    pub fn new() -> Self {
        Self(Map::new())
    }

    pub fn into_map(self) -> Map<String, Value> {
        self.0
    }

    fn int(&self, key: &str) -> Option<i64> {
        self.0.get(key).and_then(Value::as_i64)
    }

    fn text(&self, key: &str) -> Option<&str> {
        self.0.get(key).and_then(Value::as_str)
    }

    /// `s`: session percent (0-100).
    pub fn s(&self) -> Option<i64> {
        self.int("s")
    }
    /// `sr`: minutes until the session window resets.
    pub fn sr(&self) -> Option<i64> {
        self.int("sr")
    }
    /// `w`: week percent.
    pub fn w(&self) -> Option<i64> {
        self.int("w")
    }
    /// `wr`: minutes until the week window resets.
    pub fn wr(&self) -> Option<i64> {
        self.int("wr")
    }
    /// `st`: rate-limit status text.
    pub fn st(&self) -> Option<&str> {
        self.text("st")
    }
    /// `ok`: `Some(false)` means "no usable data" (the engine reads it as a login problem).
    pub fn ok(&self) -> Option<bool> {
        self.0.get("ok").and_then(Value::as_bool)
    }
    /// `acct`: `pro` (5h/7d windows) or `ent` (monthly spend limit).
    pub fn acct(&self) -> Option<&str> {
        self.text("acct")
    }
    /// `rd`: reset date text for `ent` accounts (`"Nov 1"`).
    pub fn rd(&self) -> Option<&str> {
        self.text("rd")
    }
    /// `t`: local wall-clock epoch (UTC epoch shifted by the tz offset).
    pub fn t(&self) -> Option<i64> {
        self.int("t")
    }
    /// `tf`: hour format the clock should render (12 or 24).
    pub fn tf(&self) -> Option<i64> {
        self.int("tf")
    }
    /// `src`: `codex` or `off`; absent for Claude (the clock reads absent as Claude).
    pub fn src(&self) -> Option<&str> {
        self.text("src")
    }
    /// `why`: `login` when the Claude login expired.
    pub fn why(&self) -> Option<&str> {
        self.text("why")
    }

    /// The subset the settings window draws the clock from (`paths.set_usage_payload`): only
    /// [`PAYLOAD_KEYS`], in that order; one-shot alerts (`m`, `x`, `b`) are never stored.
    pub fn persisted(&self) -> UsagePayload {
        let mut out = Map::new();
        for key in PAYLOAD_KEYS {
            if let Some(v) = self.0.get(key) {
                out.insert(key.to_owned(), v.clone());
            }
        }
        UsagePayload(out)
    }
}

impl Deref for UsagePayload {
    type Target = Map<String, Value>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for UsagePayload {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl From<Map<String, Value>> for UsagePayload {
    fn from(map: Map<String, Value>) -> Self {
        Self(map)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn persisted_keeps_only_payload_keys_in_order() {
        let p: UsagePayload = serde_json::from_value(
            json!({"s": 5, "m": "alert", "b": 1, "w": 9, "ok": true, "x": 1}),
        )
        .unwrap();
        assert_eq!(
            serde_json::to_string(&p.persisted()).unwrap(),
            r#"{"s":5,"w":9,"ok":true}"#
        );
    }

    #[test]
    fn typed_getters_ignore_wrong_types() {
        let p: UsagePayload =
            serde_json::from_value(json!({"s": "5", "ok": 1, "st": "allowed"})).unwrap();
        assert_eq!(p.s(), None);
        assert_eq!(p.ok(), None);
        assert_eq!(p.st(), Some("allowed"));
    }
}
