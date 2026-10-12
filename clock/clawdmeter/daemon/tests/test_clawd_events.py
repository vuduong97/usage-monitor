#!/usr/bin/env python3
"""clawd-home events: session tracking, Claude/pi hook mapping, pet moods,
pokes + focus, layer resolution, the state push and the local event server.

Run: python -m pytest daemon/tests/test_clawd_events.py -q
"""
import asyncio
import json
import random
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
    v = t.view(T0)
    assert (v.anim, v.text) == ("thinking", "Thinking")
    t.ingest(claude("PreToolUse", tool_name="Bash"), T0 + 1)
    v = t.view(T0 + 1)
    assert (v.anim, v.text) == ("typing", "Running")
    assert t.sessions["claude:s1"].tool == "Bash"      # raw name kept for `clawd status`
    t.ingest(claude("Stop"), T0 + 2)
    assert t.view(T0 + 2).anim == "happy"
    assert t.view(T0 + 2 + ev.DONE_SHOW + 0.1) is None


def test_priority_attention_over_error_over_subagents():
    t = ev.Tracker()
    t.ingest(claude("PreToolUse", tool_name="Task"), T0)
    t.ingest(claude("SubagentStart"), T0)
    t.ingest(claude("SubagentStart"), T0)
    assert (t.view(T0).anim, t.view(T0).text) == ("juggling", "3 agents")
    t.ingest(claude("PostToolUseFailure", tool_name="Bash"), T0 + 1)
    v = t.view(T0 + 1)
    assert (v.anim, v.text) == ("error", "Run failed")
    t.ingest(claude("PermissionRequest", sid="s2", tool_name="Bash",
                    tool_input={"command": "npm test"}), T0 + 1)
    v = t.view(T0 + 1)
    assert (v.anim, v.text) == ("notification", "Allow Bash: npm test")
    # error passes, attention stays until that session moves on
    assert t.view(T0 + 10).anim == "notification"
    t.ingest(claude("PreToolUse", sid="s2", tool_name="Bash"), T0 + 11)
    assert (t.view(T0 + 11).anim, t.view(T0 + 11).text) == ("juggling", "4 agents")
    t.ingest(claude("SubagentStop"), T0 + 12)
    t.ingest(claude("SubagentStop"), T0 + 12)
    v = t.view(T0 + 12)          # s1 failed a tool: its debugging shows through, counted
    assert (v.anim, v.text, v.variants) == ("debugger", "Debugging (2)", ())
    t.ingest(claude("PreToolUse", sid="s2", tool_name="Edit"), T0 + 13)
    v = t.view(T0 + 13)
    assert (v.anim, v.text) == ("headphones", "2 agents")


@pytest.mark.parametrize("tool,text", [
    ("Bash", "Running"), ("bash", "Running"), ("bg_run", "Running"),
    ("Read", "Reading"), ("read_symbol", "Reading"),
    ("MultiEdit", "Editing"), ("write", "Editing"),
    ("Grep", "Searching"), ("Glob", "Searching"),
    ("WebFetch", "Browsing"), ("batch_web_fetch", "Browsing"),
    ("TodoWrite", "Planning"), ("todo", "Planning"),
    ("mcp__cry_opera__slide_bash", "Working"),
    ("mcp__x__read", "Reading"),                 # an MCP tool goes by its own name
    ("mcp__some_long_server_name__web_fetch", "Browsing"),   # matched before the 24-char cap
    ("Frobnicate", "Working"),
])
def test_tool_shows_an_action_word(tool, text):
    t = ev.Tracker()
    t.ingest({"src": "pi", "sid": "p1", "ev": "tool", "tool": tool}, T0)
    assert t.view(T0).text == text


@pytest.mark.parametrize("tool,text", [
    ("Edit", "Edit failed"), ("bash", "Run failed"), ("Grep", "Search failed"),
    ("Frobnicate", "Tool failed"), ("mcp__x__slide_bash", "Tool failed"),
])
def test_tool_error_shows_the_failed_action(tool, text):
    t = ev.Tracker()
    t.ingest({"src": "pi", "sid": "p1", "ev": "tool_error", "tool": tool}, T0)
    assert t.view(T0).text == text


def test_tool_error_without_a_name_uses_the_running_tool_and_stop_error_says_error():
    t = ev.Tracker()
    t.ingest({"src": "pi", "sid": "p1", "ev": "tool", "tool": "read"}, T0)
    t.ingest({"src": "pi", "sid": "p1", "ev": "tool_error"}, T0)
    assert t.view(T0).text == "Read failed"
    t.ingest({"src": "pi", "sid": "p1", "ev": "stop_error"}, T0 + 10)
    assert t.view(T0 + 10).text == "Error"


def test_agent_counts_pick_the_session_headphones_juggling():
    t = ev.Tracker()
    t.ingest(claude("PreToolUse", sid="a", tool_name="Edit"), T0)
    assert t.view(T0).anim == "typing"
    t.ingest(claude("UserPromptSubmit", sid="b"), T0 + 1)     # the latest is thinking
    v = t.view(T0 + 1)
    assert (v.anim, v.text) == ("headphones", "2 agents")
    t.ingest(ev.pi_event({"ev": "prompt", "sid": "c"}), T0 + 2)
    v = t.view(T0 + 2)
    assert (v.anim, v.text) == ("juggling", "3 agents")


