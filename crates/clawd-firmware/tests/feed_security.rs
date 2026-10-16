//! Feed security rules (red team #15) and what a clock needs from a release
//! (`feed.plan/needed/offer`).
mod common;

use std::collections::HashMap;

use clawd_contracts::{AppPaths, Env};
use clawd_firmware::crc32::crc32_hex;
use clawd_firmware::feed::{self, Feed, Release};
use clawd_firmware::firmware::Bundle;
use clawd_testkit::fw::{clawd_pak, desk_pak, FakeFeed, TOKEN};
use clawd_testkit::{FakeHttpServer, FakeResponse};
use serde_json::{json, Value};

fn rel_of(m: &feed::Manifest) -> Release {
    feed::make_release(m, HashMap::new())
}

async fn manifest(ff: &FakeFeed) -> feed::Manifest {
    Feed::new(&ff.url(), TOKEN, "stable", None)
        .latest()
        .await
        .unwrap()
        .unwrap()
}

#[tokio::test]
async fn redirects_are_never_followed_and_the_token_never_leaves() {
    let target = FakeHttpServer::start().unwrap();
    target.route(
        "GET",
        "/v1/latest",
        FakeResponse::json(200, &json!({"v": 1})),
    );
    let origin = FakeHttpServer::start().unwrap();
    let location = format!("{}/v1/latest", target.url());
    origin.route(
        "GET",
        "/v1/latest",
        FakeResponse::status(302).with_header("Location", &location),
    );
    let f = Feed::new(&origin.url(), TOKEN, "stable", None);
    assert_eq!(
        f.latest().await.unwrap_err().0,
        "feed: HTTP 302 for /v1/latest"
    );
    assert!(
        target.requests().is_empty(),
        "the redirect target never saw the token"
    );
    origin.route(
        "POST",
        "/v1/grant",
        FakeResponse::status(307).with_header("Location", &location),
    );
    let ff = FakeFeed::start();
    ff.publish_default(6);
    let rel = rel_of(&manifest(&ff).await);
    let e = f.fetch(&rel, &["fw"], None).await.unwrap_err();
    assert_eq!(e.0, "feed: HTTP 307 for /v1/grant");
    assert!(target.requests().is_empty());
}

#[tokio::test]
async fn a_grant_may_only_name_relative_feed_paths() {
    let ff = FakeFeed::start();
    ff.publish_default(6);
    let m = manifest(&ff).await;
    let key = "A".repeat(43) + "=";
    let origin = FakeHttpServer::start().unwrap();
    origin.route(
        "POST",
        "/v1/grant",
        FakeResponse::json(
            200,
            &json!({"key": key, "urls": {"firmware.bin": "https://evil.example/fw"}}),
        ),
    );
    let f = Feed::new(&origin.url(), TOKEN, "stable", None);
    let e = f.fetch(&rel_of(&m), &["fw"], None).await.unwrap_err();
    assert_eq!(e.0, "feed: no URL for firmware.bin");
    let bad_grants = [
        json!({"key": "!!", "urls": {}}),
        json!({"key": key, "urls": []}),
        json!([1]),
        json!({"key": "AAAA", "urls": {}}),
    ];
    for body in bad_grants {
        let o = FakeHttpServer::start().unwrap();
        o.route("POST", "/v1/grant", FakeResponse::json(200, &body));
        let f = Feed::new(&o.url(), TOKEN, "stable", None);
        let e = f.fetch(&rel_of(&m), &["fw"], None).await.unwrap_err();
        assert_eq!(e.0, "feed: bad grant", "{body}");
    }
}

#[tokio::test]
async fn a_feed_url_that_is_not_https_or_loopback_never_gets_the_token() {
    let f = Feed::new("http://feed.example.com", TOKEN, "stable", None);
    let e = f.latest().await.unwrap_err();
    assert!(e.0.contains("https"), "{e}");
    for ok in [
        "https://fw.example.com/",
        "http://127.0.0.1:8787",
        "http://localhost:9",
        "http://[::1]:9",
    ] {
        assert!(feed::url_allowed(ok), "{ok}");
    }
    let bad = [
        "http://fw.example.com",
        "http://127.0.0.1.evil.com",
        "http://localhost.evil.com",
        "http://127.0.0.1@evil.com",
        "ftp://x",
        "https://",
        "feed",
        "",
    ];
    for b in bad {
        assert!(!feed::url_allowed(b), "{b}");
    }
}

fn env(pairs: &[(&str, &str)]) -> Env {
    Env::from_map(
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect(),
    )
}

