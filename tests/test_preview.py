"""The clock's screen in the settings window: the stored usage payload, the renderer
wrapper, the BMP and the bridge call. The real renderer is only exercised when it is built."""
import base64
import hashlib
import json
import os
import stat
import struct
import sys
import time
from pathlib import Path

import pytest

import clawd_desktop.preview as pv
from clawd_desktop import engine as eng
from clawd_desktop import paths
from clawd_desktop import settings_api as sa
from clawd_desktop.discovery import ClockFinder, ClockStore
from tests.test_settings_api import FakeApi

PAYLOAD = {"s": 42, "sr": 134, "w": 67, "wr": 3960, "st": "allowed", "t": 1760191500, "tf": 24}
posix_only = pytest.mark.skipif(sys.platform == "win32", reason="the fake renderer is a shell script")


# ---- the payload the engine keeps -------------------------------------------------
def test_payload_roundtrip_keeps_only_the_screen_keys(tmp_path):
    f = tmp_path / "p.json"
    paths.set_usage_payload({**PAYLOAD, "m": "Claude needs you", "x": True, "b": 1, "junk": 3}, now=123.5, path=f)
    got = paths.usage_payload(f)
    assert got == (PAYLOAD, 123.5)                       # no alert (m, x), beep (b) or unknown key


@pytest.mark.parametrize("text", ["", "nope", "[]", "{}", '{"at": "x", "payload": {}}', '{"at": 1, "payload": 3}',
                                  '{"at": true, "payload": {}}'])
def test_unreadable_payload_file_is_none(tmp_path, text):
    f = tmp_path / "p.json"
    f.write_text(text)
    assert paths.usage_payload(f) is None


def test_missing_payload_file_is_none(tmp_path):
    assert paths.usage_payload(tmp_path / "none.json") is None


def test_engine_keeps_every_payload_for_the_settings_window(tmp_path):
    e = eng.Engine(ClockFinder(ClockStore(tmp_path / "clock.json")))
    e._on_payload({"ok": False})
    kept = paths.usage_payload()
    assert kept is not None and kept[0] == {"ok": False}
    e._on_payload(PAYLOAD)
    kept = paths.usage_payload()
    assert kept is not None and kept[0] == PAYLOAD


def test_engine_survives_an_unwritable_payload_file(tmp_path, monkeypatch):
    def boom(*_a, **_k):
        raise OSError("read-only")
    monkeypatch.setattr(paths, "set_usage_payload", boom)
    e = eng.Engine(ClockFinder(ClockStore(tmp_path / "clock.json")))
    e._on_payload(PAYLOAD)
    assert e.payload == PAYLOAD and e.token is True


# ---- live payload and BMP -------------------------------------------------------------
def test_live_payload_moves_the_wall_clock_to_now_only_when_there_is_one():
    now = 1_800_000_000.0
    live = pv.live_payload(PAYLOAD, now)
    assert live is not None
    assert live["t"] == int(now) + time.localtime(now).tm_gmtoff
    assert {k: v for k, v in live.items() if k != "t"} == {k: v for k, v in PAYLOAD.items() if k != "t"}
    assert PAYLOAD["t"] == 1760191500                    # the stored one is not touched
    assert pv.live_payload({"s": 1}, now) == {"s": 1}    # the user turned the clock off: no wall clock
    assert pv.live_payload(None, now) is None and pv.live_payload({}, now) is None


