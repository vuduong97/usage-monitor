"""Setup wizard bookkeeping (firstrun.py) and the uninstaller's cleanup."""
import json

import pytest

from clawd_desktop import __version__, firstrun, paths


@pytest.fixture
def prefs_dir(tmp_path, monkeypatch):
    monkeypatch.setattr(paths, "CONFIG_DIR", tmp_path)
    monkeypatch.setattr(paths, "PREFS_FILE", tmp_path / "desktop.json")
    return tmp_path


def write(d, data):
    (d / "desktop.json").write_text(json.dumps(data))


def read(d):
    return json.loads((d / "desktop.json").read_text())


def test_fresh_install_sees_every_step(prefs_dir):
    assert firstrun.pending() == ["clock", "agents", "usage"]
    assert firstrun.should_open()


def test_upgrade_from_0_1_sees_only_the_new_steps(prefs_dir):
    write(prefs_dir, {"first_run_done": True, "autostart": True})
    assert firstrun.load()["setup_done"] == ["clock"]
    assert read(prefs_dir)["autostart"] is True
    assert firstrun.pending() == ["agents", "usage"]


def test_steps_done_and_opened_once_per_version(prefs_dir):
    firstrun.mark_shown()
    assert read(prefs_dir)["setup_shown"] == __version__ and not firstrun.should_open()
    assert firstrun.mark_done(["clock", "nonsense", "agents"]) == ["usage"]
    write(prefs_dir, {**read(prefs_dir), "setup_shown": "0.0.1"})
    assert firstrun.should_open()
    firstrun.mark_done(["usage"])
    assert firstrun.pending() == [] and not firstrun.should_open()


def test_a_new_agent_is_offered_once(prefs_dir):
    assert firstrun.new_agents(["claude", "codex", "pi"], installed=["claude"]) == ["codex", "pi"]
    firstrun.mark_seen(["codex"])
    assert firstrun.new_agents(["claude", "codex", "pi"], installed=["claude"]) == ["pi"]
    firstrun.mark_seen(["pi"])
    assert firstrun.new_agents(["claude", "codex", "pi", "gemini"], installed=[]) == ["claude", "gemini"]


def test_first_run_installs_no_hooks(prefs_dir, monkeypatch):
    from clawd_desktop import agents, hooks, tray
    monkeypatch.setattr(paths, "HOOK_SCRIPT", prefs_dir / "clawd-event.sh")
    monkeypatch.setattr(tray.autostart, "supported", lambda: False)
    monkeypatch.setattr(agents, "refresh_plugins", lambda: [])
    called = []
    monkeypatch.setattr(hooks, "apply", lambda *a, **k: called.append(a))
    tray.first_run()
    assert called == []
    assert read(prefs_dir)["first_run_done"] is True
    assert firstrun.pending() == ["clock", "agents", "usage"]


def test_cleanup_disconnects_every_agent(monkeypatch, capsys):
    from clawd_desktop import agents, autostart, cli
    monkeypatch.setattr(autostart, "disable", lambda: None)
    monkeypatch.setattr(agents, "uninstall_all", lambda: ["claude: Đã ngắt.", "pi: Chưa nối."])
    assert cli.main(["cleanup"]) == 0
    out = capsys.readouterr().out
    assert "claude: Đã ngắt." in out and "pi: Chưa nối." in out
