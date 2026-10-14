"""The firmware feed: manifest checks, download + decrypt, what a clock needs, and the
install of a release's firmware and packs."""
import copy

import pytest

from clawd_desktop import feed, ota
from clawd_desktop.clockapi import ClockError
from tests.feedkit import CLAWD, DESK, IMAGE, FakeFeed, crc, release_files

QUIET = {"hold": lambda *_: None, "release": lambda: None, "sleep": lambda _s: None}


def test_latest_and_fetch_decrypt_and_check():
    ff = FakeFeed()
    ff.publish(6, notes="Màn chờ hiện IP")
    f = ff.client()
    m = f.latest()
    assert m.seq == 6 and m.notes == "Màn chờ hiện IP" and m.file("fw").size == len(IMAGE)
    rel = feed.make_release(m)
    assert rel.styles == () and rel.missing({"fw", "style"}) == {"fw", "style"}
    with pytest.raises(ValueError):
        rel.read()                                       # not downloaded yet
    rel = f.fetch(rel, feed.STYLE_KINDS)
    assert [p.id for p in rel.styles] == ["synth", "paper"] and rel.styles[1].read()
    assert "GET /v1/file/6/firmware.bin" not in ff.calls     # only what was asked for
    rel = f.fetch(rel, {"fw", "clawdpak", "deskpak"})
    assert rel.read() == IMAGE and rel.pack("clawdpak") == (CLAWD, crc(CLAWD))
    assert rel.pack("deskpak") == (DESK, crc(DESK)) and rel.missing(feed.KINDS) == set()


def test_no_release_wrong_token_and_beta_channel():
    ff = FakeFeed()
    assert ff.client().latest() is None
    ff.publish(7, channel="beta")
    assert ff.client().latest() is None and ff.client("beta").latest().seq == 7
    bad = feed.Feed("https://fw.test", "nope", transport=ff.client()._http._transport)
    with pytest.raises(feed.FeedError):
        bad.latest()


def test_a_tampered_file_or_a_wrong_hash_is_refused():
    ff = FakeFeed()
    ff.publish(6)
    ff.tamper.add("desk.pak")
    f = ff.client()
    rel = feed.make_release(f.latest())
    with pytest.raises(feed.FeedError, match="decrypt"):
        f.fetch(rel, {"deskpak"})
    ff.tamper.clear()
    m, key, blobs = ff.releases[6]
    m["files"][0]["sha256"] = "0" * 64                    # the firmware's row lies
    with pytest.raises(feed.FeedError, match="manifest"):
        f.fetch(feed.make_release(feed.parse_manifest(m)), {"fw"})


@pytest.mark.parametrize("change", [
    lambda m: m["files"].append({**m["files"][0], "name": "../x"}),
    lambda m: m["files"].append({**m["files"][1], "name": "clawd.pak"}),        # duplicate
    lambda m: m["files"][1].update(kind="exe"),
    lambda m: m["files"][1].update(enc=m["files"][1]["size"]),
    lambda m: m["files"][1].update(crc="xyz"),
    lambda m: m.update(v=2),
    lambda m: m.update(files=m["files"][1:]),                                    # no firmware
    lambda m: m.update(seq="6"),
])
def test_manifest_checks(change):
    m = FakeFeed().publish(6)
    feed.parse_manifest(copy.deepcopy(m))
    bad = copy.deepcopy(m)
    change(bad)
    with pytest.raises(ValueError):
        feed.parse_manifest(bad)


def heap(seq=5, pak=17, desk=31, **kw):
    return {"variant": "esp8266-clawd", "fwSeq": seq, "layouts": 1, "styleApi": 1, "pak": pak, "desk": desk,
            "uptime": 100, **kw}


