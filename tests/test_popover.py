"""The menu-bar panel's numbers and the clawd.pak it borrows from the clock (no GUI)."""
from clawd_desktop import popover
from clawd_desktop.discovery import Clock
from clawd_desktop.preview import PakCache

NOW = 1_800_000_000.0
A = Clock(host="10.0.0.7", name="clawd")
PAK = b"CLWD" + bytes([1, 0, 1, 0]) + bytes(4)        # header only: enough to stand for a pack


def snap(**kw):
    base = {"clock": A, "last_ok": NOW - 5, "failing": False, "scanning": False, "token": True,
            "payload": {"s": 10, "sr": 105, "w": 41, "wr": 6965, "acct": "pro", "ok": True},
            "payload_at": NOW, "manual_config": False, "config_targets": []}
    base.update(kw)
    return base


def test_left_is_short():
    assert [popover.left(m) for m in (0, 45, 65, 105, 1439, 6965, -3)] == \
        ["0p", "45p", "1g05", "1g45", "23g59", "4n 20g", "0p"]


def test_connected_shows_two_bars_and_no_clock_name():
    v = popover.view(snap(), NOW)
    assert v["state"] == "ok" and v["label"] == "Đã kết nối"
    assert v["rows"] == [{"name": "Phiên", "pct": 10, "left": "1g45"},
                         {"name": "Tuần", "pct": 41, "left": "4n 20g"}]


def test_reset_counts_down_from_the_reading():
    v = popover.view(snap(payload_at=NOW - 300), NOW)      # read 5 minutes ago
    assert v["rows"][0]["left"] == "1g40"
    assert "số liệu" not in v["label"]


def test_old_reading_says_when():
    v = popover.view(snap(payload_at=NOW - 3600, last_ok=NOW - 3600), NOW)
    assert "· số liệu lúc " in v["label"]


def test_problem_replaces_the_label_but_keeps_the_bars():
    v = popover.view(snap(clock=None, last_ok=None), NOW)
    assert v["state"] == "error" and v["label"].startswith("Không thấy đồng hồ")
    assert len(v["rows"]) == 2


def test_no_usage_no_rows():
    assert popover.view(snap(payload=None, payload_at=None), NOW)["rows"] == []
    assert popover.view(snap(payload={"ok": False}), NOW)["rows"] == []


def test_percent_is_clamped_and_whole():
    v = popover.view(snap(payload={"s": 104.6, "sr": 10, "w": -2, "wr": 10, "ok": True}), NOW)
    assert [r["pct"] for r in v["rows"]] == [100, 0]


def test_enterprise_has_one_monthly_bar():
    v = popover.view(snap(payload={"s": 37, "sr": 9000, "w": 0, "wr": 0, "acct": "ent", "rd": "Nov 1", "ok": True}), NOW)
    assert v["rows"] == [{"name": "Tháng", "pct": 37, "left": "Nov 1"}]


class FakeApi:
    calls = 0

    def __init__(self, base, auth=None, heap=None, data=PAK):
        self.base, self.auth = base, auth
        self._heap = {"pak": 1} if heap is None else heap
        self._data = data

    def heap(self):
        FakeApi.calls += 1
        return self._heap

    def get_file(self, path, max_bytes):
        assert path == "/clawd.pak"
        return self._data

    def close(self):
        pass


class Keys:
    def get(self, key):
        return ("admin", "pw") if key == "clawd" else None


def test_pak_comes_from_the_cache_first(tmp_path, monkeypatch):
    path = tmp_path / "clawd.pak"
    PakCache(path).save(PAK, 1)
    monkeypatch.setattr(popover.pv, "pak_count", lambda d: 1)
    got = popover.load_pak("http://10.0.0.7", "clawd", api_factory=lambda *a, **k: 1 / 0, cache_path=path)
    assert got == PAK


def test_pak_is_read_off_the_clock_once_and_kept(tmp_path, monkeypatch):
    path = tmp_path / "clawd.pak"
    monkeypatch.setattr(popover.pv, "pak_count", lambda d: 1)
    monkeypatch.setattr(popover, "_pak_failed", 0.0)
    seen = {}

    def factory(base, auth=None):
        seen["auth"] = auth
        return FakeApi(base, auth)
    assert popover.load_pak("http://10.0.0.7", "clawd", api_factory=factory, keychain=Keys(),
                            cache_path=path, now=lambda: NOW) == PAK
    assert seen["auth"] == ("admin", "pw") and path.read_bytes() == PAK


def test_pak_failure_waits_before_trying_again(tmp_path, monkeypatch):
    monkeypatch.setattr(popover, "_pak_failed", 0.0)
    FakeApi.calls = 0

    def factory(base, auth=None):
        return FakeApi(base, auth, heap={})          # a clock without a pack
    def load(base, at):
        return popover.load_pak(base, "k", api_factory=factory, keychain=Keys(),
                                cache_path=tmp_path / "p.pak", now=lambda: at)
    assert load("http://10.0.0.7", NOW) is None
    assert load("http://10.0.0.7", NOW + 60) is None
    assert FakeApi.calls == 1
    assert load("", NOW + 9999) is None               # no clock, no try
