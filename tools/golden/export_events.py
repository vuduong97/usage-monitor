#!/usr/bin/env python3
"""Golden exporter for phase 04 (events): runs the REAL `clawd_desktop.events` and records what the
Rust `clawd-events` crate must reproduce, into `desktop-rs/fixtures/events/`.

Fixtures (each `{input, output}`):
  helpers.json      vocab helpers (ascii_text, is_test_command, is_debug_*, tool_action, ...),
                    claude_event / pi_event / tool_summary, wire / state_url / heap_url,
                    read_event_port / read_pet_config, the EVENTS / CLAUDE_HOOK_EVENTS lists.
  scenarios.json    named Brain scenarios: a list of timed steps (hook payloads, pokes, pet
                    observations, `desired`/`view`/`state` reads); the output is one entry per step.
                    Ported from tests/test_clawd_events.py (the mapping table says which).

Determinism: the two random draws (`rng.choice`, `rng.uniform`) use `FirstRng` (first choice, lower
bound), which the Rust side mirrors with `FirstRng`; wall-clock time is `T0 + t`; local time is the
scenario's `hour`/`minute` (settable with the `set` op).
"""
from __future__ import annotations

import json
import os
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import _harness as h  # noqa: E402

ctx = h.isolate()
import clawd_desktop.events as ev  # noqa: E402  (after isolate())

T0 = 1_760_000_000.0


class FirstRng:
    """Deterministic stand-in for random.Random: first choice, lower bound."""

    def choice(self, seq):
        return seq[0]

    def uniform(self, a, b):
        return a


# ---- scenario runner -------------------------------------------------------------------
def view_dict(v):
    if v is None:
        return None
    return {"anim": v.anim, "text": v.text, "layer": v.layer, "seq": list(v.seq), "rep": v.rep,
            "urgent": v.urgent, "variants": list(v.variants), "debug": v.debug}


def run_scenario(spec):
    env = {"hour": spec.get("hour", 14), "minute": spec.get("minute", 0),
           "only": spec.get("usage_only", False)}
    cfg = ev.PetConfig(**spec.get("cfg", {}))
    pet = ev.Pet(cfg, None, localtime=lambda _ts: time.struct_time(
        (2026, 10, 11, env["hour"], env["minute"], 0, 6, 284, 0)), rng=FirstRng())
    pet.desk_new = spec.get("desk_new", False)
    b = ev.Brain(pet, usage_only=lambda: env["only"])
    b.tracker.rng = FirstRng()
    out = []
    for st in spec["steps"]:
        t = T0 + st.get("t", 0)
        op = st["op"]
        if op in ("claude", "pi", "event"):
            e = (ev.claude_event(st["payload"]) if op == "claude"
                 else ev.pi_event(st["payload"]) if op == "pi" else st["payload"])
            out.append({"event": e, "changed": b.tracker.ingest(e, t) if e is not None else None})
        elif op == "poke":
            out.append(b.pokes.handle(st["cmd"], t))
        elif op == "observe":
            pet.observe(t, st.get("idle"), st.get("usage"))
            out.append(None)
        elif op == "view":
            out.append(view_dict(b.tracker.view(t)))
        elif op == "desired":
            out.append(b.desired(t))
        elif op == "run_idle":
            # [(t - T0, anim, seq)] every time desired() changes (like the Python test's run_idle)
            changes, last, cur = [], None, t
            while cur < t + st["secs"]:
                d = b.desired(cur)
                key = (d["a"], tuple(d["seq"]))
                if key != last:
                    changes.append([cur - T0, d["a"], list(d["seq"])])
                    last = key
                cur += st["step"]
            out.append(changes)
        elif op == "state":
            out.append({"desired": b.desired(t), "sessions": b.tracker.snapshot(t),
                        "turns": pet.turns, "away": pet.away, "focus_left": b.pokes.focus_left(t)})
        elif op == "sessions":
            out.append(b.tracker.snapshot(t))
        elif op == "pet_moment":
            out.append(view_dict(pet.moment(t)))
        elif op == "pet_continuous":
            out.append(view_dict(pet.continuous(t)))
        elif op == "pet_dim":
            out.append(pet.dim(t))
        elif op == "pet_on_turn":
            pet.on_turn(t)
            out.append(pet.turns)
        elif op == "pet_clear":
            pet.moments.clear()
            out.append(None)
        elif op == "pokes_moment":
            out.append(view_dict(b.pokes.moment(t)))
        elif op == "pokes_focus_left":
            out.append(b.pokes.focus_left(t))
        elif op == "set_session":
            setattr(b.tracker.sessions[st["sid"]], st["field"], st["value"])
            out.append(None)
        elif op == "set":
            for key in ("hour", "minute"):
                if key in st:
                    env[key] = st[key]
            if "usage_only" in st:
                env["only"] = st["usage_only"]
            if "desk_new" in st:
                pet.desk_new = st["desk_new"]
            out.append(None)
        else:
            raise ValueError(op)
    return out


def C(t, name, sid="s1", **kw):
    """A Claude hook payload step."""
    return {"op": "claude", "t": t, "payload": {"hook_event_name": name, "session_id": sid, **kw}}


