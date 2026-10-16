//! Running the renderer program and the feed renderer store (`tests/test_preview.py`,
//! `tests/test_renderer_feed.py` window part). The "renderer" is the `fake_clawd_render` test
//! binary, copied next to a `mode` file, so this runs on every OS.
mod common;

use std::collections::HashMap;
use std::time::Duration;

use clawd_contracts::{Env, UsagePayload};
use clawd_firmware::feed::{self, Feed, Release};
use clawd_firmware::preview::{self, load_renderer, PreviewError, CLASSIC, FRAME_BYTES};
use clawd_firmware::renderer_store::load_feed_renderer;
use clawd_testkit::fw::{release_files, FakeFeed, TOKEN};
use common::{fake_renderer_folder, renderer_zip, zip_of};
use serde_json::json;

fn payload(v: serde_json::Value) -> UsagePayload {
    UsagePayload(v.as_object().unwrap().clone())
}

fn load(folder: &std::path::Path) -> preview::Renderer {
    load_renderer(&Env::empty(), Some(folder), "").expect("renderer")
}

fn log(folder: &std::path::Path, name: &str) -> String {
    std::fs::read_to_string(folder.join(name)).unwrap_or_default()
}

fn clip_bytes(frames: &[(u16, u16, u16, u16, u16, Vec<u8>)]) -> Vec<u8> {
    let mut out = b"CLP1".to_vec();
    out.extend_from_slice(&(frames.len() as u16).to_le_bytes());
    for (d, x, y, w, h, rgb) in frames {
        for n in [d, x, y, w, h] {
            out.extend_from_slice(&n.to_le_bytes());
        }
        out.extend_from_slice(rgb);
    }
    out
}

#[tokio::test]
async fn draw_returns_the_frame_and_passes_the_payload_on() {
    let t = tempfile::tempdir().unwrap();
    let d = fake_renderer_folder(t.path(), "frame", "44e244f");
    let r = load(&d);
    let p = payload(json!({"s": 5}));
    let frame = r.draw("synth", Some(&p)).await.unwrap();
    assert_eq!(frame.len(), FRAME_BYTES);
    let args = log(&d, "args.log");
    let words: Vec<&str> = args.split_whitespace().collect();
    assert_eq!(words[0], "--pak");
    assert_eq!(words[words.len() - 2], "--style");
    assert!(words[words.len() - 1]
        .replace('\\', "/")
        .ends_with("styles/synth.pak"));
    assert_eq!(
        log(&d, "stdin.log").trim(),
        r#"{"s": 5}"#,
        "json.dumps form"
    );
    r.draw(CLASSIC, None).await.unwrap();
    let second = log(&d, "args.log").lines().nth(1).unwrap().to_owned();
    assert!(!second.contains("--style"), "the classic screen: no pack");
    assert_eq!(
        log(&d, "stdin.log").lines().nth(1).unwrap_or(""),
        "",
        "nothing: the waiting screen"
    );
    assert!(matches!(
        r.draw("nope", None).await,
        Err(PreviewError::Invalid(_))
    ));
}

#[tokio::test]
async fn clip_asks_for_one_and_parses_it() {
    let t = tempfile::tempdir().unwrap();
    let d = fake_renderer_folder(t.path(), "clip", "44e244f");
    let full = (400u16, 0u16, 0u16, 240u16, 240u16, vec![1u8; FRAME_BYTES]);
    let spot = (120u16, 10u16, 20u16, 2u16, 1u16, vec![1u8, 2, 3, 4, 5, 6]);
    std::fs::write(d.join("clip.bin"), clip_bytes(&[full, spot])).unwrap();
    let r = load(&d);
    let frames = r
        .clip("paper", Some(&payload(json!({"s": 42}))))
        .await
        .unwrap();
    assert_eq!(frames.len(), 2);
    let args = log(&d, "args.log");
    let words: Vec<&str> = args.split_whitespace().collect();
    assert_eq!(*words.last().unwrap(), "--clip");
    let at = words.iter().position(|w| *w == "--style").unwrap();
    assert!(words[at + 1].replace('\\', "/").ends_with("paper.pak"));
}