def test_plan_needed_and_offer(tmp_path):
    ff = FakeFeed()
    m = feed.parse_manifest(ff.publish(6))
    rel = feed.make_release(m)
    old = heap(5)                                          # before pack CRCs
    assert feed.plan(old, rel) == {"fw": "need", "clawdpak": "unknown", "deskpak": "unknown"}
    assert feed.needed(old, rel) == ["fw", "clawdpak", "deskpak"] and feed.offer(old, m) == 6
    same = heap(6, pakCrc=crc(CLAWD), deskCrc=crc(DESK))
    assert feed.plan(same, rel) == {"clawdpak": "ok", "deskpak": "ok"} and feed.offer(same, m) == 0
    diff = heap(6, pakCrc=crc(CLAWD), deskCrc="deadbeef")
    assert feed.needed(diff, rel) == ["deskpak"] and feed.worth_it(diff, rel) and feed.offer(diff, m) == 6
    busy = heap(6, pakCrc="", deskCrc=crc(DESK))           # still checksumming: nothing to offer yet
    assert feed.plan(busy, rel)["clawdpak"] == "wait" and not feed.worth_it(busy, rel)
    gone = heap(6, desk=0, pakCrc=crc(CLAWD), deskCrc="")  # desk.pak missing
    assert feed.plan(gone, rel)["deskpak"] == "need"
    assert feed.plan({"version": "stock"}, rel) == {} and feed.offer({"version": "stock"}, m) == 0
    from tests.test_firmware_ota import make_bundle
    assert feed.offer(old, m, make_bundle(tmp_path, seq=6)) == 6    # the bundle is as new: offer it
    assert feed.offer(heap(7), m, make_bundle(tmp_path / "b", seq=7)) == 0


class FakeClock:
    """The clock's side of an install: /update, then a reboot into `after`; packs."""

    def __init__(self, before, after, fail_uploads=0, bad_crc=0):
        self.h = dict(before)
        self.after = dict(after)
        self.log = []
        self.fail_uploads, self.bad_crc = fail_uploads, bad_crc
        self.polls = 0

    def heap(self):
        self.polls += 1
        if self.polls == 3:                     # the background checksum finished meanwhile
            self.h.update({k: v for k, v in self.after.items() if k.endswith("Crc")})
        return dict(self.h)

    def upload_firmware(self, image):
        self.log.append(("fw", len(image)))

    def wait_back(self, started, clock=None, tick=None):
        tick()
        self.h = {**self.after, "pakCrc": "", "deskCrc": ""}    # just booted: CRCs not known yet
        return dict(self.h)

    def upload_pack(self, kind, data):
        if self.fail_uploads:
            self.fail_uploads -= 1
            raise ClockError("no answer")
        self.log.append((kind, len(data)))
        got = crc(data)
        if self.bad_crc:
            self.bad_crc -= 1
            got = "00000000"
        self.h["pakCrc" if kind == "clawdpak" else "deskCrc"] = got
        return {"ok": True, "anims": 17, "crc": got}

    def save(self, changes):
        self.log.append(("save", changes))
        return {"ok": True}


def fetched(ff, seq=6):
    f = ff.client()
    return f.fetch(feed.make_release(f.latest()), feed.KINDS)


def test_install_release_firmware_then_only_the_packs_that_differ(monkeypatch):
    monkeypatch.setattr(ota, "SETTLE", 0)
    ff = FakeFeed()
    ff.publish(6)
    rel = fetched(ff)
    c = FakeClock(heap(5), heap(6, pakCrc=crc(CLAWD), deskCrc="0badc0de"))
    steps = []
    h = ota.install_release(c, rel, {"brightness": 30}, progress=steps.append, **QUIET)
    assert c.log == [("fw", len(IMAGE)), ("deskpak", len(DESK)), ("save", {"brightness": 30})]
    assert h["deskCrc"] == crc(DESK) and h["fwSeq"] == 6
    assert steps[0] == "check" and "upload" in steps and "deskpak" in steps and steps[-1] == "done"