def P(t, ev_name, sid="p1", **kw):
    """A pi extension event step."""
    return {"op": "pi", "t": t, "payload": {"ev": ev_name, "sid": sid, **kw}}


def E(t, ev_name, sid="p1", **kw):
    """A raw tracker event step (what an adapter emits)."""
    return {"op": "event", "t": t, "payload": {"src": "pi", "sid": sid, "ev": ev_name, **kw}}


V = lambda t: {"op": "view", "t": t}            # noqa: E731
D = lambda t: {"op": "desired", "t": t}         # noqa: E731
S = lambda t: {"op": "state", "t": t}           # noqa: E731
POKE = lambda t, **cmd: {"op": "poke", "t": t, "cmd": cmd}   # noqa: E731

SCENARIOS: dict[str, dict] = {}


def scenario(name, steps, **spec):
    SCENARIOS[name] = {"steps": steps, **spec}


# -- tracker --
scenario("single_session_flow", [
    V(0), C(0, "UserPromptSubmit"), V(0), C(1, "PreToolUse", tool_name="Bash"), V(1), {"op": "sessions", "t": 1},
    C(2, "Stop"), V(2), V(2 + ev.DONE_SHOW + 0.1)])

scenario("priority_attention_over_error_over_subagents", [
    C(0, "PreToolUse", tool_name="Task"), C(0, "SubagentStart"), C(0, "SubagentStart"), V(0),
    C(1, "PostToolUseFailure", tool_name="Bash"), V(1),
    C(1, "PermissionRequest", sid="s2", tool_name="Bash", tool_input={"command": "npm test"}), V(1),
    V(10), C(11, "PreToolUse", sid="s2", tool_name="Bash"), V(11),
    C(12, "SubagentStop"), C(12, "SubagentStop"), V(12),
    C(13, "PreToolUse", sid="s2", tool_name="Edit"), V(13)])

for _tool in ["Bash", "bash", "bg_run", "Read", "read_symbol", "MultiEdit", "write", "Grep", "Glob",
              "WebFetch", "batch_web_fetch", "TodoWrite", "todo", "mcp__cry_opera__slide_bash",
              "mcp__x__read", "mcp__some_long_server_name__web_fetch", "Frobnicate"]:
    scenario(f"tool_word:{_tool}", [E(0, "tool", tool=_tool), V(0), {"op": "sessions", "t": 0}])

for _tool in ["Edit", "bash", "Grep", "Frobnicate", "mcp__x__slide_bash"]:
    scenario(f"tool_error_word:{_tool}", [E(0, "tool_error", tool=_tool), V(0)])

scenario("tool_error_without_a_name", [
    E(0, "tool", tool="read"), E(0, "tool_error"), V(0), E(10, "stop_error"), V(10)])

scenario("agent_counts", [
    C(0, "PreToolUse", sid="a", tool_name="Edit"), V(0), C(1, "UserPromptSubmit", sid="b"), V(1),
    P(2, "prompt", sid="c"), V(2)])

scenario("a_subagent_counts_as_an_agent", [
    C(0, "PreToolUse", tool_name="Task"), C(0, "SubagentStart"), V(0),
    C(1, "PreToolUse", sid="b", tool_name="Edit"), V(1), C(2, "PreToolUse", tool_name="Read"), V(2)])

scenario("children_without_a_main_session", [
    P(0, "prompt", sid="child1", sub=True), V(0), P(0, "prompt", sid="child2", sub=True), V(0)])

scenario("pi_children_never_celebrate", [
    P(0, "tool", sid="main", tool="subagent"), P(0, "prompt", sid="child1", sub=True), V(0),
    P(0, "prompt", sid="child2", sub=True), V(0),
    P(1, "stop", sid="child1", sub=True), P(1, "stop", sid="child2", sub=True), S(1)])

scenario("compact_end_and_stale_sessions", [
    C(0, "PreCompact"), V(0), C(1, "PostCompact"), V(1), C(2, "SessionEnd"), V(2),
    {"op": "sessions", "t": 2},
    C(0, "PreToolUse", tool_name="Bash"), V(ev.SESSION_TTL - 1), V(ev.SESSION_TTL + 1)])

scenario("attention_expires_after_ttl", [
    C(0, "Notification", message="Claude needs your permission to use Bash", notification_type="permission_prompt"),
    V(ev.ATTENTION_TTL - 1), V(ev.ATTENTION_TTL + 1)])

scenario("attention_seen_hides_until_next_question", [
    P(0, "prompt", sid="p2"), P(0, "attention", msg="pi needs you"), V(0),
    P(0, "attention_seen"), {"op": "sessions", "t": 0}, V(0), P(0, "attention_seen"), {"op": "sessions", "t": 0},
    P(0, "attention", msg="Allow rm?"), V(0), P(0, "attention_seen"), P(0, "attention_done"),
    {"op": "sessions", "t": 0}, P(0, "attention_seen", sid="p2"), {"op": "sessions", "t": 0}])