def test_a_subagent_counts_as_an_agent():
    t = ev.Tracker()
    t.ingest(claude("PreToolUse", tool_name="Task"), T0)
    t.ingest(claude("SubagentStart"), T0)
    v = t.view(T0)
    assert (v.anim, v.text) == ("headphones", "2 agents")
    t.ingest(claude("PreToolUse", sid="b", tool_name="Edit"), T0 + 1)
    v = t.view(T0 + 1)
    assert (v.anim, v.text) == ("juggling", "3 agents")
    t.ingest(claude("PreToolUse", tool_name="Read"), T0 + 2)  # shows through, counted
    v = t.view(T0 + 2)
    assert (v.anim, v.text) == ("reading", "Reading (3)")


def test_children_still_running_after_the_main_session_stops():
    t = ev.Tracker()
    t.ingest(ev.pi_event({"ev": "prompt", "sid": "child1", "sub": True}), T0)
    v = t.view(T0)
    assert (v.anim, v.text) == ("headphones", "1 agent")
    t.ingest(ev.pi_event({"ev": "prompt", "sid": "child2", "sub": True}), T0)
    v = t.view(T0)
    assert (v.anim, v.text) == ("juggling", "2 agents")


def test_pi_child_processes_count_as_subagents_and_never_celebrate():
    turns = []
    t = ev.Tracker(on_turn=turns.append)
    t.ingest(ev.pi_event({"ev": "tool", "sid": "main", "tool": "subagent"}), T0)
    t.ingest(ev.pi_event({"ev": "prompt", "sid": "child1", "sub": True}), T0)
    assert (t.view(T0).anim, t.view(T0).text) == ("headphones", "2 agents")
    t.ingest(ev.pi_event({"ev": "prompt", "sid": "child2", "sub": True}), T0)
    assert (t.view(T0).anim, t.view(T0).text) == ("juggling", "3 agents")
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


def test_attention_seen_hides_needs_you_until_the_next_question():
    t = ev.Tracker()
    pi = lambda e, sid="p1", **kw: t.ingest(ev.pi_event({"ev": e, "sid": sid, **kw}), T0)
    pi("prompt", sid="p2")
    pi("attention", msg="pi needs you")
    v = t.view(T0)
    assert (v.anim, v.text) == ("notification", "pi needs you")
    pi("attention_seen")
    assert t.sessions["pi:p1"].state == "waiting"
    assert t.view(T0).anim == "thinking"         # the other session shows, not p1
    pi("attention_seen")                           # a repeat changes nothing
    assert t.sessions["pi:p1"].state == "waiting"
    pi("attention", msg="Allow rm?")             # a new question calls again
    assert t.view(T0).text == "Allow rm?"
    pi("attention_seen")
    pi("attention_done")                           # answered: back to work
    assert t.sessions["pi:p1"].state == "thinking"
    pi("attention_seen", sid="p2")                # not waiting on anything: ignored
    assert t.sessions["pi:p2"].state == "thinking"


def test_questions_look_left_and_right_other_attention_stays_notification():
    t = ev.Tracker()
    pi = lambda e, sid="p1", **kw: t.ingest(ev.pi_event({"ev": e, "sid": sid, **kw}), T0)
    pi("prompt")
    pi("attention", tool="ask_user_question", msg="Which DB?")
    v = t.view(T0)
    assert (v.anim, v.text, v.names, v.rep, v.urgent) == (
        "question", "Which DB?", ("react left", "react right"), True, True)
    pi("attention_seen")                           # seen: hidden, like any attention
    assert t.view(T0) is None
    pi("attention_done")                           # answered: back to work
    assert t.view(T0).anim == "thinking" and not t.sessions["pi:p1"].asking
    pi("attention", msg="pi needs you")           # a plain pi dialog
    assert t.view(T0).anim == "notification"
    pi("attention_done")
    for tool in ("ask_user", "question"):
        pi("attention", tool=tool, msg="Q")
        assert t.view(T0).anim == "question"
        pi("attention_done")
    # Claude: AskUserQuestion is a question; a later generic Notification keeps it
    t.ingest(claude("PreToolUse", sid="c1", tool_name="AskUserQuestion",
                    tool_input={"questions": [{"question": "Which anim?"}]}), T0 + 1)
    v = t.view(T0 + 1)
    assert (v.anim, v.text) == ("question", "Which anim?")
    t.ingest(claude("Notification", sid="c1", message="Claude needs your input"), T0 + 2)
    assert t.view(T0 + 2).anim == "question"
    t.ingest(claude("PostToolUse", sid="c1", tool_name="AskUserQuestion"), T0 + 3)
    t.ingest(claude("PermissionRequest", sid="c1", tool_name="Bash",
                    tool_input={"command": "rm -rf x"}), T0 + 4)
    v = t.view(T0 + 4)
    assert v.anim == "notification" and v.text.startswith("Allow") and v.seq == ()
    t.ingest(claude("Notification", sid="c2", message="Claude needs your permission",
                    notification_type="permission_prompt"), T0 + 5)
    assert t.view(T0 + 5).anim == "notification"


