//! Ports of `tests/test_popover.py` (the view, `left`) and the `popover_msg` protocol.
mod common;

use clawd_app::engine::Status;
use clawd_app::popover::{
    base64, left, pak_retry_allowed, parse_message, view, PopoverMsg, PAK_RETRY_S,
};
use common::*;
use serde_json::json;

#[test]
fn left_is_short() {
    let got: Vec<String> = [0.0, 45.0, 65.0, 105.0, 1439.0, 6965.0, -3.0]
        .into_iter()
        .map(left)
        .collect();
    assert_eq!(got, ["0p", "45p", "1g05", "1g45", "23g59", "4n 20g", "0p"]);
}

#[test]
fn connected_shows_two_bars_and_no_clock_name() {
    let v = view(&popover_snap(), NOW);
    assert_eq!((v.state.as_str(), v.label.as_str()), ("ok", "Đã kết nối"));
    let rows: Vec<_> = v
        .rows
        .iter()
        .map(|r| (r.name.as_str(), r.pct, r.left.as_str()))
        .collect();
    assert_eq!(rows, [("Phiên", 10, "1g45"), ("Tuần", 41, "4n 20g")]);
}

#[test]
fn reset_counts_down_from_the_reading() {
    let v = view(
        &Status {
            payload_at: Some(NOW - 300.0),
            ..popover_snap()
        },
        NOW,
    );
    assert_eq!(v.rows[0].left, "1g40");
    assert!(!v.label.contains("số liệu"));
}

#[test]
fn old_reading_says_when() {
    let s = Status {
        payload_at: Some(NOW - 3600.0),
        last_ok: Some(NOW - 3600.0),
        ..popover_snap()
    };
    assert!(view(&s, NOW).label.contains("· số liệu lúc "));
}

#[test]
fn problem_replaces_the_label_but_keeps_the_bars() {
    let v = view(
        &Status {
            clock: None,
            last_ok: None,
            ..popover_snap()
        },
        NOW,
    );
    assert_eq!(v.state, "error");
    assert!(v.label.starts_with("Không thấy đồng hồ"));
    assert_eq!(v.rows.len(), 2);
}

#[test]
fn no_usage_no_rows() {
    let none = Status {
        payload: None,
        payload_at: None,
        ..popover_snap()
    };
    assert!(view(&none, NOW).rows.is_empty());
    let bad = Status {
        payload: Some(payload(json!({"ok": false}))),
        ..popover_snap()
    };
    assert!(view(&bad, NOW).rows.is_empty());
}

#[test]
fn percent_is_clamped_and_whole() {
    let s = Status {
        payload: Some(payload(
            json!({"s": 104.6, "sr": 10, "w": -2, "wr": 10, "ok": true}),
        )),
        ..popover_snap()
    };
    let pcts: Vec<_> = view(&s, NOW).rows.iter().map(|r| r.pct).collect();
    assert_eq!(pcts, [100, 0]);
    // Python's round() is banker's rounding.
    let s = Status {
        payload: Some(payload(json!({"s": 42.5, "w": 0.5, "ok": true}))),
        ..popover_snap()
    };
    let pcts: Vec<_> = view(&s, NOW).rows.iter().map(|r| r.pct).collect();
    assert_eq!(pcts, [42, 0]);
}

#[test]
fn enterprise_has_one_monthly_bar() {
    let s = Status {
        payload: Some(payload(
            json!({"s": 37, "sr": 9000, "w": 0, "wr": 0, "acct": "ent", "rd": "Nov 1", "ok": true}),
        )),
        ..popover_snap()
    };
    let v = view(&s, NOW);
    assert_eq!(v.rows.len(), 1);
    assert_eq!(
        (
            v.rows[0].name.as_str(),
            v.rows[0].pct,
            v.rows[0].left.as_str()
        ),
        ("Tháng", 37, "Nov 1")
    );
}

#[test]
fn the_view_serialises_as_the_page_expects() {
    let v = serde_json::to_value(view(&popover_snap(), NOW)).unwrap();
    assert_eq!(v["state"], "ok");
    assert_eq!(
        v["rows"][0],
        json!({"name": "Phiên", "pct": 10, "left": "1g45"})
    );
}

#[test]
fn page_messages_are_parsed_like_popover_py() {
    assert_eq!(
        parse_message(&json!({"cmd": "ready"})),
        Some(PopoverMsg::Ready)
    );
    assert_eq!(
        parse_message(&json!({"cmd": "settings"})),
        Some(PopoverMsg::Settings)
    );
    assert_eq!(
        parse_message(&json!({"cmd": "close"})),
        Some(PopoverMsg::Close)
    );
    assert_eq!(
        parse_message(&json!({"cmd": "size", "h": 92.7})),
        Some(PopoverMsg::Size(92.0))
    );
    assert_eq!(
        parse_message(&json!({"cmd": "size", "h": 40})),
        Some(PopoverMsg::Size(40.0))
    );
    assert_eq!(
        parse_message(&json!({"cmd": "size", "h": 400})),
        Some(PopoverMsg::Size(400.0))
    );
    for bad in [
        json!({"cmd": "size", "h": 39}),
        json!({"cmd": "size", "h": 401}),
        json!({"cmd": "size", "h": "90"}),
        json!({"cmd": "size"}),
        json!({"cmd": "nope"}),
        json!({"cmd": 3}),
        json!("ready"),
    ] {
        assert_eq!(parse_message(&bad), None, "{bad}");
    }
}

#[test]
fn base64_matches_rfc_4648() {
    for (raw, want) in [
        (&b""[..], ""),
        (b"f", "Zg=="),
        (b"fo", "Zm8="),
        (b"foo", "Zm9v"),
        (b"foob", "Zm9vYg=="),
        (b"fooba", "Zm9vYmE="),
        (b"foobar", "Zm9vYmFy"),
    ] {
        assert_eq!(base64(raw), want);
    }
    assert_eq!(base64(&[0xff, 0xfe, 0xfd]), "//79");
}

