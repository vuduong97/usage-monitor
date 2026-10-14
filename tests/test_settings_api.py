"""The settings window bridge, without pywebview: input checks, diff saves,
login, style packs and the update job."""
import hashlib
import json
import time
import zlib

import pytest

from clawd_desktop import firmware as fw, settings_api as sa
from clawd_desktop.clockapi import AuthNeeded, ClockError
from clawd_desktop.discovery import ClockStore
from tests.test_firmware_ota import make_bundle

CFG = {"hostname": "clawd", "wifi": [{"ssid": "Home", "passSet": True}, {"ssid": "Phone", "passSet": True}],
       "brightness": 45, "autoBrightness": False, "rotation": 0, "mode": "usage",
       "usage": {"usageUrl": "", "pollSec": 60, "screen": 0, "anim": 9, "layout": 0},
       "clock": {"tz": "", "tzPosix": "UTC0", "nightEnabled": False, "nightStart": "22:00", "nightEnd": "07:00",
                 "nightLevel": 0},
       "auth": {"enabled": False, "user": "admin", "passSet": False}, "features": {"usage": True}}
OLD_HEAP = {"version": "2.16.0", "pak": 17, "desk": 31, "uptime": 900}
NEW_HEAP = {"version": "2.16.0", "variant": "esp8266-clawd", "fwSeq": 1, "layouts": 3, "pak": 17, "desk": 31,
            "uptime": 5}
STYLED_HEAP = {**NEW_HEAP, "fwSeq": 2, "layouts": 1, "styleApi": 1, "style": ""}


class FakeApi:
    def __init__(self, base, auth=None, heap=None, password=None):
        self.base, self.auth = base, auth
        self.heap_ = dict(heap or NEW_HEAP)
        self.cfg = json.loads(json.dumps(CFG))
        self.password = password
        self.saved = []
        self.uploads = 0
        self.styles = []                  # packs sent (their bytes), "" for a clear
        self.packs = []                   # clawd/desk packs sent (their kind)
        self.style_crc = ""               # what /api/style reports for the installed pack
        self.after = NEW_HEAP             # what the clock runs after an update
        self.status_ = {"mode": "sta", "connected": True, "ssid": "Home", "rssi": -58}

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

    def status(self):
        self._check()
        return dict(self.status_)

    def scan(self):
        return [{"ssid": "A", "rssi": -70, "enc": True}, {"ssid": "A", "rssi": -50, "enc": True},
                {"ssid": "", "rssi": -40}, {"ssid": "B", "rssi": -60, "enc": False}]

    def upload_firmware(self, image):
        self.uploads += 1

    def wait_back(self, started, clock=None, tick=None):
        self.heap_ = dict(self.after)
        return self.heap_

    def install_style(self, pack):
        self._check()
        self.styles.append(pack)
        sid = pack[16:32].rstrip(b"\0").decode()
        self.heap_["style"] = sid
        return {"api": 1, "active": True, "id": sid}

    def upload_pack(self, kind, data):
        self.packs.append(kind)
        self.heap_["pakCrc" if kind == "clawdpak" else "deskCrc"] = f"{zlib.crc32(data):08x}"
        self.heap_["pak" if kind == "clawdpak" else "desk"] = 17
        return {"ok": True, "anims": 1, "crc": f"{zlib.crc32(data):08x}"}

    def style(self):
        return {"api": 1, "active": bool(self.heap_.get("style")), "id": self.heap_.get("style", ""),
                "crc": self.style_crc}

    def clear_style(self):
        self._check()
        self.styles.append("")
        self.heap_["style"] = ""
        return {"api": 1, "active": False}


class Keys:
    def __init__(self):
        self.d = {}

    def get(self, key):
        return self.d.get(key)

    def put(self, key, user, pw):
        self.d[key] = (user, pw)
        return True


def bridge(tmp_path, heap=None, password=None, bundle=None, feed=None):
    made = []

    def factory(base, auth=None):
        a = FakeApi(base, auth, heap, password)
        made.append(a)
        return a
    keys = Keys()
    notes = []
    b = sa.SettingsApi("http://10.0.0.5", "clawd", notify=notes.append, api_factory=factory,
                       bundle=bundle, store=ClockStore(tmp_path / "clock.json"), keychain=keys, feed=feed)
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
    assert s["ok"] and s["fw"]["styles"] == {"state": "unavailable", "active": "classic", "packs": []}
    assert s["app"]["clock"] == "http://10.0.0.5"
    assert b.preview(edited())["changes"] == {}
    r = b.save(edited(brightness=60, auth={"enabled": True}))
    assert r["ok"] and r["sent"] == {"brightness": 60} and not r["reboot"] and not r["styleSent"]
    assert made[0].saved == [{"brightness": 60}]
    assert b.save(edited(brightness=60))["sent"] == {}     # nothing left to send


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


