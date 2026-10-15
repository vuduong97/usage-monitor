use std::time::Duration;

/// `engine.daemon_running(port, timeout)`: another Clawd daemon (this app, or the old
/// LaunchAgent) owns the event port: `GET http://127.0.0.1:<port>/state` answers 200 with
/// `"desired"` in the body. Callers get `port` from [`crate::read_event_port`]; the default
/// timeout is 1 s. Used for the single-instance check (there is no mutex/plugin).
pub async fn daemon_running(port: u16, timeout: Duration) -> bool {
    todo!("phase 04")
}
