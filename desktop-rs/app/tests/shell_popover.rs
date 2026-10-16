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