#[test]
fn from_sources_takes_env_then_config_then_build() {
    let config: HashMap<&str, &str> = HashMap::from([
        ("feed_url", "https://cfg.example/"),
        ("feed_token", "cfg-token"),
        ("feed_channel", "beta"),
    ]);
    let lookup = |k: &str| config.get(k).map(|v| (*v).to_owned());
    let f = feed::from_sources(&Env::empty(), lookup, "https://build.example", "b", None).unwrap();
    assert_eq!((f.url(), f.channel()), ("https://cfg.example", "beta"));
    let e = env(&[
        ("CLAWD_FEED_URL", "https://env.example"),
        ("CLAWD_FEED_TOKEN", "t"),
        ("CLAWD_FEED_CHANNEL", "stable"),
    ]);
    let f = feed::from_sources(&e, lookup, "", "", None).unwrap();
    assert_eq!((f.url(), f.channel()), ("https://env.example", "stable"));
    let none = |_: &str| None;
    let f = feed::from_sources(&Env::empty(), none, "https://build.example", "b", None).unwrap();
    assert_eq!((f.url(), f.channel()), ("https://build.example", "stable"));
    assert!(feed::from_sources(&Env::empty(), none, "", "b", None).is_none());
    assert!(feed::from_sources(&Env::empty(), none, "https://x.example", "", None).is_none());
    let plain = env(&[
        ("CLAWD_FEED_URL", "http://feed.example.com"),
        ("CLAWD_FEED_TOKEN", "t"),
    ]);
    assert!(feed::from_sources(&plain, none, "", "", None).is_none());
    let local = env(&[
        ("CLAWD_FEED_URL", "http://127.0.0.1:8787"),
        ("CLAWD_FEED_TOKEN", "t"),
    ]);
    assert!(feed::from_sources(&local, none, "", "", None).is_some());
}

#[test]
fn from_settings_with_everything_in_the_environment_reads_no_config() {
    // the env is complete, so the config reader (phase 06) is never reached
    let t = tempfile::tempdir().unwrap();
    let paths = AppPaths::new(t.path().join("home"));
    let e = env(&[
        ("CLAWD_FEED_URL", "https://env.example"),
        ("CLAWD_FEED_TOKEN", "t"),
        ("CLAWD_FEED_CHANNEL", "beta"),
    ]);
    let f = feed::from_settings(&paths, &e).unwrap();
    assert_eq!((f.url(), f.channel()), ("https://env.example", "beta"));
}

fn heap(seq: u64, extra: Value) -> Value {
    let mut h = json!({"variant": "esp8266-clawd", "fwSeq": seq, "layouts": 1, "styleApi": 1,
                       "pak": 17, "desk": 31, "uptime": 100});
    for (k, v) in extra.as_object().unwrap() {
        h[k] = v.clone();
    }
    h
}

fn bundle(seq: u64) -> Bundle {
    Bundle {
        path: "x".into(),
        size: 1,
        sha256: String::new(),
        variant: "esp8266-clawd".into(),
        seq,
        layouts: 1,
        commit: String::new(),
        style_api: 0,
        styles: Vec::new(),
    }
}

#[tokio::test]
async fn plan_needed_and_offer() {
    let ff = FakeFeed::start();
    let m = feed::parse_manifest(&ff.publish_default(6)).unwrap();
    let rel = rel_of(&m);
    let (clawd, desk) = (crc32_hex(&clawd_pak()), crc32_hex(&desk_pak()));
    let old = heap(5, json!({}));
    let plan = |h: &Value| serde_json::to_string(&feed::plan(h, Some(&rel))).unwrap();
    assert_eq!(
        plan(&old),
        r#"{"fw":"need","clawdpak":"unknown","deskpak":"unknown"}"#
    );
    assert_eq!(
        feed::needed(&old, Some(&rel)),
        ["fw", "clawdpak", "deskpak"]
    );
    assert_eq!(feed::offer(&old, Some(&m), None), 6);
    let same = heap(6, json!({"pakCrc": clawd, "deskCrc": desk}));
    assert_eq!(plan(&same), r#"{"clawdpak":"ok","deskpak":"ok"}"#);
    assert_eq!(feed::offer(&same, Some(&m), None), 0);
    let diff = heap(6, json!({"pakCrc": clawd, "deskCrc": "deadbeef"}));
    assert_eq!(feed::needed(&diff, Some(&rel)), ["deskpak"]);
    assert!(feed::worth_it(&diff, Some(&rel)) && feed::offer(&diff, Some(&m), None) == 6);
    let busy = heap(6, json!({"pakCrc": "", "deskCrc": desk}));
    assert_eq!(feed::plan(&busy, Some(&rel))["clawdpak"], "wait");
    assert!(!feed::worth_it(&busy, Some(&rel)));
    let gone = heap(6, json!({"desk": 0, "pakCrc": clawd, "deskCrc": ""}));
    assert_eq!(feed::plan(&gone, Some(&rel))["deskpak"], "need");
    let stock = json!({"version": "stock"});
    assert!(feed::plan(&stock, Some(&rel)).is_empty());
    assert_eq!(feed::offer(&stock, Some(&m), None), 0);
    assert!(feed::plan(&old, None).is_empty() && feed::offer(&old, None, None) == 0);
    // a bundle as new as the feed's wins (a private build)
    assert_eq!(feed::offer(&old, Some(&m), Some(&bundle(6))), 6);
    assert_eq!(
        feed::offer(&heap(7, json!({})), Some(&m), Some(&bundle(7))),
        0
    );
}
