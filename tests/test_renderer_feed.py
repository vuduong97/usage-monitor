"""The clock-screen renderer of a feed release: the manifest's `renderers` list, download,
unpack, the checks before each use, and the settings window switching to it."""
from __future__ import annotations

import io
import json
import sys
import zipfile

import pytest

import clawd_desktop.preview as pv
from clawd_desktop import feed
from tests.feedkit import FakeFeed, renderer_zip
from tests.test_settings_api import FEED_HEAP, bridge, feed_settled

PLAT = feed.renderer_platform()
OTHER = "mac" if PLAT != "mac" else "win-x64"


def latest_release(c: feed.Feed) -> feed.Release:
    m = c.latest()
    assert m is not None
    return feed.make_release(m)


def renderer_of(c: feed.Feed, rel: feed.Release, plat: str) -> tuple[feed.FeedFile, bytes]:
    got = c.fetch_renderer(rel, plat)
    assert got is not None
    return got
posix_only = pytest.mark.skipif(sys.platform == "win32", reason="the fake renderer is a shell script")


# ---- manifest ------------------------------------------------------------------------------
def test_manifest_lists_renderers_per_platform(tmp_path):
    ff = FakeFeed()
    z = renderer_zip(tmp_path)
    ff.publish(6, renderers=[("win-x64", z), ("mac", z)])
    m = ff.client().latest()
    assert m is not None
    assert [r.platform for r in m.renderers] == ["win-x64", "mac"]
    mac = m.renderer("mac")
    assert mac is not None and mac.name == "renderer/mac.zip" and m.renderer("linux-x64") is None
    assert all(f.kind != "renderer" for f in m.files)         # never among the files older apps read


def test_a_release_without_renderers_parses_as_before():
    m = feed.parse_manifest(FakeFeed().publish(6))
    assert m.renderers == () and m.renderer(PLAT) is None


@pytest.mark.parametrize("change", [
    lambda r: r.update(platform="amiga"),
    lambda r: r.update(kind="fw"),
    lambda r: r.update(name="../x.zip"),
    lambda r: r.update(name="firmware.bin"),                   # clashes with a file
    lambda r: r.update(sha256="nope"),
    lambda r: r.update(enc=r["size"]),
    lambda r: r.update(size=feed.RENDERER_MAX + 1, enc=feed.RENDERER_MAX + 29),
])
def test_a_bad_renderer_row_drops_the_renderers_never_the_release(tmp_path, change):
    m = FakeFeed().publish(6, renderers=[("win-x64", renderer_zip(tmp_path)), ("mac", b"zip")])
    change(m["renderers"][0])
    parsed = feed.parse_manifest(m)
    assert parsed.renderers == () and parsed.file("fw") is not None


def test_two_renderers_for_one_platform_are_refused(tmp_path):
    m = FakeFeed().publish(6, renderers=[("mac", b"a")])
    m["renderers"].append({**m["renderers"][0], "name": "renderer/mac2.zip"})
    assert feed.parse_manifest(m).renderers == ()


def test_renderer_platform_names_the_running_system(monkeypatch):
    for plat, want in (("win32", "win-x64"), ("darwin", "mac"), ("linux", "linux-x64")):
        monkeypatch.setattr(feed.sys, "platform", plat)
        assert feed.renderer_platform() == want


# ---- download ------------------------------------------------------------------------------
def test_fetch_renderer_downloads_decrypts_and_checks(tmp_path):
    ff = FakeFeed()
    z = renderer_zip(tmp_path)
    ff.publish(6, renderers=[("win-x64", z), ("mac", b"other")])
    c = ff.client(cache_dir=tmp_path / "cache")
    rel = latest_release(c)
    f, data = renderer_of(c, rel, "win-x64")
    assert data == z and f.platform == "win-x64"
    assert ff.got("renderer/win-x64.zip") == 1 and ff.got("renderer/mac.zip") == 0   # only this platform's
    assert c.fetch_renderer(rel, "linux-x64") is None
    ff.calls.clear()
    assert renderer_of(c, rel, "win-x64")[1] == z                                  # from the encrypted cache
    assert ff.got("renderer/win-x64.zip") == 0
    ff.tamper.add("renderer/mac.zip")
    with pytest.raises(feed.FeedError):
        c.fetch_renderer(rel, "mac")


def test_a_worker_that_does_not_sign_renderers_is_a_feed_error(tmp_path):
    ff = FakeFeed()
    ff.sign_renderers = False
    ff.publish(6, renderers=[(PLAT, renderer_zip(tmp_path))])
    c = ff.client()
    with pytest.raises(feed.FeedError, match="no URL"):
        c.fetch_renderer(latest_release(c), PLAT)


# ---- unpack --------------------------------------------------------------------------------
SHA = "ab" * 32


