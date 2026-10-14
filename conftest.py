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


@pytest.fixture(autouse=True)
def _private_payload_file(tmp_path, monkeypatch):
    """And the last usage payload the settings window draws the clock from."""
    from clawd_desktop import paths
    monkeypatch.setattr(paths, "PAYLOAD_FILE", tmp_path / "usage_payload.json")
    monkeypatch.setattr(paths, "PAK_FILE", tmp_path / "clawd.pak")
    monkeypatch.setattr(paths, "RENDERER_DIR", tmp_path / "renderer-feed")
