"""Puts the repo root on sys.path so `import clawd_desktop` works from tests."""
import os
import sys

import pytest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))


@pytest.fixture(autouse=True)
def _private_pause_file(tmp_path, monkeypatch):
    """Never touch the real ~/.config pause lease (a running Clawd reads it)."""
    from clawd_desktop import pause
    monkeypatch.setattr(pause, "PAUSE_FILE", tmp_path / "pause.json")


@pytest.fixture(autouse=True)
def _private_prefs_file(tmp_path, monkeypatch):
    """Same for the app preferences (desktop.json)."""
    from clawd_desktop import paths
    monkeypatch.setattr(paths, "PREFS_FILE", tmp_path / "desktop.json")
