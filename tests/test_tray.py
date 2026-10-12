"""Tray status text, icons, autostart files (no GUI needed)."""
import plistlib
import time

from clawd_desktop import autostart, icons, tray
from clawd_desktop.discovery import Clock

NOW = 1_800_000_000.0
A = Clock(host="10.0.0.7", name="clawd")


def snap(**kw):
    base = {"clock": None, "last_ok": None, "failing": False, "scanning": False, "token": True,
            "payload": None, "payload_at": None, "manual_config": False, "config_targets": []}
    base.update(kw)
    return base


def test_connected_with_usage_detail():
    s, head, detail = tray.status(snap(clock=A, last_ok=NOW - 10, payload={"s": 42, "w": 17, "ok": True},
                                       payload_at=NOW - 10), NOW)
    assert s == "ok" and head == "Đã kết nối: clawd (10.0.0.7)"
    assert detail.startswith("Cập nhật ") and detail.endswith("Phiên 42% · Tuần 17%")


def test_problems_come_first():
    assert tray.status(snap(token=False, clock=A, last_ok=NOW), NOW)[0] == "error"
    assert "claude login" in tray.status(snap(token=False), NOW)[1]
    assert tray.status(snap(scanning=True), NOW)[:2] == ("busy", "Đang dò tìm đồng hồ trong mạng…")
    assert tray.status(snap(), NOW)[0] == "error"
    assert tray.status(snap(clock=A), NOW)[0] == "busy"
    assert tray.status(snap(clock=A, last_ok=NOW - 3600), NOW)[0] == "busy"
    assert tray.status(snap(clock=A, failing=True), NOW)[0] == "error"


def test_blocked_lan_explains_the_permission():
    s, head, _ = tray.status(snap(lan_blocked=True), NOW)
    assert s == "error" and ("Mạng cục bộ" in head or "Tường lửa" in head)


def test_manual_config_targets():
    s, head, _ = tray.status(snap(manual_config=True, config_targets=["http://1.2.3.4/api/usage"],
                                  last_ok=NOW - 5), NOW)
    assert s == "ok" and head.endswith("http://1.2.3.4/api/usage")


def test_no_data_payload_has_no_detail():
    assert tray.status(snap(clock=A, last_ok=NOW, payload={"ok": False}, payload_at=NOW), NOW)[2] == ""


def test_icons_have_a_status_dot():
    ok, err = icons.status_icon("ok"), icons.status_icon("error")
    assert ok.size == (64, 64) and ok.mode == "RGBA"
    assert ok.getpixel((52, 52)) != err.getpixel((52, 52))
    assert icons.base_icon(32).getpixel((16, 16))[3] == 255


def test_launch_agent_restarts_only_after_a_crash():
    agent = autostart.launch_agent(["/Applications/Clawd.app/Contents/MacOS/Clawd"])
    assert agent["KeepAlive"] == {"SuccessfulExit": False} and agent["RunAtLoad"] is True
    plistlib.dumps(agent)


def test_mac_autostart_writes_and_removes_the_plist(tmp_path, monkeypatch):
    monkeypatch.setattr(autostart, "PLIST", tmp_path / "x.plist")
    assert not autostart.is_enabled("darwin")
    autostart.enable("darwin")
    assert autostart.is_enabled("darwin")
    assert plistlib.loads((tmp_path / "x.plist").read_bytes())["Label"] == autostart.LABEL
    autostart.disable("darwin")
    assert not autostart.is_enabled("darwin")
