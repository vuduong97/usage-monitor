use std::time::Duration;

use hyper::Method;

use crate::httpc;

/// `engine.daemon_running(port, timeout)`: another Clawd daemon (this app, or the old
/// LaunchAgent) owns the event port: `GET http://127.0.0.1:<port>/state` answers 200 with
/// `"desired"` in the body. Callers get `port` from [`crate::read_event_port`]; the default
/// timeout is 1 s. Used for the single-instance check (there is no mutex/plugin).
pub async fn daemon_running(port: u16, timeout: Duration) -> bool {
    // A live daemon accepts a loopback connection at once (the kernel completes it from the
    // backlog even when the app is busy). A closed port is not always refused at once: on
    // Windows it can take about 2 s, which would hold the tray back for the whole `timeout`
    // on the normal start (no other daemon). So the HTTP probe only runs when something listens.
    let listening = tokio::time::timeout(
        timeout.min(LISTEN_CHECK),
        tokio::net::TcpStream::connect(("127.0.0.1", port)),
    )
    .await;
    if !matches!(listening, Ok(Ok(_))) {
        return false;
    }
    let url = format!("http://127.0.0.1:{port}/state");
    match httpc::request(Method::GET, &url, None, timeout).await {
        Ok(reply) => reply.status == 200 && contains(&reply.body, b"desired"),
        Err(_) => false,
    }
}

/// How long to wait for the TCP connect before concluding that nothing listens on the port.
const LISTEN_CHECK: Duration = Duration::from_millis(250);

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}
