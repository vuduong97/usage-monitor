"""Codex usage from ~/.codex/sessions, and the usage source the user picks."""
import asyncio
import json
import os
from pathlib import Path

import pytest

from clawd_desktop import codex_usage, paths, usage

NOW = 1_783_000_000


def row(rl, ts="2026-07-02T17:50:57.643Z"):
    return json.dumps({"timestamp": ts, "type": "event_msg",
                       "payload": {"type": "token_count", "info": None, "rate_limits": rl}})


LIMITS = {"limit_id": "codex", "primary": {"used_percent": 12.4, "window_minutes": 300, "resets_at": NOW + 3600},
          "secondary": {"used_percent": 43.0, "window_minutes": 10080, "resets_at": NOW + 86400 * 2},
          "plan_type": "plus", "rate_limit_reached_type": None}


def session(home, day, name, lines, mtime=None):
    d = home / ".codex" / "sessions" / day
    d.mkdir(parents=True, exist_ok=True)
    f = d / name
    f.write_text("\n".join(lines) + "\n")
    if mtime:
        os.utime(f, (mtime, mtime))
    return f


def test_maps_windows_by_length():
    p = codex_usage.payload_from(LIMITS, NOW, now=NOW)
    assert p == {"s": 12, "sr": 60, "w": 43, "wr": 2880, "st": "allowed", "acct": "pro", "ok": True, "src": "codex"}


def test_a_past_reset_reads_as_zero_and_limits_reached_as_rejected():
    rl = {**LIMITS, "primary": {**LIMITS["primary"], "resets_at": NOW - 1}, "rate_limit_reached_type": "primary"}
    p = codex_usage.payload_from(rl, NOW, now=NOW)
    assert p["s"] == 0 and "sr" not in p and p["st"] == "rejected"


def test_older_resets_in_seconds():
    rl = {"primary": {"used_percent": 5, "window_minutes": 300, "resets_in_seconds": 600}}
    assert codex_usage.payload_from(rl, NOW, now=NOW + 60)["sr"] == 9


def test_usage_based_plan_has_no_windows():
    assert codex_usage.payload_from({**LIMITS, "primary": None, "secondary": None}, NOW) is None


def test_reads_the_newest_session_with_limits(tmp_path):
    session(tmp_path, "2026/07/01", "rollout-a.jsonl", [row({**LIMITS, "primary": {**LIMITS["primary"], "used_percent": 1}})], NOW - 900)
    session(tmp_path, "2026/07/02", "rollout-b.jsonl",
            [row(LIMITS), '{"type":"response_item"}', row({**LIMITS, "primary": None, "secondary": None}), "{broken"], NOW - 100)
    p = codex_usage.read(home=tmp_path, env={}, now=NOW)
    assert p["s"] == 12 and p["w"] == 43                      # the null-window row is skipped
    assert codex_usage.available(home=tmp_path, env={})


def test_no_codex_no_data(tmp_path):
    assert codex_usage.read(home=tmp_path, env={}) is None
    assert not codex_usage.available(home=tmp_path, env={})


def test_codex_home_moves_it(tmp_path):
    assert codex_usage.sessions_dir(env={"CODEX_HOME": str(tmp_path / "cx")}) == tmp_path / "cx" / "sessions"


@pytest.fixture
def prefs(tmp_path, monkeypatch):
    monkeypatch.setattr(paths, "CONFIG_DIR", tmp_path)
    monkeypatch.setattr(paths, "PREFS_FILE", tmp_path / "desktop.json")

    def set_source(v):
        (tmp_path / "desktop.json").write_text(json.dumps({"usage_source": v}))
    return set_source


def test_usage_source_values(prefs):
    assert usage.read_usage_source() == "claude"
    for v, want in [("codex", "codex"), ("off", "off"), ("claude:/x/.claude-work", "claude:/x/.claude-work"),
                    ("claude:", "claude"), ("gemini", "claude"), (3, "claude")]:
        prefs(v)
        assert usage.read_usage_source() == want


def test_poll_active_follows_the_source(prefs, monkeypatch):
    monkeypatch.setattr(usage, "read_clock_setting", lambda: "24")
    prefs("off")
    p, dead = asyncio.run(usage.poll_active())
    assert p["ok"] is False and p["src"] == "off" and "t" in p and not dead
    prefs("codex")
    monkeypatch.setattr(codex_usage, "read", lambda: {"s": 5, "w": 9, "ok": True, "src": "codex", "acct": "pro", "st": "allowed"})
    p, dead = asyncio.run(usage.poll_active())
    assert p["src"] == "codex" and p["s"] == 5 and "t" in p
    monkeypatch.setattr(codex_usage, "read", lambda: None)
    assert asyncio.run(usage.poll_active()) == (None, True)
    seen = []

    async def fake(selector, only):
        seen.append(only)
        return {"s": 1, "ok": True}, False
    monkeypatch.setattr(usage, "poll_claude", fake)
    prefs("claude:/x/work")
    p, _ = asyncio.run(usage.poll_active())
    assert "src" not in p and Path(seen[-1]) == Path("/x/work")
    prefs("claude")
    asyncio.run(usage.poll_active())
    assert seen[-1] is None
