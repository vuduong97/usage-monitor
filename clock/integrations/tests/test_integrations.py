#!/usr/bin/env python3
"""clawd-home integrations: the Claude Code hook script + settings merge, the
`clawd` CLI, end to end against the daemon's event server.

Run from the repo root:
  uv run --python 3.12 --with httpx --with pytest python -m pytest integrations/tests -q
"""
import asyncio
import importlib.machinery
import importlib.util
import json
import subprocess
import sys
import threading
import time
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "clawdmeter"))
sys.path.insert(0, str(ROOT / "integrations" / "claude"))

import daemon.clawd_events as ev  # noqa: E402
import install  # noqa: E402

HOOK = ROOT / "integrations" / "claude" / "clawd-event.sh"
CLI = ROOT / "integrations" / "cli" / "clawd"


def load_cli():
    loader = importlib.machinery.SourceFileLoader("clawd_cli", str(CLI))
    spec = importlib.util.spec_from_loader("clawd_cli", loader)
    mod = importlib.util.module_from_spec(spec)
    loader.exec_module(mod)
    return mod


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
        fut = asyncio.run_coroutine_threadsafe(self.server.close(), self.loop)
        fut.result(5)
        self.loop.call_soon_threadsafe(self.loop.stop)
        self.thread.join(5)


def free_port() -> int:
    import socket
    s = socket.socket()
    s.bind(("127.0.0.1", 0))
    p = s.getsockname()[1]
    s.close()
    return p


# ---- settings merge -----------------------------------------------------------
EXISTING = {
    "model": "opus",
    "hooks": {
        "PreToolUse": [{"matcher": "Write", "hooks": [{"type": "command", "command": "node guard.cjs"}]}],
        "Stop": [{"matcher": "*", "hooks": [{"type": "command", "command": "node heartbeat.cjs"}]}],
    },
}


def test_event_list_matches_daemon():
    assert sorted(install.EVENTS) == ev.CLAUDE_HOOK_EVENTS


def test_merge_keeps_existing_hooks_and_is_idempotent():
    once = install.merge(EXISTING, "'/x/clawd-event.sh'")
    assert once["model"] == "opus"
    assert once["hooks"]["PreToolUse"][0] == EXISTING["hooks"]["PreToolUse"][0]
    assert once["hooks"]["Stop"][0] == EXISTING["hooks"]["Stop"][0]
    for e in install.EVENTS:
        assert sum(install._ours(g) for g in once["hooks"][e]) == 1
    assert install.merge(once, "'/x/clawd-event.sh'") == once
    assert install.merge(once, "", install=False) == EXISTING
    assert EXISTING["hooks"]["Stop"] == [{"matcher": "*", "hooks": [{"type": "command", "command": "node heartbeat.cjs"}]}]


def test_installer_backs_up_and_uninstalls(tmp_path):
    f = tmp_path / "settings.json"
    f.write_text(json.dumps(EXISTING))
    assert install.main(["--settings", str(f)]) == 0
    assert list(tmp_path.glob("settings.json.bak-clawd-*"))
    data = json.loads(f.read_text())
    assert any(install._ours(g) for g in data["hooks"]["PermissionRequest"])
    assert install.main(["--settings", str(f)]) == 0          # "already up to date"
    assert install.main(["--settings", str(f), "--uninstall"]) == 0
    assert json.loads(f.read_text()) == EXISTING
    bad = tmp_path / "bad.json"
    bad.write_text("{nope")
    assert install.main(["--settings", str(bad)]) == 1
    assert bad.read_text() == "{nope"


# ---- hook script -------------------------------------------------------------
def run_hook(port, payload: dict):
    t = time.perf_counter()
    p = subprocess.run(["sh", str(HOOK)], input=json.dumps(payload).encode(),
                       env={"PATH": "/usr/bin:/bin", "CLAWD_EVENT_PORT": str(port)},
                       capture_output=True, timeout=10)
    return p, (time.perf_counter() - t) * 1000


def test_hook_is_fast_and_silent_without_daemon():
    times = []
    for _ in range(5):
        p, ms = run_hook(free_port(), {"hook_event_name": "Stop", "session_id": "x"})
        assert p.returncode == 0 and p.stdout == b"" and p.stderr == b""
        times.append(ms)
    assert sorted(times)[2] < 50, times          # median under 50 ms


def test_hook_forwards_payload_to_daemon():
    with LiveServer() as srv:
        p, _ = run_hook(srv.server.port, {"hook_event_name": "PreToolUse", "session_id": "abc",
                                          "tool_name": "Bash", "tool_input": {"command": "ls"}})
        assert p.returncode == 0 and p.stdout == b""
        s = srv.brain.tracker.sessions["claude:abc"]
        assert (s.state, s.tool) == ("working", "Bash")


# ---- CLI ---------------------------------------------------------------------------
def test_cli_builds_requests():
    cli = load_cli()
    assert cli.build(["say", "drink", "water", "--secs", "5"]) == \
        ("POST", "/poke", {"cmd": "say", "text": "drink water", "secs": 5.0})
    assert cli.build(["play", "double", "jump"]) == ("POST", "/poke", {"cmd": "play", "anim": "double jump"})
    assert cli.build(["focus", "25"]) == ("POST", "/poke", {"cmd": "focus", "mins": "25"})
    assert cli.build(["wave"]) == ("POST", "/poke", {"cmd": "wave"})
    assert cli.build(["status"]) == ("GET", "/state", None)
    with pytest.raises(SystemExit):
        cli.build(["dance"])


def test_cli_end_to_end(monkeypatch, capsys):
    cli = load_cli()
    monkeypatch.setenv("CLAWD_EVENT_PORT", str(free_port()))
    assert cli.main(["wave"]) == 1                      # no daemon
    with LiveServer() as srv:
        monkeypatch.setenv("CLAWD_EVENT_PORT", str(srv.server.port))
        assert cli.main(["say", "drink water"]) == 0
        d = srv.brain.desired(time.time())
        assert (d["a"], d["txt"]) == ("idle", "Drink water")
        assert cli.main(["focus", "abc"]) == 2
        assert cli.main(["focus", "25"]) == 0
        capsys.readouterr()
        assert cli.main(["status"]) == 0
        out = capsys.readouterr().out
        assert "clock: idle - Drink water" in out and "focus: 2" in out
