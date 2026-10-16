use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::OnceLock;

use serde_json::Value;

use crate::base::AgentCtx;
use crate::registry::Registry;

/// The adapters' `normalize` never reads the machine, so one shared registry serves every call.
fn shared_registry() -> &'static Registry {
    static REGISTRY: OnceLock<Registry> = OnceLock::new();
    REGISTRY.get_or_init(|| Registry::new(AgentCtx::system()))
}

/// `agents.normalize(agent_id, payload, hint)`: tracker events for one hook payload; `None` for
/// an agent id nobody knows. A payload shape the adapter did not expect is dropped (empty
/// list), never an error, because the event server must never answer 500. `hint` is the
/// `?e=<event>` of the hook URL.
///
/// Has exactly the shape of `clawd_events::NormalizeFn`; the app passes it to the event server
/// as `Arc::new(clawd_agents::normalize)`.
pub fn normalize(agent_id: &str, payload: &Value, hint: &str) -> Option<Vec<Value>> {
    let agent = shared_registry().get(agent_id)?;
    Some(catch_unwind(AssertUnwindSafe(|| agent.normalize(payload, hint))).unwrap_or_default())
}
