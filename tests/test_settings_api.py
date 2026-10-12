"""The settings window bridge, without pywebview: input checks, diff saves,
login, layout gating and the update job."""
import hashlib
import json
import time

import pytest

from clawd_desktop import firmware as fw, settings_api as sa
from clawd_desktop.clockapi import AuthNeeded, ClockError
from clawd_desktop.discovery import ClockStore

CFG = {"hostname": "clawd", "wifi": [{"ssid": "Home", "passSet": True}, {"ssid": "Phone", "passSet": True}],
       "brightness": 45, "autoBrightness": False, "rotation": 0, "mode": "usage",
       "usage": {"usageUrl": "", "pollSec": 60, "screen": 0, "anim": 9, "layout": 0},
       "clock": {"tz": "", "tzPosix": "UTC0", "nightEnabled": False, "nightStart": "22:00", "nightEnd": "07:00",
                 "nightLevel": 0},
       "auth": {"enabled": False, "user": "admin", "passSet": False}, "features": {"usage": True}}
OLD_HEAP = {"version": "2.16.0", "pak": 17, "desk": 31, "uptime": 900}
NEW_HEAP = {"version": "2.16.0", "variant": "esp8266-clawd", "fwSeq": 1, "layouts": 3, "pak": 17, "desk": 31,
            "uptime": 5}


class FakeApi:
    def __init__(self, base, auth=None, heap=None, password=None):
        self.base, self.auth = base, auth
        self.heap_ = dict(heap or NEW_HEAP)
        self.cfg = json.loads(json.dumps(CFG))
        self.password = password
        self.saved = []
        self.uploads = 0

    def set_auth(self, auth):
        self.auth = auth

    def _check(self):
        if self.password and (not self.auth or self.auth[1] != self.password):
            raise AuthNeeded("pw")

    def heap(self):
        return self.heap_

    def config(self):
        self._check()
        return json.loads(json.dumps(self.cfg))

    def save(self, changes):
        self._check()
        self.saved.append(changes)
        for k, v in changes.items():
            if isinstance(v, dict):
                self.cfg.setdefault(k, {}).update(v)
            else:
                self.cfg[k] = v
        return {"ok": True, "reboot": "wifi" in changes or "hostname" in changes}

    def scan(self):
        return [{"ssid": "A", "rssi": -70, "enc": True}, {"ssid": "A", "rssi": -50, "enc": True},
                {"ssid": "", "rssi": -40}, {"ssid": "B", "rssi": -60, "enc": False}]

    def upload_firmware(self, image):
        self.uploads += 1

    def wait_back(self, started, clock=None, tick=None):
        self.heap_ = dict(NEW_HEAP)
        return self.heap_


class Keys:
    def __init__(self):
        self.d = {}

    def get(self, key):
        return self.d.get(key)

    def put(self, key, user, pw):
        self.d[key] = (user, pw)
        return True


def bridge(tmp_path, heap=None, password=None, bundle=None):
    made = []

    def factory(base, auth=None):
        a = FakeApi(base, auth, heap, password)
        made.append(a)
        return a
    keys = Keys()
    notes = []
    b = sa.SettingsApi("http://10.0.0.5", "clawd", notify=notes.append, api_factory=factory,
                       bundle=bundle, store=ClockStore(tmp_path / "clock.json"), keychain=keys)
    return b, made, keys, notes


def edited(**over):
    v = sa.initial_edit(CFG)
    for k, val in over.items():
        if isinstance(val, dict):
            v.setdefault(k, {}).update(val)
        else:
            v[k] = val
    return v


def test_clean_drops_unknown_keys_and_checks_values():
    assert sa.clean({"brightness": 50, "auth": {"enabled": True}, "features": {"x": 1}}) == {"brightness": 50}
    for bad in ({"brightness": 101}, {"brightness": True}, {"rotation": "1"}, {"hostname": "Bad Name"},
                {"clock": {"nightStart": "25:00"}}, {"usage": {"layout": 16}},
                {"wifi": []}, {"wifi": [{"ssid": "A", "pass": "short"}]}, {"wifi": [{"ssid": "A"}, {"ssid": "A"}]}):
        with pytest.raises(ValueError):
            sa.clean(bad)
    assert sa.clean({"wifi": [{"ssid": "A", "pass": ""}, {"ssid": "B", "pass": "12345678"}]}) == \
        {"wifi": [{"ssid": "A"}, {"ssid": "B", "pass": "12345678"}]}


def test_state_and_a_one_post_save(tmp_path):
    b, made, _k, _n = bridge(tmp_path)
    s = b.state()
    assert s["ok"] and s["fw"]["layouts"] == ["ready", "ready"] and s["app"]["clock"] == "http://10.0.0.5"
    assert b.preview(edited())["changes"] == {}
    r = b.save(edited(brightness=60, usage={"layout": 1}, auth={"enabled": True}))
    assert r["ok"] and r["sent"] == {"brightness": 60, "usage": {"layout": 1}} and not r["reboot"]
    assert made[0].saved == [{"brightness": 60, "usage": {"layout": 1}}]
    assert b.save(edited(brightness=60, usage={"layout": 1}))["sent"] == {}     # nothing left to send


