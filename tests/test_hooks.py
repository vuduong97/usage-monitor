"""Claude Code hook install/uninstall, the hook script, the `clawd` CLI, end to end
against the real event server."""
import asyncio
import importlib.machinery
import importlib.util
import json
import socket
import subprocess
import sys
import threading
import time
from pathlib import Path

import pytest

from clawd_desktop import events as ev
from clawd_desktop import hooks

ROOT = Path(__file__).resolve().parents[1]
CLI = ROOT / "integrations" / "cli" / "clawd"

EXISTING = {
    "model": "opus",
    "hooks": {
        "PreToolUse": [{"matcher": "Write", "hooks": [{"type": "command", "command": "node guard.cjs"}]}],
        "Stop": [{"matcher": "*", "hooks": [{"type": "command", "command": "node heartbeat.cjs"}]}],
    },
}
OLD_CLAWD_HOME = {"matcher": "*", "hooks": [{"type": "command",
                                             "command": "'/x/clawd-home/integrations/claude/clawd-event.sh'"}]}


def free_port() -> int:
    s = socket.socket()
    s.bind(("127.0.0.1", 0))
    p = s.getsockname()[1]
    s.close()
    return p


class LiveServer:
    """The real EventServer on a free port, in a background event loop."""

    def __enter__(self):
        self.brain = ev.Brain(ev.Pet(localtime=lambda _t: time.struct_time((2026, 10, 11, 14, 0, 0, 6, 284, 0))))
        self.loop = asyncio.new_event_loop()
        self.server = ev.EventServer(self.brain, port=0)
        started = threading.Event()

        def run():
            asyncio.set_event_loop(self.loop)
            self.loop.run_until_complete(self.server.start())
            started.set()
            self.loop.run_forever()
        self.thread = threading.Thread(target=run, daemon=True)
        self.thread.start()
        assert started.wait(5)
        return self

    def __exit__(self, *exc):
        asyncio.run_coroutine_threadsafe(self.server.close(), self.loop).result(5)
        self.loop.call_soon_threadsafe(self.loop.stop)
        self.thread.join(5)


# ---- settings merge ------------------------------------------------------------
def test_merge_keeps_existing_hooks_and_is_idempotent():
    h = hooks.handler("darwin", port=47913, script=Path("/x/clawd-event.sh"))
    once = hooks.merge(EXISTING, h)
    assert once["model"] == "opus"
    assert once["hooks"]["PreToolUse"][0] == EXISTING["hooks"]["PreToolUse"][0]
    for e in hooks.EVENTS:
        assert sum(hooks._ours(g) for g in once["hooks"][e]) == 1
    assert hooks.merge(once, h) == once
    assert hooks.merge(once, None) == EXISTING


def test_merge_replaces_the_old_clawd_home_hook():
    old = json.loads(json.dumps(EXISTING))
    old["hooks"]["Stop"].append(OLD_CLAWD_HOME)
    script = Path("/new/clawd-event.sh")
    new = hooks.merge(old, hooks.handler("darwin", port=47913, script=script))
    stops = [g["hooks"][0]["command"] for g in new["hooks"]["Stop"]]
    assert stops == ["node heartbeat.cjs", f"'{script}'"]       # the path as the OS writes it (\\ on Windows)


def test_windows_hook_is_async_curl_in_exec_form():
    h = hooks.handler("win32", port=47913)
    assert h["command"] == "curl.exe" and h["async"] is True
    assert h["args"][-1] == "http://127.0.0.1:47913/hook/claude" and "@-" in h["args"]
    assert hooks._ours({"hooks": [h]})
    once = hooks.merge(EXISTING, h)
    assert hooks.merge(once, None) == EXISTING


def test_apply_backs_up_installs_and_removes(tmp_path):
    f = tmp_path / "settings.json"
    f.write_text(json.dumps(EXISTING))
    h = hooks.handler("darwin", port=47913, script=tmp_path / "clawd-event.sh")
    assert hooks.apply(True, files=[f], hook=h) == [f"{f}: hooks installed"]
    assert list(tmp_path.glob("settings.json.bak-clawd-*"))
    assert hooks.is_installed([f])
    assert hooks.apply(True, files=[f], hook=h) == [f"{f}: already up to date"]
    hooks.apply(False, files=[f])
    assert json.loads(f.read_text()) == EXISTING and not hooks.is_installed([f])


