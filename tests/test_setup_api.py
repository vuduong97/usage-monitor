"""The settings window's setup wizard and Agent tab (settings_api), without pywebview."""
import json
from pathlib import Path

import pytest

from clawd_desktop import agents, paths, settings_api as sa
from clawd_desktop.discovery import Clock, ClockStore
from tests.test_settings_api import FakeApi


class FakeMdns:
    def __init__(self, clocks):
        self.clocks = clocks
        self.started = self.stopped = False

    def start(self):
        self.started = True
        return True

    def names(self):
        return sorted(self.clocks)

    def resolve(self, n):
        return self.clocks.get(n)

    def stop(self):
        self.stopped = True


@pytest.fixture
def home(tmp_path, monkeypatch):
    monkeypatch.setattr(paths, "CONFIG_DIR", tmp_path / "cfg")
    monkeypatch.setattr(paths, "PREFS_FILE", tmp_path / "cfg" / "desktop.json")
    return tmp_path


def make(home, clocks=None, base=""):
    notes = []
    reg = agents.registry(home=home, platform="darwin", port=47913, script=Path("/x/clawd-event.sh"),
                          which=lambda _n: None, env={})
    for a in reg.values():
        a.prepare = lambda: None
    reg = {k: v for k, v in reg.items() if k != "claude"}        # Claude reads the real ~/.claude dirs
    mdns = FakeMdns(clocks or {})
    b = sa.SettingsApi(base, "", notify=notes.append, api_factory=lambda b, auth=None: FakeApi(b),
                       store=ClockStore(home / "clock.json"), agents_reg=reg, mdns_factory=lambda: mdns)
    return b, notes, mdns


def test_agents_lists_found_and_marks_them_offered(home, monkeypatch):
    monkeypatch.setattr(sa, "_has_wsl", lambda: False)        # GitHub's Windows runners have wsl.exe
    (home / ".codex").mkdir()
    b, _, _ = make(home)
    r = b.agents()
    by = {a["id"]: a for a in r["agents"]}
    assert by["codex"]["found"] and not by["codex"]["installed"] and by["codex"]["beta"]
    assert not by["gemini"]["found"] and "signals" in by["codex"] and r["wsl"] is False
    assert json.loads(paths.PREFS_FILE.read_text())["agents_seen"] == ["codex"]


def test_set_agent_and_errors_are_shown(home):
    (home / ".cursor").mkdir()
    b, notes, _ = make(home)
    r = b.set_agent("cursor", True)
    assert r["ok"] and {a["id"]: a for a in r["agents"]}["cursor"]["installed"]
    assert {"cmd": "setup"} in notes
    (home / ".gemini").mkdir()
    (home / ".gemini/settings.json").write_text("{broken")
    r = b.set_agent("gemini", True)
    assert not r["ok"] and "JSON" in r["error"]
    assert b.set_agent("nobody", True)["code"] == "bad"


def test_set_agents_syncs_to_the_ticks(home):
    (home / ".codex").mkdir()
    (home / ".cursor").mkdir()
    b, _, _ = make(home)
    b.set_agent("cursor", True)
    r = b.set_agents(["codex"])
    by = {a["id"]: a["installed"] for a in r["agents"]}
    assert r["ok"] and by["codex"] and not by["cursor"]
    r = b.disconnect_all()
    assert not any(a["installed"] for a in r["agents"])


def test_usage_sources_and_choice(home, monkeypatch):
    from clawd_desktop import codex_usage, usage
    claude = home / ".claude"
    claude.mkdir()
    monkeypatch.setattr(usage, "read_config_dirs", lambda: [claude])
    monkeypatch.setattr(usage, "read_onoff_setting", lambda name, default="off": "off")
    monkeypatch.setattr(codex_usage, "available", lambda: True)
    monkeypatch.setattr(codex_usage, "read", lambda: None)
    b, notes, _ = make(home)
    r = b.usage_sources()
    assert [o["id"] for o in r["options"]] == ["claude", "codex", "off"] and r["current"] == "claude"
    assert "chưa có giới hạn" in r["options"][1]["detail"]
    assert b.set_usage_source("codex")["current"] == "codex" and {"cmd": "usage_source"} in notes
    assert b.set_usage_source("gemini")["code"] == "bad"
    b.set_usage_source("claude")
    assert "usage_source" not in json.loads(paths.PREFS_FILE.read_text())
    work = home / ".claude-work"
    work.mkdir()
    monkeypatch.setattr(usage, "read_config_dirs", lambda: [claude, work])
    ids = [o["id"] for o in b.usage_sources()["options"]]
    assert ids[:3] == ["claude", f"claude:{claude}", f"claude:{work}"]
    assert b.set_usage_source(f"claude:{work}")["current"] == f"claude:{work}"


def test_setup_state_and_finish(home):
    b, notes, _ = make(home)
    s = b.setup_state()
    assert s["pending"] == ["clock", "agents", "usage"] and s["fresh"]
    s = b.finish_setup(["clock", "agents"])
    assert s["pending"] == ["usage"] and not s["fresh"] and {"cmd": "setup"} in notes


def test_clock_step_lists_mdns_clocks_and_picks_one_by_name(home, monkeypatch):
    monkeypatch.setattr(sa, "answers", lambda base: base == "http://10.0.0.7")
    c = Clock(host="10.0.0.7", name="clawd-desk", version="2.16.0")
    b, notes, mdns = make(home, {"clawd-desk._clawd._tcp.local.": c})
    r = b.setup_clocks()
    assert mdns.started and r["clocks"][0]["name"] == "clawd-desk" and r["current"] is None
    r = b.setup_use_clock("clawd-desk")
    assert r["ok"] and r["current"]["answers"]
    assert notes[-1] == {"cmd": "use_clock", "name": "clawd-desk", "host": "10.0.0.7", "port": 80,
                         "version": "2.16.0"}
    assert b.setup_use_clock("other")["code"] == "gone"
    b.close_mdns()
    assert mdns.stopped