def test_a_question_expires_and_stopping_forgets_it():
    t = ev.Tracker()
    t.ingest(ev.pi_event({"ev": "attention", "sid": "p1", "tool": "ask_user_question"}), T0)
    assert t.view(T0).text == "Has a question"
    t.view(T0 + ev.ATTENTION_TTL + 1)
    assert t.sessions["pi:p1"].state == "idle" and not t.sessions["pi:p1"].asking
    t.ingest(ev.pi_event({"ev": "attention", "sid": "p2", "tool": "question"}), T0)
    t.ingest(ev.pi_event({"ev": "abort", "sid": "p2"}), T0)
    assert not t.sessions["pi:p2"].asking
    assert ev.is_question_tool("mcp__srv__ask_user_question")
    assert not ev.is_question_tool("Bash") and not ev.is_question_tool(None)


def test_seen_session_alone_shows_nothing_and_expires_like_attention():
    t = ev.Tracker()
    t.ingest(ev.pi_event({"ev": "attention", "sid": "p1"}), T0)
    t.ingest(ev.pi_event({"ev": "attention_seen", "sid": "p1"}), T0 + 1)
    assert t.view(T0 + 1) is None
    t.view(T0 + 1 + ev.ATTENTION_TTL + 1)
    assert t.sessions["pi:p1"].state == "idle"


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


def test_ascii_text_folds_accents():
    assert ev.ascii_text("Café naïve Đà Lạt") == "Cafe naive Da Lat"
    assert ev.ascii_text("a" * 200, 10) == "aaaaaaa..."


# ---- pet ------------------------------------------------------------------------------
def test_pet_falls_asleep_and_wakes_with_greeting(tmp_path):
    p = ev.Pet(store=tmp_path / "pet.json", localtime=local(8, 30))
    p.observe(T0, 10, None)
    assert p.moment(T0) is None and p.continuous(T0) is None
    p.observe(T0 + 1, 301, None)
    # away: no endless sleep, the usage screens (with the usual idle picks)
    assert p.moment(T0 + 1) is None and p.continuous(T0 + 90) is None
    p.observe(T0 + 100, 2, None)
    assert p.continuous(T0 + 100) is None
    v = p.moment(T0 + 100)
    assert (v.names, v.text, v.urgent) == (("wake", "happy"), "Good morning!", True)
    assert p.moment(T0 + 100 + ev.WAKE_SECS + 3.1) is None
    # second return the same morning: no second "good morning"
    p.observe(T0 + 200, 400, None)
    p.observe(T0 + 300, 1, None)
    assert p.moment(T0 + 300).text == "Welcome back!"
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
    assert (v.anim, v.text) == ("low battery", "Session 85%")
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
    assert (v.anim, v.text) == ("heart eyes", "10 turns today!")
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
    assert b.pokes.handle({"cmd": "say", "text": "drink water"}, T0) == {"ok": True}
    d = b.desired(T0)
    assert (d["a"], d["txt"]) == ("idle", "Drink water")
    assert b.desired(T0 + 11)["a"] == "typing"


def test_clock_text_starts_with_a_capital():
    b = brain_at()
    b.pokes.handle({"cmd": "say", "text": "break time, 2 min"}, T0)
    assert b.desired(T0)["txt"] == "Break time, 2 min"
    b.pokes.handle({"cmd": "clear"}, T0)
    b.tracker.ingest(claude("PreToolUse", tool_name="mcp__web__fetch"), T0)
    assert b.desired(T0)["txt"] == "Working"          # raw tool names never reach the clock
    b.tracker.ingest(claude("Stop"), T0)
    assert b.desired(T0)["txt"] == "Working"          # minDisplay: typing stays 1 s
    assert b.desired(T0 + 1)["txt"] == "Done!"


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


def test_a_question_during_focus_keeps_its_text_and_holds_like_notification():
    b = brain_at()
    b.pokes.handle({"cmd": "focus", "mins": 25}, T0)
    b.tracker.ingest(claude("PreToolUse", tool_name="AskUserQuestion",
                            tool_input={"questions": [{"question": "which DB?"}]}), T0)
    d = b.desired(T0)
    assert (d["a"], d["seq"], d["rep"], d["u"], d["txt"], d["cd"]) == (
        "question", ["react left", "react right"], True, True, "Which DB?", 0)
    b.tracker.ingest(claude("PostToolUse", tool_name="AskUserQuestion"), T0 + 1)
    assert b.desired(T0 + 1)["a"] == "question"       # min display, like notification
    assert b.desired(T0 + 5.5)["a"] != "question"


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
    assert (d["a"], d["txt"]) == ("happy", "Focus done!")
    assert b.pokes.focus_until == 0
    b.pokes.handle({"cmd": "focus", "mins": 5}, T0)
    b.pokes.handle({"cmd": "focus", "mins": "stop"}, T0)
    assert b.pokes.focus_left(T0) == 0


def test_pet_layers_below_agent_and_dim_always():
    b = brain_at(23)
    b.pet.observe(T0, 900, None)
    b.pet.moments.clear()
    d = b.desired(T0)
    assert (d["a"], d["dim"]) == ("", 15)              # away: usage screens, dimmed at night
    b.tracker.ingest(claude("UserPromptSubmit"), T0)
    d = b.desired(T0)
    assert (d["a"], d["dim"]) == ("thinking", 15)


