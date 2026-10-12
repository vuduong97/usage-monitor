"""The bundled firmware: manifest, when to offer it, and the install sequence."""
import hashlib
import json
import zlib

import pytest

from clawd_desktop import firmware as fw, ota, pause
from clawd_desktop.clockapi import ClockError

IMAGE = b"\xe9" + b"\x00" * 999


def make_pack(sid: str, body: bytes = b"") -> bytes:
    """A style pack as far as the app checks one: magic, CRC-32 of bytes [16..) at 12."""
    rest = sid.encode().ljust(16, b"\0") + b"\0" * 64 + body
    return b"CSTY" + bytes([1, 0, 96, 0]) + len(rest).to_bytes(4, "little") \
        + zlib.crc32(rest).to_bytes(4, "little") + rest


def write_styles(folder, ids=("synth", "paper"), api=1):
    folder.mkdir(parents=True, exist_ok=True)
    rows = []
    for sid in ids:
        data = make_pack(sid)
        (folder / f"{sid}.pak").write_bytes(data)
        rows.append({"id": sid, "name": sid.title(), "file": f"{sid}.pak", "size": len(data),
                     "crc": f"{zlib.crc32(data[16:]):08x}", "light": sid == "paper"})
    (folder / "index.json").write_text(json.dumps({"api": api, "styles": rows}))


def make_bundle(tmp_path, seq=1, layouts=1, image=IMAGE, variant="esp8266-clawd", style_api=0, styles=()):
    tmp_path.mkdir(parents=True, exist_ok=True)
    (tmp_path / "clawd-firmware.bin").write_bytes(image)
    (tmp_path / "firmware.json").write_text(json.dumps({
        "file": "clawd-firmware.bin", "size": len(image), "sha256": hashlib.sha256(image).hexdigest(),
        "variant": variant, "fwSeq": seq, "layouts": layouts, "styleApi": style_api, "commit": "abc1234"}))
    if styles:
        write_styles(tmp_path / "styles", styles)
    return fw.load_bundle(tmp_path)


OLD = {"version": "2.16.0", "pak": 17, "desk": 31, "uptime": 900}          # before fwSeq existed
NEW = {"variant": "esp8266-clawd", "fwSeq": 1, "layouts": 1, "pak": 17, "desk": 31, "uptime": 4}
STYLED = {**NEW, "fwSeq": 2, "styleApi": 1, "style": "synth"}


def test_manifest_load_and_damage_check(tmp_path):
    b = make_bundle(tmp_path)
    assert b.seq == 1 and b.layouts == 1 and b.read() == IMAGE and b.styles == ()
    (tmp_path / "clawd-firmware.bin").write_bytes(b"\xe9" + b"\x01" * 999)
    with pytest.raises(ValueError):
        b.read()
    assert fw.load_bundle(tmp_path / "missing") is None


def test_update_states(tmp_path):
    b = make_bundle(tmp_path)
    assert fw.update_state(OLD, b) == "available"
    assert fw.update_state(NEW, b) == "current"
    assert fw.update_state({**NEW, "fwSeq": 2}, b) == "current"            # never downgrade
    assert fw.update_state({"version": "2.16.0"}, b) == "incompatible"     # stock firmware
    assert fw.update_state({"variant": "esp8266", "desk": 1, "pak": 1}, b) == "incompatible"
    assert fw.update_state(OLD, None) == "none"


def test_style_packs_and_states(tmp_path):
    b = make_bundle(tmp_path, seq=2, style_api=1, styles=("synth", "paper"))
    assert [p.id for p in b.styles] == ["synth", "paper"] and b.styles[1].light
    assert b.styles[0].read()[:4] == b"CSTY"
    assert fw.style_state(NEW, b) == "update"             # seq 1 < 2, no styleApi yet
    assert fw.style_state(STYLED, b) == "ready"
    assert fw.style_state({**STYLED, "styleApi": 2}, b) == "unavailable"   # another API, no update
    assert fw.style_state(NEW, None) == "unavailable"
    assert fw.clock_style(STYLED) == "synth" and fw.clock_style(NEW) == ""
    assert fw.clock_style({**STYLED, "styleApi": True}) == ""
    pak = tmp_path / "styles" / "synth.pak"
    pak.write_bytes(pak.read_bytes()[:-1] + b"x")          # damaged after bundling
    with pytest.raises(ValueError):
        b.styles[0].read()


def test_style_index_must_match_the_image(tmp_path):
    assert make_bundle(tmp_path, style_api=2, styles=("synth",)).styles == ()   # index api 1
    assert make_bundle(tmp_path / "a", style_api=0, styles=("synth",)).styles == ()
    b = tmp_path / "b"
    make_bundle(b, style_api=1, styles=("synth",))
    idx = json.loads((b / "styles" / "index.json").read_text())
    idx["styles"][0]["file"] = "../firmware.json"
    (b / "styles" / "index.json").write_text(json.dumps(idx))
    assert fw.load_bundle(b).styles == ()                  # never a file outside styles/
    assert make_bundle(tmp_path / "c", style_api=1, styles=("Bad Id",)).styles == ()


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
    ota.install_then_save(api, b, {"brightness": 40},
                          progress=steps.append, hold=lambda s, r: holds.append(r),
                          release=lambda: released.append(1), sleep=lambda _s: None)
    assert api.log == ["heap", ("upload", len(IMAGE)), "wait", ("save", {"brightness": 40})]
    assert len(holds) >= 3 and released == [1]                 # renewed while waiting, then let go
    assert steps == ["check", "pause", "upload", "reboot", "done"]


def test_install_checks_the_new_image_draws_styles(tmp_path):
    b = make_bundle(tmp_path, seq=2, style_api=1, styles=("synth",))
    quiet = {"hold": lambda *_: None, "release": lambda: None, "sleep": lambda _s: None}
    with pytest.raises(ClockError):                             # came back without /api/style
        ota.install_then_save(FakeApi(NEW, {**NEW, "fwSeq": 2}), b, {}, need_style_api=1, **quiet)
    assert ota.install_then_save(FakeApi(NEW, STYLED), b, {}, need_style_api=1, **quiet) == STYLED


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
