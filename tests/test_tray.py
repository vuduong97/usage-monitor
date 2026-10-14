"""Tray status text, icons, autostart files (no GUI needed)."""
import plistlib

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
    assert s == "ok" and head == "Đã kết nối · clawd"
    assert detail == "Phiên 42% · Tuần 17%"


def test_old_usage_says_when():
    _, _, detail = tray.status(snap(clock=A, last_ok=NOW - 3600, payload={"s": 1, "w": 2},
                                    payload_at=NOW - 3600), NOW)
    assert detail.startswith("Phiên 1% · Tuần 2% · lúc ")


def test_unnamed_clock_shows_its_address():
    assert tray.status(snap(clock=Clock(host="10.0.0.9"), last_ok=NOW), NOW)[1] == "Đã kết nối · 10.0.0.9"


def test_problems_come_first():
    assert tray.status(snap(token=False, clock=A, last_ok=NOW), NOW)[0] == "error"
    assert "claude login" in tray.status(snap(token=False), NOW)[1]
    assert tray.status(snap(), NOW)[0] == "error"
    assert tray.status(snap(clock=A), NOW)[0] == "busy"
    assert tray.status(snap(clock=A, last_ok=NOW - 3600), NOW)[0] == "busy"
    assert tray.status(snap(clock=A, failing=True), NOW)[0] == "error"


def test_lost_clock_points_at_typing_its_ip():
    s, head, _ = tray.status(snap(), NOW)                      # nothing known: no scan to wait for
    assert s == "error" and "nhập IP" in head and "Nhập địa chỉ đồng hồ" in head
    s, head, _ = tray.status(snap(clock=A, failing=True), NOW)
    assert s == "error" and "không trả lời" in head and "Nhập địa chỉ đồng hồ" in head


def test_the_address_item_shows_while_no_push_gets_through():
    assert tray.lost(snap(), NOW)
    assert tray.lost(snap(clock=A), NOW)                       # known, but nothing got through yet
    assert tray.lost(snap(clock=A, last_ok=NOW - tray.FRESH - 1), NOW)
    assert not tray.lost(snap(clock=A, last_ok=NOW - 10), NOW)
    assert not tray.lost(snap(manual_config=True, config_targets=["http://1.2.3.4"]), NOW)


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
    px = icons.base_icon(32).getpixel((16, 16))
    assert isinstance(px, tuple) and px[3] == 255


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


# ---- the menu's address dialog and the settings window's buttons -------------------
class _NoBrowser:
    changed = 0

    def start(self):
        return True

    def stop(self):
        pass

    def names(self):
        return []

    def resolve(self, name):
        return None


class _Now:                                  # the Thread stand-in: run the dialog at once
    def __init__(self, target, daemon=None, name=None, args=()):
        self.target, self.args = target, args

    def start(self):
        self.target(*self.args)


def tray_with(tmp_path, monkeypatch, typed=None, saved=None):
    """A Tray (no GUI) with a real finder, and a dialog that answers `typed`."""
    from types import SimpleNamespace

    from clawd_desktop.discovery import ClockFinder, ClockStore
    store = ClockStore(tmp_path / "clock.json")
    if saved is not None:
        store.save(saved)
    finder = ClockFinder(store, browser=_NoBrowser())
    t = object.__new__(tray.Tray)
    t.engine = SimpleNamespace(snapshot=finder.describe, finder=finder)
    t.refresh = lambda: None
    asked, alerts = [], []
    monkeypatch.setattr(tray, "ask_text", lambda prompt, default="": (asked.append(prompt), typed)[1])
    monkeypatch.setattr(tray, "alert", alerts.append)
    monkeypatch.setattr(tray.threading, "Thread", _Now)
    return t, finder, asked, alerts


def test_menu_dialog_uses_the_typed_ip(tmp_path, monkeypatch):
    t, finder, asked, alerts = tray_with(tmp_path, monkeypatch, "10.97.40.207")
    t.manual()
    assert asked == [tray.ADDRESS_PROMPT] and alerts == []
    assert finder.targets() == ["http://10.97.40.207/api/usage"]


def test_menu_dialog_reports_a_bad_answer_and_ignores_cancel(tmp_path, monkeypatch):
    t, finder, _asked, alerts = tray_with(tmp_path, monkeypatch, "not a host:99999")
    t.manual()
    assert alerts == ["“not a host:99999” không phải địa chỉ hợp lệ."] and finder.targets() == []
    t, finder, _asked, alerts = tray_with(tmp_path, monkeypatch, None)
    t.manual()
    assert alerts == [] and finder.targets() == []


def test_nothing_pops_up_by_itself(tmp_path, monkeypatch):
    t, _finder, asked, _alerts = tray_with(tmp_path, monkeypatch, "10.97.40.207")
    assert not hasattr(t, "_ask_when_lost") and asked == []


def test_settings_window_retry_keeps_the_address(tmp_path, monkeypatch):
    import io
    import json
    t, finder, _asked, _alerts = tray_with(tmp_path, monkeypatch,
                                           saved={"manual_host": "10.97.40.207"})
    lines = [json.dumps({"cmd": "set_clock", "host": ""}).encode() + b"\n"]
    t._settings_cmds(type("P", (), {"stdout": io.BytesIO(b"".join(lines))})())
    assert finder.store.load()["host"] == "10.97.40.207" and "manual_host" not in finder.store.load()
    assert finder.targets() == ["http://10.97.40.207/api/usage"]
