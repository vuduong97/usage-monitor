//! The real `reqwest` transport against the testkit's loopback server (no outside network):
//! redirects are never followed by the client itself, only by the checked hop loop.

mod common;

use std::sync::{Arc, Mutex};

use clawd_appupdate::Source;
use clawd_contracts::{SUMS_NAME, SUMS_SIG_NAME};
use clawd_testkit::{FakeHttpServer, FakeResponse};
use common::*;
use serde_json::json;

const PATH_LATEST: &str = "/repos/o/r/releases/latest";

/// A loopback "GitHub": the API, a `/dl/<name>` redirect to `/cdn/<name>` for every asset.
fn loopback_github(payload: &[u8]) -> (FakeHttpServer, TestKey, String) {
    loopback_github_with(payload, Arc::new(Mutex::new(None)))
}

/// Like [`loopback_github`]; while `hijack` holds a URL, the installer's `/dl` route redirects there.
fn loopback_github_with(
    payload: &[u8],
    hijack: Arc<Mutex<Option<String>>>,
) -> (FakeHttpServer, TestKey, String) {
    let server = FakeHttpServer::start().expect("server");
    let key = TestKey::new();
    let base = server.url();
    let sums = format!("{}  {EXE_NAME}\n", sha(payload));
    let asset = |name: &str, size: usize| json!({"name": name, "size": size, "browser_download_url": format!("{base}/dl/{name}")});
    let release = json!({
        "tag_name": "v0.2.0", "draft": false, "prerelease": false, "body": "notes",
        "html_url": format!("{base}/release"),
        "assets": [asset(EXE_NAME, payload.len()), asset(SUMS_NAME, 100), asset(SUMS_SIG_NAME, 100)],
    });
    server.route("GET", PATH_LATEST, FakeResponse::json(200, &release));
    let files: Vec<(String, Vec<u8>)> = vec![
        (EXE_NAME.to_owned(), payload.to_vec()),
        (SUMS_NAME.to_owned(), sums.clone().into_bytes()),
        (
            SUMS_SIG_NAME.to_owned(),
            key.sign(sums.as_bytes()).into_bytes(),
        ),
    ];
    for (name, body) in files {
        let hijack = Arc::clone(&hijack);
        let own = name.clone();
        server.route_fn(
            "GET",
            &format!("/dl/{name}"),
            Arc::new(move |_| {
                let target = match hijack.lock().unwrap().clone() {
                    Some(url) if own == EXE_NAME => url,
                    _ => format!("/cdn/{own}"),
                };
                FakeResponse::status(302).with_header("Location", &target)
            }),
        );
        server.route(
            "GET",
            &format!("/cdn/{name}"),
            FakeResponse::bytes(200, body),
        );
    }
    let url = server.url();
    (server, key, url)
}

#[tokio::test]
async fn the_real_client_walks_relative_redirects_and_downloads() {
    let payload = b"installer-bytes".repeat(5000);
    let (server, key, url) = loopback_github(&payload);
    let source = Source::new("o/r", &url)
        .unwrap()
        .with_public_key(&key.public());
    let rel = source
        .latest("0.1.0", Some("windows"), Some("x86_64"))
        .await
        .unwrap()
        .expect("a release");
    assert_eq!(rel.sha256, sha(&payload));

    let api = &server.requests_to(PATH_LATEST)[0];
    assert_eq!(api.header("accept"), Some("application/vnd.github+json"));
    assert!(api.header("user-agent").unwrap().starts_with("Clawd/"));

    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join(EXE_NAME);
    source.download(&rel, &dest, None).await.unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), payload);
    assert_eq!(names(dir.path()), vec![EXE_NAME.to_owned()]);
}

#[tokio::test]
async fn the_real_client_does_not_follow_a_redirect_to_a_foreign_host() {
    let payload = b"abc".to_vec();
    let hijack = Arc::new(Mutex::new(None));
    let (server, key, url) = loopback_github_with(&payload, Arc::clone(&hijack));
    let source = Source::new("o/r", &url)
        .unwrap()
        .with_public_key(&key.public());
    let rel = source
        .latest("0.1.0", Some("windows"), Some("x86_64"))
        .await
        .unwrap()
        .unwrap();
    for target in [
        "http://evil.example/Clawd.exe".to_owned(),
        "https://evil.example/Clawd.exe".to_owned(),
        format!("http://127.0.0.2:{}/cdn/{EXE_NAME}", server.addr().port()),
    ] {
        *hijack.lock().unwrap() = Some(target.clone());
        let dir = tempfile::tempdir().unwrap();
        let e = source
            .download(&rel, &dir.path().join("x.exe"), None)
            .await
            .unwrap_err();
        assert_eq!(e.0, "địa chỉ tải không đáng tin", "{target}");
        assert!(names(dir.path()).is_empty());
    }
    // the first hop was asked each time, the CDN path never
    assert_eq!(server.requests_to(&format!("/dl/{EXE_NAME}")).len(), 3);
    assert!(server.requests_to(&format!("/cdn/{EXE_NAME}")).is_empty());
}

#[tokio::test]
async fn an_offline_machine_and_a_dropped_connection_are_errors_not_panics() {
    let (server, key, url) = loopback_github(b"abc");
    server.route(
        "GET",
        "/repos/o/dropped/releases/latest",
        FakeResponse::dropped(),
    );
    let dropped = Source::new("o/dropped", &url)
        .unwrap()
        .with_public_key(&key.public());
    let e = dropped
        .latest("0.1.0", Some("windows"), Some("x86_64"))
        .await
        .unwrap_err();
    assert!(!e.0.is_empty());

    drop(server); // nothing listens on that port any more
    let offline = Source::new("o/r", &url)
        .unwrap()
        .with_public_key(&key.public());
    let e = offline
        .latest("0.1.0", Some("windows"), Some("x86_64"))
        .await
        .unwrap_err();
    assert!(
        e.0.starts_with("ConnectError") || e.0.starts_with("RequestError"),
        "{}",
        e.0
    );
}
