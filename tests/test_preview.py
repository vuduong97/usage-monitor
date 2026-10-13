"""The clock's screen in the settings window: the stored usage payload, the renderer
wrapper, the BMP, the clip and its animated PNG, and the bridge call. The real renderer is only exercised when it is built."""
import base64
import hashlib
import json
import os
import stat
import struct
import sys
import time
import zlib
from pathlib import Path

import pytest

import clawd_desktop.preview as pv
from clawd_desktop import engine as eng
from clawd_desktop import paths
from clawd_desktop import settings_api as sa
from clawd_desktop.discovery import ClockFinder, ClockStore
from tests.test_firmware_ota import make_bundle, write_styles
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
    local = int(now) + time.localtime(now).tm_gmtoff
    assert live["t"] == local - local % 60               # on the minute: a clip never shows it change
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


# ---- clip and animated PNG ------------------------------------------------------------------
def clip_bytes(frames):
    out = [b"CLP1", struct.pack("<H", len(frames))]
    for delay, x, y, w, h, rgb in frames:
        out += [struct.pack("<5H", delay, x, y, w, h), rgb]
    return b"".join(out)


FULL = (500, 0, 0, 240, 240, bytes([7, 8, 9]) * (240 * 240))
SPOT = (120, 10, 20, 2, 1, bytes([1, 2, 3, 4, 5, 6]))


def test_parse_clip_reads_every_frame():
    frames = pv.parse_clip(clip_bytes([FULL, SPOT]))
    assert [(f.delay_ms, f.x, f.y, f.w, f.h) for f in frames] == [(500, 0, 0, 240, 240), (120, 10, 20, 2, 1)]
    assert frames[1].rgb == bytes([1, 2, 3, 4, 5, 6])


@pytest.mark.parametrize("data", [b"", b"NOPE\x01\x00", clip_bytes([SPOT]),                       # frame 0 not whole
                                  clip_bytes([FULL])[:-1], clip_bytes([FULL]) + b"x",              # cut / trailing
                                  clip_bytes([FULL, (1, 239, 0, 2, 1, bytes(6))]),                 # off the screen
                                  b"CLP1\x02\x00" + clip_bytes([FULL])[6:]])                       # count too high
def test_parse_clip_rejects_bad_data(data):
    with pytest.raises(ValueError):
        pv.parse_clip(data)


def png_chunks(png):
    assert png[:8] == b"\x89PNG\r\n\x1a\n"
    off, out = 8, []
    while off < len(png):
        (n,) = struct.unpack_from(">I", png, off)
        kind, body = png[off + 4:off + 8], png[off + 8:off + 8 + n]
        assert struct.unpack_from(">I", png, off + 8 + n)[0] == zlib.crc32(kind + body)
        out.append((kind, body))
        off += 12 + n
    return out


def test_apng_loops_the_frames_with_their_delays_and_rectangles():
    png = pv.apng(pv.parse_clip(clip_bytes([FULL, SPOT])))
    chunks = png_chunks(png)
    kinds = [k for k, _ in chunks]
    assert kinds == [b"IHDR", b"acTL", b"fcTL", b"IDAT", b"fcTL", b"fdAT", b"IEND"]
    assert struct.unpack(">IIBBBBB", chunks[0][1]) == (240, 240, 8, 2, 0, 0, 0)
    assert struct.unpack(">II", chunks[1][1]) == (2, 0)                                     # two frames, forever
    f0 = struct.unpack(">IIIIIHHBB", chunks[2][1])
    f1 = struct.unpack(">IIIIIHHBB", chunks[4][1])
    assert f0 == (0, 240, 240, 0, 0, 500, 1000, 0, 0)
    assert f1 == (1, 2, 1, 10, 20, 120, 1000, 0, 0)                                       # IDAT has no number
    assert struct.unpack_from(">I", chunks[5][1])[0] == 2                                    # sequence runs on
    assert zlib.decompress(chunks[5][1][4:]) == b"\0" + bytes([1, 2, 3, 4, 5, 6])          # filter byte + row
    raw = zlib.decompress(chunks[3][1])
    assert len(raw) == 240 * 721 and raw[:4] == bytes([0, 7, 8, 9])
    assert pv.apng_data_url([pv.parse_clip(clip_bytes([FULL]))[0]]).startswith("data:image/png;base64,")
    with pytest.raises(ValueError):
        pv.apng([])


def test_frame_cache_keeps_the_latest_few():
    c = pv.FrameCache(2)
    c.put("a", "1")
    c.put("b", "2")
    assert c.get("a") == "1"                              # a is now the freshest
    c.put("c", "3")
    assert c.get("b") is None and c.get("a") == "1" and c.get("c") == "3"