@pytest.fixture
def bundle(tmp_path):
    return make_bundle(tmp_path / "bundle", seq=2, style_api=1, styles=("synth", "paper"))


def test_a_style_the_clock_cannot_draw_is_not_sent(tmp_path, bundle):
    b, made, _k, _n = bridge(tmp_path, heap=NEW_HEAP, bundle=bundle)
    s = b.state()
    assert s["fw"]["styles"]["state"] == "update" and [p["id"] for p in s["fw"]["styles"]["packs"]] == ["synth", "paper"]
    assert b.save(edited(), "synth")["code"] == "bad" and made[0].styles == []
    assert b.save(edited(), "classic")["ok"] and made[0].styles == []    # already classic: nothing to send


def test_a_style_is_sent_first_then_the_settings(tmp_path, bundle):
    b, made, _k, _n = bridge(tmp_path, heap=STYLED_HEAP, bundle=bundle)
    assert b.state()["fw"]["styles"]["state"] == "ready"
    r = b.save(edited(wifi=[{"ssid": "Home"}]), "synth")             # reboots: the pack is kept on the clock
    assert r["ok"] and r["styleSent"] and r["reboot"] and r["fw"]["styles"]["active"] == "synth"
    assert made[0].styles == [bundle.styles[0].read()]
    assert b.save(edited(), "synth")["styleSent"] is False             # it shows that one already
    assert b.save(edited(), "classic")["ok"] and made[0].styles[-1] == ""
    for bad in ("nope", 3, ["synth"]):
        assert b.save(edited(), bad)["code"] == "bad"


def test_a_damaged_bundled_pack_is_never_sent(tmp_path, bundle):
    b, made, _k, _n = bridge(tmp_path, heap=STYLED_HEAP, bundle=bundle)
    b.state()
    p = bundle.styles[1].path
    p.write_bytes(p.read_bytes()[:-1] + b"!")
    assert b.save(edited(), "paper")["code"] == "error" and made[0].styles == []


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


def test_state_names_the_network_in_use_and_none_when_it_cannot(tmp_path):
    b, made, _k, _n = bridge(tmp_path)
    assert b.state()["net"] == {"ssid": "Home", "rssi": -58}
    made[0].status_ = {"mode": "ap", "connected": False, "ssid": "", "rssi": 0}
    assert b.state()["net"] is None

    def down():
        raise ClockError("no answer")
    made[0].status = down
    s = b.state()
    assert s["ok"] and s["net"] is None      # decoration only: the page still loads


def test_scan_keeps_the_strongest_of_each_name(tmp_path):
    b, _m, _k, _n = bridge(tmp_path)
    assert b.scan()["networks"] == [{"ssid": "A", "rssi": -50, "enc": True}, {"ssid": "B", "rssi": -60, "enc": False}]


def test_factory_reset_needs_the_word(tmp_path):
    b, _m, _k, _n = bridge(tmp_path)
    assert b.factory_reset("yes")["code"] == "bad"


def test_update_job_installs_then_saves_and_shows_the_style(tmp_path, monkeypatch, bundle):
    monkeypatch.setattr(sa.ota, "SETTLE", 0)
    b, made, _k, _n = bridge(tmp_path, heap=OLD_HEAP, bundle=bundle)
    made[0].after = STYLED_HEAP
    s = b.state()
    assert s["fw"]["update"] == "available" and s["fw"]["styles"]["state"] == "update"
    assert b.update(edited(brightness=30), 3)["code"] == "bad"
    assert b.update(edited(brightness=30), "paper")["ok"]
    for _ in range(300):
        if not b.job()["running"]:
            break
        time.sleep(0.01)
    assert not (tmp_path / "pause.json").exists()             # the lease was let go
    j = b.job()
    assert j["done"] and not j["error"], j
    assert made[0].uploads == 1 and made[0].saved == [{"brightness": 30}]
    assert made[0].styles == [bundle.styles[1].read()]
    st = b.state()
    assert st["fw"]["update"] == "current" and st["fw"]["styles"] == {
        "state": "ready", "active": "paper",
        "packs": [{"id": "synth", "name": "Synth", "light": False}, {"id": "paper", "name": "Paper", "light": True}]}


