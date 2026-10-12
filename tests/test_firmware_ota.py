"""The bundled firmware: manifest, when to offer it, and the install sequence."""
import hashlib
import json

import pytest

from clawd_desktop import firmware as fw, ota, pause
from clawd_desktop.clockapi import ClockError

IMAGE = b"\xe9" + b"\x00" * 999


def make_bundle(tmp_path, seq=1, layouts=3, image=IMAGE, variant="esp8266-clawd"):
    (tmp_path / "clawd-firmware.bin").write_bytes(image)
    (tmp_path / "firmware.json").write_text(json.dumps({
        "file": "clawd-firmware.bin", "size": len(image), "sha256": hashlib.sha256(image).hexdigest(),
        "variant": variant, "fwSeq": seq, "layouts": layouts, "commit": "abc1234"}))
    return fw.load_bundle(tmp_path)


OLD = {"version": "2.16.0", "pak": 17, "desk": 31, "uptime": 900}          # before fwSeq existed
NEW = {"variant": "esp8266-clawd", "fwSeq": 1, "layouts": 3, "pak": 17, "desk": 31, "uptime": 4}


def test_manifest_load_and_damage_check(tmp_path):
    b = make_bundle(tmp_path)
    assert b.seq == 1 and b.layouts == 3 and b.read() == IMAGE
    (tmp_path / "clawd-firmware.bin").write_bytes(b"\xe9" + b"\x01" * 999)
    with pytest.raises(ValueError):
        b.read()
    assert fw.load_bundle(tmp_path / "missing") is None


def test_update_and_layout_states(tmp_path):
    b = make_bundle(tmp_path)
    assert fw.update_state(OLD, b) == "available"
    assert fw.update_state(NEW, b) == "current"
    assert fw.update_state({**NEW, "fwSeq": 2}, b) == "current"            # never downgrade
    assert fw.update_state({"version": "2.16.0"}, b) == "incompatible"     # stock firmware
    assert fw.update_state({"variant": "esp8266", "desk": 1, "pak": 1}, b) == "incompatible"
    assert fw.update_state(OLD, None) == "none"
    assert fw.layout_state(0, OLD, b) == "ready"
    assert fw.layout_state(1, OLD, b) == "update"
    assert fw.layout_state(1, NEW, b) == "ready"
    assert fw.layout_state(2, OLD, b) == "unavailable"
    assert fw.layout_state(1, OLD, None) == "unavailable"


class FakeApi:
    def __init__(self, before, after):
        self.before, self.after = before, after
        self.log = []

    def heap(self):
        self.log.append("heap")
        return self.before

    def upload_firmware(self, image):
        self.log.append(("upload", len(image)))

    def wait_back(self, started, clock=None, tick=None):
        tick()
        self.log.append("wait")
        return self.after

    def save(self, changes):
        self.log.append(("save", changes))
        return {"ok": True}


def test_install_pauses_uploads_waits_then_saves(tmp_path):
    b = make_bundle(tmp_path)
    api = FakeApi(OLD, NEW)
    holds, released = [], []
    steps = []
    ota.install_then_save(api, b, {"usage": {"layout": 1}}, need_layout=1,
                          progress=steps.append, hold=lambda s, r: holds.append(r),
                          release=lambda: released.append(1), sleep=lambda _s: None)
    assert api.log == ["heap", ("upload", len(IMAGE)), "wait", ("save", {"usage": {"layout": 1}})]
    assert len(holds) >= 3 and released == [1]                 # renewed while waiting, then let go
    assert steps == ["check", "pause", "upload", "reboot", "done"]


def test_install_refuses_when_current_and_releases_on_failure(tmp_path):
    b = make_bundle(tmp_path)
    with pytest.raises(ClockError):
        ota.install(FakeApi(NEW, NEW), b, hold=lambda *_: None, release=lambda: None,
                    sleep=lambda _s: None)
    released = []
    with pytest.raises(ClockError):                             # came back on the old image
        ota.install(FakeApi(OLD, OLD), b, hold=lambda *_: None,
                    release=lambda: released.append(1), sleep=lambda _s: None)
    assert released == [1]


def test_pause_lease(tmp_path):
    p = tmp_path / "pause.json"
    now = [1000.0]
    r = pause.PauseReader(p, now=lambda: now[0])
    assert r.reason() is None
    pause.hold(60, "firmware update", path=p, now=lambda: now[0])
    assert r.reason() == "firmware update"
    now[0] += 61
    assert r.reason() is None                                   # lease ran out
    pause.hold(9999, "x", path=p, now=lambda: now[0])           # capped at MAX_LEASE
    assert r.reason() == "x"
    now[0] += pause.MAX_LEASE + 1
    assert r.reason() is None
    pause.release(p)
    pause.release(p)                                            # twice is fine
    assert r.reason() is None