scenario("questions_look_left_and_right", [
    P(0, "prompt"), P(0, "attention", tool="ask_user_question", msg="Which DB?"), V(0),
    P(0, "attention_seen"), V(0), P(0, "attention_done"), V(0), {"op": "sessions", "t": 0},
    P(0, "attention", msg="pi needs you"), V(0), P(0, "attention_done"),
    P(0, "attention", tool="ask_user", msg="Q"), V(0), P(0, "attention_done"),
    P(0, "attention", tool="question", msg="Q"), V(0), P(0, "attention_done"),
    C(1, "PreToolUse", sid="c1", tool_name="AskUserQuestion", tool_input={"questions": [{"question": "Which anim?"}]}), V(1),
    C(2, "Notification", sid="c1", message="Claude needs your input"), V(2),
    C(3, "PostToolUse", sid="c1", tool_name="AskUserQuestion"),
    C(4, "PermissionRequest", sid="c1", tool_name="Bash", tool_input={"command": "rm -rf x"}), V(4),
    C(5, "Notification", sid="c2", message="Claude needs your permission", notification_type="permission_prompt"), V(5)])

scenario("question_expires_and_stopping_forgets_it", [
    P(0, "attention", tool="ask_user_question"), V(0), V(ev.ATTENTION_TTL + 1), {"op": "sessions", "t": ev.ATTENTION_TTL + 1},
    P(0, "attention", sid="p2", tool="question"), P(0, "abort", sid="p2"), {"op": "sessions", "t": 0}])

_phrases = []
for _k in range(12):
    _phrases += [P(0, "attention", tool="ask_user_question"), V(0), P(0, "attention"), V(0),
                 P(0, "attention_seen"), P(0, "attention"), V(0), P(0, "attention_done")]
scenario("question_phrase_once_per_question", _phrases)

scenario("seen_session_alone_shows_nothing", [
    P(0, "attention"), P(1, "attention_seen"), V(1), V(1 + ev.ATTENTION_TTL + 1), {"op": "sessions", "t": 1 + ev.ATTENTION_TTL + 1}])

scenario("abort_returns_to_idle_without_celebrating", [
    P(0, "tool", sid="a", tool="bash"), P(1, "abort", sid="a"), V(1), S(1)])

scenario("long_thinking_becomes_ultrathink", [
    C(0, "UserPromptSubmit"), V(ev.ULTRATHINK_AFTER - 1), V(ev.ULTRATHINK_AFTER),
    C(40, "PreToolUse", tool_name="Bash"), V(40)])

scenario("ultrathink_counts_from_the_last_tool", [
    C(0, "UserPromptSubmit"), C(1, "PreToolUse", tool_name="Bash"), V(200),
    C(200, "PostToolUse", tool_name="Bash"), V(200 + ev.ULTRATHINK_AFTER - 1), V(200 + ev.ULTRATHINK_AFTER)])

scenario("ultrathink_parallel_tools_and_resets", [
    P(0, "prompt", sid="p"), P(1, "tool", sid="p", tool="bash"), P(1, "tool", sid="p", tool="bash"),
    P(2, "tool_done", sid="p", tool="bash"), V(60), P(61, "tool_error", sid="p", tool="bash"),
    V(61 + ev.ULTRATHINK_AFTER), P(100, "attention", sid="p", msg="Which?"), P(200, "attention_done", sid="p"),
    V(210), P(220, "stop", sid="p"), {"op": "sessions", "t": 220}])

for _tool in ["Read", "read", "ls", "read_symbol", "Grep", "find", "Glob"]:
    scenario(f"reading_tool:{_tool}", [
        C(0, "UserPromptSubmit"), C(1, "PreToolUse", tool_name=_tool), V(1),
        C(1.1, "PostToolUse", tool_name=_tool), V(5), C(6, "PreToolUse", tool_name="Edit"), V(6)])

scenario("debug_turns_from_prompt_or_skill", [
    C(0, "UserPromptSubmit", prompt="s\u1eeda l\u1ed7i flash.sh"), V(0),
    C(1, "PreToolUse", tool_name="Bash"), V(1), C(1.5, "PostToolUse", tool_name="Bash"),
    C(2, "PreToolUse", tool_name="Read"), V(2), C(2.5, "PostToolUse", tool_name="Read"),
    C(3, "PreToolUse", tool_name="Edit"), V(3), C(3.5, "PostToolUse", tool_name="Edit"),
    C(5, "PreToolUse", tool_name="Bash", tool_input={"command": "pytest"}), V(5),
    C(6, "PostToolUse", tool_name="Bash"), V(6 + ev.ULTRATHINK_AFTER),
    C(50, "Stop"), C(60, "UserPromptSubmit", prompt="tr\u1ea3 l\u1eddi ng\u1eafn"),
    C(61, "PreToolUse", tool_name="Edit"), V(61),
    C(62, "PreToolUse", tool_name="Skill", tool_input={"skill": "ak:debug"}),
    C(62.5, "PostToolUse", tool_name="Skill"), C(63, "PreToolUse", tool_name="Edit"), V(63)])

scenario("pi_debug_verdict", [
    P(0, "prompt", sid="p", debug=True), P(1, "tool", sid="p", tool="edit"), V(1),
    P(2, "prompt", sid="q", debug="yes"), {"op": "sessions", "t": 2}])

