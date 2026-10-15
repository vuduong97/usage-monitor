"""desktop.json is shared by two processes (the tray's engine and the settings window):
no update may be lost, and a file that is there but unreadable for a moment is never
taken for an empty one."""
import json
import threading
from pathlib import Path

import pytest

from clawd_desktop import paths


@pytest.fixture(autouse=True)
def quick(monkeypatch):
    monkeypatch.setattr(paths, "PREFS_IO_WAIT", 0.3)


def read():
    return json.loads(paths.PREFS_FILE.read_text())


def flaky(monkeypatch, method, fails):
    """Path.<method> on desktop.json raises a Windows sharing violation `fails` times."""
    real = getattr(Path, method)
    left = [fails]

    def fake(self, *a, **k):
        hit = self == paths.PREFS_FILE or (method == "replace" and Path(a[0]) == paths.PREFS_FILE)
        if hit and left[0] != 0:
            left[0] -= 1
            raise PermissionError(13, "The process cannot access the file")
        return real(self, *a, **k)
    monkeypatch.setattr(Path, method, fake)


def test_concurrent_updates_keep_every_key():
    paths.save_prefs({"keep": True})
    n = 12

    def worker(i):
        for j in range(5):
            paths.update_prefs(lambda p, j=j: p.__setitem__(f"k{i}", j))
    ts = [threading.Thread(target=worker, args=(i,)) for i in range(n)]
    for t in ts:
        t.start()
    for t in ts:
        t.join()
    assert read() == {"keep": True, **{f"k{i}": 4 for i in range(n)}}
    assert [p.name for p in paths.PREFS_FILE.parent.glob("*.tmp")] == []


def test_update_never_overwrites_a_file_it_could_not_read(monkeypatch):
    paths.save_prefs({"clock_screen": 0, "setup_done": ["clock"]})
    before = paths.PREFS_FILE.read_bytes()
    flaky(monkeypatch, "read_text", -1)                 # unreadable for good
    with pytest.raises(OSError):
        paths.update_prefs(lambda p: p.__setitem__("usage_source", "codex"))
    with pytest.raises(OSError):
        paths.set_clock_screen(1)
    assert paths.load_prefs() == {}                     # readers still get a value
    assert paths.PREFS_FILE.read_bytes() == before


def test_a_short_sharing_violation_is_waited_out(monkeypatch):
    paths.save_prefs({"clock_screen": 1, "setup_done": ["clock"]})
    flaky(monkeypatch, "read_text", 2)
    assert paths.load_prefs() == {"clock_screen": 1, "setup_done": ["clock"]}
    flaky(monkeypatch, "replace", 2)                    # a reader holds desktop.json open
    paths.set_clock_screen(0)
    assert read() == {"clock_screen": 0, "setup_done": ["clock"]}


def test_missing_or_corrupt_file_is_a_fresh_start():
    assert paths.load_prefs() == {}
    assert paths.update_prefs(lambda p: p.__setitem__("a", 1)) == {"a": 1}
    paths.PREFS_FILE.write_text("{not json")
    assert paths.load_prefs() == {}
    assert paths.update_prefs(lambda p: {"b": 2}) == {"b": 2} and read() == {"b": 2}


def test_unchanged_update_does_not_write():
    paths.save_prefs({"clock_screen": 1})
    mtime = paths.PREFS_FILE.stat().st_mtime_ns
    paths.set_clock_screen(1)
    paths.update_prefs(lambda p: None)
    assert paths.PREFS_FILE.stat().st_mtime_ns == mtime
