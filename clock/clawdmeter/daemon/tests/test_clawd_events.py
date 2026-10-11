#!/usr/bin/env python3
"""clawd-home events: session tracking, Claude/pi hook mapping, pet moods,
pokes + focus, layer resolution, the state push and the local event server.

Run: python -m pytest daemon/tests/test_clawd_events.py -q
"""
import asyncio
import json
import time

import httpx
import pytest

import daemon.clawd_events as ev

T0 = 1_760_000_000.0


def _run(coro):
    return asyncio.run(coro)


def claude(name, sid="s1", **kw):
    return ev.claude_event({"hook_event_name": name, "session_id": sid, **kw})


def local(hour, minute=0):
    """A localtime() stand-in: every timestamp maps to hour:minute on one day."""
    return lambda _ts: time.struct_time((2026, 10, 11, hour, minute, 0, 6, 284, 0))


# ---- tracker -------------------------------------------------------------------------
def test_single_session_flow_prompt_tool_stop():
    t = ev.Tracker()
    assert t.view(T0) is None
    t.ingest(claude("UserPromptSubmit"), T0)
    assert t.view(T0).anim == "thinking"
    t.ingest(claude("PreToolUse", tool_name="Bash"), T0 + 1)
    v = t.view(T0 + 1)
    assert (v.anim, v.text) == ("typing", "Bash")
    t.ingest(claude("Stop"), T0 + 2)
    assert t.view(T0 + 2).anim == "happy"
    assert t.view(T0 + 2 + ev.DONE_SHOW + 0.1) is None


def test_priority_attention_over_error_over_subagents():
    t = ev.Tracker()
    t.ingest(claude("PreToolUse", tool_name="Task"), T0)
    t.ingest(claude("SubagentStart"), T0)
    t.ingest(claude("SubagentStart"), T0)
    assert t.view(T0).anim == "juggling"
    t.ingest(claude("PostToolUseFailure", tool_name="Bash"), T0 + 1)
    v = t.view(T0 + 1)
    assert (v.anim, v.text) == ("error", "Bash failed")
    t.ingest(claude("PermissionRequest", sid="s2", tool_name="Bash",
                    tool_input={"command": "npm test"}), T0 + 1)
    v = t.view(T0 + 1)
    assert (v.anim, v.text) == ("notification", "Allow Bash: npm test")
    # error passes, attention stays until that session moves on
    assert t.view(T0 + 10).anim == "notification"
    t.ingest(claude("PreToolUse", sid="s2", tool_name="Bash"), T0 + 11)
    assert t.view(T0 + 11).anim == "juggling"
    t.ingest(claude("SubagentStop"), T0 + 12)
    assert t.view(T0 + 12).anim == "headphones"


def test_session_counts_pick_typing_headphones_building():
    t = ev.Tracker()
    t.ingest(claude("PreToolUse", sid="a", tool_name="Edit"), T0)
    assert t.view(T0).anim == "typing"
    t.ingest(claude("UserPromptSubmit", sid="b"), T0)
    assert t.view(T0).anim == "headphones"
    t.ingest(ev.pi_event({"ev": "prompt", "sid": "c"}), T0)
    v = t.view(T0)
    assert (v.anim, v.text) == ("building", "3 sessions")


def test_pi_child_processes_count_as_subagents_and_never_celebrate():
    turns = []
    t = ev.Tracker(on_turn=turns.append)
    t.ingest(ev.pi_event({"ev": "tool", "sid": "main", "tool": "subagent"}), T0)
    t.ingest(ev.pi_event({"ev": "prompt", "sid": "child1", "sub": True}), T0)
    assert t.view(T0).anim == "headphones"
    t.ingest(ev.pi_event({"ev": "prompt", "sid": "child2", "sub": True}), T0)
    assert t.view(T0).anim == "juggling"
    t.ingest(ev.pi_event({"ev": "stop", "sid": "child1", "sub": True}), T0 + 1)
    t.ingest(ev.pi_event({"ev": "stop", "sid": "child2", "sub": True}), T0 + 1)
    assert turns == []
    assert t.view(T0 + 1).anim == "typing"


