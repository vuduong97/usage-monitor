//! `EventServer` over real loopback sockets: routes (`test_event_server_routes`), the hardening
//! additions (Origin, Host, body cap incl. chunked, slow head, connection cap) and
//! `daemon_running`.

use std::sync::Arc;
use std::time::{Duration, Instant};

use clawd_contracts::{Clock, MAX_BODY};
use clawd_events::server::Limits;
use clawd_events::{daemon_running, Brain, EventServer, Pet, SharedBrain};
use clawd_testkit::{events::fake_normalize, FakeClock};
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

const T0: f64 = 1_760_000_000.0;

fn brain() -> SharedBrain {
    Brain::new(Pet::new(Default::default(), None), Box::new(|| false)).shared()
}

async fn server_with(limits: Limits) -> (EventServer, SharedBrain) {
    let brain = brain();
    let (normalize, _) = fake_normalize();
    let clock: Arc<dyn Clock> = FakeClock::new(T0).arc();
    let mut server = EventServer::new(Arc::clone(&brain), normalize)
        .with_port(0)
        .with_clock(clock)
        .with_limits(limits);
    server.start().await.expect("bind loopback");
    (server, brain)
}

async fn server() -> (EventServer, SharedBrain) {
    server_with(Limits::default()).await
}

struct Answer {
    status: u16,
    body: Value,
    raw: String,
}

/// Send `request` bytes, read the whole answer until the server closes.
async fn exchange(port: u16, request: &[u8]) -> Answer {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    stream.write_all(request).await.unwrap();
    let mut raw = Vec::new();
    let _ = tokio::time::timeout(Duration::from_secs(15), stream.read_to_end(&mut raw)).await;
    parse(&raw)
}

fn parse(raw: &[u8]) -> Answer {
    let raw = String::from_utf8_lossy(raw).into_owned();
    let status = raw
        .split(' ')
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let body = raw
        .split_once("\r\n\r\n")
        .map(|(_, b)| serde_json::from_str(b).unwrap_or(Value::Null))
        .unwrap_or(Value::Null);
    Answer { status, body, raw }
}

async fn request(port: u16, method: &str, path: &str, body: Option<&Value>, extra: &str) -> Answer {
    let data = body.map(|b| b.to_string()).unwrap_or_default();
    let head = format!(
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Length: {}\r\n{extra}\r\n",
        data.len()
    );
    exchange(port, format!("{head}{data}").as_bytes()).await
}

#[tokio::test]
async fn routes_match_test_event_server_routes() {
    let (mut server, brain) = server().await;
    let port = server.port();
    assert_ne!(port, 0);

    let hook = json!({"hook_event_name": "PreToolUse", "session_id": "z", "tool_name": "Grep",
                      "tool_response": "x".repeat(100_000)});
    let a = request(port, "POST", "/hook/claude", Some(&hook), "").await;
    assert_eq!(a.status, 200);
    assert_eq!(a.body, json!({"ok": true}));

    let a = request(port, "GET", "/state", None, "").await;
    assert_eq!(a.status, 200);
    assert_eq!(a.body["desired"]["a"], "reading");
    assert_eq!(a.body["sessions"][0]["sid"], "claude:z");
    assert_eq!(a.body["focus_left"], 0);
    assert_eq!(a.body["away"], false);

    let a = request(
        port,
        "POST",
        "/hook/pi",
        Some(&json!({"ev": "prompt", "sid": "p"})),
        "",
    )
    .await;
    assert_eq!(a.status, 200);
    assert!(brain.lock().unwrap().tracker.sessions.contains_key("pi:p"));

    let a = request(
        port,
        "POST",
        "/poke",
        Some(&json!({"cmd": "say", "text": "hi"})),
        "",
    )
    .await;
    assert_eq!((a.status, a.body["ok"].clone()), (200, json!(true)));
    let a = request(port, "POST", "/poke", Some(&json!({"cmd": "bogus"})), "").await;
    assert_eq!((a.status, a.body["ok"].clone()), (400, json!(false)));
    assert_eq!(a.body["error"], "unknown cmd 'bogus'");

    let a = request(port, "POST", "/nope", Some(&json!({})), "").await;
    assert_eq!(a.status, 404);
    let a = request(port, "GET", "/nope", None, "").await;
    assert_eq!(a.status, 404);
    let a = request(port, "DELETE", "/state", None, "").await;
    assert_eq!(a.status, 404);

    // unknown hook payloads are accepted and ignored (never fail a hook)
    let a = request(
        port,
        "POST",
        "/hook/claude",
        Some(&json!({"weird": true})),
        "",
    )
    .await;
    assert_eq!(a.status, 200);
    // an agent nobody knows
    let a = request(port, "POST", "/hook/nobody", Some(&json!({})), "").await;
    assert_eq!(a.status, 404);
    server.close().await;
}

