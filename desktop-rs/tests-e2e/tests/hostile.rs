//! Hostile requests against the loopback event server of the rig: DNS rebinding, browsers,
//! oversized and lying bodies, slow clients (red team #13). No loop runs: only the server.
use std::io::Write;
use std::time::{Duration, Instant};

use clawd_contracts::MAX_BODY;
use clawd_e2e::http::{self, connect, read_reply, Request};
use clawd_e2e::rig::{wait_until, Rig, RigOptions};
use clawd_events::server::Limits;
use serde_json::json;

fn rig() -> Rig {
    Rig::new(RigOptions::default())
}

#[test]
fn a_foreign_host_header_is_refused_but_localhost_is_not() {
    let rig = rig();
    let port = rig.port();
    for host in [
        "evil.example",
        "127.0.0.1",
        "127.0.0.1:1",
        "attacker.test:47913",
        "",
    ] {
        let reply =
            http::send(port, &Request::get(port, "/state").host(Some(host))).expect("answer");
        assert_eq!(reply.status, 403, "Host: {host:?}");
        assert_eq!(
            reply.json(),
            json!({"ok": false, "error": "host not allowed"})
        );
    }
    for host in [
        format!("localhost:{port}"),
        format!("LOCALHOST:{port}"),
        format!("127.0.0.1:{port}"),
    ] {
        let reply =
            http::send(port, &Request::get(port, "/state").host(Some(&host))).expect("answer");
        assert_eq!(reply.status, 200, "Host: {host}");
    }
    // a hook from a refused host changed nothing
    let reply = http::send(
        port,
        &Request::post(port, "/hook/claude?e=Stop", b"{}").host(Some("evil.example")),
    )
    .expect("answer");
    assert_eq!(reply.status, 403);
}

#[test]
fn a_browser_request_with_an_origin_is_refused() {
    let rig = rig();
    let port = rig.port();
    let reply = http::send(
        port,
        &Request::get(port, "/state").header("Origin", "https://evil.example"),
    )
    .expect("answer");
    assert_eq!(reply.status, 403);
    assert_eq!(reply.json()["error"], "browser requests refused");
}

#[test]
fn an_oversized_body_is_413_declared_chunked_or_lying() {
    let rig = rig();
    let port = rig.port();
    // declared: refused from the head alone
    let declared = http::send(
        port,
        &Request::post(port, "/hook/claude?e=Stop", b"{}").declared_length(MAX_BODY as u64 + 1),
    )
    .expect("answer");
    assert_eq!(declared.status, 413);
    // chunked: no length to look at, the cap applies while reading
    let big = vec![b' '; MAX_BODY + 1024];
    let chunked = http::send(
        port,
        &Request::post(port, "/hook/claude?e=Stop", &big).chunked(),
    )
    .expect("answer");
    assert_eq!(chunked.status, 413);
    // honest and large
    let honest =
        http::send(port, &Request::post(port, "/hook/claude?e=Stop", &big)).expect("answer");
    assert_eq!(honest.status, 413);
    // right at the cap is accepted as a body (and rejected as JSON, not as too large)
    let mut at_cap = b"{\"x\":\"".to_vec();
    at_cap.resize(MAX_BODY - 2, b'a');
    at_cap.extend_from_slice(b"\"}");
    assert_eq!(at_cap.len(), MAX_BODY);
    let ok =
        http::send(port, &Request::post(port, "/hook/claude?e=Stop", &at_cap)).expect("answer");
    assert_eq!(ok.status, 200);
    // the server still answers
    assert_eq!(http::get_state(port).expect("state").status, 200);
}