def test_bmp_is_bottom_up_bgr_and_the_right_size():
    rgb = bytearray(pv.FRAME_BYTES)
    rgb[0:3] = bytes([10, 20, 30])                        # top-left pixel
    last = (pv.H - 1) * pv.W * 3 + (pv.W - 1) * 3
    rgb[last:last + 3] = bytes([1, 2, 3])                 # bottom-right pixel
    url = pv.bmp_data_url(bytes(rgb))
    assert url.startswith("data:image/bmp;base64,")
    bmp = base64.b64decode(url.split(",", 1)[1])
    assert bmp[:2] == b"BM" and len(bmp) == 54 + pv.FRAME_BYTES
    assert struct.unpack_from("<I", bmp, 2)[0] == len(bmp) and struct.unpack_from("<I", bmp, 10)[0] == 54
    w, h, planes, bpp = struct.unpack_from("<iiHH", bmp, 18)
    assert (w, h, planes, bpp) == (240, 240, 1, 24)
    assert bmp[54 + 717:54 + 720] == bytes([3, 2, 1])     # end of the first stored row (the bottom one), as BGR
    assert bmp[-720:-717] == bytes([30, 20, 10])          # start of the last stored row (the top one)
    with pytest.raises(ValueError):
        pv.bmp_data_url(b"short")


def test_frame_cache_keeps_the_latest_few():
    c = pv.FrameCache(2)
    c.put("a", "1")
    c.put("b", "2")
    assert c.get("a") == "1"                              # a is now the freshest
    c.put("c", "3")
    assert c.get("b") is None and c.get("a") == "1" and c.get("c") == "3"


# ---- loading and running the renderer ----------------------------------------------------
def fake_folder(tmp_path, body, commit="44e244f", layouts=63):
    d = tmp_path / "renderer"
    d.mkdir()
    exe = d / "clawd_render"
    exe.write_text("#!/bin/sh\n" + body)
    exe.chmod(exe.stat().st_mode | stat.S_IXUSR)
    (d / "clawd.pak").write_bytes(b"CLWD")
    (d / "preview.json").write_text(json.dumps({"exe": "clawd_render", "pak": "clawd.pak", "commit": commit,
                                                "layouts": layouts, "fwSeq": 2}))
    return d


def test_load_renderer_needs_every_file(tmp_path):
    assert pv.load_renderer(tmp_path / "nothing") is None
    d = fake_folder(tmp_path, "exit 0")
    r = pv.load_renderer(d)
    assert r is not None and r.layouts == 63
    (d / "clawd.pak").unlink()
    assert pv.load_renderer(d) is None


def test_renderer_built_from_another_firmware_is_not_trusted(tmp_path):
    d = fake_folder(tmp_path, "exit 0", commit="aaaaaaa")
    assert pv.load_renderer(d, firmware_commit="bbbbbbb") is None
    assert pv.load_renderer(d, firmware_commit="aaaaaaa") is not None
    assert pv.load_renderer(d, firmware_commit="") is not None          # no bundled firmware to compare with


@posix_only
def test_draw_returns_the_frame_and_passes_the_payload_on(tmp_path):
    log = tmp_path / "args"
    d = fake_folder(tmp_path, f'echo "$@" > {log}; cat > {log}.in; head -c {pv.FRAME_BYTES} /dev/zero')
    r = pv.load_renderer(d)
    assert r is not None
    assert len(r.draw(3, {"s": 5})) == pv.FRAME_BYTES
    assert log.read_text().split()[:2] == ["--layout", "3"]
    assert json.loads(Path(f"{log}.in").read_text()) == {"s": 5}
    r.draw(0, None)
    assert Path(f"{log}.in").read_text() == ""                          # nothing at all: the waiting screen


@posix_only
def test_a_payload_the_clock_would_reject_draws_the_waiting_screen(tmp_path):
    n = tmp_path / "calls"
    d = fake_folder(tmp_path, f'echo x >> {n}; if [ "$(wc -c)" -gt 0 ]; then exit 2; fi; head -c {pv.FRAME_BYTES} /dev/zero')
    r = pv.load_renderer(d)
    assert r is not None
    assert len(r.draw(0, {"foo": 1})) == pv.FRAME_BYTES
    assert len(n.read_text().split()) == 2


@posix_only
@pytest.mark.parametrize("body", ["exit 1", "head -c 100 /dev/zero", f"head -c {pv.FRAME_BYTES + 5} /dev/zero"])
def test_a_broken_renderer_raises(tmp_path, body):
    r = pv.load_renderer(fake_folder(tmp_path, body))
    assert r is not None
    with pytest.raises(ValueError):
        r.draw(0, PAYLOAD)