def test_wifi_change_sends_the_whole_list_and_reboots(tmp_path):
    b, made, _k, _n = bridge(tmp_path)
    b.state()
    r = b.save(edited(wifi=[{"ssid": "Phone"}, {"ssid": "Home"}, {"ssid": "Cafe", "pass": "12345678"}]))
    assert r["reboot"] and made[0].saved[-1] == {"wifi": [{"ssid": "Phone"}, {"ssid": "Home"},
                                                          {"ssid": "Cafe", "pass": "12345678"}]}


def test_bad_input_is_refused_before_the_clock(tmp_path):
    b, made, _k, _n = bridge(tmp_path)
    b.state()
    r = b.save(edited(brightness=500))
    assert r["code"] == "bad" and made[0].saved == []


def test_a_layout_the_clock_cannot_draw_is_not_saved(tmp_path):
    b, made, _k, _n = bridge(tmp_path, heap=OLD_HEAP)
    s = b.state()
    assert s["fw"]["layouts"] == ["ready", "unavailable"]                 # no bundle in this test
    assert b.save(edited(usage={"layout": 1}))["code"] == "bad" and made[0].saved == []


def test_login_keeps_the_password_in_the_keychain(tmp_path):
    b, made, keys, _n = bridge(tmp_path, password="s3cret")
    assert b.state()["code"] == "auth"
    assert b.login("wrong")["code"] == "auth" and keys.d == {}
    r = b.login("s3cret")
    assert r["ok"] and r["saved"] and keys.d == {"clawd": ("admin", "s3cret")}
    assert "s3cret" not in json.dumps(r)                                   # never handed back to the page


def test_offline_and_noclock(tmp_path):
    b, made, _k, _n = bridge(tmp_path)

    def down():
        raise ClockError("no answer from http://10.0.0.5 (ConnectTimeout)")
    made[0].heap = down
    assert b.state()["code"] == "offline"
    nb = sa.SettingsApi("", "", api_factory=lambda base, auth=None: FakeApi(base),
                        store=ClockStore(tmp_path / "c.json"), keychain=Keys())
    assert nb.state()["code"] == "noclock"


def test_scan_keeps_the_strongest_of_each_name(tmp_path):
    b, _m, _k, _n = bridge(tmp_path)
    assert b.scan()["networks"] == [{"ssid": "A", "rssi": -50, "enc": True}, {"ssid": "B", "rssi": -60, "enc": False}]


def test_factory_reset_needs_the_word(tmp_path):
    b, _m, _k, _n = bridge(tmp_path)
    assert b.factory_reset("yes")["code"] == "bad"


def test_update_job_installs_then_saves_the_layout(tmp_path, monkeypatch):
    image = b"\xe9" + b"\0" * 99
    (tmp_path / "f.bin").write_bytes(image)
    (tmp_path / "firmware.json").write_text(json.dumps({"file": "f.bin", "size": len(image),
        "sha256": hashlib.sha256(image).hexdigest(), "variant": "esp8266-clawd", "fwSeq": 1, "layouts": 3}))
    bundle = fw.load_bundle(tmp_path)
    monkeypatch.setattr(sa.ota, "SETTLE", 0)
    b, made, _k, _n = bridge(tmp_path, heap=OLD_HEAP, bundle=bundle)
    s = b.state()
    assert s["fw"]["update"] == "available" and s["fw"]["layouts"] == ["ready", "update"]
    assert b.update(edited(usage={"layout": 1}), 1)["ok"]
    for _ in range(300):
        if not b.job()["running"]:
            break
        time.sleep(0.01)
    assert not (tmp_path / "pause.json").exists()             # the lease was let go
    j = b.job()
    assert j["done"] and not j["error"], j
    assert made[0].uploads == 1 and made[0].saved == [{"usage": {"layout": 1}}]
    assert b.state()["fw"]["update"] == "current"


def test_set_clock_by_hand_reconnects_and_tells_the_tray(tmp_path):
    b, made, _k, notes = bridge(tmp_path)
    r = b.set_clock("192.168.1.48")
    assert r["ok"] and made[-1].base == "http://192.168.1.48"
    assert notes == [{"cmd": "set_clock", "host": "192.168.1.48"}]
    assert b.set_clock("bad host:99999")["code"] == "bad"


def test_private_state_is_not_exposed_to_the_page(tmp_path):
    b, _m, _k, _n = bridge(tmp_path)
    public = [n for n in dir(b) if not n.startswith("_")]
    assert public == sorted(["state", "login", "preview", "save", "scan", "reboot", "factory_reset",
                             "export_backup", "import_backup", "update", "job", "busy", "set_autostart",
                             "set_hooks", "open_log", "open_web", "set_clock"])


def test_main_screen_is_handed_to_the_engine():
    seen = []
    api = FakeApi("http://c")
    s = sa.SettingsApi("http://c", "k", api_factory=lambda *a, **k: api, remember_screen=seen.append,
                       store=ClockStore(__import__("pathlib").Path("/nonexistent/x.json")))
    s.state()
    assert seen == [0]                                    # what the clock has now
    s.save({"usage": {"screen": 1}})
    assert seen[-1] == 1