def test_install_release_packs_only_and_nothing_when_current():
    ff = FakeFeed()
    ff.publish(6)
    rel = fetched(ff)
    c = FakeClock(heap(6, desk=0, pakCrc=crc(CLAWD), deskCrc=""), {})
    ota.install_release(c, rel, {}, **QUIET)
    assert c.log == [("deskpak", len(DESK))]
    c = FakeClock(heap(6, pakCrc=crc(CLAWD), deskCrc=crc(DESK)), {})
    ota.install_release(c, rel, {}, **QUIET)
    assert c.log == []


def test_pack_upload_retries_once_then_fails():
    ff = FakeFeed()
    ff.publish(6)
    rel = fetched(ff)
    c = FakeClock(heap(6, pakCrc="1", deskCrc=crc(DESK)), {}, fail_uploads=1)
    ota.install_release(c, rel, {}, **QUIET)
    assert c.log == [("clawdpak", len(CLAWD))]
    c = FakeClock(heap(6, pakCrc="1", deskCrc=crc(DESK)), {}, bad_crc=2)
    with pytest.raises(ClockError, match="damaged"):
        ota.install_release(c, rel, {}, **QUIET)
    released = []
    c = FakeClock(heap(6, pakCrc="1", deskCrc=crc(DESK)), {}, fail_uploads=2)
    with pytest.raises(ClockError):
        ota.install_release(c, rel, {}, hold=lambda *_: None, release=lambda: released.append(1),
                            sleep=lambda _s: None)
    assert released == [1]


def test_release_without_styles_and_a_bad_style_index():
    files = release_files(styles=())
    ff = FakeFeed()
    ff.publish(6, files=files, style_api=0)
    assert fetched(ff).styles == ()
    bad = [f if f[0] != "styles/index.json" else (f[0], f[1], b"{not json") for f in release_files()]
    ff.publish(7, files=bad)
    assert fetched(ff, 7).styles == ()


def test_encrypted_files_are_cached_per_release_and_reused(tmp_path):
    ff = FakeFeed()
    ff.publish(6)
    f = ff.client(cache_dir=tmp_path)
    m = f.latest()
    assert m is not None
    rel = f.fetch(feed.make_release(m), feed.STYLE_KINDS)
    files = [c for c in ff.calls if c.startswith("GET /v1/file/")]
    assert len(files) == 3
    cached = {p.name: p.read_bytes() for p in (tmp_path / "6").iterdir()}
    assert sorted(cached) == ["styles__index.json", "styles__paper.pak", "styles__synth.pak"]
    assert all(b"synth" not in b and b"paper" not in b for b in cached.values())   # still encrypted
    ff.calls.clear()
    again = ff.client(cache_dir=tmp_path)
    rel2 = again.fetch(feed.make_release(m), feed.STYLE_KINDS)
    assert [p.id for p in rel2.styles] == [p.id for p in rel.styles]
    assert not [c for c in ff.calls if c.startswith("GET /v1/file/")]        # only the key was asked for
    assert any(c == "POST /v1/grant" for c in ff.calls)


def test_a_damaged_cache_file_is_downloaded_again_and_old_releases_are_dropped(tmp_path):
    ff = FakeFeed()
    ff.publish(6)
    f = ff.client(cache_dir=tmp_path)
    m = f.latest()
    assert m is not None
    f.fetch(feed.make_release(m), feed.STYLE_KINDS)
    victim = tmp_path / "6" / "styles__synth.pak"
    data = victim.read_bytes()
    victim.write_bytes(data[:-1] + bytes([data[-1] ^ 1]))                    # same size, wrong tag
    ff.calls.clear()
    rel = f.fetch(feed.make_release(m), feed.STYLE_KINDS)
    assert [c for c in ff.calls if c.startswith("GET /v1/file/")] == ["GET /v1/file/6/styles/synth.pak"]
    assert [p.id for p in rel.styles] == ["synth", "paper"]
    ff.publish(7)
    g = ff.client(cache_dir=tmp_path)
    m7 = g.latest()
    assert m7 is not None
    g.fetch(feed.make_release(m7), feed.STYLE_KINDS)
    assert [p.name for p in tmp_path.iterdir()] == ["7"]