#[tokio::test]
async fn a_broken_clip_or_frame_raises() {
    let t = tempfile::tempdir().unwrap();
    let p = payload(json!({"s": 42}));
    for mode in ["exit1", "nope"] {
        let d = fake_renderer_folder(&t.path().join(mode), mode, "c");
        assert!(
            load(&d).clip(CLASSIC, Some(&p)).await.is_err(),
            "clip {mode}"
        );
    }
    for mode in ["exit1", "short", "long"] {
        let d = fake_renderer_folder(&t.path().join(mode), mode, "c");
        let e = load(&d).draw(CLASSIC, Some(&p)).await.unwrap_err();
        assert!(matches!(e, PreviewError::Invalid(_)), "draw {mode}");
    }
}

#[tokio::test]
async fn a_payload_the_clock_would_reject_draws_the_waiting_screen() {
    let t = tempfile::tempdir().unwrap();
    let d = fake_renderer_folder(t.path(), "reject", "c");
    let frame = load(&d)
        .draw(CLASSIC, Some(&payload(json!({"foo": 1}))))
        .await
        .unwrap();
    assert!(
        frame.iter().all(|b| *b == 9),
        "the second run had no payload"
    );
    assert_eq!(
        log(&d, "args.log").lines().count(),
        2,
        "exit 2, then retried once"
    );
    // the clip path retries too; an empty-payload run that exits 2 is an error (no endless loop)
    let d2 = fake_renderer_folder(&t.path().join("clip"), "reject", "c");
    assert!(load(&d2)
        .clip(CLASSIC, Some(&payload(json!({"foo": 1}))))
        .await
        .is_err());
    assert_eq!(log(&d2, "args.log").lines().count(), 2);
}

#[tokio::test]
async fn a_renderer_that_hangs_is_killed_at_the_timeout() {
    let t = tempfile::tempdir().unwrap();
    let d = fake_renderer_folder(t.path(), "hang", "c");
    let start = std::time::Instant::now();
    let e = load(&d)
        .draw_timeout(CLASSIC, None, Duration::from_millis(400))
        .await
        .unwrap_err();
    assert!(
        matches!(e, PreviewError::Io(ref io) if io.kind() == std::io::ErrorKind::TimedOut),
        "{e:?}"
    );
    assert!(start.elapsed() < Duration::from_secs(10));
}

#[tokio::test]
async fn a_missing_program_is_an_io_error() {
    let t = tempfile::tempdir().unwrap();
    let d = fake_renderer_folder(t.path(), "frame", "c");
    let mut r = load(&d);
    r.exe = d.join("gone");
    assert!(matches!(
        r.draw(CLASSIC, None).await,
        Err(PreviewError::Io(_))
    ));
}

// ---- the feed renderer of a release ---------------------------------------------------------

fn rel_of(m: &feed::Manifest) -> Release {
    feed::make_release(m, HashMap::new())
}

async fn setup(ff: &FakeFeed, seq: u64, renderers: &[(&str, Vec<u8>)]) -> (Feed, Release) {
    ff.publish(
        seq,
        release_files(&["synth", "paper"], 1),
        "stable",
        "",
        1,
        renderers,
    );
    let f = Feed::new(&ff.url(), TOKEN, "stable", None);
    let m = f.latest().await.unwrap().unwrap();
    (f.clone(), rel_of(&m))
}

const OTHER: &str = if cfg!(target_os = "macos") {
    "win-x64"
} else {
    "mac"
};

