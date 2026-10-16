//! The feed client against a fake Worker on loopback (`tests/test_feed.py`,
//! `tests/test_renderer_feed.py`): download + decrypt + verify, the encrypted cache, renderers.
mod common;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use clawd_firmware::crc32::crc32_hex;
use clawd_firmware::feed::{self, Feed, FeedFile, Release};
use clawd_testkit::fw::{clawd_pak, desk_pak, image, release_files, FakeFeed, TOKEN};
use clawd_testkit::{FakeHttpServer, FakeResponse};
use common::renderer_zip;
use serde_json::json;

fn client(ff: &FakeFeed, channel: &str, cache: Option<&std::path::Path>) -> Feed {
    Feed::new(
        &ff.url(),
        TOKEN,
        channel,
        cache.map(std::path::Path::to_path_buf),
    )
}

async fn latest(f: &Feed) -> feed::Manifest {
    f.latest().await.unwrap().expect("a release")
}

fn rel_of(m: &feed::Manifest) -> Release {
    feed::make_release(m, HashMap::new())
}

#[tokio::test]
async fn latest_and_fetch_decrypt_and_check() {
    let ff = FakeFeed::start();
    ff.publish(
        6,
        release_files(&["synth", "paper"], 1),
        "stable",
        "Màn chờ hiện IP",
        1,
        &[],
    );
    let f = client(&ff, "stable", None);
    let m = latest(&f).await;
    assert_eq!(
        (m.seq, m.notes.as_str(), m.file("fw").unwrap().size),
        (6, "Màn chờ hiện IP", image().len() as u64)
    );
    let rel = rel_of(&m);
    assert!(rel.styles.is_empty());
    assert_eq!(rel.missing(&["fw", "style"]), ["fw", "style"]);
    assert_eq!(
        rel.read().unwrap_err(),
        "the firmware is not downloaded yet"
    );
    let rel = f.fetch(&rel, &feed::STYLE_KINDS, None).await.unwrap();
    assert_eq!(
        rel.styles.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(),
        ["synth", "paper"]
    );
    assert!(rel.styles[1].read().is_ok());
    assert_eq!(ff.got("firmware.bin"), 0, "only what was asked for");
    let rel = f
        .fetch(&rel, &["fw", "clawdpak", "deskpak"], None)
        .await
        .unwrap();
    assert_eq!(rel.read().unwrap(), image());
    assert_eq!(
        rel.pack("clawdpak").unwrap(),
        (clawd_pak(), crc32_hex(&clawd_pak()))
    );
    assert_eq!(rel.pack("deskpak").unwrap().0, desk_pak());
    assert!(rel.missing(&feed::KINDS).is_empty());
    assert_eq!(rel.notes(), "Màn chờ hiện IP");
}

#[tokio::test]
async fn fetch_reports_progress_and_skips_what_is_in_memory() {
    let ff = FakeFeed::start();
    ff.publish_default(6);
    let f = client(&ff, "stable", None);
    let rel = rel_of(&latest(&f).await);
    let seen: Arc<Mutex<Vec<(String, u64, u64)>>> = Arc::default();
    let s = seen.clone();
    let cb = move |k: &str, d: u64, t: u64| s.lock().unwrap().push((k.to_owned(), d, t));
    let rel = f.fetch(&rel, &feed::STYLE_KINDS, Some(&cb)).await.unwrap();
    let seen = seen.lock().unwrap().clone();
    assert_eq!(seen.first().unwrap().1, 0);
    let total = seen[0].2;
    assert_eq!(seen.last().unwrap(), &(String::new(), total, total));
    assert!(seen.windows(2).all(|w| w[0].1 <= w[1].1), "monotonic");
    let grants = ff.grants();
    let again = f.fetch(&rel, &feed::STYLE_KINDS, None).await.unwrap();
    assert_eq!(
        (again, ff.grants()),
        (rel, grants),
        "nothing wanted: no grant asked"
    );
}

#[tokio::test]
async fn no_release_wrong_token_and_beta_channel() {
    let ff = FakeFeed::start();
    assert!(client(&ff, "stable", None)
        .latest()
        .await
        .unwrap()
        .is_none());
    ff.publish(7, release_files(&["synth"], 1), "beta", "", 1, &[]);
    assert!(client(&ff, "stable", None)
        .latest()
        .await
        .unwrap()
        .is_none());
    assert_eq!(
        client(&ff, "beta", None)
            .latest()
            .await
            .unwrap()
            .unwrap()
            .seq,
        7
    );
    assert_eq!(
        client(&ff, "nightly", None).channel(),
        "stable",
        "unknown channel -> stable"
    );
    let bad = Feed::new(&ff.url(), "nope", "stable", None);
    assert_eq!(
        bad.latest().await.unwrap_err().0,
        "feed: HTTP 401 for /v1/latest"
    );
}

