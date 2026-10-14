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


# ---- a search that found nothing asks for the address, once -----------------------
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
    def __init__(self, target, daemon=None):
        self.target = target

    def start(self):
        self.target()


def lost_tray(tmp_path, monkeypatch, typed):
    """A Tray (no GUI) whose finder has just given up, and a dialog that answers `typed`."""
    from types import SimpleNamespace

    from clawd_desktop.discovery import ClockFinder, ClockStore
    finder = ClockFinder(ClockStore(tmp_path / "clock.json"), browser=_NoBrowser(),
                         local_ip=lambda: "10.97.36.53")
    finder.needs_manual = True
    t = object.__new__(tray.Tray)
    t.engine = SimpleNamespace(snapshot=finder.describe, finder=finder)
    t.refresh = lambda: None
    asked, alerts = [], []
    monkeypatch.setattr(tray, "ask_text", lambda prompt, default="": (asked.append(prompt), typed)[1])
    monkeypatch.setattr(tray, "alert", alerts.append)
    monkeypatch.setattr(tray.threading, "Thread", _Now)
    return t, finder, asked, alerts


def test_lost_clock_asks_for_the_address_once(tmp_path, monkeypatch):
    t, finder, asked, _alerts = lost_tray(tmp_path, monkeypatch, "10.97.40.207")
    t._ask_when_lost()
    assert asked == [tray.LOST_PROMPT]
    assert finder.targets() == ["http://10.97.40.207/api/usage"]      # what was typed is used
    t._ask_when_lost()
    assert len(asked) == 1                                            # and nobody asks twice


def test_cancelled_prompt_is_not_repeated(tmp_path, monkeypatch):
    t, finder, asked, alerts = lost_tray(tmp_path, monkeypatch, None)
    t._ask_when_lost()
    assert finder.describe()["needs_manual"] is False       # asking clears the flag
    t._ask_when_lost()                                       # a cancelled answer is not asked again
    assert len(asked) == 1 and alerts == [] and finder.targets() == []
    assert finder._asked is True                             # the latch test_discovery checks per scan round


def test_unusable_answers_are_reported(tmp_path, monkeypatch):
    for typed in ("8.8.8.0/24", "10.0.0.0/8", "not a host:99999"):
        t, finder, _asked, alerts = lost_tray(tmp_path, monkeypatch, typed)
        t._ask_when_lost()
        assert alerts == [f"“{typed}” không phải địa chỉ hợp lệ."], typed
        assert finder.targets() == []


def test_typed_subnet_starts_a_search_there(tmp_path, monkeypatch):
    t, finder, _asked, alerts = lost_tray(tmp_path, monkeypatch, "10.97.40.0/24")
    t._ask_when_lost()
    assert alerts == [] and finder.targets() == []
    assert finder.store.load() == {"subnets": ["10.97.40.0/24"]}
