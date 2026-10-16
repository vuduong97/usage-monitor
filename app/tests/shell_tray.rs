//! Ports of `tests/test_tray.py` (status text, `lost`) and the menu of `tray.py:192-208`.
mod common;

use clawd_app::engine::Status;
use clawd_app::tray::{
    id, local_hhmm, lost, menu_entries, status, tooltip, Dot, MenuEntry, TrayChange, TrayModel,
    TrayState, FRESH, TOOLTIP_MAX,
};
use clawd_contracts::{ClockSource, ClockTarget};
use common::*;
use serde_json::json;

#[test]
fn connected_with_usage_detail() {
    let s = Status {
        clock: Some(clock_a()),
        last_ok: Some(NOW - 10.0),
        payload: Some(payload(json!({"s": 42, "w": 17, "ok": true}))),
        payload_at: Some(NOW - 10.0),
        ..snap()
    };
    let (dot, head, detail) = status(&s, NOW);
    assert_eq!(dot, Dot::Ok);
    assert_eq!(head, "Đã kết nối · clawd");
    assert_eq!(detail, "Phiên 42% · Tuần 17%");
}

#[test]
fn codex_usage_says_so() {
    let s = Status {
        clock: Some(clock_a()),
        last_ok: Some(NOW),
        payload: Some(payload(json!({"s": 5, "w": 9, "ok": true, "src": "codex"}))),
        payload_at: Some(NOW),
        ..snap()
    };
    assert_eq!(status(&s, NOW).2, "Codex · Phiên 5% · Tuần 9%");
}

#[test]
fn old_usage_says_when() {
    let s = Status {
        clock: Some(clock_a()),
        last_ok: Some(NOW - 3600.0),
        payload: Some(payload(json!({"s": 1, "w": 2}))),
        payload_at: Some(NOW - 3600.0),
        ..snap()
    };
    let detail = status(&s, NOW).2;
    assert!(detail.starts_with("Phiên 1% · Tuần 2% · lúc "), "{detail}");
    assert_eq!(detail.rsplit(' ').next().unwrap(), local_hhmm(NOW - 3600.0));
}

#[test]
fn unnamed_clock_shows_its_address() {
    let s = Status {
        clock: Some(ClockTarget::new("10.0.0.9", ClockSource::Saved)),
        last_ok: Some(NOW),
        ..snap()
    };
    assert_eq!(status(&s, NOW).1, "Đã kết nối · 10.0.0.9");
}

#[test]
fn problems_come_first() {
    let dot_of = |s: Status| status(&s, NOW).0;
    let with_clock = || Status {
        clock: Some(clock_a()),
        ..snap()
    };
    assert_eq!(
        dot_of(Status {
            token: Some(false),
            last_ok: Some(NOW),
            ..with_clock()
        }),
        Dot::Error
    );
    let (_, head, _) = status(
        &Status {
            token: Some(false),
            ..Status::default()
        },
        NOW,
    );
    assert!(head.contains("claude login"));
    assert_eq!(dot_of(snap()), Dot::Error);
    assert_eq!(dot_of(with_clock()), Dot::Busy);
    assert_eq!(
        dot_of(Status {
            last_ok: Some(NOW - 3600.0),
            ..with_clock()
        }),
        Dot::Busy
    );
    assert_eq!(
        dot_of(Status {
            failing: true,
            ..with_clock()
        }),
        Dot::Error
    );
}

#[test]
fn expired_login_does_not_read_as_a_lost_clock() {
    let s = Status {
        token: Some(false),
        clock: Some(clock_a()),
        last_ok: Some(NOW),
        ..snap()
    };
    let (dot, head, _) = status(&s, NOW);
    assert_eq!(dot, Dot::Error);
    assert!(head.starts_with("Đã kết nối clawd · ") && head.contains("claude login"));
    let s = Status {
        token: Some(false),
        clock: Some(clock_a()),
        failing: true,
        ..snap()
    };
    let head = status(&s, NOW).1;
    assert!(!head.contains("Đã kết nối") && head.contains("claude login"));
}

#[test]
fn lost_clock_points_at_typing_its_ip() {
    let (dot, head, _) = status(&snap(), NOW);
    assert_eq!(dot, Dot::Error);
    assert!(head.contains("nhập IP") && head.contains("Nhập địa chỉ đồng hồ"));
    let s = Status {
        clock: Some(clock_a()),
        failing: true,
        ..snap()
    };
    let (dot, head, _) = status(&s, NOW);
    assert_eq!(dot, Dot::Error);
    assert!(head.contains("không trả lời") && head.contains("Nhập địa chỉ đồng hồ"));
}

#[test]
fn the_address_item_shows_while_no_push_gets_through() {
    let known = || Status {
        clock: Some(clock_a()),
        ..snap()
    };
    assert!(lost(&snap(), NOW));
    assert!(lost(&known(), NOW));
    assert!(lost(
        &Status {
            last_ok: Some(NOW - FRESH - 1.0),
            ..known()
        },
        NOW
    ));
    assert!(!lost(
        &Status {
            last_ok: Some(NOW - 10.0),
            ..known()
        },
        NOW
    ));
    assert!(!lost(
        &Status {
            manual_config: true,
            config_targets: vec!["http://1.2.3.4".into()],
            ..snap()
        },
        NOW
    ));
}