def test_apply_leaves_broken_or_missing_dirs_alone(tmp_path):
    bad = tmp_path / "settings.json"
    bad.write_text("{nope")
    assert hooks.apply(True, files=[bad], hook={"type": "command", "command": "x clawd-event"}) == \
        [f"{bad}: not valid JSON, left alone"]
    assert bad.read_text() == "{nope"
    missing = tmp_path / "no-claude-here" / "settings.json"
    assert hooks.apply(True, files=[missing], hook={"command": "clawd-event"}) == []
    assert not missing.parent.exists()


def test_apply_creates_settings_in_an_existing_claude_dir(tmp_path):
    f = tmp_path / "settings.json"
    hooks.apply(True, files=[f], hook=hooks.handler("win32", port=1234))
    data = json.loads(f.read_text())
    assert set(data["hooks"]) == set(hooks.EVENTS)


# ---- the hook script -------------------------------------------------------------
@pytest.fixture()
def script(tmp_path):
    return hooks.write_script(tmp_path / "clawd-event.sh", port=47913)


def run_hook(script, port, payload: dict):
    t = time.perf_counter()
    p = subprocess.run(["sh", str(script)], input=json.dumps(payload).encode(),
                       env={"PATH": "/usr/bin:/bin", "CLAWD_EVENT_PORT": str(port)},
                       capture_output=True, timeout=10)
    return p, (time.perf_counter() - t) * 1000


@pytest.mark.skipif(sys.platform == "win32", reason="sh hook is the macOS/Linux path")
def test_hook_is_fast_and_silent_without_app(script):
    times = []
    for _ in range(5):
        p, ms = run_hook(script, free_port(), {"hook_event_name": "Stop", "session_id": "x"})
        assert p.returncode == 0 and p.stdout == b"" and p.stderr == b""
        times.append(ms)
    assert sorted(times)[2] < 100, times


@pytest.mark.skipif(sys.platform == "win32", reason="sh hook is the macOS/Linux path")
def test_hook_forwards_payload_to_app(script):
    with LiveServer() as srv:
        p, _ = run_hook(script, srv.server.port, {"hook_event_name": "PreToolUse", "session_id": "abc",
                                                  "tool_name": "Bash", "tool_input": {"command": "ls"}})
        assert p.returncode == 0 and p.stdout == b""
        s = srv.brain.tracker.sessions["claude:abc"]
        assert (s.state, s.tool) == ("working", "Bash")


# ---- CLI -------------------------------------------------------------------------
def load_cli():
    loader = importlib.machinery.SourceFileLoader("clawd_cli", str(CLI))
    spec = importlib.util.spec_from_loader("clawd_cli", loader)
    mod = importlib.util.module_from_spec(spec)
    loader.exec_module(mod)
    return mod


def test_cli_builds_requests():
    cli = load_cli()
    assert cli.build(["say", "drink", "water", "--secs", "5"]) == \
        ("POST", "/poke", {"cmd": "say", "text": "drink water", "secs": 5.0})
    assert cli.build(["play", "double", "jump"]) == ("POST", "/poke", {"cmd": "play", "anim": "double jump"})
    assert cli.build(["status"]) == ("GET", "/state", None)
    with pytest.raises(SystemExit):
        cli.build(["dance"])


def test_cli_end_to_end(monkeypatch, capsys):
    cli = load_cli()
    monkeypatch.setenv("CLAWD_EVENT_PORT", str(free_port()))
    assert cli.main(["wave"]) == 1                      # app not running
    with LiveServer() as srv:
        monkeypatch.setenv("CLAWD_EVENT_PORT", str(srv.server.port))
        assert cli.main(["say", "drink water"]) == 0
        d = srv.brain.desired(time.time())
        assert (d["a"], d["txt"]) == ("idle", "Drink water")
        capsys.readouterr()
        assert cli.main(["status"]) == 0
        assert "clock: idle - Drink water" in capsys.readouterr().out