scenario("many_agents_let_the_latest_session_show_through", [
    C(0, "PreToolUse", sid="a", tool_name="Edit"), C(0.5, "PostToolUse", sid="a", tool_name="Edit"),
    C(1, "PreToolUse", sid="b", tool_name="Edit"), C(1.5, "PostToolUse", sid="b", tool_name="Edit"),
    C(2, "PreToolUse", sid="c", tool_name="Edit"), C(2.5, "PostToolUse", sid="c", tool_name="Edit"), V(2.5),
    C(3, "PreToolUse", sid="b", tool_name="Read"), V(3),
    C(4, "PreToolUse", sid="a", tool_name="Bash", tool_input={"command": "npm test"}), V(4),
    C(4.2, "PostToolUse", sid="a", tool_name="Bash"), C(4.3, "PostToolUseFailure", sid="c", tool_name="Edit"),
    C(4.3 + ev.ERROR_SHOW, "PreToolUse", sid="c", tool_name="Edit"), V(4.3 + ev.ERROR_SHOW),
    C(10, "SessionEnd", sid="c"), C(10, "Stop", sid="a"), C(11, "UserPromptSubmit", sid="a"),
    C(11, "PostToolUse", sid="b", tool_name="Read"), V(11 + ev.ULTRATHINK_AFTER),
    C(50, "PreToolUse", sid="a", tool_name="Edit"), V(50), C(51, "SubagentStart", sid="a"), V(51),
    C(52, "PreToolUse", sid="b", tool_name="Read"), V(52)])

scenario("running_tests_and_fixing_after_a_failure", [
    C(0, "UserPromptSubmit"), C(1, "PreToolUse", tool_name="Bash", tool_input={"command": "pytest -q"}), V(1),
    C(2, "PostToolUseFailure", tool_name="Bash"), V(2), C(10, "PreToolUse", tool_name="Edit"), V(10),
    C(11, "Stop"), C(20, "UserPromptSubmit"), C(21, "PreToolUse", tool_name="Edit"), V(21),
    C(22, "Stop"), C(30, "UserPromptSubmit"), C(31, "PreToolUse", tool_name="Edit"), V(31),
    P(40, "tool", sid="u", tool="bash", cmd="go test ./..."), V(40)])

scenario("three_errors_make_clawd_dizzy", [
    C(0, "PostToolUseFailure", tool_name="Bash"), C(50, "PostToolUseFailure", tool_name="Bash"), V(50),
    C(100, "StopFailure"), V(100), V(100 + ev.DIZZY_SHOW + 0.1)])

scenario("spread_out_errors_never_dizzy", [
    C(0, "PostToolUseFailure", tool_name="Bash"), V(0), C(100, "PostToolUseFailure", tool_name="Bash"), V(100),
    C(200, "PostToolUseFailure", tool_name="Bash"), V(200)])

scenario("edit_streak_and_wizard", [C(0, "UserPromptSubmit")]
         + [C(k, "PreToolUse", tool_name="Edit" if k % 2 else "Write") for k in range(ev.EDIT_STREAK - 1)]
         + [V(9), C(9, "PreToolUse", tool_name="MultiEdit"), V(9), C(10, "PreToolUse", tool_name="Bash"), V(10),
            C(ev.WIZARD_AFTER, "PreToolUse", tool_name="Bash"), V(ev.WIZARD_AFTER),
            C(ev.WIZARD_AFTER + 1, "PreToolUse", tool_name="Grep"), V(ev.WIZARD_AFTER + 1)])

_VARIANT_CASES = [
    ([("PreToolUse", "Bash", None)]), ([("PreToolUse", "Edit", None)]), ([("PreToolUse", "memory_write", None)]),
    ([("PreToolUse", "Read", None)]), ([("PreToolUse", "Grep", None)]),
    ([("PreToolUse", "Bash", {"command": "pytest -q"})]), ([("PreToolUse", "Edit", None)] * ev.EDIT_STREAK),
    ([("PostToolUseFailure", "Bash", None), ("PreToolUse", "Read", None)])]
for _i, _steps in enumerate(_VARIANT_CASES):
    _s = [C(0, "UserPromptSubmit")]
    for _name, _tool, _inp in _steps:
        _s.append(C(1, _name, tool_name=_tool, **({"tool_input": _inp} if _inp else {})))
    _s.append(V(1 + ev.ERROR_SHOW + 1))
    scenario(f"long_animation_variants:{_i}", _s)

scenario("thinking_ultrathink_wizard_variants", [
    C(0, "UserPromptSubmit"), V(0), V(ev.ULTRATHINK_AFTER),
    C(ev.WIZARD_AFTER, "PreToolUse", tool_name="Bash"), V(ev.WIZARD_AFTER)])

# -- pet --
scenario("pet_sleeps_and_wakes_with_greeting", [
    {"op": "observe", "t": 0, "idle": 10}, {"op": "pet_moment", "t": 0}, {"op": "pet_continuous", "t": 0},
    {"op": "observe", "t": 1, "idle": 301}, {"op": "pet_moment", "t": 1}, {"op": "pet_continuous", "t": 90},
    {"op": "observe", "t": 100, "idle": 2}, {"op": "pet_continuous", "t": 100}, {"op": "pet_moment", "t": 100},
    {"op": "pet_moment", "t": 100 + ev.WAKE_SECS + 3.1},
    {"op": "observe", "t": 200, "idle": 400}, {"op": "observe", "t": 300, "idle": 1}, {"op": "pet_moment", "t": 300},
    S(300)], hour=8, minute=30)