def test_wire_and_state_url():
    assert ev.state_url("http://192.168.1.48/api/usage") == "http://192.168.1.48/api/state"
    assert ev.wire({"a": "", "txt": "", "cd": 0, "dim": -1, "focus_until": 0}) == {"a": "", "ttl": 60}
    assert ev.wire({"a": "x", "txt": "t", "cd": 5, "dim": 10, "focus_until": 1}) == \
        {"a": "x", "ttl": 60, "txt": "t", "cd": 5, "dim": 10}
    assert ev.wire({"a": "x", "seq": ["x"], "rep": False, "u": True, "txt": "", "cd": 0,
                    "dim": -1, "focus_until": 0}) == {"a": "x", "ttl": 60, "u": True}
    assert ev.wire({"a": "s", "seq": ["y", "s"], "rep": False, "u": False, "txt": "", "cd": 0,
                    "dim": -1, "focus_until": 0}) == {"a": ["y", "s"], "ttl": 60}
    assert ev.wire({"a": "typing", "seq": ["typing"], "rep": False, "v": ["building", "carrying"],
                    "u": False, "txt": "", "cd": 0, "dim": -1, "focus_until": 0}) == \
        {"a": "typing", "ttl": 60, "v": ["building", "carrying"]}
    assert "v" not in ev.wire({"a": "x", "v": [], "txt": "", "cd": 0, "dim": -1, "focus_until": 0})


@pytest.mark.parametrize("steps,anim,text,variants", [
    ([("PreToolUse", "Bash", None)], "typing", "Running", ("typing boss",)),
    ([("PreToolUse", "Edit", None)], "typing", "Editing", ("typing boss",)),
    ([("PreToolUse", "memory_write", None)], "typing", "Working", ("building", "carrying",
                                                                   "typing boss")),
    ([("PreToolUse", "Read", None)], "reading", "Reading", ()),        # reading is short: none
    ([("PreToolUse", "Grep", None)], "reading", "Searching", ()),
    ([("PreToolUse", "Bash", {"command": "pytest -q"})], "debugger", "Testing", ()),
    ([("PreToolUse", "Edit", None)] * ev.EDIT_STREAK, "typing boss", "Editing", ("typing",)),
    ([("PostToolUseFailure", "Bash", None), ("PreToolUse", "Read", None)], "debugger", "Debugging",
     ()),
])
def test_long_agent_animations_carry_same_meaning_variants(steps, anim, text, variants):
    t = ev.Tracker()
    t.ingest(claude("UserPromptSubmit"), T0)
    now = T0 + 1
    for name, tool, inp in steps:
        t.ingest(claude(name, tool_name=tool, **({"tool_input": inp} if inp else {})), now)
    now += ev.ERROR_SHOW + 1
    v = t.view(now)
    assert (v.anim, v.text, v.variants) == (anim, text, variants)
    assert all(n != anim for n in v.variants)


def test_thinking_ultrathink_and_wizard_variants():
    t = ev.Tracker()
    t.ingest(claude("UserPromptSubmit"), T0)
    assert t.view(T0).variants == ()                                   # thinking: none
    assert t.view(T0 + ev.ULTRATHINK_AFTER).variants == ("thinking",)
    t.ingest(claude("PreToolUse", tool_name="Bash"), T0 + ev.WIZARD_AFTER)
    v = t.view(T0 + ev.WIZARD_AFTER)
    assert (v.anim, v.text, v.variants) == ("wizard", "Long task", ("typing", "building", "carrying"))


def test_the_state_carries_variants_and_a_variant_change_is_pushed():
    b = brain_at()
    b.tracker.ingest(claude("PreToolUse", tool_name="Bash"), T0)
    d = b.desired(T0)
    assert (d["a"], d["v"]) == ("typing", ["typing boss"])
    b.tracker.sessions["claude:s1"].action = ""                      # same animation, new mix
    d2 = b.desired(T0 + 0.5)
    assert (d2["a"], d2["v"], d2["txt"]) == ("typing", ["building", "carrying", "typing boss"],
                                            "Working")
    assert ev.StatePusher.key(d) != ev.StatePusher.key(d2)


def test_read_pet_config_and_port(monkeypatch):
    vals = {"quiet_start": "22:30", "quiet_end": "6:45", "quiet_dim": "200",
            "away_after": "5", "milestone_turns": "x", "event_port": "5000", "idle_linger": "60"}
    c = ev.read_pet_config(vals.get)                   # idle_linger: gone, ignored
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
    # after agent activity: straight back to the usage screens
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
            assert st == 200 and body["desired"]["a"] == "reading"
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


def test_abort_returns_to_idle_without_celebrating():
    turns = []
    t = ev.Tracker(on_turn=turns.append)
    t.ingest(ev.pi_event({"ev": "tool", "sid": "a", "tool": "bash"}), T0)
    t.ingest(ev.pi_event({"ev": "abort", "sid": "a"}), T0 + 1)
    assert t.view(T0 + 1) is None and turns == []


# ---- clawd-on-desk behaviours: variants, holds, pokes, idle rotation --------------------
def test_long_thinking_becomes_ultrathink():
    t = ev.Tracker()
    t.ingest(claude("UserPromptSubmit"), T0)
    assert t.view(T0 + ev.ULTRATHINK_AFTER - 1).anim == "thinking"
    v = t.view(T0 + ev.ULTRATHINK_AFTER)
    assert (v.anim, v.text) == ("ultrathink", "Thinking hard")
    t.ingest(claude("PreToolUse", tool_name="Bash"), T0 + 40)
    assert t.view(T0 + 40).anim == "typing"