# ---- loading and running the renderer ----------------------------------------------------
def fake_folder(tmp_path, body, commit="44e244f", styles=("synth", "paper")):
    d = tmp_path / "renderer"
    d.mkdir()
    write_styles(d / "styles", styles)
    exe = d / "clawd_render"
    exe.write_text("#!/bin/sh\n" + body)
    exe.chmod(exe.stat().st_mode | stat.S_IXUSR)
    (d / "clawd.pak").write_bytes(b"CLWD")
    (d / "preview.json").write_text(json.dumps({"exe": "clawd_render", "pak": "clawd.pak", "commit": commit,
                                                "layouts": 1, "styleApi": 1, "fwSeq": 2}))
    return d


def test_load_renderer_needs_every_file(tmp_path):
    assert pv.load_renderer(tmp_path / "nothing") is None
    d = fake_folder(tmp_path, "exit 0")
    r = pv.load_renderer(d)
    assert r is not None and sorted(r.styles) == ["paper", "synth"]
    (d / "styles" / "index.json").unlink()
    r = pv.load_renderer(d)
    assert r is not None and r.styles == {}                             # no packs: the classic screen still draws
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
    assert len(r.draw("synth", {"s": 5})) == pv.FRAME_BYTES
    args = log.read_text().split()
    assert args[-2] == "--style" and args[-1].endswith("styles/synth.pak")
    assert json.loads(Path(f"{log}.in").read_text()) == {"s": 5}
    r.draw(pv.CLASSIC, None)
    assert "--style" not in log.read_text().split()                     # the classic screen: no pack
    assert Path(f"{log}.in").read_text() == ""                          # nothing at all: the waiting screen
    with pytest.raises(ValueError):
        r.draw("nope", None)


@posix_only
def test_clip_asks_for_one_and_parses_it(tmp_path):
    log = tmp_path / "args"
    src = tmp_path / "clip.bin"
    src.write_bytes(clip_bytes([FULL, SPOT]))
    r = pv.load_renderer(fake_folder(tmp_path, f'echo "$@" > {log}; cat > /dev/null; cat {src}'))
    assert r is not None
    assert len(r.clip("paper", PAYLOAD)) == 2
    args = log.read_text().split()
    assert args[-1] == "--clip" and args[args.index("--style") + 1].endswith("paper.pak")


@posix_only
@pytest.mark.parametrize("body", ["exit 1", "printf nope"])
def test_a_broken_clip_raises(tmp_path, body):
    r = pv.load_renderer(fake_folder(tmp_path, "cat > /dev/null; " + body))
    assert r is not None
    with pytest.raises(ValueError):
        r.clip(pv.CLASSIC, PAYLOAD)


@posix_only
def test_a_payload_the_clock_would_reject_draws_the_waiting_screen(tmp_path):
    n = tmp_path / "calls"
    d = fake_folder(tmp_path, f'echo x >> {n}; if [ "$(wc -c)" -gt 0 ]; then exit 2; fi; head -c {pv.FRAME_BYTES} /dev/zero')
    r = pv.load_renderer(d)
    assert r is not None
    assert len(r.draw(pv.CLASSIC, {"foo": 1})) == pv.FRAME_BYTES
    assert len(n.read_text().split()) == 2


@posix_only
@pytest.mark.parametrize("body", ["exit 1", "head -c 100 /dev/zero", f"head -c {pv.FRAME_BYTES + 5} /dev/zero"])
def test_a_broken_renderer_raises(tmp_path, body):
    r = pv.load_renderer(fake_folder(tmp_path, body))
    assert r is not None
    with pytest.raises(ValueError):
        r.draw(pv.CLASSIC, PAYLOAD)


# ---- the bridge call ----------------------------------------------------------------------------
class FakeRenderer:
    def __init__(self, styles=("synth", "paper"), fail=False):
        self.styles = {sid: Path(f"/x/{sid}.pak") for sid in styles}
        self.fail, self.calls = fail, []

    def _byte(self, style):
        return 0 if style == pv.CLASSIC else 1 + sorted(self.styles).index(style)

    def draw(self, style, payload):
        self.calls.append((style, payload))
        if self.fail:
            raise ValueError("boom")
        return bytes([self._byte(style)]) * pv.FRAME_BYTES

    def clip(self, style, payload):
        self.calls.append((style, payload))
        if self.fail:
            raise ValueError("boom")
        return pv.parse_clip(clip_bytes([(400, 0, 0, 240, 240, bytes([self._byte(style)]) * pv.FRAME_BYTES), SPOT]))


NOW = 1_800_000_000.0


def api_with(renderer, stored=(PAYLOAD, NOW - 30), bundle=None):
    return sa.SettingsApi("http://c", "k", api_factory=lambda *a, **k: FakeApi("http://c"),  # type: ignore[arg-type,return-value]
                          renderer=renderer, bundle=bundle,  # type: ignore[arg-type]
                          store=ClockStore(Path("/nonexistent/x.json")), payload_source=lambda: stored,
                          now=lambda: NOW)


