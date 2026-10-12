"""The settings window's clock client: diffs, digest login, retries, OTA upload."""
import json

import httpx
import pytest

from clawd_desktop import clockapi as ca

BASE = "http://10.0.0.5"
CHALLENGE = 'Digest realm="clawd", nonce="n0nce", qop="auth", algorithm=MD5, opaque="op"'


class FakeClock:
    """Answers like the firmware; `password` turns digest auth on."""

    def __init__(self, password: str | None = None):
        self.password = password
        self.calls: list[tuple[str, str, bool]] = []    # (method, path, signed)
        self.cfg = {"brightness": 45, "usage": {"screen": 0, "layout": 0},
                    "wifi": [{"ssid": "Home", "passSet": True}]}
        self.saved: list[dict] = []
        self.uptime = 900
        self.down = 0                                     # next N requests time out

    def transport(self):
        def handler(req: httpx.Request) -> httpx.Response:
            signed = req.headers.get("authorization", "").startswith("Digest ")
            self.calls.append((req.method, req.url.path, signed))
            if self.down:
                self.down -= 1
                raise httpx.ConnectTimeout("slow", request=req)
            p = req.url.path
            if p == "/api/heap":
                return httpx.Response(200, json={"uptime": self.uptime, "fwSeq": 1, "layouts": 3})
            if self.password is not None:
                h = req.headers.get("authorization", "")
                if not (h.startswith("Digest ") and 'username="admin"' in h):
                    return httpx.Response(401, headers={"WWW-Authenticate": CHALLENGE})
            if p == "/api/config" and req.method == "GET":
                return httpx.Response(200, json=self.cfg)
            if p == "/api/config" and req.method == "POST":
                body = json.loads(req.content)
                self.saved.append(body)
                return httpx.Response(200, json={"ok": True, "reboot": "wifi" in body})
            if p == "/update":
                assert b'name="firmware"' in req.content
                return httpx.Response(200, text="OK")
            if p == "/api/scan":
                return httpx.Response(200, json=[{"ssid": "Home", "rssi": -50, "enc": True}])
            if p in ("/api/reboot", "/api/factory", "/api/import"):
                return httpx.Response(200, json={"ok": True})
            if p == "/api/export":
                return httpx.Response(200, json={"wifi": [{"ssid": "Home", "pass": "x"}]})
            if p == "/api/fs/get" and req.url.params.get("path") == "/clawd.pak":
                return httpx.Response(200, content=b"CLWD" + bytes(96))
            return httpx.Response(404)
        return httpx.MockTransport(handler)


def api(fake, auth=None):
    return ca.ClockApi(BASE, auth=auth, transport=fake.transport(), sleep=lambda _s: None)


# ---- config_diff ---------------------------------------------------------------
def test_diff_sends_only_changed_keys_nested():
    old = {"brightness": 45, "rotation": 0, "usage": {"screen": 0, "layout": 0, "anim": 9}}
    new = {"brightness": 60, "rotation": 0, "usage": {"screen": 0, "layout": 1, "anim": 9}}
    assert ca.config_diff(old, new) == {"brightness": 60, "usage": {"layout": 1}}


def test_diff_sends_a_changed_list_whole_and_nothing_when_equal():
    old = {"wifi": [{"ssid": "A"}, {"ssid": "B"}]}
    new = {"wifi": [{"ssid": "B"}, {"ssid": "A"}]}
    assert ca.config_diff(old, new) == {"wifi": [{"ssid": "B"}, {"ssid": "A"}]}
    assert ca.config_diff(old, json.loads(json.dumps(old))) == {}


def test_diff_new_keys_go_and_missing_keys_stay():
    assert ca.config_diff({"a": 1, "b": 2}, {"a": 1, "c": 3}) == {"c": 3}
    assert ca.config_diff({"usage": 5}, {"usage": {"layout": 1}}) == {"usage": {"layout": 1}}


# ---- requests --------------------------------------------------------------------
def test_save_posts_the_diff_and_skips_an_empty_one():
    fake = FakeClock()
    c = api(fake)
    assert c.save({}) == {"ok": True, "reboot": False, "unchanged": True}
    assert fake.saved == []
    assert c.save({"brightness": 60})["ok"]
    assert fake.saved == [{"brightness": 60}]


def test_password_missing_is_auth_needed():
    fake = FakeClock(password="pw")
    with pytest.raises(ca.AuthNeeded):
        api(fake).config()


def test_digest_login_and_one_signed_upload():
    fake = FakeClock(password="pw")
    c = api(fake, auth=("admin", "pw"))
    c.upload_firmware(b"\xe9" * 1000)
    uploads = [x for x in fake.calls if x[1] == "/update"]
    assert uploads == [("POST", "/update", True)]       # signed the first time: sent once


def test_upload_size_guard():
    c = api(FakeClock())
    with pytest.raises(ca.ClockError):
        c.upload_firmware(b"")
    with pytest.raises(ca.ClockError):
        c.upload_firmware(b"x" * (ca.OTA_MAX + 1))


def test_get_retries_a_slow_clock_then_gives_up():
    fake = FakeClock()
    fake.down = 2
    assert api(fake).config()["brightness"] == 45      # 2 timeouts, 3rd try answers
    fake.down = 5
    with pytest.raises(ca.ClockError):
        api(fake).config()


def test_scan_export_restore_reboot():
    fake = FakeClock()
    c = api(fake)
    assert c.scan()[0]["ssid"] == "Home"
    assert c.export()["wifi"][0]["pass"] == "x"
    c.restore({"brightness": 1})
    c.reboot()
    c.factory_reset()
    assert [p for _m, p, _s in fake.calls][-3:] == ["/api/import", "/api/reboot", "/api/factory"]


def test_wait_back_needs_a_boot_after_the_upload():
    fake = FakeClock()
    t = [1000.0]
    c = ca.ClockApi(BASE, transport=fake.transport(),
                    sleep=lambda s: t.__setitem__(0, t[0] + s))
    fake.uptime = 5000                                   # still the old run: keeps waiting
    with pytest.raises(ca.ClockError):
        c.wait_back(999.0, deadline=10, clock=lambda: t[0])
    fake.uptime = 3                                      # booted 3 s ago
    ticks = []
    h = c.wait_back(t[0] - 10, deadline=10, clock=lambda: t[0], tick=lambda: ticks.append(1))
    assert h["uptime"] == 3 and ticks


def test_get_file_reads_one_file_signed_and_minds_its_size():
    fake = FakeClock(password="pw")
    a = api(fake, auth=("admin", "pw"))
    assert a.get_file("/clawd.pak", 1000) == b"CLWD" + bytes(96)
    assert fake.calls[-1] == ("GET", "/api/fs/get", True)
    with pytest.raises(ca.ClockError):
        a.get_file("/clawd.pak", 50)                       # bigger than allowed
    with pytest.raises(ca.ClockError):
        a.get_file("/nope", 1000)                          # 404
    with pytest.raises(ca.AuthNeeded):
        api(FakeClock(password="pw")).get_file("/clawd.pak", 1000)