#[tokio::test]
async fn the_window_previews_with_the_releases_renderer_and_reuses_it() {
    let t = tempfile::tempdir().unwrap();
    let root = t.path().join("renderer");
    let ff = FakeFeed::start();
    let plat = feed::renderer_platform();
    let (f, rel) = setup(&ff, 6, &[(plat, renderer_zip("c000006", &[]))]).await;
    let r = load_feed_renderer(&f, &rel, &root, plat)
        .await
        .expect("renderer");
    let mut ids: Vec<_> = r.styles.keys().cloned().collect();
    ids.sort();
    assert_eq!(ids, ["paper", "synth"]);
    assert_eq!(ff.got(&format!("renderer/{plat}.zip")), 1);
    let again = load_feed_renderer(&f, &rel, &root, plat)
        .await
        .expect("renderer");
    assert_eq!(again, r);
    assert_eq!(
        ff.got(&format!("renderer/{plat}.zip")),
        1,
        "unpacked already: no download"
    );
}

#[tokio::test]
async fn the_window_keeps_its_fallback_when_the_renderer_is_missing_or_unusable() {
    let t = tempfile::tempdir().unwrap();
    let root = t.path().join("renderer");
    let ff = FakeFeed::start();
    let plat = feed::renderer_platform();
    // none for this computer
    let (f, rel) = setup(&ff, 6, &[(OTHER, renderer_zip("c000006", &[]))]).await;
    assert!(load_feed_renderer(&f, &rel, &root, plat).await.is_none());
    // not a renderer (a stray dll)
    let (f, rel) = setup(
        &ff,
        7,
        &[(plat, renderer_zip("c000007", &[("other.dll", b"x")]))],
    )
    .await;
    assert!(load_feed_renderer(&f, &rel, &root, plat).await.is_none());
    assert!(!root.exists() || std::fs::read_dir(&root).unwrap().next().is_none());
    // built from another commit than the release
    let (f, rel) = setup(&ff, 8, &[(plat, renderer_zip("another", &[]))]).await;
    assert!(load_feed_renderer(&f, &rel, &root, plat).await.is_none());
    // a release from a Worker that does not sign renderers
    ff.sign_renderers(false);
    let (f, rel) = setup(&ff, 9, &[(plat, renderer_zip("c000009", &[]))]).await;
    assert!(load_feed_renderer(&f, &rel, &root, plat).await.is_none());
    assert_eq!(
        rel.styles.len(),
        0,
        "the release itself is untouched by the renderer"
    );
    // a "-dirty" firmware commit matches the renderer built from the clean one
    ff.sign_renderers(true);
    ff.publish(
        10,
        release_files(&["synth"], 1),
        "stable",
        "",
        1,
        &[(plat, renderer_zip("c000010", &[]))],
    );
    ff.edit_manifest(10, |m| m["commit"] = json!("c000010-dirty"));
    let m = f.latest().await.unwrap().unwrap();
    assert!(load_feed_renderer(&f, &rel_of(&m), &root, plat)
        .await
        .is_some());
}

#[tokio::test]
async fn a_changed_unpacked_file_is_unpacked_again() {
    let t = tempfile::tempdir().unwrap();
    let root = t.path().join("renderer");
    let ff = FakeFeed::start();
    let plat = feed::renderer_platform();
    let (f, rel) = setup(&ff, 6, &[(plat, renderer_zip("c000006", &[]))]).await;
    let r = load_feed_renderer(&f, &rel, &root, plat).await.unwrap();
    std::fs::write(&r.pak, b"changed").unwrap();
    let again = load_feed_renderer(&f, &rel, &root, plat).await.unwrap();
    assert_eq!(std::fs::read(&again.pak).unwrap(), b"pak");
    assert_eq!(ff.got(&format!("renderer/{plat}.zip")), 2);
}

#[tokio::test]
async fn zip_helper_builds_what_the_store_expects() {
    // guards the test helper itself: a zip with only a preview.json is not a renderer
    let t = tempfile::tempdir().unwrap();
    let bad = zip_of(&[(
        "preview.json",
        br#"{"exe":"clawd_render","pak":"clawd.pak"}"#,
    )]);
    assert!(preview::unpack_renderer(&bad, &t.path().join("r"), &"ab".repeat(32)).is_err());
}