#[tokio::test]
async fn responses_are_python_json_dumps_bytes() {
    let (mut server, _) = server().await;
    let a = request(server.port(), "POST", "/hook/claude", Some(&json!({})), "").await;
    assert!(a.raw.ends_with("\r\n\r\n{\"ok\": true}"), "{}", a.raw);
    assert!(a
        .raw
        .to_ascii_lowercase()
        .contains("content-type: application/json"));
    let a = request(server.port(), "GET", "/state", None, "").await;
    assert!(
        a.raw
            .contains("\"desired\": {\"a\": \"\", \"seq\": [], \"rep\": false"),
        "{}",
        a.raw
    );
    server.close().await;
}

#[tokio::test]
async fn hook_query_hint_reaches_normalize_and_bad_json_is_400() {
    let brain = brain();
    let (normalize, seen) = fake_normalize();
    let mut server = EventServer::new(brain, normalize).with_port(0);
    server.start().await.unwrap();
    let port = server.port();
    let a = request(
        port,
        "POST",
        "/hook/claude?e=Stop%20now&x=1",
        Some(&json!({})),
        "",
    )
    .await;
    assert_eq!(a.status, 200);
    let long = "y".repeat(100);
    request(
        port,
        "POST",
        &format!("/hook/pi?e={long}"),
        Some(&json!({})),
        "",
    )
    .await;
    {
        let seen = seen.lock().unwrap();
        assert_eq!(seen[0], ("claude".to_owned(), "Stop now".to_owned()));
        assert_eq!(seen[1].1.len(), 40);
    }
    let raw =
        format!("POST /poke HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Length: 5\r\n\r\n{{nope");
    let a = exchange(port, raw.as_bytes()).await;
    assert_eq!(
        (a.status, a.body["error"].clone()),
        (400, json!("bad json"))
    );
    // an empty body is `{}`: poke with no cmd is an unknown command, not a parse failure
    let a = request(port, "POST", "/poke", None, "").await;
    assert_eq!(a.status, 400);
    assert_eq!(a.body["error"], "unknown cmd None");
    // a JSON list as poke body is treated as `{}`
    let a = request(port, "POST", "/poke", Some(&json!([1, 2])), "").await;
    assert_eq!(a.status, 400);
    server.close().await;
}

#[tokio::test]
async fn a_hook_event_wakes_the_pusher_and_a_poke_too() {
    let (mut server, brain) = server().await;
    let wake = brain.lock().unwrap().wake.clone();
    let port = server.port();
    let hook = json!({"hook_event_name": "PreToolUse", "session_id": "w", "tool_name": "Bash"});
    request(port, "POST", "/hook/claude", Some(&hook), "").await;
    tokio::time::timeout(Duration::from_secs(1), wake.notified())
        .await
        .expect("hook wakes the brain");
    request(port, "POST", "/poke", Some(&json!({"cmd": "wave"})), "").await;
    tokio::time::timeout(Duration::from_secs(1), wake.notified())
        .await
        .expect("poke wakes the brain");
    // an ignored payload changes nothing and does not wake
    request(port, "POST", "/hook/claude", Some(&json!({"weird": 1})), "").await;
    assert!(
        tokio::time::timeout(Duration::from_millis(200), wake.notified())
            .await
            .is_err()
    );
    server.close().await;
}

#[tokio::test]
async fn origin_header_is_403_even_on_state() {
    let (mut server, _) = server().await;
    let port = server.port();
    let a = request(
        port,
        "POST",
        "/poke",
        Some(&json!({"cmd": "wave"})),
        "Origin: https://evil.example\r\n",
    )
    .await;
    assert_eq!(a.status, 403);
    let a = request(port, "GET", "/state", None, "Origin: null\r\n").await;
    assert_eq!(a.status, 403);
    server.close().await;
}