scenario("pet_welcome_back_outside_morning", [
    {"op": "observe", "t": 0, "idle": 900}, {"op": "observe", "t": 10, "idle": 1}, {"op": "pet_moment", "t": 10}], hour=14)

for _hour, _minute in [(23, 30), (3, 0), (7, 0), (12, 0), (23, 0), (22, 59), (6, 59)]:
    scenario(f"pet_dim:{_hour}:{_minute}", [{"op": "pet_dim", "t": 0}], hour=_hour, minute=_minute)

scenario("pet_usage_low_and_reset", [
    {"op": "observe", "t": 0, "idle": 1, "usage": {"s": 85, "sr": 30, "ok": True}}, {"op": "pet_moment", "t": 0},
    {"op": "pet_clear", "t": 0},
    {"op": "observe", "t": 60, "idle": 1, "usage": {"s": 86, "sr": 29, "ok": True}}, {"op": "pet_moment", "t": 60},
    {"op": "observe", "t": 120, "idle": 1, "usage": {"s": 0, "sr": 299, "ok": True}}, {"op": "pet_moment", "t": 120},
    {"op": "pet_clear", "t": 120},
    {"op": "observe", "t": 200, "idle": 1, "usage": {"ok": False}}, {"op": "pet_moment", "t": 200},
    {"op": "observe", "t": 2000, "idle": 1, "usage": {"s": 90, "acct": "ent"}}, {"op": "pet_moment", "t": 2000},
    {"op": "observe", "t": 2100, "idle": 1, "usage": {"s": 90.5}}, {"op": "pet_moment", "t": 2100}], hour=14)

scenario("pet_milestone_every_n_turns", [{"op": "pet_on_turn", "t": 0}] * 9 + [
    {"op": "pet_moment", "t": 0}, {"op": "pet_on_turn", "t": 0}, {"op": "pet_moment", "t": 0}, S(0)], hour=15)

scenario("pet_milestone_disabled", [{"op": "pet_on_turn", "t": 0}] * 12 + [{"op": "pet_moment", "t": 0}],
         hour=15, cfg={"milestone": 0})

scenario("pet_evening_yawn_once_per_interval", [
    {"op": "observe", "t": 0, "idle": 1}, {"op": "pet_moment", "t": 0}, {"op": "pet_clear", "t": 0},
    {"op": "observe", "t": 60, "idle": 1}, {"op": "pet_moment", "t": 60},
    {"op": "observe", "t": 1900, "idle": 1}, {"op": "pet_moment", "t": 1900}], hour=22, minute=15)

# -- pokes, focus, layering --
scenario("poke_say_beats_agent_then_agent_returns", [
    C(0, "PreToolUse", tool_name="Bash"), POKE(0, cmd="say", text="drink water"), D(0), D(11)])

scenario("clock_text_starts_with_a_capital", [
    POKE(0, cmd="say", text="break time, 2 min"), D(0), POKE(0, cmd="clear"),
    C(0, "PreToolUse", tool_name="mcp__web__fetch"), D(0), C(0, "Stop"), D(0), D(1)])

scenario("poke_validation", [
    POKE(0, cmd="say", text="  "), POKE(0, cmd="play"), POKE(0, cmd="focus", mins="abc"),
    POKE(0, cmd="focus", mins=9999), POKE(0, cmd="dance"), POKE(0, cmd="play", anim="double jump", secs=3),
    {"op": "pokes_moment", "t": 0}, POKE(0, cmd="clear"), {"op": "pokes_moment", "t": 0},
    POKE(0, cmd=5), POKE(0, cmd="it's"), POKE(0, cmd=None), POKE(0, cmd="poke", text=5),
    {"op": "pokes_moment", "t": 0}, POKE(0, cmd="wave"), {"op": "pokes_moment", "t": 1},
    POKE(0, cmd="say", text=123, anim="x" * 40, secs="abc"), {"op": "pokes_moment", "t": 1},
    POKE(0, cmd="say", text="t", secs=-5), {"op": "pokes_moment", "t": 0.5}, {"op": "pokes_moment", "t": 1.5},
    POKE(0, cmd="play", anim="a", secs=99999), {"op": "pokes_moment", "t": 200},
    POKE(0, cmd="play", anim="a", secs=True), {"op": "pokes_moment", "t": 0.5},
    POKE(0, cmd="focus", mins=0), POKE(0, cmd="focus", mins=0.5), {"op": "pokes_focus_left", "t": 0},
    POKE(0, cmd="focus", mins=True), {"op": "pokes_focus_left", "t": 0},
    POKE(0, cmd="focus", mins=[1]), POKE(0, cmd="focus", mins="  25  "), {"op": "pokes_focus_left", "t": 1.2},
    POKE(0, cmd="focus", mins="off"), {"op": "pokes_focus_left", "t": 0},
    POKE(0, cmd="focus", mins="nan"), POKE(0, cmd="focus", mins=-1), POKE(0, cmd="focus")])

scenario("question_during_focus_holds_like_notification", [
    POKE(0, cmd="focus", mins=25),
    C(0, "PreToolUse", tool_name="AskUserQuestion", tool_input={"questions": [{"question": "which DB?"}]}), D(0),
    C(1, "PostToolUse", tool_name="AskUserQuestion"), D(1), D(5.5)])