def test_update_job_fails_when_the_new_image_draws_no_styles(tmp_path, monkeypatch, bundle):
    monkeypatch.setattr(sa.ota, "SETTLE", 0)
    b, made, _k, _n = bridge(tmp_path, heap=OLD_HEAP, bundle=bundle)
    made[0].after = {**NEW_HEAP, "fwSeq": 2}
    b.state()
    assert b.update(None, "synth")["ok"]
    for _ in range(300):
        if not b.job()["running"]:
            break
        time.sleep(0.01)
    assert b.job()["error"] and made[0].styles == []


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
                             "set_hooks", "open_log", "open_web", "set_clock", "usage_preview",
                             "clawd_pak", "check_update", "fw"])


def test_main_screen_is_handed_to_the_engine():
    seen = []
    api = FakeApi("http://c")
    s = sa.SettingsApi("http://c", "k", api_factory=lambda *a, **k: api, remember_screen=seen.append,
                       store=ClockStore(__import__("pathlib").Path("/nonexistent/x.json")))
    s.state()
    assert seen == [0]                                    # what the clock has now
    s.save({"usage": {"screen": 1}})
    assert seen[-1] == 1


def test_a_clock_on_the_removed_clawd_screen_is_moved_to_auto_on_save(tmp_path):
    """The page no longer offers the Clawd main screen (2): it shows Auto for such a clock,
    and that is one real change, so Save writes 0 (and only then; opening writes nothing)."""
    b, made, _k, _n = bridge(tmp_path)
    made[0].cfg["usage"]["screen"] = 2
    b.state()
    assert made[0].saved == []                                    # opening the window changed nothing
    view = sa._clock_view(b._cfg)
    assert view["usage"]["screen"] == 2                           # the page compares against the real value
    r = b.save(edited(usage={"screen": 0}))
    assert r["ok"] and r["sent"] == {"usage": {"screen": 0}}
    assert made[0].saved == [{"usage": {"screen": 0}}]


def test_window_follows_the_clock_the_tray_found_at_a_new_address(tmp_path):
    # The window opened on 10.0.0.5; the clock then moved (another network / new DHCP lease)
    # and the tray's scan saved its new address in clock.json. "Thử lại" must reach it there.
    b, made, keys, _n = bridge(tmp_path)
    keys.d["clawd"] = ("admin", "pw")

    def down():
        raise ClockError("no answer from http://10.0.0.5 (ConnectTimeout)")
    made[0].heap = down
    assert b.state()["code"] == "offline"                      # nothing better known yet
    ClockStore(tmp_path / "clock.json").save({"name": "", "host": "192.168.1.48", "port": 80})
    s = b.state()
    assert s["ok"] and s["app"]["clock"] == "http://192.168.1.48"
    assert made[-1].base == "http://192.168.1.48" and made[-1].auth == ("admin", "pw")   # same clock, same login
    assert s["app"]["key"] == "clawd"


def test_window_follows_a_typed_address_and_ignores_the_same_one(tmp_path):
    b, made, _k, _n = bridge(tmp_path)

    def down():
        raise ClockError("no answer from http://10.0.0.5 (ConnectTimeout)")
    made[0].heap = down
    ClockStore(tmp_path / "clock.json").save({"host": "10.0.0.5", "port": 80})
    assert b.state()["code"] == "offline" and len(made) == 1  # the store says the same: no reconnect
    ClockStore(tmp_path / "clock.json").save({"manual_host": "192.168.1.9:8080"})
    assert b.state()["app"]["clock"] == "http://192.168.1.9:8080"


def test_window_asks_the_trays_newer_address_first(tmp_path):
    # Waiting out the old address costs ~60 s (3 tries x 20 s on a real network): when the tray
    # already saved another address, go there without asking the old one at all.
    b, made, _k, _n = bridge(tmp_path)
    calls = []

    def down():
        calls.append(1)
        raise ClockError("no answer from http://10.0.0.5 (ConnectTimeout)")
    made[0].heap = down
    ClockStore(tmp_path / "clock.json").save({"name": "clawd", "host": "192.168.1.48", "port": 80})
    s = b.state()
    assert s["ok"] and s["app"]["clock"] == "http://192.168.1.48" and calls == []


