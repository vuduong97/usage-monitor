use serde_json::Value;

/// `clockapi.config_diff(old, new)`: what to POST to `/api/config` to turn `old` into `new`.
/// Nested objects are compared key by key and only changed keys are sent. Lists go whole: the
/// firmware treats a present `wifi` list as the complete list (order = priority, a missing row =
/// deletion). Keys only in `old` are left alone. A non-object `new` yields `{}`.
pub fn config_diff(old: &Value, new: &Value) -> Value {
    todo!("phase 08")
}