@pytest.fixture
def bundle(tmp_path):
    return make_bundle(tmp_path / "bundle", seq=2, style_api=1, styles=("synth", "paper"))


def test_usage_preview_draws_every_style_with_the_real_usage(bundle):
    r = FakeRenderer()
    out = api_with(r, bundle=bundle).usage_preview()
    assert out["ok"] and out["exact"] and sorted(out["frames"]) == ["classic", "paper", "synth"]
    assert out["sentAt"] == NOW - 30 and out["now"] == NOW
    live = out["payload"]
    assert live["s"] == 42 and live["w"] == 67 and live["t"] != PAYLOAD["t"]      # real numbers, wall clock now
    assert [c[0] for c in r.calls] == ["classic", "synth", "paper"] and all(c[1] == live for c in r.calls)
    png = base64.b64decode(out["frames"]["synth"].split(",", 1)[1])
    assert out["frames"]["synth"].startswith("data:image/png;base64,") and b"acTL" in png     # an animated loop


def test_usage_preview_without_a_bundle_draws_only_the_classic_screen():
    out = api_with(FakeRenderer()).usage_preview()
    assert sorted(out["frames"]) == ["classic"]


def test_usage_preview_without_motion_draws_still_frames(bundle):
    r = FakeRenderer()
    b = api_with(r, bundle=bundle)
    still = b.usage_preview(["synth"], False)
    assert still["frames"]["synth"].startswith("data:image/bmp;base64,")
    assert b.usage_preview(["synth"], "yes")["frames"]["synth"].startswith("data:image/png;base64,")  # not a bool: animate
    assert b.usage_preview(["synth"], False) == still and len(r.calls) == 2                            # each kind cached


def test_usage_preview_reuses_frames_within_the_minute(bundle):
    r = FakeRenderer()
    b = api_with(r, bundle=bundle)
    b.usage_preview(["paper"])
    b.usage_preview(["paper"])
    assert len(r.calls) == 1


def test_usage_preview_ignores_bad_style_lists_and_skips_what_the_renderer_lacks(bundle):
    r = FakeRenderer(styles=("synth",))
    out = api_with(r, bundle=bundle).usage_preview(["x", 99, True])      # not a style list: all of them
    assert sorted(out["frames"]) == ["classic", "synth"]                  # the renderer has no paper
    out = api_with(FakeRenderer(), bundle=bundle).usage_preview(["paper", "paper", "classic"])
    assert sorted(out["frames"]) == ["classic", "paper"]


def test_usage_preview_without_a_renderer_hands_the_page_the_real_payload():
    out = api_with(None).usage_preview()
    assert out["exact"] is False and "frames" not in out and out["payload"]["s"] == 42


def test_usage_preview_falls_back_when_the_renderer_fails(capsys):
    out = api_with(FakeRenderer(fail=True)).usage_preview()
    assert out["ok"] and out["exact"] is False and out["payload"]["s"] == 42
    assert "preview renderer failed" in capsys.readouterr().out


def test_usage_preview_before_any_usage_draws_the_waiting_screen():
    r = FakeRenderer()
    out = api_with(r, stored=None).usage_preview(["classic"])
    assert out["exact"] and out["payload"] is None and out["sentAt"] is None
    assert r.calls == [("classic", None)]


# ---- the real renderer (only where tools/build_preview.py has built it) ---------------------------------
REAL = pv.load_renderer()


@pytest.mark.skipif(REAL is None or not os.environ.get("CLAWD_GOLDEN"), reason="needs the built renderer + CLAWD_GOLDEN=1")
def test_real_renderer_is_deterministic_and_draws_each_style():
    assert REAL is not None
    ids = [pv.CLASSIC, *sorted(REAL.styles)]
    frames = {i: REAL.draw(i, PAYLOAD) for i in ids}
    assert len(ids) == 47 and all(len(f) == pv.FRAME_BYTES for f in frames.values())   # classic + 46 packs
    assert len({hashlib.sha256(f).hexdigest() for f in frames.values()}) == len(ids)   # all different
    assert REAL.draw("synth", PAYLOAD) == frames["synth"]
    assert REAL.draw(pv.CLASSIC, None) != REAL.draw(pv.CLASSIC, PAYLOAD)              # waiting screen vs usage