def test_ultrathink_between_tools_counts_from_the_last_tool_and_parallel_tools():
    t = ev.Tracker()
    t.ingest(claude("UserPromptSubmit"), T0)
    t.ingest(claude("PreToolUse", tool_name="Bash"), T0 + 1)
    assert t.view(T0 + 200).anim == "typing"                 # a long tool is not thinking
    t.ingest(claude("PostToolUse", tool_name="Bash"), T0 + 200)
    assert t.view(T0 + 200 + ev.ULTRATHINK_AFTER - 1).anim == "typing"
    v = t.view(T0 + 200 + ev.ULTRATHINK_AFTER)
    assert (v.anim, v.text) == ("ultrathink", "Thinking hard")
    # pi runs tools in parallel: quiet only once every one of them finished
    u = ev.Tracker()
    pi = lambda e, at, **kw: u.ingest(ev.pi_event({"ev": e, "sid": "p", **kw}), at)
    pi("prompt", T0)
    pi("tool", T0 + 1, tool="bash")
    pi("tool", T0 + 1, tool="bash")
    pi("tool_done", T0 + 2, tool="bash")
    assert u.view(T0 + 60).anim == "typing"
    pi("tool_error", T0 + 61, tool="bash")
    assert u.view(T0 + 61 + ev.ULTRATHINK_AFTER).anim == "ultrathink"
    # a question answered, a compaction, a new prompt or the end reset the clock
    pi("attention", T0 + 100, msg="Which?")
    pi("attention_done", T0 + 200)
    assert u.view(T0 + 210).anim == "thinking"
    pi("stop", T0 + 220)
    assert u.sessions["pi:p"].running == 0


@pytest.mark.parametrize("tool", ["Read", "read", "ls", "read_symbol", "Grep", "find", "Glob"])
def test_reading_and_searching_tools_read(tool):
    t = ev.Tracker()
    t.ingest(claude("UserPromptSubmit"), T0)
    t.ingest(claude("PreToolUse", tool_name=tool), T0 + 1)
    v = t.view(T0 + 1)
    assert v.anim == "reading" and v.text in ("Reading", "Searching")
    t.ingest(claude("PostToolUse", tool_name=tool), T0 + 1.1)
    assert t.view(T0 + 5).anim == "reading"                  # stays until the next tool
    t.ingest(claude("PreToolUse", tool_name="Edit"), T0 + 6)
    assert t.view(T0 + 6).anim == "typing"


@pytest.mark.parametrize("prompt,debug", [
    ("/ak:debug the flash script", True), ("/ak:fix it", True), ("/skill:ak-debug x", True),
    ("why does this debugging fail", True), ("Fix the build", True), ("a bug in flash.sh", True),
    ("s\u1eeda l\u1ed7i flash.sh", True), ("su\u031b\u0309a lo\u0302\u0303i", True),  # NFD input too
    ("tr\u1ea3 l\u1eddi b\u1eb1ng ti\u1ebfng Vi\u1ec7t", False), ("tra loi nhanh", False),
    ("add a prefix and a suffix", False), ("debounce the input", False),
    ('<skill name="ak-debug" location="x">\nbody\n</skill>\n\nwhy?', True),
    ('<skill name="ak-cook" location="x">\nfix bugs, debug\n</skill>\n\n--auto', False),
    (None, False)])
def test_debug_prompts(prompt, debug):
    assert ev.is_debug_prompt(prompt) is debug


def test_debug_skill_loads():
    assert ev.is_debug_skill_load("Skill", {"skill": "ak:debug"})
    assert ev.is_debug_skill_load("Skill", {"command": "/ak:fix"})
    assert not ev.is_debug_skill_load("Skill", {"skill": "ak:cook"})
    assert ev.is_debug_skill_load("Read", {"file_path": "/u/.agents/skills/ak-debug/SKILL.md"})
    assert ev.is_debug_skill_load("read", {"path": "skills/ak-fix/SKILL.md"})
    assert not ev.is_debug_skill_load("read", {"path": "skills/ak-debug/references/x.md"})
    assert not ev.is_debug_skill_load("Edit", {"file_path": "skills/ak-debug/SKILL.md"})
    assert not ev.is_debug_skill_load("Read", None)