#[test]
fn pak_failure_waits_before_trying_again() {
    assert!(pak_retry_allowed(0.0, NOW));
    assert!(!pak_retry_allowed(NOW, NOW + PAK_RETRY_S - 1.0));
    assert!(pak_retry_allowed(NOW, NOW + PAK_RETRY_S));
}

// `test_popover.py` load_pak cases over the real PakCache and ClockApi, a fake clock device on
// loopback and a temp home. The key is empty on purpose: no keychain is read.

use std::sync::Mutex;

use clawd_app::popover::load_pak;
use clawd_firmware::preview::PakCache;
use clawd_testkit::clock::FakeClockDevice;
use clawd_testkit::{FakeResponse, TempConfig};

/// `load_pak` remembers a failure in one process-wide cell: these tests take turns.
static PAK_TESTS: Mutex<()> = Mutex::new(());

/// A valid `clawd.pak`: one animation, 8x8, two frames.
fn pak_bytes() -> Vec<u8> {
    let (w, h, frames) = (8usize, 8usize, 2usize);
    let fbytes = (w * h).div_ceil(2);
    let mut head = b"CLWD".to_vec();
    head.extend_from_slice(&[1, 0, 1, 0, 0, 0, 0, 0]);
    let frames_off = 12 + 72;
    let holds_off = frames_off + fbytes * frames;
    let mut desc = b"anim0".to_vec();
    desc.resize(16, 0);
    desc.extend_from_slice(&[w as u8, h as u8, 0, 0]);
    for n in [frames, 0, frames - 1, fbytes] {
        desc.extend_from_slice(&(n as u16).to_le_bytes());
    }
    desc.extend(vec![0u8; 32]);
    desc.extend_from_slice(&(frames_off as u32).to_le_bytes());
    desc.extend_from_slice(&(holds_off as u32).to_le_bytes());
    desc.extend_from_slice(&[0; 4]);
    let body: Vec<u8> = [vec![0x12u8; fbytes * frames], vec![100, 0, 100, 0]].concat();
    [head, desc, body].concat()
}

/// A clock whose heap says it holds a pak and serves `/clawd.pak`.
fn clock_with_pak(pak: &[u8]) -> FakeClockDevice {
    let device = FakeClockDevice::start(None);
    device.set_file("/clawd.pak", pak);
    device.override_path(
        "/api/heap",
        FakeResponse::json(200, &json!({"uptime": 900, "pak": 1, "fwSeq": 1})),
    );
    device
}

fn load(tc: &TempConfig, base: &str, now: f64) -> Option<Vec<u8>> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime")
        .block_on(load_pak(
            base,
            "",
            tc.os_names(),
            tc.paths().pak_file(),
            now,
        ))
}

#[test]
fn pak_comes_from_the_cache_first() {
    let _turn = PAK_TESTS.lock().unwrap_or_else(|e| e.into_inner());
    let tc = TempConfig::new();
    let device = clock_with_pak(&pak_bytes());
    std::fs::create_dir_all(tc.paths().config_dir()).unwrap();
    let cached = pak_bytes();
    PakCache::new(tc.paths().pak_file())
        .save(&cached, 1)
        .expect("seed the cache");
    assert_eq!(load(&tc, &device.url(), 3.0e9), Some(cached));
    assert!(
        device.server().requests().is_empty(),
        "the clock is not asked while the cache has the pack"
    );
    assert_eq!(
        load(&tc, "", 3.0e9),
        Some(pak_bytes()),
        "and no clock is needed at all"
    );
}

#[test]
fn pak_is_read_off_the_clock_once_and_kept() {
    let _turn = PAK_TESTS.lock().unwrap_or_else(|e| e.into_inner());
    let tc = TempConfig::new();
    let device = clock_with_pak(&pak_bytes());
    assert_eq!(
        load(&tc, "", 3.0e9),
        None,
        "no clock, no cache: nothing to draw"
    );
    assert_eq!(load(&tc, &device.url(), 3.0e9), Some(pak_bytes()));
    let asked = device.server().requests().len();
    assert!(asked > 0);
    assert!(
        PakCache::new(tc.paths().pak_file()).load().is_some(),
        "kept on disk"
    );
    assert_eq!(load(&tc, &device.url(), 3.0e9), Some(pak_bytes()));
    assert_eq!(
        device.server().requests().len(),
        asked,
        "the second read is the cache's"
    );
}

#[test]
fn pak_failure_waits_before_trying_again_end_to_end() {
    let _turn = PAK_TESTS.lock().unwrap_or_else(|e| e.into_inner());
    let tc = TempConfig::new();
    let device = FakeClockDevice::start(None);
    device.override_path("/api/heap", FakeResponse::status(500));
    let t0 = 1.0e9;
    assert_eq!(load(&tc, &device.url(), t0), None);
    let asked = device.server().requests().len();
    assert!(asked > 0, "the first attempt reached the clock");
    assert_eq!(load(&tc, &device.url(), t0 + 10.0), None);
    assert_eq!(
        device.server().requests().len(),
        asked,
        "no new attempt within PAK_RETRY_S"
    );
    // The clock is fixed and the wait is over: it is read again and kept.
    device.override_path(
        "/api/heap",
        FakeResponse::json(200, &json!({"uptime": 900, "pak": 1, "fwSeq": 1})),
    );
    device.set_file("/clawd.pak", &pak_bytes());
    assert_eq!(
        load(&tc, &device.url(), t0 + PAK_RETRY_S),
        Some(pak_bytes())
    );
}