def test_compact_and_end_and_stale_sessions():
    t = ev.Tracker()
    t.ingest(claude("PreCompact"), T0)
    assert t.view(T0).anim == "sweeping"
    t.ingest(claude("PostCompact"), T0 + 1)
    assert t.view(T0 + 1).anim == "thinking"
    t.ingest(claude("SessionEnd"), T0 + 2)
    assert t.view(T0 + 2) is None and not t.sessions
    # a crashed session (no Stop) stops counting after SESSION_TTL
    t.ingest(claude("PreToolUse", tool_name="Bash"), T0)
    assert t.view(T0 + ev.SESSION_TTL - 1).anim == "typing"
    assert t.view(T0 + ev.SESSION_TTL + 1) is None


def test_attention_expires_after_ttl():
    t = ev.Tracker()
    t.ingest(claude("Notification", message="Claude needs your permission to use Bash",
                    notification_type="permission_prompt"), T0)
    assert t.view(T0 + ev.ATTENTION_TTL - 1).anim == "notification"
    assert t.view(T0 + ev.ATTENTION_TTL + 1) is None


# ---- Claude hook mapping --------------------------------------------------------------
def test_claude_mapping():
    assert claude("UserPromptSubmit")["ev"] == "prompt"
    assert claude("PreToolUse", tool_name="Read")["ev"] == "tool"
    q = claude("PreToolUse", tool_name="AskUserQuestion",
               tool_input={"questions": [{"question": "Which DB?"}]})
    assert (q["ev"], q["msg"]) == ("attention", "Which DB?")
    assert claude("Notification", message="Claude is waiting for your input",
                  notification_type="idle_prompt") is None
    assert claude("StopFailure")["ev"] == "stop_error"
    assert claude("SessionStart")["sid"] == "claude:s1"
    assert ev.claude_event({"hook_event_name": "Stop"}) is None      # no session id
    assert ev.claude_event([1, 2]) is None
    assert claude("SomethingNew") is None
    assert set(ev.CLAUDE_HOOK_EVENTS) >= {"PreToolUse", "PermissionRequest", "Stop", "SessionEnd"}


def test_pi_event_validation():
    assert ev.pi_event({"ev": "nope", "sid": "x"}) is None
    assert ev.pi_event({"ev": "tool"}) is None
    e = ev.pi_event({"ev": "attention", "sid": "x", "msg": "Pick one"})
    assert (e["sid"], e["msg"], e["sub"]) == ("pi:x", "Pick one", False)


def test_ascii_text_folds_vietnamese():
    assert ev.ascii_text("uống nước đi bạn") == "uong nuoc di ban"
    assert ev.ascii_text("a" * 200, 10) == "aaaaaaa..."


# ---- pet ------------------------------------------------------------------------------
def test_pet_falls_asleep_and_wakes_with_greeting(tmp_path):
    p = ev.Pet(store=tmp_path / "pet.json", localtime=local(8, 30))
    p.observe(T0, 10, None)
    assert p.moment(T0) is None and p.continuous(T0) is None
    p.observe(T0 + 1, 301, None)
    assert p.moment(T0 + 1).anim == "yawn"
    assert p.moment(T0 + 6) is None
    assert p.continuous(T0 + 6).anim == "sleeping"
    p.observe(T0 + 100, 2, None)
    assert p.continuous(T0 + 100) is None
    assert p.moment(T0 + 100).anim == "wake"
    v = p.moment(T0 + 102.5)
    assert (v.anim, v.text) == ("happy", "Chao buoi sang!")
    # second return the same morning: no second "good morning"
    p.observe(T0 + 200, 400, None)
    p.observe(T0 + 300, 1, None)
    p.moment(T0 + 300)
    assert p.moment(T0 + 302.5).text == "Chao mung tro lai!"
    assert json.loads((tmp_path / "pet.json").read_text())["greeted"] == "2026-10-11"