def test_debug_turns_from_the_prompt_or_the_skill_show_the_debugger():
    t = ev.Tracker()
    t.ingest(claude("UserPromptSubmit", prompt="s\u1eeda l\u1ed7i flash.sh"), T0)
    assert t.view(T0).anim == "thinking"
    for k, tool in enumerate(("Bash", "Read", "Edit")):
        t.ingest(claude("PreToolUse", tool_name=tool), T0 + 1 + k)
        v = t.view(T0 + 1 + k)                                 # whatever the tool: the debugger,
        assert (v.anim, v.text, v.variants) == ("debugger", "Debugging", ())  # and it says so
        t.ingest(claude("PostToolUse", tool_name=tool), T0 + 1.5 + k)
    t.ingest(claude("PreToolUse", tool_name="Bash", tool_input={"command": "pytest"}), T0 + 5)
    v = t.view(T0 + 5)
    assert (v.anim, v.text) == ("debugger", "Testing")       # a test run is the debugger
    t.ingest(claude("PostToolUse", tool_name="Bash"), T0 + 6)
    assert t.view(T0 + 6 + ev.ULTRATHINK_AFTER).anim == "ultrathink"  # beats the debugger
    t.ingest(claude("Stop"), T0 + 50)
    t.ingest(claude("UserPromptSubmit", prompt="tr\u1ea3 l\u1eddi ng\u1eafn"), T0 + 60)
    t.ingest(claude("PreToolUse", tool_name="Edit"), T0 + 61)
    assert t.view(T0 + 61).text == "Editing"                 # only that turn
    t.ingest(claude("PreToolUse", tool_name="Skill", tool_input={"skill": "ak:debug"}), T0 + 62)
    t.ingest(claude("PostToolUse", tool_name="Skill"), T0 + 62.5)
    t.ingest(claude("PreToolUse", tool_name="Edit"), T0 + 63)
    assert t.view(T0 + 63).text == "Debugging"               # loading the skill starts one
    u = ev.Tracker()                                         # pi sends only the verdict
    u.ingest(ev.pi_event({"ev": "prompt", "sid": "p", "debug": True}), T0)
    u.ingest(ev.pi_event({"ev": "tool", "sid": "p", "tool": "edit"}), T0 + 1)
    assert u.view(T0 + 1).debug
    assert ev.pi_event({"ev": "prompt", "sid": "p", "debug": "yes"})["debug"] is False


def test_many_agents_let_the_latest_sessions_reading_debugger_ultrathink_through():
    t = ev.Tracker()
    for k, sid in enumerate("abc"):
        t.ingest(claude("PreToolUse", sid=sid, tool_name="Edit"), T0 + k)
        t.ingest(claude("PostToolUse", sid=sid, tool_name="Edit"), T0 + k + 0.5)
    assert (t.view(T0 + 2.5).anim, t.view(T0 + 2.5).text) == ("juggling", "3 agents")
    t.ingest(claude("PreToolUse", sid="b", tool_name="Read"), T0 + 3)
    v = t.view(T0 + 3)
    assert (v.anim, v.text) == ("reading", "Reading (3)")
    t.ingest(claude("PreToolUse", sid="a", tool_name="Bash",
                    tool_input={"command": "npm test"}), T0 + 4)
    assert t.view(T0 + 4).text == "Testing (3)"
    t.ingest(claude("PostToolUse", sid="a", tool_name="Bash"), T0 + 4.2)
    t.ingest(claude("PostToolUseFailure", sid="c", tool_name="Edit"), T0 + 4.3)
    t.ingest(claude("PreToolUse", sid="c", tool_name="Edit"), T0 + 4.3 + ev.ERROR_SHOW)
    v = t.view(T0 + 4.3 + ev.ERROR_SHOW)
    assert (v.anim, v.text) == ("debugger", "Debugging (3)")
    t.ingest(claude("SessionEnd", sid="c"), T0 + 10)
    t.ingest(claude("Stop", sid="a"), T0 + 10)
    t.ingest(claude("UserPromptSubmit", sid="a"), T0 + 11)
    t.ingest(claude("PostToolUse", sid="b", tool_name="Read"), T0 + 11)
    v = t.view(T0 + 11 + ev.ULTRATHINK_AFTER)
    assert (v.anim, v.text) == ("ultrathink", "Thinking hard (2)")
    t.ingest(claude("PreToolUse", sid="a", tool_name="Edit"), T0 + 50)
    assert (t.view(T0 + 50).anim, t.view(T0 + 50).text) == ("headphones", "2 agents")
    t.ingest(claude("SubagentStart", sid="a"), T0 + 51)          # a subagent adds one
    assert (t.view(T0 + 51).anim, t.view(T0 + 51).text) == ("juggling", "3 agents")
    t.ingest(claude("PreToolUse", sid="b", tool_name="Read"), T0 + 52)
    v = t.view(T0 + 52)
    assert (v.anim, v.text) == ("reading", "Reading (3)")


@pytest.mark.parametrize("cmd,test", [
    ("pytest daemon/tests -q", True), ("cd x && npm test", True), ("pnpm run test:unit", True),
    ("go test ./...", True), ("cargo test --release", True), ("node --test x.test.ts", True),
    ("uv run python -m pytest -q", True), ("ls tests", False), ("cat pytest.ini", False),
    ("git commit -m 'npm testing'", False), (None, False)])
def test_test_commands(cmd, test):
    assert ev.is_test_command(cmd) is test


def test_running_tests_and_fixing_after_a_failure_show_the_debugger():
    t = ev.Tracker()
    t.ingest(claude("UserPromptSubmit"), T0)
    t.ingest(claude("PreToolUse", tool_name="Bash", tool_input={"command": "pytest -q"}), T0 + 1)
    v = t.view(T0 + 1)
    assert (v.anim, v.text) == ("debugger", "Testing")
    t.ingest(claude("PostToolUseFailure", tool_name="Bash"), T0 + 2)
    assert t.view(T0 + 2).anim == "error"
    t.ingest(claude("PreToolUse", tool_name="Edit"), T0 + 10)
    v = t.view(T0 + 10)
    assert (v.anim, v.text) == ("debugger", "Debugging")     # rest of the turn after an error
    t.ingest(claude("Stop"), T0 + 11)
    t.ingest(claude("UserPromptSubmit"), T0 + 20)
    t.ingest(claude("PreToolUse", tool_name="Edit"), T0 + 21)
    assert t.view(T0 + 21).text == "Debugging"               # and the turn right after
    t.ingest(claude("Stop"), T0 + 22)
    t.ingest(claude("UserPromptSubmit"), T0 + 30)
    t.ingest(claude("PreToolUse", tool_name="Edit"), T0 + 31)
    v = t.view(T0 + 31)
    assert (v.anim, v.text, v.debug) == ("typing", "Editing", False)  # then it is over
    u = ev.Tracker()                                          # pi sends the bash command along
    u.ingest(ev.pi_event({"ev": "tool", "sid": "p", "tool": "bash", "cmd": "go test ./..."}), T0)
    assert u.view(T0).anim == "debugger"