def test_unpack_then_load_and_every_file_is_checked(tmp_path):
    root = tmp_path / "r"
    folder = pv.unpack_renderer(renderer_zip(tmp_path), root, SHA)
    assert folder == root / SHA[:16]
    r = pv.load_unpacked(root, SHA)
    assert r is not None and sorted(r.styles) == ["paper", "synth"] and r.commit == "c000006"
    assert pv.load_unpacked(root, "cd" * 32) is None                    # another zip: not unpacked
    assert pv.load_unpacked(root, SHA, "zzzzzzz") is None               # built from another firmware
    (folder / "styles" / "synth.pak").write_bytes(b"changed")
    assert pv.load_unpacked(root, SHA) is None                          # a changed file: unpack again
    pv.unpack_renderer(renderer_zip(tmp_path), root, SHA)
    assert pv.load_unpacked(root, SHA) is not None


@pytest.mark.skipif(sys.platform == "win32", reason="POSIX file modes")
def test_unpacked_program_is_executable(tmp_path):
    folder = pv.unpack_renderer(renderer_zip(tmp_path), tmp_path / "r", SHA)
    assert (folder / "clawd_render").stat().st_mode & 0o100


def test_unpacking_a_newer_renderer_drops_the_older(tmp_path):
    root = tmp_path / "r"
    pv.unpack_renderer(renderer_zip(tmp_path), root, SHA)
    pv.unpack_renderer(renderer_zip(tmp_path, commit="c000007"), root, "cd" * 32)
    assert [p.name for p in root.iterdir()] == ["cd" * 8]


def _zip(files: dict) -> bytes:
    b = io.BytesIO()
    with zipfile.ZipFile(b, "w") as z:
        for n, data in files.items():
            z.writestr(n, data)
    return b.getvalue()


GOOD = {"preview.json": json.dumps({"exe": "clawd_render", "pak": "clawd.pak"}), "clawd_render": "x",
        "clawd.pak": "CLWD"}


@pytest.mark.parametrize("files", [
    {**GOOD, "../evil": "x"},
    {**GOOD, "/abs": "x"},
    {**GOOD, "styles/../../x.pak": "x"},
    {**GOOD, "other.dll": "x"},
    {k: v for k, v in GOOD.items() if k != "preview.json"},
    {**GOOD, "preview.json": "not json"},
    {**GOOD, "preview.json": json.dumps({"exe": "clawd.pak", "pak": "clawd.pak"})},      # not the program
    {**GOOD, "preview.json": json.dumps({"exe": "clawd_render.exe", "pak": "clawd.pak"})},   # not in the zip
    {k: v for k, v in GOOD.items() if k != "clawd.pak"},
])
def test_a_zip_that_is_not_a_renderer_is_refused_and_writes_nothing(tmp_path, files):
    with pytest.raises(ValueError):
        pv.unpack_renderer(_zip(files), tmp_path / "r", SHA)
    assert not (tmp_path / "r").exists() or not any((tmp_path / "r").iterdir())


def test_not_a_zip_at_all(tmp_path):
    with pytest.raises(ValueError):
        pv.unpack_renderer(b"PK nope", tmp_path / "r", SHA)


# ---- the settings window -------------------------------------------------------------------
def test_window_previews_with_the_releases_renderer_and_reuses_it(tmp_path):
    ff = FakeFeed()
    ff.publish(6, renderers=[(PLAT, renderer_zip(tmp_path))])
    b, _m, _k, _n = bridge(tmp_path, heap=FEED_HEAP, feed=ff.client())
    assert b._renderer is None
    b.state()
    feed_settled(b)
    assert b._renderer is not None and sorted(b._renderer.styles) == ["paper", "synth"]
    assert ff.got(f"renderer/{PLAT}.zip") == 1
    ff.calls.clear()
    b2, _m, _k, _n = bridge(tmp_path, heap=FEED_HEAP, feed=ff.client())     # the next window
    b2.state()
    feed_settled(b2)
    assert b2._renderer is not None and ff.got(f"renderer/{PLAT}.zip") == 0   # unpacked already: no download


def test_window_keeps_its_fallback_when_the_renderer_is_missing_or_unusable(tmp_path, capsys):
    ff = FakeFeed()
    ff.publish(6, renderers=[(OTHER, renderer_zip(tmp_path))])
    b, _m, _k, _n = bridge(tmp_path, heap=FEED_HEAP, feed=ff.client())
    b.state()
    feed_settled(b)
    assert b._renderer is None                                  # none for this computer
    ff.publish(7, renderers=[(PLAT, _zip({**GOOD, "other.dll": "x"}))])
    b2, _m, _k, _n = bridge(tmp_path, heap=FEED_HEAP, feed=ff.client())
    b2.state()
    fw = feed_settled(b2)
    assert b2._renderer is None and fw["bundle"]["seq"] == 7   # the release itself still counts
    assert "renderer from the feed" in capsys.readouterr().out
    ff.publish(8, renderers=[(PLAT, renderer_zip(tmp_path, commit="another"))])
    b3, _m, _k, _n = bridge(tmp_path, heap=FEED_HEAP, feed=ff.client())
    b3.state()
    feed_settled(b3)
    assert b3._renderer is None                                 # built from another commit than the release
    assert "does not match" in capsys.readouterr().out


