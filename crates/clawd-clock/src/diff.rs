use serde_json::{Map, Value};

use crate::pyval;

/// `clockapi.config_diff(old, new)`: what to POST to `/api/config` to turn `old` into `new`.
/// Nested objects are compared key by key and only changed keys are sent. Lists go whole: the
/// firmware treats a present `wifi` list as the complete list (order = priority, a missing row =
/// deletion). Keys only in `old` are left alone. A non-object `new` yields `{}`.
pub fn config_diff(old: &Value, new: &Value) -> Value {
    let mut out = Map::new();
    let Value::Object(new) = new else {
        return Value::Object(out);
    };
    let empty = Map::new();
    let old = old.as_object().unwrap_or(&empty);
    for (k, v) in new {
        match old.get(k) {
            None => {
                out.insert(k.clone(), v.clone());
            }
            Some(before) if v.is_object() && before.is_object() => {
                let sub = config_diff(before, v);
                if sub.as_object().is_some_and(|m| !m.is_empty()) {
                    out.insert(k.clone(), sub);
                }
            }
            Some(before) if !pyval::eq(v, before) => {
                out.insert(k.clone(), v.clone());
            }
            Some(_) => {}
        }
    }
    Value::Object(out)
}