def test_three_errors_in_two_minutes_make_clawd_dizzy():
    t = ev.Tracker()
    t.ingest(claude("PostToolUseFailure", tool_name="Bash"), T0)
    t.ingest(claude("PostToolUseFailure", tool_name="Bash"), T0 + 50)
    assert t.view(T0 + 50).anim == "error"
    t.ingest(claude("StopFailure"), T0 + 100)
    v = t.view(T0 + 100)
    assert (v.anim, v.urgent) == ("dizzy", True)
    assert t.view(T0 + 100 + ev.DIZZY_SHOW + 0.1) is None
    u = ev.Tracker()                                          # spread out: never dizzy
    for k in range(3):
        u.ingest(claude("PostToolUseFailure", tool_name="Bash"), T0 + k * 100)
        assert u.view(T0 + k * 100).anim == "error"


def test_edit_streak_types_like_a_boss_and_long_turns_cast_wizard():
    t = ev.Tracker()
    t.ingest(claude("UserPromptSubmit"), T0)
    for k in range(ev.EDIT_STREAK - 1):
        t.ingest(claude("PreToolUse", tool_name="Edit" if k % 2 else "Write"), T0 + k)
    assert t.view(T0 + 9).anim == "typing"
    t.ingest(claude("PreToolUse", tool_name="MultiEdit"), T0 + 9)
    v = t.view(T0 + 9)
    assert (v.anim, v.text) == ("typing boss", "Editing")
    t.ingest(claude("PreToolUse", tool_name="Bash"), T0 + 10)
    assert t.view(T0 + 10).anim == "typing"                  # the streak broke
    t.ingest(claude("PreToolUse", tool_name="Bash"), T0 + ev.WIZARD_AFTER)
    v = t.view(T0 + ev.WIZARD_AFTER)
    assert (v.anim, v.text) == ("wizard", "Long task")      # the text follows the animation
    t.ingest(claude("PreToolUse", tool_name="Grep"), T0 + ev.WIZARD_AFTER + 1)
    assert t.view(T0 + ev.WIZARD_AFTER + 1).anim == "reading"   # reading beats wizard


def test_min_display_holds_agent_animations_but_not_against_a_poke():
    b = brain_at()
    b.tracker.ingest(claude("PostToolUseFailure", tool_name="Bash"), T0)
    assert b.desired(T0)["a"] == "error"
    b.tracker.ingest(claude("PreToolUse", tool_name="Bash"), T0 + 1)
    b.tracker.sessions["claude:s1"].error_until = 0           # error over early
    assert b.desired(T0 + 1)["a"] == "error"                  # held for minDisplay.error
    assert b.desired(T0 + 5)["txt"] == "Debugging"
    b.pokes.handle({"cmd": "poke"}, T0 + 5.2)
    d = b.desired(T0 + 5.2)
    assert (d["a"], d["u"]) == ("react left", True)           # a poke never waits


def test_pokes_alternate_and_three_quick_ones_annoy():
    p = ev.Pokes()
    names = []
    for k in range(2):
        p.handle({"cmd": "poke"}, T0 + k * 10)
        names.append(p.moment(T0 + k * 10).anim)
    assert names == ["react left", "react right"]
    assert p.moment(T0 + 10 + ev.POKE_SECS + 0.1) is None
    for k in range(3):
        p.handle({"cmd": "poke"}, T0 + 100 + k)
    v = p.moment(T0 + 102)
    assert (v.anim, v.urgent) == ("annoyed", True)
    assert p.moment(T0 + 102 + ev.ANNOYED_SECS + 0.1) is None


def idle_brain(seed=1):
    return ev.Brain(ev.Pet(localtime=local(14), rng=random.Random(seed)))


def run_idle(b, start, secs, step=0.5):
    """[(t, anim, seq)] every time desired() changes over `secs` seconds."""
    out, last, t = [], None, start
    while t < start + secs:
        d = b.desired(t)
        k = (d["a"], tuple(d["seq"]))
        if k != last:
            out.append((t, d["a"], tuple(d["seq"])))
            last = k
        t += step
    return out


