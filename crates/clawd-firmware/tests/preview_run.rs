//! Running the renderer program (`tests/test_preview.py`). The "renderer" is the
//! `fake_clawd_render` test binary, copied next to a `mode` file, so this runs on every OS.
mod common;

use std::time::Duration;

use clawd_contracts::{Env, UsagePayload};
use clawd_firmware::preview::{self, load_renderer, PreviewError, CLASSIC, FRAME_BYTES};
use common::fake_renderer_folder;
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

type ClipFrame = (u16, u16, u16, u16, u16, Vec<u8>);

fn clip_bytes(frames: &[ClipFrame]) -> Vec<u8> {
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

// ---- remote style packs ---------------------------------------------------------------------

#[tokio::test]
async fn a_remote_style_pack_is_drawn_by_the_bundled_renderer() {
    let t = tempfile::tempdir().unwrap();
    let d = fake_renderer_folder(t.path(), "frame", "c");
    let remote = t.path().join("cache").join("tetris.pak");
    std::fs::create_dir_all(remote.parent().unwrap()).unwrap();
    std::fs::write(&remote, clawd_testkit::fw::make_style_pack("tetris")).unwrap();
    let r = load(&d).with_styles([("tetris".to_owned(), remote.clone())]);
    r.draw("tetris", None).await.unwrap();
    let args = log(&d, "args.log");
    assert!(
        args.split_whitespace().last().unwrap() == remote.to_string_lossy(),
        "{args}"
    );
}