#[test]
fn manual_config_targets() {
    let s = Status {
        manual_config: true,
        config_targets: vec!["http://1.2.3.4/api/usage".into()],
        last_ok: Some(NOW - 5.0),
        ..snap()
    };
    let (dot, head, _) = status(&s, NOW);
    assert_eq!(dot, Dot::Ok);
    assert!(head.ends_with("http://1.2.3.4/api/usage"));
    let s = Status { last_ok: None, ..s };
    assert_eq!(status(&s, NOW).1, "Đang gửi tới · http://1.2.3.4/api/usage");
}

#[test]
fn no_data_payload_has_no_detail() {
    let s = Status {
        clock: Some(clock_a()),
        last_ok: Some(NOW),
        payload: Some(payload(json!({"ok": false}))),
        payload_at: Some(NOW),
        ..snap()
    };
    assert_eq!(status(&s, NOW).2, "");
}

#[test]
fn icons_are_three_distinct_pngs() {
    for dot in [Dot::Ok, Dot::Busy, Dot::Error] {
        assert_eq!(&dot.png()[..8], b"\x89PNG\r\n\x1a\n", "{}", dot.as_str());
    }
    assert_ne!(Dot::Ok.png(), Dot::Error.png());
    assert_ne!(Dot::Ok.png(), Dot::Busy.png());
}

fn texts(m: &TrayModel) -> Vec<String> {
    menu_entries(m)
        .into_iter()
        .map(|e| match e {
            MenuEntry::Item { text, .. } => text,
            MenuEntry::Separator => "-".to_owned(),
        })
        .collect()
}

#[test]
fn the_plain_menu_is_head_settings_quit() {
    let m = TrayModel {
        head: "Đang kết nối x…".into(),
        ..TrayModel::default()
    };
    assert_eq!(
        texts(&m),
        ["Đang kết nối x…", "-", "Cài đặt…", "-", "Thoát Clawd"]
    );
}

#[test]
fn the_full_menu_is_tray_py_192_to_208_in_order() {
    let m = TrayModel {
        head: "H".into(),
        detail: "D".into(),
        app_new: "0.3.0".into(),
        fw_new: 12,
        setup_left: vec!["clock".into()],
        new_agents: vec!["pi".into()],
        new_agent_names: "Pi, Codex".into(),
        lost: true,
    };
    assert_eq!(
        texts(&m),
        [
            "H",
            "D",
            "-",
            "● Khởi động lại để cập nhật Clawd 0.3.0",
            "● Có firmware mới (#12)",
            "● Hoàn tất thiết lập…",
            "● Tìm thấy Pi, Codex: nối với Clawd…",
            "Cài đặt…",
            "Nhập địa chỉ đồng hồ…",
            "-",
            "Thoát Clawd",
        ]
    );
    let items: Vec<(&str, bool)> = menu_entries(&m)
        .iter()
        .filter_map(|e| match e {
            MenuEntry::Item { id, enabled, .. } => Some((*id, *enabled)),
            MenuEntry::Separator => None,
        })
        .collect();
    assert_eq!(items[0], (id::HEAD, false));
    assert_eq!(items[1], (id::DETAIL, false));
    assert!(items[2..].iter().all(|(_, enabled)| *enabled));
    assert_eq!(items.last().map(|i| i.0), Some(id::QUIT));
}

#[test]
fn the_tooltip_is_cut_to_120_characters() {
    assert_eq!(tooltip("Đã kết nối", ""), "Clawd: Đã kết nối");
    assert_eq!(tooltip("a", "b"), "Clawd: a\nb");
    assert_eq!(tooltip(&"é".repeat(500), "").chars().count(), TOOLTIP_MAX);
}

#[test]
fn only_a_difference_is_redrawn() {
    let mut st = TrayState::new();
    let first = st.set_status(Dot::Ok, "h".into(), "d".into(), false);
    assert_eq!(
        first,
        TrayChange {
            icon: Some(Dot::Ok),
            tooltip: true,
            menu: true
        }
    );
    let same = st.set_status(Dot::Ok, "h".into(), "d".into(), false);
    assert!(same.is_empty());
    let dot_only = st.set_status(Dot::Error, "h".into(), "d".into(), false);
    assert_eq!(
        dot_only,
        TrayChange {
            icon: Some(Dot::Error),
            tooltip: false,
            menu: false
        }
    );
    let lost_flip = st.set_status(Dot::Error, "h".into(), "d".into(), true);
    assert_eq!(
        lost_flip,
        TrayChange {
            icon: None,
            tooltip: false,
            menu: true
        }
    );
    assert!(st.update(|m| m.fw_new = 3));
    assert!(!st.update(|m| m.fw_new = 3));
}

#[test]
fn local_time_is_a_valid_clock_reading_that_advances_with_the_epoch() {
    use clawd_app::tray::local_hhmmss;
    let parse = |t: f64| -> (u32, u32, u32) {
        let s = local_hhmmss(t);
        let p: Vec<u32> = s.split(':').map(|x| x.parse().unwrap()).collect();
        assert_eq!(p.len(), 3, "{s}");
        assert!(p[0] < 24 && p[1] < 60 && p[2] < 60, "{s}");
        (p[0], p[1], p[2])
    };
    let (_, m0, s0) = parse(NOW);
    let (_, m1, s1) = parse(NOW + 61.0);
    assert_eq!((m1 + 60 - m0) % 60, 1);
    assert_eq!(s1, (s0 + 1) % 60);
    // Summer and winter both convert (the zone rules of the date, not today's offset).
    parse(NOW - 180.0 * 86_400.0);
    assert_eq!(local_hhmm(NOW).len(), 5);
}
