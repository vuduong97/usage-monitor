use std::ops::{Deref, DerefMut};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// `desktop.json`: app preferences. A **map newtype, not a typed struct** (red team #11): two
/// processes write the file and older/newer versions add keys, so every key, including unknown
/// ones and ones of the wrong type, must survive a read-modify-write.
///
/// Typed accessors are as tolerant as the Python readers (`firstrun.py:22-28`,
/// `paths.py:185-194`): a value of the wrong type reads as `None` / empty, and is never
/// rewritten unless the caller sets that key. `serde_json` is built with `preserve_order`,
/// so keys keep the order they were read in.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Prefs(pub Map<String, Value>);

impl Prefs {
    pub fn new() -> Self {
        Self(Map::new())
    }

    pub fn into_map(self) -> Map<String, Value> {
        self.0
    }

    /// `isinstance(v, int) and not isinstance(v, bool)`: JSON integers only (not `1.0`).
    pub fn get_i64(&self, key: &str) -> Option<i64> {
        self.0.get(key).and_then(Value::as_i64)
    }

    pub fn get_str(&self, key: &str) -> Option<&str> {
        self.0.get(key).and_then(Value::as_str)
    }

    pub fn get_bool(&self, key: &str) -> Option<bool> {
        self.0.get(key).and_then(Value::as_bool)
    }

    /// `Some(strings)` when the value is a JSON array (elements that are not strings dropped);
    /// `None` when the key is missing or not an array.
    pub fn get_str_list(&self, key: &str) -> Option<Vec<String>> {
        let items = self.0.get(key)?.as_array()?;
        Some(
            items
                .iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect(),
        )
    }

    /// `clock_screen`: the clock's main screen as the settings window last saw it
    /// (`paths.clock_screen`). `None` when missing, a float, a bool or a string.
    pub fn clock_screen(&self) -> Option<i64> {
        self.get_i64("clock_screen")
    }

    /// `usage_source` as stored (`claude`, `claude:<dir>`, `codex`, `off`); validation is
    /// `clawd_config::read_usage_source` (`usage.read_usage_source`).
    pub fn usage_source(&self) -> Option<&str> {
        self.get_str("usage_source")
    }

    /// True when `setup_done` is a list (otherwise `firstrun.migrate` derives it from
    /// `first_run_done`).
    pub fn has_setup_done_list(&self) -> bool {
        self.0.get("setup_done").is_some_and(Value::is_array)
    }

    /// `setup_done`: wizard steps finished or skipped; non-string elements dropped.
    pub fn setup_done(&self) -> Vec<String> {
        self.get_str_list("setup_done").unwrap_or_default()
    }

    /// 0.1.x's `first_run_done`, with Python truthiness (`if prefs.get("first_run_done")`).
    pub fn first_run_done(&self) -> bool {
        self.0.get("first_run_done").is_some_and(python_truthy)
    }

    /// `setup_shown`: the app version that last opened the wizard by itself.
    pub fn setup_shown(&self) -> Option<&str> {
        self.get_str("setup_shown")
    }

    /// `agents_seen`: agent ids already offered to the user; non-string elements dropped.
    pub fn agents_seen(&self) -> Vec<String> {
        self.get_str_list("agents_seen").unwrap_or_default()
    }
}

/// Python's `bool(value)` for a JSON value.
fn python_truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_none_or(|f| f != 0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

impl Deref for Prefs {
    type Target = Map<String, Value>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Prefs {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl From<Map<String, Value>> for Prefs {
    fn from(map: Map<String, Value>) -> Self {
        Self(map)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn prefs(v: Value) -> Prefs {
        serde_json::from_value(v).unwrap()
    }

    #[test]
    fn wrong_types_read_as_none_and_survive() {
        let p = prefs(
            json!({"clock_screen": 1.0, "setup_done": "clock", "usage_source": 7, "x_future": {"a": 1}}),
        );
        assert_eq!(p.clock_screen(), None);
        assert!(!p.has_setup_done_list());
        assert!(p.setup_done().is_empty());
        assert_eq!(p.usage_source(), None);
        let round = serde_json::to_value(&p).unwrap();
        assert_eq!(round["x_future"]["a"], 1);
        assert_eq!(round["clock_screen"], 1.0);
    }

    #[test]
    fn bool_is_not_an_int_and_lists_drop_bad_elements() {
        let p = prefs(json!({"clock_screen": true, "setup_done": ["clock", 3, "usage"]}));
        assert_eq!(p.clock_screen(), None);
        assert_eq!(p.setup_done(), vec!["clock", "usage"]);
        assert_eq!(
            prefs(json!({"clock_screen": 300})).clock_screen(),
            Some(300)
        );
    }

    #[test]
    fn first_run_done_uses_python_truthiness() {
        assert!(prefs(json!({"first_run_done": true})).first_run_done());
        assert!(prefs(json!({"first_run_done": "yes"})).first_run_done());
        assert!(!prefs(json!({"first_run_done": 0})).first_run_done());
        assert!(!prefs(json!({})).first_run_done());
    }

    #[test]
    fn key_order_is_preserved() {
        let p = prefs(json!({"b": 1, "a": 2}));
        assert_eq!(serde_json::to_string(&p).unwrap(), r#"{"b":1,"a":2}"#);
    }
}