def test_pet_quiet_hours_dim_wraps_midnight():
    assert ev.Pet(localtime=local(23, 30)).dim(T0) == 15
    assert ev.Pet(localtime=local(3, 0)).dim(T0) == 15
    assert ev.Pet(localtime=local(7, 0)).dim(T0) == -1
    assert ev.Pet(localtime=local(12, 0)).dim(T0) == -1


def test_pet_usage_low_and_reset():
    p = ev.Pet(localtime=local(14, 0))
    p.observe(T0, 1, {"s": 85, "sr": 30, "ok": True})
    v = p.moment(T0)
    assert (v.anim, v.text) == ("low battery", "Phien 85%")
    p.moments.clear()
    p.observe(T0 + 60, 1, {"s": 86, "sr": 29, "ok": True})
    assert p.moment(T0 + 60) is None                 # not again within low_every
    p.observe(T0 + 120, 1, {"s": 0, "sr": 299, "ok": True})
    assert p.moment(T0 + 120).anim == "double jump"
    p.moments.clear()
    p.observe(T0 + 200, 1, {"ok": False})
    assert p.moment(T0 + 200) is None


def test_pet_milestone_every_n_turns_and_new_day(tmp_path):
    p = ev.Pet(store=tmp_path / "pet.json", localtime=local(15, 0))
    for _ in range(9):
        p.on_turn(T0)
    assert p.moment(T0) is None
    p.on_turn(T0)
    v = p.moment(T0)
    assert (v.anim, v.text) == ("heart eyes", "10 luot hom nay!")
    assert ev.Pet(store=tmp_path / "pet.json", localtime=local(15, 0)).turns == 10
    q = ev.Pet(store=tmp_path / "pet.json",
               localtime=lambda _t: time.struct_time((2026, 10, 12, 9, 0, 0, 0, 285, 0)))
    q.on_turn(T0)
    assert q.turns == 1


def test_pet_evening_yawn_once_per_interval():
    p = ev.Pet(localtime=local(22, 15))
    p.observe(T0, 1, None)
    assert p.moment(T0).anim == "yawn"
    p.moments.clear()
    p.observe(T0 + 60, 1, None)
    assert p.moment(T0 + 60) is None


# ---- pokes, focus and layering ------------------------------------------------------
def brain_at(hour=14):
    return ev.Brain(ev.Pet(localtime=local(hour)))


def test_poke_say_beats_agent_then_agent_returns():
    b = brain_at()
    b.tracker.ingest(claude("PreToolUse", tool_name="Bash"), T0)
    assert b.pokes.handle({"cmd": "say", "text": "uống nước"}, T0) == {"ok": True}
    d = b.desired(T0)
    assert (d["a"], d["txt"]) == ("idle", "uong nuoc")
    assert b.desired(T0 + 11)["a"] == "typing"


def test_poke_validation():
    p = ev.Pokes()
    assert not p.handle({"cmd": "say", "text": "  "}, T0)["ok"]
    assert not p.handle({"cmd": "play"}, T0)["ok"]
    assert not p.handle({"cmd": "focus", "mins": "abc"}, T0)["ok"]
    assert not p.handle({"cmd": "focus", "mins": 9999}, T0)["ok"]
    assert not p.handle({"cmd": "dance"}, T0)["ok"]
    assert p.handle({"cmd": "play", "anim": "double jump", "secs": 3}, T0)["ok"]
    assert p.moment(T0).anim == "double jump"
    assert p.handle({"cmd": "clear"}, T0)["ok"] and p.moment(T0) is None


