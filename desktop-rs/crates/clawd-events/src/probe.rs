use std::time::Duration;

use hyper::Method;

use crate::httpc;

/// `engine.daemon_running(port, timeout)`: another Clawd daemon (this app, or the old
/// LaunchAgent) owns the event port: `GET http://127.0.0.1:<port>/state` answers 200 with
/// `"desired"` in the body. Callers get `port` from [`crate::read_event_port`]; the default
/// timeout is 1 s. Used for the single-instance check (there is no mutex/plugin).
pub async fn daemon_running(port: u16, timeout: Duration) -> bool {
    let url = format!("http://127.0.0.1:{port}/state");
    match httpc::request(Method::GET, &url, None, timeout).await {
        Ok(reply) => reply.status == 200 && contains(&reply.body, b"desired"),
        Err(_) => false,
    }
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}