#[test]
fn bad_json_unknown_agents_and_unknown_routes_are_refused_politely() {
    let rig = rig();
    let port = rig.port();
    let bad = http::send(
        port,
        &Request::post(port, "/hook/claude?e=Stop", b"{not json"),
    )
    .expect("a");
    assert_eq!(
        (bad.status, bad.json()),
        (400, json!({"ok": false, "error": "bad json"}))
    );
    let unknown = http::post_hook(port, "nobody", "Stop", &json!({})).expect("a");
    assert_eq!(unknown.status, 404);
    let route = http::send(port, &Request::get(port, "/nope")).expect("a");
    assert_eq!(route.status, 404);
    let put = http::send(port, &Request::new(port, "PUT", "/state")).expect("a");
    assert_eq!(put.status, 404);
}

#[test]
fn slowloris_clients_do_not_starve_a_normal_one_and_are_cut_at_the_limit() {
    let limits = Limits {
        header_read_timeout: Duration::from_millis(700),
        connection_timeout: Duration::from_millis(1500),
        max_connections: 4,
    };
    let rig = Rig::new(RigOptions {
        limits: Some(limits),
        ..RigOptions::default()
    });
    let port = rig.port();

    // Three clients that send half a head and then nothing (the cap is 4).
    let mut slow = Vec::new();
    for _ in 0..3 {
        let mut stream = connect(port).expect("connect");
        stream
            .write_all(b"GET /state HTTP/1.1\r\nHost: 127.0.0.1:1")
            .expect("write");
        slow.push(stream);
    }
    // A normal client is still served at once (one slot is free).
    let started = Instant::now();
    assert_eq!(http::get_state(port).expect("state").status, 200);
    assert!(started.elapsed() < Duration::from_secs(2));

    // Every slow one is cut after the header timeout (a closed socket or an error answer),
    // well inside the overall connection limit plus slack.
    let cut_by = Instant::now();
    for mut stream in slow {
        let outcome = read_reply(&mut stream);
        let closed = match outcome {
            Err(_) => true,
            Ok(reply) => reply.status >= 400,
        };
        assert!(closed, "a half-sent head must not be served");
    }
    assert!(
        cut_by.elapsed() < Duration::from_secs(5),
        "slow clients lingered {:?}",
        cut_by.elapsed()
    );
}

#[test]
fn a_full_house_of_stalled_clients_frees_up_when_they_time_out() {
    let limits = Limits {
        header_read_timeout: Duration::from_millis(600),
        connection_timeout: Duration::from_millis(1200),
        max_connections: 2,
    };
    let rig = Rig::new(RigOptions {
        limits: Some(limits),
        ..RigOptions::default()
    });
    let port = rig.port();
    let _stalled: Vec<_> = (0..2)
        .map(|_| {
            let mut s = connect(port).expect("connect");
            s.write_all(b"POST /poke HTTP/1.1\r\n").expect("write");
            s
        })
        .collect();
    // The next client waits for a slot and is served once the stalled ones are cut.
    let started = Instant::now();
    let reply = http::get_state(port).expect("served after the stalled ones time out");
    assert_eq!(reply.status, 200);
    assert!(
        started.elapsed() < Duration::from_secs(8),
        "{:?}",
        started.elapsed()
    );
    assert!(wait_until(3.0, || http::get_state(port).is_ok()));
}

#[test]
fn a_connection_that_sends_the_head_but_never_the_body_is_cut() {
    let limits = Limits {
        header_read_timeout: Duration::from_millis(500),
        connection_timeout: Duration::from_millis(1200),
        max_connections: 8,
    };
    let rig = Rig::new(RigOptions {
        limits: Some(limits),
        ..RigOptions::default()
    });
    let port = rig.port();
    let mut stream = connect(port).expect("connect");
    stream
        .write_all(
            format!("POST /hook/claude?e=Stop HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Length: 100\r\n\r\n{{")
                .as_bytes(),
        )
        .expect("write");
    let started = Instant::now();
    let closed = match read_reply(&mut stream) {
        Err(_) => true,
        Ok(reply) => reply.status >= 400,
    };
    assert!(closed);
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "{:?}",
        started.elapsed()
    );
    assert_eq!(http::get_state(port).expect("state").status, 200);
}