scenario("focus_countdown_reading_then_done", [
    POKE(0, cmd="focus", mins=25), D(0), C(5, "PreToolUse", tool_name="Bash"), D(5),
    C(6, "PermissionRequest", tool_name="Bash"), D(6), C(7, "Stop"), D(1500),
    {"op": "pokes_focus_left", "t": 1500},
    POKE(0, cmd="focus", mins=5), POKE(0, cmd="focus", mins="stop"), {"op": "pokes_focus_left", "t": 0}])

scenario("pet_layers_below_agent_and_dim_always", [
    {"op": "observe", "t": 0, "idle": 900}, {"op": "pet_clear", "t": 0}, D(0), C(0, "UserPromptSubmit"), D(0)], hour=23)

scenario("state_carries_variants", [
    C(0, "PreToolUse", tool_name="Bash"), D(0),
    {"op": "set_session", "t": 0, "sid": "claude:s1", "field": "action", "value": ""}, D(0.5)])

scenario("min_display_holds_agent_animations_but_not_against_a_poke", [
    C(0, "PostToolUseFailure", tool_name="Bash"), D(0), C(1, "PreToolUse", tool_name="Bash"),
    {"op": "set_session", "t": 1, "sid": "claude:s1", "field": "error_until", "value": 0}, D(1), D(5),
    POKE(5.2, cmd="poke"), D(5.2)])

scenario("pokes_alternate_and_three_quick_ones_annoy", [
    POKE(0, cmd="poke"), {"op": "pokes_moment", "t": 0}, POKE(10, cmd="poke"), {"op": "pokes_moment", "t": 10},
    {"op": "pokes_moment", "t": 10 + ev.POKE_SECS + 0.1},
    POKE(100, cmd="poke"), POKE(101, cmd="poke"), POKE(102, cmd="poke"), {"op": "pokes_moment", "t": 102},
    {"op": "pokes_moment", "t": 102 + ev.ANNOYED_SECS + 0.1}])

scenario("usage_screen_blocks_animations_but_not_what_needs_you", [
    C(0, "PreToolUse", tool_name="Bash"), D(0), D(4000), POKE(5, cmd="say", text="hi"), D(5),
    {"op": "set", "t": 0, "usage_only": False}, D(6),
    {"op": "set", "t": 0, "usage_only": True}, C(7, "PermissionRequest", tool_name="Bash"), D(7)], usage_only=True, hour=14)

# -- idle picks (deterministic rng: always the first allowed pick, gap = IDLE_GAP[0]) --
def RUN(t, secs, step=0.5):
    return {"op": "run_idle", "t": t, "secs": secs, "step": step}


scenario("idle_picks_new_desk", [RUN(0, 3000)], desk_new=True)
scenario("idle_picks_old_desk", [RUN(0, 3000)], desk_new=False)
scenario("done_then_usage_restarts_the_gap", [
    C(0, "UserPromptSubmit"), C(5, "Stop"), D(6), D(5 + ev.DONE_SHOW - 0.1),
    RUN(5 + ev.DONE_SHOW + 0.1, 400, 1.0),
    C(300, "PreToolUse", tool_name="Bash"), D(300), C(301, "SessionEnd"), D(301 + ev.MIN_DISPLAY_WORKING),
    D(301 + ev.MIN_DISPLAY_WORKING + 100), RUN(301 + ev.MIN_DISPLAY_WORKING + 100, 400, 1.0)], desk_new=True)
scenario("away_keeps_usage_and_picks_then_wakes_on_return", [
    {"op": "observe", "t": 0, "idle": 900}, RUN(0, 1800, 1.0),
    {"op": "observe", "t": 1800, "idle": 1}, D(1800), S(1800)], desk_new=True)