def test_focus_countdown_reading_then_done():
    b = brain_at()
    b.pokes.handle({"cmd": "focus", "mins": 25}, T0)
    d = b.desired(T0)
    assert (d["a"], d["txt"], d["cd"]) == ("reading", "Focus", 1500)
    # agent work keeps its animation but shows the timer
    b.tracker.ingest(claude("PreToolUse", tool_name="Bash"), T0 + 5)
    d = b.desired(T0 + 5)
    assert (d["a"], d["txt"], d["cd"]) == ("typing", "Focus", 1495)
    # a question keeps its own text, no timer
    b.tracker.ingest(claude("PermissionRequest", tool_name="Bash"), T0 + 6)
    d = b.desired(T0 + 6)
    assert (d["a"], d["cd"]) == ("notification", 0) and d["txt"].startswith("Allow")
    b.tracker.ingest(claude("Stop"), T0 + 7)
    d = b.desired(T0 + 1500)
    assert (d["a"], d["txt"]) == ("happy", "Xong focus!")
    assert b.pokes.focus_until == 0
    b.pokes.handle({"cmd": "focus", "mins": 5}, T0)
    b.pokes.handle({"cmd": "focus", "mins": "stop"}, T0)
    assert b.pokes.focus_left(T0) == 0


def test_pet_layers_below_agent_and_dim_always():
    b = brain_at(23)
    b.pet.observe(T0, 900, None)
    b.pet.moments.clear()
    d = b.desired(T0)
    assert (d["a"], d["dim"]) == ("sleeping", 15)
    b.tracker.ingest(claude("UserPromptSubmit"), T0)
    d = b.desired(T0)
    assert (d["a"], d["dim"]) == ("thinking", 15)


def test_wire_and_state_url():
    assert ev.state_url("http://192.168.1.48/api/usage") == "http://192.168.1.48/api/state"
    assert ev.wire({"a": "", "txt": "", "cd": 0, "dim": -1, "focus_until": 0}) == {"a": "", "ttl": 60}
    assert ev.wire({"a": "x", "txt": "t", "cd": 5, "dim": 10, "focus_until": 1}) == \
        {"a": "x", "ttl": 60, "txt": "t", "cd": 5, "dim": 10}


def test_read_pet_config_and_port(monkeypatch):
    vals = {"quiet_start": "22:30", "quiet_end": "6:45", "quiet_dim": "200",
            "away_after": "5", "milestone_turns": "x", "event_port": "5000"}
    c = ev.read_pet_config(vals.get)
    assert (c.quiet_start, c.quiet_end, c.quiet_dim, c.away_after, c.milestone) == \
        (22 * 60 + 30, 6 * 60 + 45, 100, 30, 10)
    monkeypatch.delenv("CLAWD_EVENT_PORT", raising=False)
    assert ev.read_event_port(vals.get) == 5000
    assert ev.read_event_port({}.get) == ev.EVENT_PORT
    monkeypatch.setenv("CLAWD_EVENT_PORT", "nonsense")
    assert ev.read_event_port({}.get) == ev.EVENT_PORT


# ---- state push ----------------------------------------------------------------------
class Recorder:
    def __init__(self, status=200):
        self.bodies, self.urls, self.status = [], [], status

    def transport(self):
        def handler(req):
            self.urls.append(str(req.url))
            self.bodies.append(json.loads(req.content))
            return httpx.Response(self.status)
        return httpx.MockTransport(handler)


def test_pusher_sends_on_change_resends_and_clears():
    b = brain_at()
    now = [T0]
    rec = Recorder()
    p = ev.StatePusher(b, lambda: ["http://clock/api/usage"], rec.transport(),
                       idle=lambda: 1.0, clock=lambda: now[0])
    assert _run(p.step())                               # first state: a clear
    assert rec.bodies[-1] == {"a": "", "ttl": 60} and rec.urls[-1] == "http://clock/api/state"
    assert not _run(p.step())                           # nothing changed, nothing live
    b.tracker.ingest(claude("PreToolUse", tool_name="Bash"), now[0])
    assert _run(p.step()) and rec.bodies[-1]["a"] == "typing"
    now[0] += 5
    assert not _run(p.step())                           # unchanged within RESEND_EVERY
    now[0] += ev.RESEND_EVERY
    assert _run(p.step()) and rec.bodies[-1]["a"] == "typing"   # keep-alive
    b.tracker.ingest(claude("SessionEnd"), now[0])
    assert _run(p.step()) and rec.bodies[-1] == {"a": "", "ttl": 60}
    assert len(rec.bodies) == 4