#[tokio::test]
async fn host_header_must_be_loopback_with_the_bound_port() {
    let (mut server, _) = server().await;
    let port = server.port();
    for host in [
        "evil.example",
        "evil.example:80",
        "127.0.0.1",
        "127.0.0.1:1",
        "localhost",
        "[::1]:1",
    ] {
        let raw = format!("GET /state HTTP/1.1\r\nHost: {host}\r\n\r\n");
        assert_eq!(exchange(port, raw.as_bytes()).await.status, 403, "{host}");
    }
    for host in [
        format!("127.0.0.1:{port}"),
        format!("localhost:{port}"),
        format!("LocalHost:{port}"),
    ] {
        let raw = format!("GET /state HTTP/1.1\r\nHost: {host}\r\n\r\n");
        assert_eq!(exchange(port, raw.as_bytes()).await.status, 200, "{host}");
    }
    // HTTP/1.0 without Host (old clients) is served
    assert_eq!(
        exchange(port, b"GET /state HTTP/1.0\r\n\r\n").await.status,
        200
    );
    server.close().await;
}

#[tokio::test]
async fn declared_oversized_body_is_413_without_reading_it() {
    let (mut server, _) = server().await;
    let port = server.port();
    let raw = format!(
        "POST /hook/claude HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Length: {}\r\n\r\n",
        MAX_BODY + 1
    );
    let started = Instant::now();
    assert_eq!(exchange(port, raw.as_bytes()).await.status, 413);
    assert!(started.elapsed() < Duration::from_secs(3));
    server.close().await;
}

#[tokio::test]
async fn body_exactly_at_the_cap_is_read_but_a_byte_more_is_413() {
    let (mut server, _) = server().await;
    let port = server.port();
    // valid JSON padded to the cap: {"pad":"xxxx..."}
    let overhead = r#"{"pad":""}"#.len();
    let body = format!(r#"{{"pad":"{}"}}"#, "x".repeat(MAX_BODY - overhead));
    assert_eq!(body.len(), MAX_BODY);
    let head = format!(
        "POST /hook/claude HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Length: {}\r\n\r\n",
        body.len()
    );
    let mut payload = head.into_bytes();
    payload.extend_from_slice(body.as_bytes());
    assert_eq!(exchange(port, &payload).await.status, 200);
    server.close().await;
}

#[tokio::test]
async fn chunked_body_over_the_cap_is_413() {
    let (mut server, _) = server().await;
    let port = server.port();
    let stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    let (mut read_half, mut write_half) = stream.into_split();
    // Read the answer while still sending: the server answers 413 and closes mid-upload, and a
    // client that only reads afterwards can lose the answer to the connection reset.
    let reader = tokio::spawn(async move {
        let mut raw = Vec::new();
        let _ =
            tokio::time::timeout(Duration::from_secs(10), read_half.read_to_end(&mut raw)).await;
        raw
    });
    let head = format!(
        "POST /hook/claude HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nTransfer-Encoding: chunked\r\n\r\n"
    );
    write_half.write_all(head.as_bytes()).await.unwrap();
    let chunk = vec![b'a'; 64 * 1024];
    let mut sent = 0;
    while sent <= MAX_BODY + 128 * 1024 {
        let frame = [format!("{:x}\r\n", chunk.len()).as_bytes(), &chunk, b"\r\n"].concat();
        if write_half.write_all(&frame).await.is_err() {
            break;
        }
        sent += chunk.len();
    }
    let _ = write_half.write_all(b"0\r\n\r\n").await;
    let answer = parse(&reader.await.unwrap());
    assert_eq!(answer.status, 413, "{}", answer.raw);
    assert!(
        sent > MAX_BODY / 2,
        "a large part of the body was streamed before the cut"
    );
    server.close().await;
}

#[tokio::test]
async fn slow_header_client_is_cut_within_the_limit() {
    let limits = Limits {
        header_read_timeout: Duration::from_millis(500),
        connection_timeout: Duration::from_secs(2),
        max_connections: 64,
    };
    let (mut server, _) = server_with(limits).await;
    let port = server.port();
    let mut stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    stream
        .write_all(b"GET /state HTTP/1.1\r\nHost: 127.0.0.1:1")
        .await
        .unwrap(); // never finishes
    let started = Instant::now();
    let mut raw = Vec::new();
    let finished = tokio::time::timeout(Duration::from_secs(5), stream.read_to_end(&mut raw)).await;
    assert!(finished.is_ok(), "the server must close the slow client");
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "{:?}",
        started.elapsed()
    );
    // and other clients are served meanwhile
    assert_eq!(request(port, "GET", "/state", None, "").await.status, 200);
    server.close().await;
}

