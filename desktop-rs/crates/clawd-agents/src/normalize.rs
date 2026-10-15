use serde_json::Value;

/// `agents.normalize(agent_id, payload, hint)`: tracker events for one hook payload; `None` for
/// an agent id nobody knows. A payload shape the adapter did not expect is dropped (empty
/// list), never an error, because the event server must never answer 500. `hint` is the
/// `?e=<event>` of the hook URL.
///
/// Has exactly the shape of `clawd_events::NormalizeFn`; the app passes it to the event server
/// as `Arc::new(clawd_agents::normalize)`.
pub fn normalize(agent_id: &str, payload: &Value, hint: &str) -> Option<Vec<Value>> {
    todo!("phase 05")
}