# ---- helpers fixture ---------------------------------------------------------------------
def helpers():
    out = {}
    cases = ["Café naïve Đà Lạt", "", "  a \t b\n c ", "a" * 200, "Tiếng Việt: đẹp quá", "x" * 90, "x" * 91,
             "abc def ghi", "😀 emoji", "ﬁ ligature ² ½", "\x1c\x1d tab\x1f sep"]
    out["ascii_text"] = [[c, None, ev.ascii_text(c)] for c in cases] + [
        ["a" * 200, 10, ev.ascii_text("a" * 200, 10)], ["abcde fghij", 9, ev.ascii_text("abcde fghij", 9)],
        ["abcd efghij", 9, ev.ascii_text("abcd efghij", 9)], ["Đà Lạt", 24, ev.ascii_text("Đà Lạt", 24)]]
    cmds = ["pytest daemon/tests -q", "cd x && npm test", "pnpm run test:unit", "go test ./...",
            "cargo test --release", "node --test x.test.ts", "uv run python -m pytest -q", "ls tests",
            "cat pytest.ini", "git commit -m 'npm testing'", None, "make check", "dotnet test", "(pytest)", "yarn test:e2e x",
            "npx jest", "cargo nextest run", "gradlew test", "pytest"]
    out["is_test_command"] = [[c, ev.is_test_command(c)] for c in cmds]
    prompts = ["/ak:debug the flash script", "/ak:fix it", "/skill:ak-debug x", "why does this debugging fail",
               "Fix the build", "a bug in flash.sh", "s\u1eeda l\u1ed7i flash.sh", "su\u031b\u0309a lo\u0302\u0303i",
               "tr\u1ea3 l\u1eddi b\u1eb1ng ti\u1ebfng Vi\u1ec7t", "tra loi nhanh", "add a prefix and a suffix",
               "debounce the input", '<skill name="ak-debug" location="x">\nbody\n</skill>\n\nwhy?',
               '<skill name="ak-cook" location="x">\nfix bugs, debug\n</skill>\n\n--auto', None, 5, "",
               "bugs", "BUG", "fixes", "(fix)", "FIX!", "<skill name=\"ak-fix\">", "<skill name=\"ak-cook\">fix",
               "<skill name=\"debug\">x</skill>", "<skill name=\"AK:DEBUG\">x</skill>"]
    out["is_debug_prompt"] = [[p, ev.is_debug_prompt(p)] for p in prompts]
    loads = [["Skill", {"skill": "ak:debug"}], ["Skill", {"command": "/ak:fix"}], ["Skill", {"skill": "ak:cook"}],
             ["Read", {"file_path": "/u/.agents/skills/ak-debug/SKILL.md"}], ["read", {"path": "skills/ak-fix/SKILL.md"}],
             ["read", {"path": "skills/ak-debug/references/x.md"}], ["Edit", {"file_path": "skills/ak-debug/SKILL.md"}],
             ["Read", None], ["skill", {"skill": 5}], ["skill", {"skill": " /fix/ "}],
             ["Read", {"file_path": "C:\\x\\ak-debug\\SKILL.md"}], ["Read", {"file_path": 5}], ["", {}]]
    out["is_debug_skill_load"] = [[t, i, ev.is_debug_skill_load(t, i)] for t, i in loads]
    names = ["AskUserQuestion", "ask_user", "question", "request_user_input", "mcp__srv__ask_user_question", "Bash",
             "", "  Question  ", "mcp__x", "mcp__x__y__question"]
    out["is_question_tool"] = [[n, ev.is_question_tool(n)] for n in names]
    tools = ["Bash", "bash", "bg_run", "PowerShell", "Read", "read_symbol", "LS", "view", "MultiEdit", "write",
             "apply_patch", "delete", "Grep", "rg", "WebFetch", "google_web_search", "TodoWrite", "update_plan",
             "mcp__x__read", "mcp__x__a__grep", "mcp__", "Frobnicate", "", " bash ", "mcp__a__b__c__read"]
    out["tool_action"] = [[n, ev.tool_action(n)] for n in names + tools]
    sums = [["Bash", {"command": " ls -la "}], ["Read", {"file_path": "/a"}], ["Read", {"command": "  ", "url": "u"}],
            ["Task", {"description": "d"}], ["Task", {}], ["Task", None], ["Task", "x"], ["G", {"pattern": 5, "path": "p"}]]
    out["tool_summary"] = [[t, i, ev._tool_summary(t, i)] for t, i in sums]
    claude_payloads = [
        {"hook_event_name": "UserPromptSubmit", "session_id": "s1"},
        {"hook_event_name": "UserPromptSubmit", "session_id": "s1", "prompt": "fix the bug"},
        {"hook_event_name": "PreToolUse", "session_id": "s1", "tool_name": "Read"},
        {"hook_event_name": "PreToolUse", "session_id": "s1", "tool_name": "Bash", "tool_input": {"command": "x" * 400}},
        {"hook_event_name": "PreToolUse", "session_id": "s1", "tool_name": "Bash", "tool_input": {"command": 5}},
        {"hook_event_name": "PreToolUse", "session_id": "s1", "tool_name": "AskUserQuestion",
         "tool_input": {"questions": [{"question": "Which DB?"}]}},
        {"hook_event_name": "PreToolUse", "session_id": "s1", "tool_name": "Skill", "tool_input": {"skill": "ak:debug"}},
        {"hook_event_name": "PreToolUse", "session_id": "s1", "tool_name": 5},
        {"hook_event_name": "PermissionRequest", "session_id": "s1", "tool_name": "Bash", "tool_input": {"command": " npm test "}},
        {"hook_event_name": "PermissionRequest", "session_id": "s1", "tool_name": "Edit", "tool_input": {"file_path": "/a/b"}},
        {"hook_event_name": "PermissionRequest", "session_id": "s1"},
        {"hook_event_name": "Notification", "session_id": "s1", "message": "Claude is waiting for your input", "notification_type": "idle_prompt"},
        {"hook_event_name": "Notification", "session_id": "s1", "message": "Claude is waiting for your input"},
        {"hook_event_name": "Notification", "session_id": "s1", "message": "Claude needs your permission", "notification_type": "permission_prompt"},
        {"hook_event_name": "Notification", "session_id": "s1"},
        {"hook_event_name": "Notification", "session_id": "s1", "message": 5},
        {"hook_event_name": "StopFailure", "session_id": "s1"}, {"hook_event_name": "SessionStart", "session_id": "s1"},
        {"hook_event_name": "Stop", "session_id": "s1", "cursor_version": "1.0"},
        {"hook_event_name": "Stop"}, {"hook_event_name": "Stop", "session_id": ""}, {"hook_event_name": "Stop", "session_id": 5},
        {"hook_event_name": 5, "session_id": "s1"}, {"hook_event_name": "SomethingNew", "session_id": "s1"},
        [1, 2], None, "x", {}]
    for name in ev.CLAUDE_HOOK_EVENTS:
        claude_payloads.append({"hook_event_name": name, "session_id": "z"})
    out["claude_event"] = [[p, ev.claude_event(p)] for p in claude_payloads]
    pi_payloads = [
        {"ev": "nope", "sid": "x"}, {"ev": "tool"}, {"ev": "tool", "sid": ""}, {"ev": "tool", "sid": 0},
        {"ev": "attention", "sid": "x", "msg": "Pick one"}, {"ev": "tool", "sid": "y" * 300, "cmd": "c" * 400, "sub": 1},
        {"ev": "prompt", "sid": "p", "debug": True}, {"ev": "prompt", "sid": "p", "debug": "yes"},
        {"ev": "tool", "sid": 7, "tool": 5, "msg": 0}, {"ev": "tool", "sid": "a", "tool": None}, {"ev": 5, "sid": "a"},
        [1], None, {"ev": "end", "sid": "e", "sub": False}]
    for name in sorted(ev.EVENTS):
        pi_payloads.append({"ev": name, "sid": "k"})
    out["pi_event"] = [[p, ev.pi_event(p)] for p in pi_payloads]
    desired = [
        {"a": "", "seq": [], "rep": False, "v": [], "u": False, "txt": "", "cd": 0, "dim": -1, "focus_until": 0},
        {"a": "x", "seq": ["x"], "rep": False, "v": [], "u": True, "txt": "", "cd": 0, "dim": -1, "focus_until": 0},
        {"a": "x", "seq": ["x"], "rep": False, "v": [], "u": False, "txt": "t", "cd": 5, "dim": 10, "focus_until": 1},
        {"a": "s", "seq": ["y", "s"], "rep": False, "v": [], "u": False, "txt": "", "cd": 0, "dim": -1, "focus_until": 0},
        {"a": "question", "seq": ["react left", "react right"], "rep": True, "v": [], "u": True, "txt": "Your turn!", "cd": 0, "dim": 0, "focus_until": 0},
        {"a": "typing", "seq": ["typing"], "rep": False, "v": ["building", "carrying"], "u": False, "txt": "", "cd": 0, "dim": -1, "focus_until": 0},
        {"a": "x", "seq": ["x"], "rep": False, "v": [], "u": False, "txt": "Ünï \"q\"", "cd": 0, "dim": 15, "focus_until": 0}]
    out["wire"] = [[d, json.dumps(ev.wire(d), separators=(",", ":"))] for d in desired]
    targets = ["http://192.168.1.48/api/usage", "http://clock.local:8080/api/usage?x=1#f", "HTTP://Host/api/usage",
               "no-scheme/api/usage", "http://[::1]:8080/api/usage", "https://c/api/usage", ""]
    out["urls"] = [[t, ev.state_url(t), ev.heap_url(t)] for t in targets]
    ports = []
    for env, cfg in [(None, {}), (None, {"event_port": "5000"}), (None, {"event_port": " 5000 "}), ("6000", {"event_port": "5000"}),
                     ("0", {}), ("70000", {}), ("65535", {}), ("65536", {}), ("abc", {}), ("-1", {}), ("abc", {"event_port": "5000"}),
                     (None, {"event_port": "x"}), (None, {"event_port": ""}), (None, {"event_port": "0"}), ("  7  ", {})]:
        if env is None:
            os.environ.pop("CLAWD_EVENT_PORT", None)
        else:
            os.environ["CLAWD_EVENT_PORT"] = env
        ports.append([{"env": env, "config": cfg}, ev.read_event_port(cfg.get)])
    os.environ.pop("CLAWD_EVENT_PORT", None)
    out["read_event_port"] = ports
    pet_cfgs = [{}, {"quiet_start": "22:30", "quiet_end": "6:45", "quiet_dim": "200", "away_after": "5",
                     "milestone_turns": "x", "event_port": "5000", "idle_linger": "60"},
                {"quiet_start": " 7:05 ", "quiet_end": "25:75", "quiet_dim": "-5", "away_after": "9000", "milestone_turns": "-3"},
                {"quiet_start": "7pm", "quiet_end": "1:5", "quiet_dim": "x", "away_after": "", "milestone_turns": "0"},
                {"quiet_start": "00:00", "quiet_end": "24:00", "quiet_dim": "50", "away_after": " 45 ", "milestone_turns": "7"}]
    out["read_pet_config"] = [[c, vars(ev.read_pet_config(c.get))] for c in pet_cfgs]
    out["constants"] = {"EVENTS": sorted(ev.EVENTS), "CLAUDE_HOOK_EVENTS": list(ev.CLAUDE_HOOK_EVENTS)}
    return out


def main():
    h.write_fixture("events", "helpers", input={"note": "cases are [..inputs.., expected]"}, output=helpers(), ctx=ctx)
    results = {name: run_scenario(spec) for name, spec in SCENARIOS.items()}
    h.write_fixture("events", "scenarios", input=SCENARIOS, output=results, ctx=ctx)
    print(f"wrote {len(SCENARIOS)} scenarios")


if __name__ == "__main__":
    main()