FEED_HEAP = {**STYLED_HEAP, "fwSeq": 5, "style": "synth"}


def wait_job(b):
    for _ in range(500):
        if not b.job()["running"]:
            break
        time.sleep(0.01)
    return b.job()


def feed_settled(b):
    """state() reads the feed in the background: wait for that, then ask for the firmware facts."""
    t = b._feed_thread
    if t is not None:
        t.join(10)
    return b.fw()["fw"]


def test_feed_release_is_offered_installed_with_its_packs_and_style(tmp_path, monkeypatch):
    from tests.feedkit import DESK, FakeFeed
    monkeypatch.setattr(sa.ota, "SETTLE", 0)
    monkeypatch.setattr(sa.ota, "CRC_WAIT", 0)          # the clock never finishes its checksum here
    ff = FakeFeed()
    ff.publish(6, notes="Màn chờ có IP")
    b, made, _k, _n = bridge(tmp_path, heap=FEED_HEAP, feed=ff.client())
    made[0].after = {**FEED_HEAP, "fwSeq": 6, "pakCrc": "", "deskCrc": "", "pak": 17, "desk": 0}
    b.state()
    fw_ = feed_settled(b)
    assert fw_["update"] == "available" and fw_["bundle"]["source"] == "feed" and fw_["bundle"]["seq"] == 6
    assert fw_["bundle"]["notes"] == "Màn chờ có IP" and fw_["plan"]["fw"] == "need"
    assert [p["id"] for p in fw_["styles"]["packs"]] == ["synth", "paper"] and fw_["styles"]["state"] == "ready"
    assert not any("firmware.bin" in c for c in ff.calls)       # only the style packs so far
    assert b.update(None, None)["ok"]
    j = wait_job(b)
    assert j["done"] and not j["error"], j
    assert made[0].uploads == 1 and made[0].packs == ["clawdpak", "deskpak"]   # CRC not known in time: sent
    assert made[0].styles and made[0].heap_["deskCrc"] == f"{zlib.crc32(DESK):08x}"   # synth re-sent (CRC differs)
    st = b.state()
    assert st["fw"]["update"] == "current"


def test_feed_older_than_the_bundle_is_ignored_and_errors_show(tmp_path, bundle):
    from tests.feedkit import FakeFeed
    ff = FakeFeed()
    ff.publish(1)
    b, _m, _k, _n = bridge(tmp_path, heap=FEED_HEAP, bundle=bundle, feed=ff.client())
    b.state()
    assert feed_settled(b)["bundle"]["source"] == "app"
    ff.channels.clear()
    ff.releases.clear()
    broken = FakeFeed().client()
    broken._http.headers["authorization"] = "Bearer nope"
    b2, _m, _k, _n = bridge(tmp_path, heap=FEED_HEAP, feed=broken)
    b2.state()
    s = {"fw": feed_settled(b2)}
    assert s["fw"]["update"] == "none" and s["fw"]["feedError"]
    assert b2.update(None, None)["code"] == "bad"
    assert b2.check_update()["fw"]["feedError"]


def test_state_does_not_wait_for_a_slow_feed(tmp_path, bundle, monkeypatch):
    """The window shows the clock and the bundled packs at once; the feed's release (with its
    packs) arrives later, and the page picks it up through fw() while `feedBusy` is true."""
    import threading

    from tests.feedkit import FakeFeed
    ff = FakeFeed()
    ff.publish(6, notes="mới")
    client = ff.client()
    gate, entered = threading.Event(), threading.Event()
    real = client.fetch

    def slow_fetch(rel, kinds, progress=lambda *_a: None):
        entered.set()
        assert gate.wait(10)                                 # the packs are still downloading
        return real(rel, kinds, progress)
    monkeypatch.setattr(client, "fetch", slow_fetch)
    b, _m, _k, _n = bridge(tmp_path, heap=FEED_HEAP, bundle=bundle, feed=client)
    s = b.state()                                            # returns while the download waits
    assert s["ok"] and s["fw"]["feedBusy"] and s["fw"]["bundle"]["source"] == "app"
    assert entered.wait(10) and b.fw()["fw"]["feedBusy"]
    gate.set()
    fw_ = feed_settled(b)
    assert not fw_["feedBusy"] and fw_["bundle"]["source"] == "feed" and fw_["bundle"]["notes"] == "mới"
    b.state()                                                # asked again within the recheck time: no new check
    assert not b.fw()["fw"]["feedBusy"]