@pytest.mark.skipif(REAL is None or not os.environ.get("CLAWD_GOLDEN"), reason="needs the built renderer + CLAWD_GOLDEN=1")
def test_real_clips_loop_each_style():
    assert REAL is not None
    live = pv.live_payload(PAYLOAD, NOW)
    frames = REAL.clip(pv.CLASSIC, live)                # corner mascot: a 4 s rest + one act
    assert len(frames) > 10 and 5_000 < sum(f.delay_ms for f in frames) < 15_000
    for sid in REAL.styles:
        frames = REAL.clip(sid, live)
        total = sum(f.delay_ms for f in frames)
        assert 1_000 <= total <= 50_000, (sid, total)
        assert len(pv.apng(frames)) < 400_000, sid
    assert REAL.clip("synth", live) == REAL.clip("synth", live)                         # deterministic


# ---- the clock's own clawd.pak for the approximate preview ---------------------------------------
def make_pak(count=2, w=4, h=3, frames=2):
    fbytes = (w * h + 1) // 2
    head = b"CLWD" + struct.pack("<HH", 1, count) + bytes(4)
    descs, body = b"", b""
    base = 12 + 72 * count
    for i in range(count):
        frames_off = base + len(body)
        body += bytes([0x12]) * (fbytes * frames)
        holds_off = base + len(body)
        body += struct.pack("<%dH" % frames, *([100] * frames))
        descs += (f"anim{i}".encode().ljust(16, b"\0") + bytes([w, h, 0, 0])
                  + struct.pack("<4H", frames, 0, frames - 1, fbytes) + bytes(32)
                  + struct.pack("<II", frames_off, holds_off) + bytes(4))
    return head + descs + body


def test_pak_count_reads_a_pack_and_refuses_anything_else():
    assert pv.pak_count(make_pak(3)) == 3
    for bad in [b"", b"NOPE" + bytes(20), make_pak()[:-1], b"CLWD" + struct.pack("<HH", 2, 1) + bytes(200),
                b"CLWD" + struct.pack("<HH", 1, 0) + bytes(8), b"CLWD" + bytes(pv.PAK_MAX)]:
        with pytest.raises(ValueError):
            pv.pak_count(bad)


class PakApi(FakeApi):
    def __init__(self, *a, pak=None, **k):
        super().__init__(*a, **k)
        self.pak, self.reads = pak, 0

    def get_file(self, path, max_bytes):
        self.reads += 1
        assert path == "/clawd.pak" and max_bytes == pv.PAK_MAX
        if self.pak is None:
            raise sa.ClockError("HTTP 404: no such file")
        return self.pak


def pak_api(pak, heap_pak=17, clock=None):
    fake = PakApi("http://c", pak=pak, heap={"pak": heap_pak, "uptime": 5})
    t = clock or [NOW]
    b = sa.SettingsApi("http://c", "k", api_factory=lambda *a, **k: fake,  # type: ignore[arg-type,return-value]
                       store=ClockStore(Path("/nonexistent/x.json")), now=lambda: t[0])
    b.state()
    return b, fake


def test_clawd_pak_reads_the_clock_once_and_keeps_it():
    pak = make_pak()
    b, fake = pak_api(pak)
    first = b.clawd_pak()
    assert first["ok"] and base64.b64decode(first["pak"]) == pak
    assert paths.PAK_FILE.read_bytes() == pak
    assert json.loads(paths.PAK_FILE.with_name("clawd.pak.json").read_text()) == {"count": 17}
    assert sorted(f.name for f in paths.PAK_FILE.parent.glob("clawd*")) == ["clawd.pak", "clawd.pak.json"]  # no .tmp left
    assert pv.PakCache(paths.PAK_FILE).load() == (pak, 17)
    assert b.clawd_pak() == first and fake.reads == 1                     # the cache answers now
    b2, fake2 = pak_api(make_pak(3), heap_pak=3)                          # the clock got a new pack
    assert base64.b64decode(b2.clawd_pak()["pak"]) == make_pak(3) and fake2.reads == 1


def test_clawd_pak_without_one_on_the_clock_says_so_and_does_not_hammer_it():
    clock = [NOW]
    b, fake = pak_api(None, clock=clock)
    assert b.clawd_pak()["ok"] is False and fake.reads == 1
    assert b.clawd_pak()["ok"] is False and fake.reads == 1               # waits before asking again
    clock[0] += sa.PAK_RETRY_S + 1
    b.clawd_pak()
    assert fake.reads == 2
    b0, fake0 = pak_api(make_pak(), heap_pak=0)                           # heap: no pack on the clock
    assert b0.clawd_pak()["code"] == "nopak" and fake0.reads == 0


def test_clawd_pak_refuses_a_damaged_pack_and_keeps_the_old_one():
    good = make_pak()
    pak_api(good)[0].clawd_pak()
    b, _ = pak_api(b"CLWD junk", heap_pak=5)                               # a different, broken pack
    assert base64.b64decode(b.clawd_pak()["pak"]) == good
    assert paths.PAK_FILE.read_bytes() == good