#[tokio::test]
async fn a_bad_manifest_is_a_feed_error() {
    let server = FakeHttpServer::start().unwrap();
    server.route(
        "GET",
        "/v1/latest",
        FakeResponse::json(200, &json!({"v": 2})),
    );
    let f = Feed::new(&server.url(), TOKEN, "stable", None);
    assert_eq!(
        f.latest().await.unwrap_err().0,
        "feed: bad manifest (not a v1 manifest)"
    );
    let server = FakeHttpServer::start().unwrap();
    server.route("GET", "/v1/latest", FakeResponse::text(200, "<html>"));
    let f = Feed::new(&server.url(), TOKEN, "stable", None);
    assert!(f
        .latest()
        .await
        .unwrap_err()
        .0
        .starts_with("feed: bad manifest ("));
    let dead = Feed::new("http://127.0.0.1:1", TOKEN, "stable", None);
    assert!(dead.latest().await.unwrap_err().0.starts_with("feed: "));
}

#[tokio::test]
async fn a_tampered_file_or_a_wrong_hash_is_refused() {
    let ff = FakeFeed::start();
    ff.publish_default(6);
    ff.tamper("desk.pak");
    let f = client(&ff, "stable", None);
    let rel = rel_of(&latest(&f).await);
    let e = f.fetch(&rel, &["deskpak"], None).await.unwrap_err();
    assert!(e.0.contains("decrypt"), "{e}");
    ff.untamper();
    ff.edit_manifest(6, |m| m["files"][0]["sha256"] = json!("0".repeat(64)));
    let rel = rel_of(&latest(&f).await);
    let e = f.fetch(&rel, &["fw"], None).await.unwrap_err();
    assert_eq!(e.0, "firmware.bin: does not match the manifest");
}

#[tokio::test]
async fn the_first_failure_ends_the_fetch() {
    let ff = FakeFeed::start();
    ff.publish_default(6);
    ff.tamper("styles/synth.pak");
    let f = client(&ff, "stable", None);
    let rel = rel_of(&latest(&f).await);
    assert!(f.fetch(&rel, &feed::STYLE_KINDS, None).await.is_err());
}

#[tokio::test]
async fn release_without_styles_and_a_bad_style_index() {
    let ff = FakeFeed::start();
    ff.publish(6, release_files(&[], 0), "stable", "", 0, &[]);
    let f = client(&ff, "stable", None);
    let rel = f
        .fetch(&rel_of(&latest(&f).await), &feed::KINDS, None)
        .await
        .unwrap();
    assert!(rel.styles.is_empty());
    let bad: Vec<_> = release_files(&["synth", "paper"], 1)
        .into_iter()
        .map(|f| {
            if f.0 == "styles/index.json" {
                (f.0, f.1, b"{not json".to_vec())
            } else {
                f
            }
        })
        .collect();
    ff.publish(7, bad, "stable", "", 1, &[]);
    let rel = f
        .fetch(&rel_of(&latest(&f).await), &feed::KINDS, None)
        .await
        .unwrap();
    assert!(rel.styles.is_empty());
}

#[tokio::test]
async fn encrypted_files_are_cached_per_release_and_reused() {
    let t = tempfile::tempdir().unwrap();
    let ff = FakeFeed::start();
    ff.publish_default(6);
    let f = client(&ff, "stable", Some(t.path()));
    let m = latest(&f).await;
    let rel = f
        .fetch(&rel_of(&m), &feed::STYLE_KINDS, None)
        .await
        .unwrap();
    assert_eq!(ff.file_calls().len(), 3);
    let mut names: Vec<String> = std::fs::read_dir(t.path().join("6"))
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    assert_eq!(
        names,
        [
            "styles__index.json",
            "styles__paper.pak",
            "styles__synth.pak"
        ]
    );
    for n in &names {
        let b = std::fs::read(t.path().join("6").join(n)).unwrap();
        assert!(
            !b.windows(5).any(|w| w == b"synth") && !b.windows(5).any(|w| w == b"paper"),
            "{n} is still encrypted"
        );
    }
    let grants = ff.grants();
    let again = client(&ff, "stable", Some(t.path()));
    let rel2 = again
        .fetch(&rel_of(&m), &feed::STYLE_KINDS, None)
        .await
        .unwrap();
    assert_eq!(rel2.styles, rel.styles);
    assert_eq!(ff.file_calls().len(), 3, "only the key was asked for");
    assert_eq!(ff.grants(), grants + 1);
}