def test_idle_picks_slip_in_between_usage_screens():
    assert {n for p in ev.IDLE_PICKS for n in p} <= set(ev.PASS_SECS)
    tied = {"reading", "thinking", "ultrathink", "typing", "typing boss", "building", "wizard",
            "debugger", "headphones", "juggling", "carrying", "sweeping", "error", "dizzy",
            "notification", "happy", "react left", "react right", "annoyed"}
    assert not tied & {n for p in ev.IDLE_PICKS for n in p}
    b = idle_brain()
    b.pet.desk_new = True
    changes = run_idle(b, T0, 3 * 3600)
    assert changes[0][1] == ""                            # starts on the usage screens
    picks = [(t, a, seq) for t, a, seq in changes if a]
    assert len(picks) >= 25
    for (t0, a, seq), (t1, a1, _) in zip(changes, changes[1:]):
        if a:                                             # one pass, then usage again
            names = seq or (a,)
            assert a1 == "" and abs((t1 - t0) - sum(ev.PASS_SECS[n] for n in names)) <= 0.5
        else:                                             # a gap of 3-6 minutes
            assert ev.IDLE_GAP[0] - 0.5 <= t1 - t0 <= ev.IDLE_GAP[1] + 0.5
    names = [seq or (a,) for _, a, seq in picks]
    assert all(x != y for x, y in zip(names, names[1:]))   # never the same pick twice in a row
    assert set(names) == set(ev.IDLE_PICKS)
    assert ("yawn", "sleeping", "wake") in names


def test_idle_picks_skip_new_animations_on_an_older_desk_pak():
    b = idle_brain(2)
    picks = {seq or (a,) for _, a, seq in run_idle(b, T0, 3 * 3600) if a}
    assert picks and not any(ev.IDLE_PICKS_NEW.intersection(p) for p in picks)


def test_done_then_usage_and_agent_activity_restarts_the_gap():
    b = idle_brain()
    b.tracker.ingest(claude("UserPromptSubmit"), T0)
    b.tracker.ingest(claude("Stop"), T0 + 5)
    assert b.desired(T0 + 6)["a"] == "happy"
    assert b.desired(T0 + 5 + ev.DONE_SHOW - 0.1)["a"] == "happy"
    free = T0 + 5 + ev.DONE_SHOW + 0.1
    first = free
    while not b.desired(first)["a"]:                    # usage screens until the first pick
        first += 0.5
    assert free + ev.IDLE_GAP[0] - 0.5 <= first <= free + ev.IDLE_GAP[1] + 0.5
    # an agent turn during a pick cancels it; the next pick waits a full gap again
    assert b.desired(first + 0.5)["a"]
    b.tracker.ingest(claude("PreToolUse", tool_name="Bash"), first + 1)
    assert b.desired(first + 1)["a"] == "typing"
    b.tracker.ingest(claude("SessionEnd"), first + 2)
    assert b.pet.pick is None
    assert b.desired(first + 2 + ev.MIN_DISPLAY_WORKING)["a"] == ""
    assert b.pet.next_pick >= first + 2 + ev.IDLE_GAP[0]


def test_away_keeps_usage_and_picks_then_wakes_on_return():
    b = idle_brain()
    b.pet.observe(T0, 900, None)
    changes = run_idle(b, T0, 1800)
    assert all(a != "sleeping" or seq for _, a, seq in changes)   # sleeping only inside the chain
    assert sum(1 for _, a, _ in changes if a) >= 4
    t = T0 + 1800
    b.pet.observe(t, 1, None)
    d = b.desired(t)
    assert (d["seq"], d["txt"], d["u"]) == (["wake", "happy"], "Welcome back!", True)
    assert b.pet.pick is None


def test_probe_desk_turns_new_picks_on_and_keeps_them_without_answer():
    b = idle_brain()
    answers = [{"desk": 31}, {"desk": 30}]

    def handler(req):
        assert str(req.url) == "http://clock/api/heap"
        if not answers:
            return httpx.Response(503)
        return httpx.Response(200, json=answers.pop(0))
    p = ev.StatePusher(b, lambda: ["http://clock/api/usage"], httpx.MockTransport(handler),
                       idle=lambda: None, clock=lambda: T0)
    _run(p.probe_desk())
    assert b.pet.desk_new is True
    _run(p.probe_desk())
    assert b.pet.desk_new is False
    b.pet.desk_new = True
    _run(p.probe_desk())                                  # no answer: unchanged
    assert b.pet.desk_new is True


def test_pusher_wakes_up_right_when_an_idle_pick_ends():
    b = idle_brain()
    p = ev.StatePusher(b, lambda: [], idle=lambda: None, clock=lambda: T0)
    assert p._wait_for(T0) == 1.0
    b.pet.desk_new = True
    b.pet.next_pick = T0
    assert b.desired(T0)["a"]
    ends = b.pet.pick_ends()
    assert p._wait_for(ends - 0.3) == pytest.approx(0.3)
    assert p._wait_for(ends + 1) == 0.05


def test_every_variant_is_an_animation_in_the_desk_pak():
    import pathlib
    import re
    anims = pathlib.Path(__file__).resolve().parents[3] / "tools" / "desk" / "desk_anims.js"
    names = set(re.findall(r"name:\s*'([^']+)'", anims.read_text()))
    used = {n for vs in ev.VARIANTS.values() for n in vs} | set(ev.VARIANTS) | set(ev.WORKING_VARIANTS)
    assert used <= names, used - names
    assert all(1 <= len(vs) <= 3 for vs in (*ev.VARIANTS.values(), ev.WORKING_VARIANTS))  # the clock keeps 3
    idle = {n for p in ev.IDLE_PICKS for n in p}
    assert idle <= names, idle - names
    assert len(names) >= ev.DESK_NEW_MIN                  # the pak that has every idle pick