def test_pusher_retries_after_failed_delivery_and_no_targets():
    b = brain_at()
    rec = Recorder(status=500)
    p = ev.StatePusher(b, lambda: ["http://clock/api/usage"], rec.transport(),
                       idle=lambda: None, clock=lambda: T0)
    assert not _run(p.step())
    assert not _run(p.step()) and len(rec.bodies) == 2   # still pending: tried again
    q = ev.StatePusher(b, lambda: [], rec.transport(), idle=lambda: None, clock=lambda: T0)
    assert not _run(q.step())


# ---- event server -----------------------------------------------------------------
async def _request(port, method, path, body=None, headers=None):
    r, w = await asyncio.open_connection("127.0.0.1", port)
    data = json.dumps(body).encode() if body is not None else b""
    head = f"{method} {path} HTTP/1.1\r\nHost: x\r\nContent-Length: {len(data)}\r\n"
    for k, v in (headers or {}).items():
        head += f"{k}: {v}\r\n"
    w.write(head.encode() + b"\r\n" + data)
    await w.drain()
    raw = await r.read()
    w.close()
    status = int(raw.split(b" ", 2)[1])
    return status, json.loads(raw.split(b"\r\n\r\n", 1)[1] or b"null")


def test_event_server_routes():
    async def go():
        b = brain_at()
        s = ev.EventServer(b, port=0, clock=lambda: T0)
        await s.start()
        try:
            st, _ = await _request(s.port, "POST", "/hook/claude",
                                   {"hook_event_name": "PreToolUse", "session_id": "z",
                                    "tool_name": "Grep", "tool_response": "x" * 100000})
            assert st == 200 and b.wake.is_set()
            st, body = await _request(s.port, "GET", "/state")
            assert st == 200 and body["desired"]["a"] == "typing"
            assert body["sessions"][0]["sid"] == "claude:z"
            st, _ = await _request(s.port, "POST", "/hook/pi", {"ev": "prompt", "sid": "p"})
            assert st == 200 and "pi:p" in b.tracker.sessions
            st, body = await _request(s.port, "POST", "/poke", {"cmd": "say", "text": "hi"})
            assert st == 200 and body["ok"]
            st, body = await _request(s.port, "POST", "/poke", {"cmd": "bogus"})
            assert st == 400 and not body["ok"]
            st, _ = await _request(s.port, "POST", "/poke", {"cmd": "wave"},
                                   headers={"Origin": "https://evil.example"})
            assert st == 403
            st, _ = await _request(s.port, "POST", "/nope", {})
            assert st == 404
            # unknown hook payloads are accepted and ignored (never fail a hook)
            st, _ = await _request(s.port, "POST", "/hook/claude", {"weird": True})
            assert st == 200
        finally:
            await s.close()
    _run(go())


def test_daemon_wires_events(monkeypatch, tmp_path):
    import daemon.claude_usage_daemon as mod
    monkeypatch.setattr(mod, "ATTENTION_DIR", tmp_path)
    monkeypatch.setattr(mod, "read_config_value", lambda name: {"event_port": "0"}.get(name))
    monkeypatch.setattr(mod, "read_http_targets", lambda: [])    # never push to a real clock
    monkeypatch.delenv("CLAWD_EVENT_PORT", raising=False)

    async def go():
        pub = mod.Publisher()
        stop = asyncio.Event()
        task, server = await mod.start_events(pub, stop)
        assert task is not None and server.port not in (0, ev.EVENT_PORT)
        pub.on_payload({"s": 12, "ok": True})
        stop.set()
        await task
        await server.close()
    _run(go())