#[tokio::test]
async fn a_damaged_cache_file_is_downloaded_again_and_old_releases_are_dropped() {
    let t = tempfile::tempdir().unwrap();
    let ff = FakeFeed::start();
    ff.publish_default(6);
    let f = client(&ff, "stable", Some(t.path()));
    let m = latest(&f).await;
    f.fetch(&rel_of(&m), &feed::STYLE_KINDS, None)
        .await
        .unwrap();
    let victim = t.path().join("6").join("styles__synth.pak");
    let mut data = std::fs::read(&victim).unwrap();
    *data.last_mut().unwrap() ^= 1; // same size, wrong tag
    std::fs::write(&victim, data).unwrap();
    let before = ff.file_calls().len();
    let rel = f
        .fetch(&rel_of(&m), &feed::STYLE_KINDS, None)
        .await
        .unwrap();
    assert_eq!(
        &ff.file_calls()[before..],
        ["GET /v1/file/6/styles/synth.pak"]
    );
    assert_eq!(rel.styles.len(), 2);
    ff.publish_default(7);
    let g = client(&ff, "stable", Some(t.path()));
    let m7 = latest(&g).await;
    g.fetch(&rel_of(&m7), &feed::STYLE_KINDS, None)
        .await
        .unwrap();
    let left: Vec<String> = std::fs::read_dir(t.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    assert_eq!(left, ["7"]);
}

// ---- renderers ----------------------------------------------------------------------------

#[tokio::test]
async fn fetch_renderer_downloads_decrypts_and_checks() {
    let t = tempfile::tempdir().unwrap();
    let ff = FakeFeed::start();
    let z = renderer_zip("c000006", &[]);
    ff.publish(
        6,
        release_files(&["synth"], 1),
        "stable",
        "",
        1,
        &[("win-x64", z.clone()), ("mac", b"other".to_vec())],
    );
    let f = client(&ff, "stable", Some(t.path()));
    let m = latest(&f).await;
    assert_eq!(
        m.renderers
            .iter()
            .map(|r| r.platform.as_str())
            .collect::<Vec<_>>(),
        ["win-x64", "mac"]
    );
    assert_eq!(m.renderer("mac").unwrap().name, "renderer/mac.zip");
    assert!(m.files.iter().all(|f| f.kind != "renderer"));
    let rel = rel_of(&m);
    let (row, data): (FeedFile, Vec<u8>) =
        f.fetch_renderer(&rel, "win-x64").await.unwrap().unwrap();
    assert_eq!((data, row.platform.as_str()), (z.clone(), "win-x64"));
    assert_eq!(
        (ff.got("renderer/win-x64.zip"), ff.got("renderer/mac.zip")),
        (1, 0)
    );
    assert!(f.fetch_renderer(&rel, "linux-x64").await.unwrap().is_none());
    assert_eq!(
        f.fetch_renderer(&rel, "win-x64").await.unwrap().unwrap().1,
        z
    );
    assert_eq!(
        ff.got("renderer/win-x64.zip"),
        1,
        "from the encrypted cache"
    );
    ff.tamper("renderer/mac.zip");
    assert!(f.fetch_renderer(&rel, "mac").await.is_err());
    let bare = Release::from_bundle(rel.bundle.clone());
    assert!(f.fetch_renderer(&bare, "mac").await.unwrap().is_none());
}

#[tokio::test]
async fn a_worker_that_does_not_sign_renderers_is_a_feed_error() {
    let ff = FakeFeed::start();
    ff.sign_renderers(false);
    let plat = feed::renderer_platform();
    ff.publish(
        6,
        release_files(&["synth"], 1),
        "stable",
        "",
        1,
        &[(plat, renderer_zip("c1", &[]))],
    );
    let f = client(&ff, "stable", None);
    let rel = rel_of(&latest(&f).await);
    let e = f.fetch_renderer(&rel, plat).await.unwrap_err();
    assert!(e.0.contains("no URL"), "{e}");
}

#[test]
fn renderer_platform_names_the_running_system() {
    let p = feed::renderer_platform();
    assert!(feed::RENDERER_PLATFORMS.contains(&p));
    if cfg!(windows) {
        assert_eq!(p, "win-x64");
    }
    if cfg!(target_os = "macos") {
        assert_eq!(p, "mac");
    }
}