#[tokio::test]
async fn slow_body_client_is_cut_by_the_connection_limit() {
    let limits = Limits {
        header_read_timeout: Duration::from_secs(5),
        connection_timeout: Duration::from_millis(700),
        max_connections: 64,
    };
    let (mut server, _) = server_with(limits).await;
    let port = server.port();
    let mut stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    let head =
        format!("POST /poke HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Length: 50\r\n\r\n{{");
    stream.write_all(head.as_bytes()).await.unwrap();
    let started = Instant::now();
    let mut raw = Vec::new();
    let finished = tokio::time::timeout(Duration::from_secs(5), stream.read_to_end(&mut raw)).await;
    assert!(finished.is_ok());
    assert!(started.elapsed() < Duration::from_secs(3));
    server.close().await;
}

#[tokio::test]
async fn hung_connections_over_the_cap_do_not_block_new_requests_past_the_timeout() {
    let limits = Limits {
        header_read_timeout: Duration::from_secs(1),
        connection_timeout: Duration::from_secs(1),
        max_connections: 8,
    };
    let (mut server, _) = server_with(limits).await;
    let port = server.port();
    let mut hung = Vec::new();
    for _ in 0..70 {
        let mut s = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        s.write_all(b"GET /state HTTP/1.1\r\n").await.unwrap(); // head never ends
        hung.push(s);
    }
    let started = Instant::now();
    let a = request(port, "GET", "/state", None, "").await;
    assert_eq!(a.status, 200);
    // the hung ones are cut after 1 s each, in waves of 8: bounded, not forever
    assert!(
        started.elapsed() < Duration::from_secs(15),
        "{:?}",
        started.elapsed()
    );
    drop(hung);
    server.close().await;
}

#[tokio::test]
async fn a_hundred_concurrent_requests_all_answer() {
    let (mut server, _) = server().await;
    let port = server.port();
    let hook = json!({"hook_event_name": "PreToolUse", "session_id": "c", "tool_name": "Bash"});
    let mut tasks = Vec::new();
    for i in 0..100 {
        let hook = hook.clone();
        tasks.push(tokio::spawn(async move {
            if i % 2 == 0 {
                request(port, "POST", "/hook/claude", Some(&hook), "")
                    .await
                    .status
            } else {
                request(port, "GET", "/state", None, "").await.status
            }
        }));
    }
    for task in tasks {
        assert_eq!(task.await.unwrap(), 200);
    }
    server.close().await;
}

#[tokio::test]
async fn port_zero_picks_a_free_port_and_a_taken_port_fails() {
    let (mut first, _) = server().await;
    let (normalize, _) = fake_normalize();
    let mut second = EventServer::new(brain(), normalize).with_port(first.port());
    assert!(second.start().await.is_err(), "the port is taken");
    first.close().await;
    // after close the port is free again
    let mut third = {
        let (normalize, _) = fake_normalize();
        EventServer::new(brain(), normalize).with_port(0)
    };
    third.start().await.unwrap();
    assert_ne!(third.port(), 0);
    third.close().await;
}

#[tokio::test]
async fn close_drops_open_connections_and_stops_listening() {
    let (mut server, _) = server().await;
    let port = server.port();
    let mut idle = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    idle.write_all(b"GET /state HTTP/1.1\r\n").await.unwrap();
    server.close().await;
    let mut buf = Vec::new();
    let read = tokio::time::timeout(Duration::from_secs(3), idle.read_to_end(&mut buf)).await;
    assert!(read.is_ok(), "the open connection is closed");
    assert!(TcpStream::connect(("127.0.0.1", port)).await.is_err());
}

#[tokio::test]
async fn daemon_running_sees_this_server_and_nothing_else() {
    let (mut server, _) = server().await;
    let port = server.port();
    assert!(daemon_running(port, Duration::from_secs(2)).await);
    server.close().await;
    assert!(!daemon_running(port, Duration::from_secs(1)).await);
    // something else on the port: 200 without "desired"
    let other = clawd_testkit::FakeHttpServer::start().unwrap();
    other.route(
        "GET",
        "/state",
        clawd_testkit::FakeResponse::text(200, "hello"),
    );
    assert!(!daemon_running(other.addr().port(), Duration::from_secs(1)).await);
}