# ---- the bridge call ----------------------------------------------------------------------------
class FakeRenderer:
    def __init__(self, layouts=63, fail=False):
        self.layouts, self.fail, self.calls = layouts, fail, []

    def draw(self, layout, payload):
        self.calls.append((layout, payload))
        if self.fail:
            raise ValueError("boom")
        return bytes([layout]) * pv.FRAME_BYTES


NOW = 1_800_000_000.0


def api_with(renderer, stored=(PAYLOAD, NOW - 30)):
    return sa.SettingsApi("http://c", "k", api_factory=lambda *a, **k: FakeApi("http://c"),  # type: ignore[arg-type,return-value]
                          renderer=renderer,  # type: ignore[arg-type]
                          store=ClockStore(Path("/nonexistent/x.json")), payload_source=lambda: stored,
                          now=lambda: NOW)


def test_usage_preview_draws_every_layout_with_the_real_usage():
    r = FakeRenderer()
    out = api_with(r).usage_preview()
    assert out["ok"] and out["exact"] and sorted(out["frames"]) == ["0", "1", "2", "3", "4", "5"]
    assert out["sentAt"] == NOW - 30 and out["now"] == NOW
    live = out["payload"]
    assert live["s"] == 42 and live["w"] == 67 and live["t"] != PAYLOAD["t"]      # real numbers, wall clock now
    assert [c[0] for c in r.calls] == [0, 1, 2, 3, 4, 5] and all(c[1] == live for c in r.calls)
    assert out["frames"]["1"].startswith("data:image/bmp;base64,")


def test_usage_preview_reuses_frames_within_the_minute():
    r = FakeRenderer()
    b = api_with(r)
    b.usage_preview([1])
    b.usage_preview([1])
    assert len(r.calls) == 1


def test_usage_preview_ignores_bad_layout_lists_and_skips_what_the_renderer_lacks():
    r = FakeRenderer(layouts=0b11)
    out = api_with(r).usage_preview(["x", 99, True])               # not a layout list: all six
    assert sorted(out["frames"]) == ["0", "1"]                       # the renderer only has two
    assert sorted(api_with(FakeRenderer()).usage_preview([4, 4, 2])["frames"]) == ["2", "4"]


def test_usage_preview_without_a_renderer_hands_the_page_the_real_payload():
    out = api_with(None).usage_preview()
    assert out["exact"] is False and "frames" not in out and out["payload"]["s"] == 42


def test_usage_preview_falls_back_when_the_renderer_fails(capsys):
    out = api_with(FakeRenderer(fail=True)).usage_preview()
    assert out["ok"] and out["exact"] is False and out["payload"]["s"] == 42
    assert "preview renderer failed" in capsys.readouterr().out


def test_usage_preview_before_any_usage_draws_the_waiting_screen():
    r = FakeRenderer()
    out = api_with(r, stored=None).usage_preview([0])
    assert out["exact"] and out["payload"] is None and out["sentAt"] is None
    assert r.calls == [(0, None)]


# ---- the real renderer (only where tools/build_preview.py has built it) ---------------------------------
REAL = pv.load_renderer()


@pytest.mark.skipif(REAL is None or not os.environ.get("CLAWD_GOLDEN"), reason="needs the built renderer + CLAWD_GOLDEN=1")
def test_real_renderer_is_deterministic_and_draws_each_layout():
    assert REAL is not None
    frames = {i: REAL.draw(i, PAYLOAD) for i in range(6)}
    assert all(len(f) == pv.FRAME_BYTES for f in frames.values())
    assert len({hashlib.sha256(f).hexdigest() for f in frames.values()}) == 6      # six different screens
    assert REAL.draw(1, PAYLOAD) == frames[1]
    assert REAL.draw(0, None) != REAL.draw(0, PAYLOAD)                                   # waiting screen vs usage