def test_window_without_a_worker_that_signs_renderers_still_gets_the_packs(tmp_path):
    ff = FakeFeed()
    ff.sign_renderers = False
    ff.publish(6, renderers=[(PLAT, renderer_zip(tmp_path))])
    b, _m, _k, _n = bridge(tmp_path, heap=FEED_HEAP, feed=ff.client())
    b.state()
    fw = feed_settled(b)
    assert b._renderer is None and [p["id"] for p in fw["styles"]["packs"]] == ["synth", "paper"]


@posix_only
def test_usage_preview_draws_with_the_feed_renderer(tmp_path):
    ff = FakeFeed()
    ff.publish(6, renderers=[(PLAT, renderer_zip(tmp_path, body=f"cat > /dev/null; head -c {pv.FRAME_BYTES} /dev/zero"))])
    b, _m, _k, _n = bridge(tmp_path, heap=FEED_HEAP, feed=ff.client())
    b.state()
    feed_settled(b)
    r = b.usage_preview(["classic", "synth"], False)
    assert r["exact"] and sorted(r["frames"]) == ["classic", "synth"]


@pytest.mark.parametrize(("a", "b", "same"), [
    ("abc1234", "abc1234", True),
    ("abc1234", "abc1234def0", True),                 # git abbreviates per checkout
    ("ABC1234DEF", "abc1234", True),
    ("abc1234", "abc1235", False),
    ("abc12", "abc1234", False),                      # too short to tell
    ("abc12", "abc12", True),
])
def test_same_commit(a, b, same):
    assert pv.same_commit(a, b) is same


def test_a_renderer_row_with_an_unhashable_kind_drops_only_the_renderers(tmp_path):
    m = FakeFeed().publish(6, renderers=[("win-x64", renderer_zip(tmp_path))])
    m["renderers"][0]["kind"] = []
    parsed = feed.parse_manifest(m)
    assert parsed.renderers == () and parsed.file("fw") is not None


def test_a_renderer_for_another_render_api_is_not_run(tmp_path):
    root = tmp_path / "r"
    folder = pv.unpack_renderer(renderer_zip(tmp_path), root, SHA)
    m = json.loads((folder / "preview.json").read_text())
    (folder / "preview.json").write_text(json.dumps({**m, "renderApi": pv.RENDER_API + 1}))
    assert pv.load_renderer(folder) is None


@pytest.mark.parametrize("name", ["C:/x.pak", "styles\\x.pak", "..\\evil", "styles\\..\\..\\x.pak", "\\abs"])
def test_windows_style_names_never_leave_the_folder(tmp_path, name):
    """zipfile turns a backslash into "/" on Windows (so styles\\x.pak is styles/x.pak there);
    elsewhere such a name is refused. Either way nothing lands outside the renderer folder."""
    root = tmp_path / "r"
    try:
        folder = pv.unpack_renderer(_zip({**GOOD, name: "x"}), root, SHA)
    except ValueError:
        assert not root.exists() or not any(root.iterdir())
        return
    assert name == "styles\\x.pak" and sys.platform == "win32"
    assert all(p.resolve().is_relative_to(folder.resolve()) for p in root.rglob("*") if p.is_file())
    assert (folder / "styles" / "x.pak").is_file()


def test_size_and_count_limits(tmp_path, monkeypatch):
    monkeypatch.setattr(pv, "UNPACK_MAX", 10)
    with pytest.raises(ValueError):
        pv.unpack_renderer(_zip({**GOOD, "clawd.pak": "CLWD" * 10}), tmp_path / "r", SHA)
    monkeypatch.setattr(pv, "UNPACK_MAX", 10_000)
    monkeypatch.setattr(pv, "UNPACK_FILES", 2)
    with pytest.raises(ValueError):
        pv.unpack_renderer(_zip(GOOD), tmp_path / "r", SHA)


def test_a_marker_naming_a_file_outside_the_folder_is_not_trusted(tmp_path):
    root = tmp_path / "r"
    folder = pv.unpack_renderer(renderer_zip(tmp_path), root, SHA)
    mark = json.loads((folder / pv.MARK).read_text())
    mark["files"]["../outside"] = "00" * 32
    (folder / pv.MARK).write_text(json.dumps(mark))
    assert pv.load_unpacked(root, SHA) is None
