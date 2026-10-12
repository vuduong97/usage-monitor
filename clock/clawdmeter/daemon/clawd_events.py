"""clawd-home: agent events, pet moods and pokes -> the clock's desk view.

Everything the clock's full-screen Clawd shows is decided here, on the Mac,
so it can be tested and changed without reflashing. The clock only plays a
named animation from /desk.pak with an optional text line, countdown and
brightness cap (POST /api/state, see smalltv UsageClient.cpp).

Sources, all on 127.0.0.1 (EventServer):
  POST /hook/claude  raw Claude Code hook payload (integrations/claude)
  POST /hook/pi      normalized pi extension event (integrations/pi)
  POST /poke         `clawd` CLI commands (integrations/cli)
  GET  /state        what the clock should show now + live sessions (debug)

Layers, highest first: poke moment > agent activity > focus timer > pet
moment > idle pick > nothing (the clock shows its usage screens).

When nothing else is up the clock shows usage, and every IDLE_GAP seconds
(random) Clawd slips in one idle pick for one pass: an animation, or the
yawn -> sleeping -> wake chain. That holds whether you are at the Mac or away;
coming back plays wake + "Welcome back!" first. Anything above cancels the pick
and starts a new gap, so the next pick comes IDLE_GAP after the screen frees up.

A view can be a sequence the clock plays through on its own: each animation
plays one full pass, the clock switches at pass ends, and only urgent views
(needs you, errors, pokes, waking up) cut the current pass short.
"""
from __future__ import annotations

import asyncio
import json
import os
import random
import re
import subprocess
import time
import unicodedata
import urllib.parse
from dataclasses import dataclass, replace
from pathlib import Path
from typing import Callable

import httpx

EVENT_HOST = "127.0.0.1"
EVENT_PORT = 47913
STATE_PATH = "/api/state"
HEAP_PATH = "/api/heap"
STATE_TTL = 60            # seconds the clock keeps a state without a resend
RESEND_EVERY = 20         # resend an unchanged live state this often
PUSH_TIMEOUT = 1.5
COALESCE = 0.3            # gather an event burst before pushing
MAX_BODY = 4 * 1024 * 1024

SESSION_TTL = 600         # a working/thinking session silent this long went away
ATTENTION_TTL = 1800      # how long "needs you" stays up without an answer
IDLE_FORGET = 3600        # drop idle sessions after this
TEXT_MAX = 90

# Timings from clawd-on-desk's theme (themes/clawd/theme.json), in seconds.
ERROR_SHOW = 5.0          # autoReturn.error
DONE_SHOW = 7.0           # the "Done!" happy, then the usage screens
DIZZY_SHOW = 6.0          # autoReturn.dizzy
MIN_DISPLAY = {"notification": 5.0, "question": 5.0, "error": 5.0, "dizzy": 5.0, "sweeping": 5.5, "happy": 4.0}
MIN_DISPLAY_WORKING = 1.0  # minDisplay.working / .thinking: every other agent animation
WAKE_SECS = 1.5           # wakeDuration
POKE_SECS = 2.5           # reactions.clickLeft / clickRight
ANNOYED_SECS = 3.5        # reactions.annoyed

# Agent variants (clawd-home's own triggers).
ULTRATHINK_AFTER = 30     # this long with no tool running (from the prompt or the last tool)
EDIT_STREAK = 10          # this many edit tools in a row: "typing boss"
WIZARD_AFTER = 1800       # a turn running this long: "wizard"
DIZZY_ERRORS, DIZZY_WINDOW = 3, 120   # this many errors within the window: "dizzy"
ANNOYED_POKES, ANNOYED_WINDOW = 3, 5  # this many pokes within the window: "annoyed"

# Idle picks: what Clawd slips in between the usage screens, one pass each, never
# the same pick twice in a row. Nothing tied to an agent action (reading, typing...).
IDLE_GAP = (180.0, 360.0)  # seconds of usage screens between two picks
IDLE_PICKS = (("idle",), ("look",), ("bubble",), ("coffee hand",), ("shy",), ("outlaw",),
              ("yawn", "sleeping", "wake"))
# Picks only a newer desk.pak has (31+ animations, `desk` in /api/heap). An older
# clock would play its first animation ("thinking") for a name it does not know.
IDLE_PICKS_NEW = frozenset({"coffee hand", "shy", "outlaw"})
DESK_NEW_MIN = 31
DESK_PROBE_EVERY = 300     # re-read the clocks' animation count this often
DESK_PROBE_TIMEOUT = 12.0  # the clock answers HTTP slowly for a while after boot
# One pass of each idle animation on the clock (desk.pak: intro + one loop, or to
# the end of a one-shot), seconds. The pick ends the state right then: the clock
# drops a desk view at once, so a longer guess shows the start of a second pass.
PASS_SECS = {"idle": 16.08, "look": 10.05, "bubble": 4.5, "coffee hand": 4.4, "shy": 3.21,
             "outlaw": 16.0, "yawn": 7.0, "sleeping": 10.05, "wake": 1.5}

# A shell command that runs a test suite (Bash / bash tool input).
TEST_CMD = re.compile(
    r"(?:^|[\s;&|(])(?:pytest|py\.test|jest|vitest|mocha|tox|nox|rspec|phpunit|ctest"
    r"|go\s+test|cargo\s+(?:test|nextest)|(?:npm|pnpm|yarn|bun)(?:\s+run)?\s+test(?::\S+)?"
    r"|node\s+--test|deno\s+test|make\s+(?:test|check)|python3?\s+-m\s+(?:pytest|unittest)"
    r"|pio\s+test|swift\s+test|dotnet\s+test|mvn\s+test|gradlew?\s+test)(?=$|[\s;&|)])")


def is_test_command(cmd) -> bool:
    return isinstance(cmd, str) and bool(TEST_CMD.search(cmd))


# A turn the user asked to debug: a debug/fix command or skill, or the word itself.
# "loi" alone is not enough: without accents "lỗi" (error) is "lời" (reply).
DEBUG_SKILLS = re.compile(r"^(?:ak[:-])?(?:debug|fix)$", re.I)
DEBUG_WORDS = re.compile(r"(?<!\w)(?:/(?:ak:|skill:ak-)(?:debug|fix)|debug\w*|fix|bugs?|lỗi)(?!\w)", re.I)
SKILL_BLOCK = re.compile(r"<skill\b[^>]*>[\s\S]*?(?:</skill>|$)")
SKILL_NAME = re.compile(r"<skill\s+name=\"([^\"]+)\"")
DEBUG_SKILL_FILE = re.compile(r"(?:^|[/\\])ak-(?:debug|fix)[/\\]SKILL\.md$", re.I)


def is_debug_prompt(text) -> bool:
    """A prompt that asks for debugging. Skill bodies a harness expanded into the
    prompt (pi's <skill name=...>...</skill>) only count by their name: their
    text mentions fixes and bugs whatever the task is."""
    if not isinstance(text, str):
        return False
    text = unicodedata.normalize("NFC", text)
    if any(DEBUG_SKILLS.match(n) for n in SKILL_NAME.findall(text)):
        return True
    return bool(DEBUG_WORDS.search(SKILL_BLOCK.sub(" ", text)))


def is_debug_skill_load(tool, inp) -> bool:
    """A tool call that loads the debug/fix skill: Claude Code's Skill tool, or
    reading that skill's SKILL.md (how pi and Claude both load a skill file)."""
    if not isinstance(inp, dict):
        return False
    name = str(tool or "").lower()
    if name == "skill":
        return bool(DEBUG_SKILLS.match(str(inp.get("skill") or inp.get("command") or "").strip("/ ")))
    if tool_action(name) == "read":
        path = inp.get("file_path") or inp.get("path")
        return isinstance(path, str) and bool(DEBUG_SKILL_FILE.search(path))
    return False


def log(msg: str) -> None:
    print(f"[{time.strftime('%H:%M:%S')}] {msg}", flush=True)


def ascii_text(text, limit: int = TEXT_MAX) -> str:
    """The clock's fonts are ASCII: fold accents (d-stroke too), collapse
    whitespace, cap the length."""
    text = str(text or "").replace("\u0111", "d").replace("\u0110", "D")
    text = unicodedata.normalize("NFKD", text).encode("ascii", "ignore").decode()
    text = " ".join(text.split())
    return text if len(text) <= limit else text[: limit - 3].rstrip() + "..."


# Tool name (Claude Code or pi, any case) -> the action the clock shows.
# Anything not listed reads as plain "Working" / "Tool failed".
ACTION_TEXT = {                      # action: (while running, when it failed)
    "run": ("Running", "Run failed"),
    "read": ("Reading", "Read failed"),
    "edit": ("Editing", "Edit failed"),
    "search": ("Searching", "Search failed"),
    "browse": ("Browsing", "Browse failed"),
    "plan": ("Planning", "Plan failed"),
    "": ("Working", "Tool failed"),
}
READING_ACTIONS = ("read", "search")   # the last tool read or searched: "reading"
# Per-session animations that still show (with the agent count) when 2+ agents run;
# a debugging session shows through too, whatever its animation.
SESSION_SHOWS_THROUGH = {"ultrathink", "debugger", "reading"}
# Same-meaning variants of a long agent animation (the state's `v`): the clock plays
# the animation 3 passes, then one variant picked at random, then the animation
# again, all under the same text. So a variant must show what the text says:
# keyboard work stays at a keyboard. Clocks with older firmware ignore `v`.
VARIANTS = {
    "typing": ("typing boss",),
    "typing boss": ("typing",),
    "wizard": ("typing", "building", "carrying"),   # "Long task": any kind of work
    "ultrathink": ("thinking",),
}
WORKING_VARIANTS = ("building", "carrying", "typing boss")  # a tool with no action: "Working"
WIZARD_TEXT = "Long task"
DEBUG_TEXT = "Debugging"
TOOL_ACTIONS = {
    **dict.fromkeys(("bash", "bashoutput", "bg_run"), "run"),
    **dict.fromkeys(("read", "notebookread", "ls", "read_symbol", "read_enclosing"), "read"),
    **dict.fromkeys(("edit", "multiedit", "write", "notebookedit"), "edit"),
    **dict.fromkeys(("grep", "glob", "find", "symbol_search"), "search"),
    **dict.fromkeys(("webfetch", "websearch", "web_fetch", "batch_web_fetch"), "browse"),
    **dict.fromkeys(("todowrite", "todo"), "plan"),
}


# Tools an agent uses to ask the user something: the clock shows Clawd looking left
# and right with a "?" (react left/right) instead of the plain notification.
QUESTION_TOOLS = {"ask_user_question", "ask_user", "question", "askuserquestion"}
QUESTION_SEQ = ("react left", "react right")
# What the clock says while a question is open. The question itself never shows
# (it never even leaves the hook): one phrase is picked per question, at random,
# never the same as the question before. ASCII, <= 20 chars so it does not scroll.
QUESTION_TEXTS = (
    "Hey! Got a question", "Your turn!", "Hey! Over to you", "Your call, boss",
    "Help me pick?", "Psst... a question", "Knock knock!", "Hmm, help me out?",
    "Pick one for me?",
)


def is_question_tool(name) -> bool:
    name = str(name or "").strip().lower()
    if name.startswith("mcp__"):
        name = name.split("__", 2)[-1]
    return name in QUESTION_TOOLS


def tool_action(name) -> str:
    """The action key for a raw tool name; an MCP tool (mcp__<server>__<tool>)
    is looked up by its own name."""
    name = str(name or "").strip().lower()
    if name.startswith("mcp__"):
        name = name.split("__", 2)[-1]
    return TOOL_ACTIONS.get(name, "")


@dataclass(frozen=True)
class View:
    anim: str                 # what it is called (for logs, `clawd status`, holds)
    text: str = ""
    layer: str = ""           # poke | agent | focus | pet
    seq: tuple[str, ...] = ()  # the clock plays these in order (default: just anim)
    rep: bool = False         # cycle seq instead of looping its last animation
    urgent: bool = False      # replace what plays now instead of at its pass end
    variants: tuple[str, ...] = ()  # the clock mixes one in every 3 passes (`v`)
    debug: bool = False       # a debugging session (shows through many agents)

    @property
    def names(self) -> tuple[str, ...]:
        return self.seq or (self.anim,)


def min_display(anim: str) -> float:
    return MIN_DISPLAY.get(anim, MIN_DISPLAY_WORKING)


# =============================================================================
# Agent sessions
# =============================================================================
@dataclass
class Sess:
    sid: str
    src: str
    sub: bool = False          # a child agent process (e.g. `pi -p` run by a subagent)
    state: str = "idle"        # idle | thinking | working | attention | waiting | sweeping
    #   waiting = still needs an answer, but the user has seen it (hidden)
    tool: str = ""             # raw tool name, for `clawd status`
    action: str = ""           # its ACTION_TEXT key, for the clock
    msg: str = ""
    asking: bool = False       # the attention is a question tool (react left/right)
    ask_text: str = ""         # the QUESTION_TEXTS phrase picked for that question
    subs: int = 0              # live subagents this session started
    last: float = 0.0
    error_until: float = 0.0
    error_text: str = ""
    done_until: float = 0.0
    since: float = 0.0         # when state last changed
    turn_start: float = 0.0    # first prompt/tool of the running turn
    running: int = 0           # tools started and not finished yet
    quiet_since: float = 0.0   # no tool running since then (ultrathink)
    testing: bool = False      # the running tool is a test command
    debugging: bool = False    # a tool failed in this turn or the one before
    debug_next: bool = False   # ... so the next turn debugs too
    edits: int = 0             # edit tools in a row


EVENTS = {"start", "prompt", "tool", "tool_done", "tool_error", "subagent_start",
          "subagent_stop", "attention", "attention_seen", "attention_done", "compact",
          "compact_done",
          "stop", "stop_error", "abort", "end"}


class Tracker:
    """Per-session agent state, resolved into one desk animation."""

    def __init__(self, on_turn: Callable[[float], None] | None = None,
                 rng: random.Random | None = None) -> None:
        self.sessions: dict[str, Sess] = {}
        self.on_turn = on_turn
        self.rng = rng or random.Random()
        self.last_ask_text = ""        # no phrase twice in a row
        self.errors: list[float] = []  # recent failures (dizzy)
        self.dizzy_until = 0.0

    def _error(self, now: float) -> None:
        self.errors = [t for t in self.errors if now - t < DIZZY_WINDOW] + [now]
        if len(self.errors) >= DIZZY_ERRORS:
            self.errors.clear()
            self.dizzy_until = now + DIZZY_SHOW

    def ingest(self, ev: dict, now: float) -> bool:
        kind = str(ev.get("ev") or "")
        sid = str(ev.get("sid") or "")[:200]
        if kind not in EVENTS or not sid:
            return False
        if kind == "end":
            return self.sessions.pop(sid, None) is not None
        s = self.sessions.get(sid)
        if s is None:
            s = self.sessions[sid] = Sess(sid=sid, src=str(ev.get("src") or ""),
                                          sub=bool(ev.get("sub")), since=now, quiet_since=now)
        s.last = now
        before = s.state
        self._apply(s, kind, ev, now)
        if s.state != before:
            s.since = now
        return True

    def _apply(self, s: Sess, kind: str, ev: dict, now: float) -> None:
        raw = ev.get("tool") or ""
        tool = ascii_text(raw, 24)
        if kind in ("tool_done", "tool_error"):
            s.running = max(s.running - 1, 0)
            if not s.running:
                s.quiet_since = now
        if kind == "prompt":
            s.state, s.msg = "thinking", ""
            s.turn_start, s.edits, s.testing = now, 0, False
            s.running, s.quiet_since = 0, now
            s.debugging, s.debug_next = s.debug_next or bool(ev.get("debug")), False
        elif kind == "tool":
            s.state, s.msg = "working", ""
            if not s.turn_start:
                s.turn_start = now
            s.running = min(s.running + 1, 16)
            if tool:
                s.tool, s.action = tool, tool_action(raw)
                s.edits = s.edits + 1 if s.action == "edit" else 0
            s.testing = is_test_command(ev.get("cmd"))
            if ev.get("debug"):
                s.debugging = True
        elif kind == "tool_done":
            s.state, s.msg, s.testing = "working", "", False
        elif kind == "attention_done":
            if s.state in ("attention", "waiting"):
                s.state, s.msg, s.asking = "thinking", "", False
                s.quiet_since = now
        elif kind == "attention_seen":         # the user went to that session: stop calling
            if s.state == "attention":
                s.state, s.msg = "waiting", ""
        elif kind == "tool_error":
            if s.state not in ("attention", "waiting"):
                s.state = "working"
            action = tool_action(raw) if tool else s.action
            s.error_until, s.error_text = now + ERROR_SHOW, ACTION_TEXT[action][1]
            s.testing, s.debugging, s.debug_next = False, True, True
            self._error(now)
        elif kind == "subagent_start":
            s.subs = min(s.subs + 1, 8)
        elif kind == "subagent_stop":
            s.subs = max(s.subs - 1, 0)
        elif kind == "attention":
            open_q = s.asking and s.state in ("attention", "waiting")
            asking = is_question_tool(raw)
            if not raw and open_q:
                asking = True                   # a generic nudge while a question is open
            if asking and not open_q:           # a new question: pick its phrase once
                pool = [t for t in QUESTION_TEXTS if t != self.last_ask_text]
                s.ask_text = self.last_ask_text = self.rng.choice(pool)
            msg = "" if asking else ascii_text(ev.get("msg") or "")
            s.state, s.msg, s.asking = "attention", msg, asking
        elif kind == "compact":
            s.state = "sweeping"
        elif kind == "compact_done":
            s.state, s.quiet_since = "thinking", now
        elif kind == "stop":
            s.state, s.subs, s.msg = "idle", 0, ""
            s.done_until = now + DONE_SHOW
            if not s.sub and self.on_turn is not None:
                self.on_turn(now)
        elif kind == "abort":                   # the user cancelled: back to idle, quietly
            s.state, s.subs, s.msg = "idle", 0, ""
        elif kind == "stop_error":
            s.state, s.subs, s.msg = "idle", 0, ""
            s.error_until, s.error_text = now + ERROR_SHOW, "Error"
            s.debug_next = True
            self._error(now)
        if kind in ("stop", "abort", "stop_error"):
            s.turn_start, s.debugging, s.edits, s.testing = 0.0, False, 0, False
            s.running = 0
        if s.state not in ("attention", "waiting"):
            s.asking = False

    def expire(self, now: float) -> None:
        for sid, s in list(self.sessions.items()):
            age = now - s.last
            if s.state in ("attention", "waiting") and age > ATTENTION_TTL:
                s.state, s.msg, s.asking = "idle", "", False
            elif s.state in ("thinking", "working", "sweeping") and age > SESSION_TTL:
                s.state, s.subs = "idle", 0
            if s.state == "idle" and age > IDLE_FORGET:
                del self.sessions[sid]

    def view(self, now: float) -> View | None:
        self.expire(now)
        live = list(self.sessions.values())
        main = [s for s in live if not s.sub]

        def latest(xs):
            return max(xs, key=lambda s: s.last)

        attn = [s for s in main if s.state == "attention"]
        if attn:
            a = latest(attn)
            if a.asking:
                return View("question", a.ask_text or QUESTION_TEXTS[0], "agent",
                            seq=QUESTION_SEQ, rep=True, urgent=True)
            return View("notification", a.msg or "Needs you", "agent", urgent=True)
        if self.dizzy_until > now:
            return View("dizzy", "Too many errors", "agent", urgent=True)
        errs = [s for s in main if s.error_until > now]
        if errs:
            return View("error", latest(errs).error_text or "Error", "agent", urgent=True)
        if any(s.state == "sweeping" for s in main):
            return View("sweeping", "Compacting", "agent")
        # Agents: running main sessions + their subagents + live child processes.
        active = [s for s in main if s.state in ("thinking", "working")]
        agents = len(active) + sum(s.subs for s in main) + sum(
            1 for s in live if s.sub and s.state in ("thinking", "working"))
        if agents == 1 and active:
            return self._session(active[0], now)
        if agents:
            # The most recent session's reading/debugger/ultrathink shows through,
            # counted; anything plainer leaves the many-agents animation.
            if active:
                v = self._session(latest(active), now)
                if v.anim in SESSION_SHOWS_THROUGH or v.debug:
                    return replace(v, text=f"{v.text} ({agents})")
            # One extra agent beside a running session: headphones; more: juggling.
            # With no main session running, the agents left are all extra.
            extra = agents - 1 if active else agents
            noun = "agent" if agents == 1 else "agents"
            return View("headphones" if extra <= 1 else "juggling", f"{agents} {noun}", "agent")
        if any(s.done_until > now for s in main):
            return View("happy", "Done!", "agent")
        return None

    @staticmethod
    def _session(s: Sess, now: float) -> View:
        """One thinking/working session: ultrathink > testing (debugger) >
        reading > typing boss > wizard > typing (thinking before its first tool).
        The text always says what the animation shows: a debugging turn is the
        debugger ("Debugging"), a long turn the wizard ("Long task")."""
        if not s.running and now - s.quiet_since >= ULTRATHINK_AFTER:
            return View("ultrathink", "Thinking hard", "agent", variants=VARIANTS["ultrathink"])
        if s.state == "thinking":
            return View("thinking", "Thinking", "agent")
        if s.testing:
            return View("debugger", "Testing", "agent")
        if s.debugging:
            return View("debugger", DEBUG_TEXT, "agent", debug=True)
        if s.action in READING_ACTIONS:
            anim = "reading"
        elif s.edits >= EDIT_STREAK:
            anim = "typing boss"
        elif s.turn_start and now - s.turn_start >= WIZARD_AFTER:
            anim = "wizard"
        else:
            anim = "typing"
        if anim == "wizard":
            return View(anim, WIZARD_TEXT, "agent", variants=VARIANTS[anim])
        variants = WORKING_VARIANTS if anim == "typing" and not s.action else VARIANTS.get(anim, ())
        return View(anim, ACTION_TEXT[s.action][0], "agent", variants=variants)

    def snapshot(self, now: float) -> list[dict]:
        return [{"sid": s.sid, "src": s.src, "sub": s.sub, "state": s.state,
                 "tool": s.tool, "subs": s.subs, "running": s.running,
                 "debug": s.debugging, "age": round(now - s.last)}
                for s in self.sessions.values()]


# ---- Claude Code hook payloads -> events ----------------------------------------
def _tool_summary(tool: str, inp) -> str:
    if not isinstance(inp, dict):
        return tool
    for key in ("command", "file_path", "path", "url", "pattern", "description"):
        v = inp.get(key)
        if isinstance(v, str) and v.strip():
            return f"{tool}: {v.strip()}"
    return tool


_CLAUDE_SIMPLE = {
    "SessionStart": "start", "UserPromptSubmit": "prompt", "PostToolUse": "tool_done",
    "PostToolUseFailure": "tool_error", "SubagentStart": "subagent_start",
    "SubagentStop": "subagent_stop", "Stop": "stop", "StopFailure": "stop_error",
    "PreCompact": "compact", "PostCompact": "compact_done", "SessionEnd": "end",
}
CLAUDE_HOOK_EVENTS = sorted([*_CLAUDE_SIMPLE, "PreToolUse", "PermissionRequest", "Notification"])


def claude_event(p) -> dict | None:
    """Map one Claude Code hook payload (its stdin JSON) onto a tracker event."""
    if not isinstance(p, dict):
        return None
    name = p.get("hook_event_name")
    sid = p.get("session_id")
    if not isinstance(name, str) or not isinstance(sid, str) or not sid:
        return None
    raw_tool = p.get("tool_name")
    tool = raw_tool if isinstance(raw_tool, str) else ""
    ev: dict[str, object] = {"src": "claude", "sid": f"claude:{sid}", "tool": tool}
    inp = p.get("tool_input")
    if isinstance(inp, dict) and isinstance(inp.get("command"), str):
        ev["cmd"] = inp["command"][:300]
    if name == "UserPromptSubmit" and is_debug_prompt(p.get("prompt")):
        ev["debug"] = True                  # only the verdict travels, never the prompt
    if name == "PreToolUse" and is_debug_skill_load(tool, inp):
        ev["debug"] = True
    if name in _CLAUDE_SIMPLE:
        return {**ev, "ev": _CLAUDE_SIMPLE[name]}
    if name == "PreToolUse":
        if tool == "AskUserQuestion":           # the question text stays on this machine
            return {**ev, "ev": "attention"}
        return {**ev, "ev": "tool"}
    if name == "PermissionRequest":
        return {**ev, "ev": "attention", "msg": "Allow " + _tool_summary(tool, p.get("tool_input"))}
    if name == "Notification":
        kind = str(p.get("notification_type") or "")
        msg = str(p.get("message") or "")
        if kind == "idle_prompt" or (not kind and "waiting for your input" in msg.lower()):
            return None          # the turn already ended; nothing to answer
        return {**ev, "ev": "attention", "msg": msg or "Claude needs you"}
    return None


def pi_event(p) -> dict | None:
    """Validate a pi extension event (already normalized by the extension)."""
    if not isinstance(p, dict) or p.get("ev") not in EVENTS or not p.get("sid"):
        return None
    return {"src": "pi", "sid": f"pi:{str(p['sid'])[:160]}", "ev": p["ev"],
            "tool": str(p.get("tool") or ""), "msg": str(p.get("msg") or ""),
            "cmd": str(p.get("cmd") or "")[:300], "sub": bool(p.get("sub")),
            "debug": isinstance(p.get("debug"), bool) and bool(p["debug"])}


# =============================================================================
# Pet
# =============================================================================
@dataclass
class Moment:
    anim: str
    text: str
    secs: float
    start: float = 0.0
    seq: tuple[str, ...] = ()
    urgent: bool = False


@dataclass
class PetConfig:
    quiet_start: int = 23 * 60     # minutes since local midnight
    quiet_end: int = 7 * 60
    quiet_dim: int = 15            # brightness cap in quiet hours, %
    away_after: int = 300          # HID idle seconds before Clawd dozes off
    low_pct: int = 80              # session % that worries Clawd
    low_every: int = 900
    yawn_from: int = 22 * 60
    yawn_every: int = 1800
    milestone: int = 10            # turns per day


def _in_window(minute: int, start: int, end: int) -> bool:
    if start == end:
        return False
    return start <= minute < end if start < end else (minute >= start or minute < end)


def _play(queue: list[Moment], now: float, layer: str) -> View | None:
    while queue:
        m = queue[0]
        if not m.start:
            m.start = now
        if now - m.start < m.secs:
            return View(m.anim, m.text, layer, seq=m.seq, urgent=m.urgent)
        queue.pop(0)
    return None


class Pet:
    def __init__(self, cfg: PetConfig | None = None, store: Path | None = None,
                 localtime: Callable[[float], time.struct_time] = time.localtime,
                 rng: random.Random | None = None) -> None:
        self.cfg = cfg or PetConfig()
        self.store = store
        self.localtime = localtime
        self.rng = rng or random.Random()
        self.moments: list[Moment] = []
        self.away = False
        self.desk_new = False                  # every clock has the IDLE_PICKS_NEW animations
        self.pick: Moment | None = None        # the idle pick on screen
        self.next_pick = 0.0                   # when the next one starts (0: not scheduled)
        self.last_pick: tuple[str, ...] = ()
        self.last_low = 0.0
        self.last_yawn = 0.0
        self.last_pct: float | None = None
        self.last_reset_min: int | None = None
        self.day, self.turns, self.greeted = "", 0, ""
        self._load()

    # -- persistence (turns + greeting per day) --
    def _load(self) -> None:
        if not self.store:
            return
        try:
            d = json.loads(self.store.read_text())
            self.day, self.turns = str(d.get("day", "")), int(d.get("turns", 0))
            self.greeted = str(d.get("greeted", ""))
        except (OSError, ValueError, TypeError, AttributeError):
            pass

    def _save(self) -> None:
        if not self.store:
            return
        try:
            self.store.parent.mkdir(parents=True, exist_ok=True)
            tmp = self.store.with_suffix(".tmp")
            tmp.write_text(json.dumps({"day": self.day, "turns": self.turns, "greeted": self.greeted}))
            tmp.replace(self.store)
        except OSError:
            pass

    def _today(self, now: float) -> str:
        return time.strftime("%Y-%m-%d", self.localtime(now))

    def _minute(self, now: float) -> int:
        t = self.localtime(now)
        return t.tm_hour * 60 + t.tm_min

    def push(self, anim: str, text: str, secs: float) -> None:
        self.moments.append(Moment(anim, ascii_text(text), secs))

    # -- inputs --
    def observe(self, now: float, idle_s: float | None, usage: dict | None) -> None:
        c = self.cfg
        if idle_s is not None:
            away = idle_s >= c.away_after
            if away and not self.away:
                self.away = True               # only remembered for the greeting
                self.moments = []
            elif not away and self.away:
                self.away = False
                today, minute = self._today(now), self._minute(now)
                if 5 * 60 <= minute < 11 * 60 and self.greeted != today:
                    self.greeted = today
                    self._save()
                    text = "Good morning!"
                else:
                    text = "Welcome back!"
                self.moments = [Moment("wake", text, WAKE_SECS + 3, seq=("wake", "happy"), urgent=True)]
        if self.away:
            return
        if (_in_window(self._minute(now), c.yawn_from, c.quiet_start)
                and now - self.last_yawn >= c.yawn_every and not self.moments):
            self.last_yawn = now
            self.push("yawn", "So sleepy...", 4)
        if isinstance(usage, dict) and usage.get("ok", True) is not False \
                and isinstance(usage.get("s"), (int, float)) and usage.get("acct") != "ent":
            pct = float(usage["s"])
            sr = usage.get("sr") if isinstance(usage.get("sr"), int) else None
            prev, prev_sr = self.last_pct, self.last_reset_min
            reset = prev is not None and (
                (prev >= 20 and pct <= prev - 30)
                or (sr is not None and prev_sr is not None and sr > prev_sr + 120 and pct < prev))
            if reset:
                self.push("double jump", "Fresh quota!", 5)
            elif pct >= c.low_pct and now - self.last_low >= c.low_every and not self.moments:
                self.last_low = now
                self.push("low battery", f"Session {int(pct)}%", 6)
            self.last_pct, self.last_reset_min = pct, sr

    def on_turn(self, now: float) -> None:
        today = self._today(now)
        if today != self.day:
            self.day, self.turns = today, 0
        self.turns += 1
        self._save()
        if self.cfg.milestone and self.turns % self.cfg.milestone == 0:
            self.push("heart eyes", f"{self.turns} turns today!", 4)

    # -- outputs --
    def moment(self, now: float) -> View | None:
        return _play(self.moments, now, "pet")

    def hold_off(self, now: float) -> None:
        """Something else is on screen: drop the idle pick and start a new gap."""
        self.pick = None
        self.next_pick = now + self.rng.uniform(*IDLE_GAP)

    def pick_ends(self) -> float | None:
        return self.pick.start + self.pick.secs if self.pick else None

    def continuous(self, now: float) -> View | None:
        """The idle pick due now, or None (the usage screens)."""
        if self.pick and now - self.pick.start >= self.pick.secs:
            self.hold_off(now)
        if not self.pick:
            if not self.next_pick:
                self.hold_off(now)
            if now < self.next_pick:
                return None
            choices = [p for p in IDLE_PICKS if self.desk_new or not IDLE_PICKS_NEW.intersection(p)]
            choices = [p for p in choices if p != self.last_pick] or choices
            seq = self.rng.choice(choices)
            self.last_pick = seq
            self.pick = Moment(seq[0], "", sum(PASS_SECS[n] for n in seq), start=now,
                               seq=seq if len(seq) > 1 else ())
        m = self.pick
        return View(m.anim, "", "idle", seq=m.seq)

    def dim(self, now: float) -> int:
        c = self.cfg
        return c.quiet_dim if _in_window(self._minute(now), c.quiet_start, c.quiet_end) else -1


def read_hid_idle() -> float | None:
    """Seconds since the last keyboard/mouse input (macOS), None if unknown.
    A locked screen gets no input either, so this also covers "Mac locked"."""
    try:
        out = subprocess.run(["ioreg", "-c", "IOHIDSystem", "-d", "4"], capture_output=True,
                             text=True, timeout=3).stdout
    except (OSError, subprocess.SubprocessError):
        return None
    m = re.search(r'"HIDIdleTime"\s*=\s*(\d+)', out)
    return int(m.group(1)) / 1e9 if m else None


# =============================================================================
# Pokes (`clawd` CLI) and focus
# =============================================================================
class Pokes:
    def __init__(self) -> None:
        self.moments: list[Moment] = []
        self.focus_until = 0.0
        self.poked: list[float] = []
        self.left = False

    def handle(self, cmd: dict, now: float) -> dict:
        op = cmd.get("cmd")
        if op == "play":
            anim = ascii_text(cmd.get("anim") or "", 15)
            if not anim:
                return {"ok": False, "error": "anim required"}
            self._one(anim, cmd.get("text") or "", cmd.get("secs") or 6, now)
        elif op == "wave":
            self._one("happy", cmd.get("text") or "Hi!", cmd.get("secs") or 4, now)
        elif op == "say":
            text = cmd.get("text") or ""
            if not str(text).strip():
                return {"ok": False, "error": "text required"}
            self._one(cmd.get("anim") or "idle", text, cmd.get("secs") or 10, now)
        elif op == "focus":
            mins = cmd.get("mins")
            if mins in (0, "0", "stop", "off", None):
                self.focus_until = 0.0
            else:
                try:
                    m = float(mins)
                except (TypeError, ValueError):
                    return {"ok": False, "error": "mins must be a number"}
                if not 0 < m <= 600:
                    return {"ok": False, "error": "mins must be 1..600"}
                self.focus_until = now + m * 60
        elif op == "poke":                       # the desktop pet's clicks
            self.poked = [t for t in self.poked if now - t < ANNOYED_WINDOW] + [now]
            if len(self.poked) >= ANNOYED_POKES:
                self.poked.clear()
                self._one("annoyed", cmd.get("text") or "", ANNOYED_SECS, now)
            else:
                self.left = not self.left
                self._one("react left" if self.left else "react right", cmd.get("text") or "",
                          POKE_SECS, now)
        elif op == "clear":
            self.moments.clear()
        else:
            return {"ok": False, "error": f"unknown cmd {op!r}"}
        return {"ok": True}

    def _one(self, anim: str, text, secs, now: float) -> None:
        try:
            s = max(1.0, min(float(secs), 300.0))
        except (TypeError, ValueError):
            s = 6.0
        self.moments = [Moment(ascii_text(anim, 15), ascii_text(text), s, now, urgent=True)]

    def moment(self, now: float) -> View | None:
        if self.focus_until and now >= self.focus_until:
            self.focus_until = 0.0
            self.moments = [Moment("happy", "Focus done!", 6, now, urgent=True)]
        return _play(self.moments, now, "poke")

    def focus_left(self, now: float) -> int:
        return int(self.focus_until - now + 0.999) if self.focus_until > now else 0


# =============================================================================
# Resolution + push
# =============================================================================
FOCUS_KEEPS_TEXT = {"notification", "question", "error"}


class Brain:
    """Owns every layer and decides the one state the clock should show."""

    def __init__(self, pet: Pet | None = None) -> None:
        self.pet = pet or Pet()
        self.tracker = Tracker(on_turn=self.pet.on_turn)
        self.pokes = Pokes()
        self.wake = asyncio.Event()
        self.usage: dict | None = None          # latest usage payload (Publisher)
        self.shown: View | None = None          # last view handed out, for minDisplay
        self.shown_since = 0.0

    def _hold(self, v: View | None, now: float) -> View | None:
        """clawd-on-desk's minDisplay: an agent animation stays up at least
        min_display(anim) seconds before anything but a poke replaces it."""
        sh = self.shown
        if (sh is not None and sh.layer == "agent" and (v is None or v.layer != "poke")
                and (v is None or v.names != sh.names)
                and now - self.shown_since < min_display(sh.anim)):
            return sh
        if (v is None or sh is None or v.names != sh.names or v.rep != sh.rep
                or v.variants != sh.variants):
            self.shown_since = now
        self.shown = v
        return v

    def desired(self, now: float) -> dict:
        """{'a', 'seq', 'rep', 'v', 'u', 'txt', 'cd', 'dim', 'focus_until'} for this instant."""
        v = self.pokes.moment(now) or self.tracker.view(now)
        focus = self.pokes.focus_left(now)
        cd = 0
        if focus and (v is None or (v.layer == "agent" and v.anim not in FOCUS_KEEPS_TEXT)):
            v = replace(v, text="Focus") if v else View("reading", "Focus", "focus")
            cd = focus
        if v is None:
            v = self.pet.moment(now)
        if v is None:
            v = self.pet.continuous(now)
        v = self._hold(v, now)
        if v is not None and v.layer != "idle":
            self.pet.hold_off(now)               # the next idle pick waits a full gap
        txt = v.text if v else ""
        txt = txt[:1].upper() + txt[1:]          # the clock text always starts with a capital
        return {"a": v.anim if v else "", "seq": list(v.names) if v else [],
                "rep": bool(v and v.rep), "v": list(v.variants) if v else [],
                "u": bool(v and v.urgent), "txt": txt, "cd": cd,
                "dim": self.pet.dim(now), "focus_until": int(self.pokes.focus_until) if cd else 0}


def state_url(target: str) -> str:
    """An http_targets push URL (…/api/usage) -> that clock's /api/state URL."""
    parts = urllib.parse.urlsplit(target)
    return urllib.parse.urlunsplit((parts.scheme, parts.netloc, STATE_PATH, "", ""))


def heap_url(target: str) -> str:
    """An http_targets push URL (…/api/usage) -> that clock's /api/heap URL."""
    parts = urllib.parse.urlsplit(target)
    return urllib.parse.urlunsplit((parts.scheme, parts.netloc, HEAP_PATH, "", ""))


def wire(d: dict) -> dict:
    seq = d.get("seq") or []
    out = {"a": seq if len(seq) > 1 else d["a"], "ttl": STATE_TTL}
    if d.get("rep"):
        out["rep"] = True
    if d.get("v"):
        out["v"] = list(d["v"])
    if d.get("u"):
        out["u"] = True
    if d["txt"]:
        out["txt"] = d["txt"]
    if d["cd"]:
        out["cd"] = d["cd"]
    if d["dim"] >= 0:
        out["dim"] = d["dim"]
    return out


class StatePusher:
    """Pushes Brain.desired() to every clock: at once on a change (after a
    short coalesce), then every RESEND_EVERY seconds while something is up so
    the clock's TTL never lapses, and one final clear when it all goes quiet."""

    def __init__(self, brain: Brain, targets: Callable[[], list[str]],
                 transport: httpx.AsyncBaseTransport | None = None,
                 idle: Callable[[], float | None] = read_hid_idle,
                 clock: Callable[[], float] = time.time) -> None:
        self.brain, self.targets, self.transport = brain, targets, transport
        self.idle, self.clock = idle, clock
        self.last_key: tuple | None = None
        self.last_sent = 0.0
        self._ok: dict[str, bool] = {}
        self._last_idle_check = -1e9
        self._last_desk_probe = -1e9
        self._desk_task: asyncio.Task | None = None

    async def probe_desk(self) -> None:
        """Read every clock's animation count (`desk` in /api/heap): the newer idle
        picks only play when all the clocks that answer have them. No answer
        keeps what was known."""
        counts: list[int] = []
        async with httpx.AsyncClient(timeout=DESK_PROBE_TIMEOUT, transport=self.transport) as http:
            for t in self.targets():
                try:
                    r = await http.get(heap_url(t))
                    n = r.json().get("desk") if r.status_code == 200 else None
                except (httpx.HTTPError, ValueError, AttributeError):
                    n = None
                if isinstance(n, int) and not isinstance(n, bool):
                    counts.append(n)
        if counts:
            new = min(counts) >= DESK_NEW_MIN
            if new != self.brain.pet.desk_new:
                log(f"Clock desk.pak has {min(counts)} animations: "
                    + ("all idle picks on" if new else "newer idle picks off"))
            self.brain.pet.desk_new = new

    def _maybe_probe_desk(self, now: float) -> None:
        if now - self._last_desk_probe < DESK_PROBE_EVERY:
            return
        if self._desk_task is not None and not self._desk_task.done():
            return
        self._last_desk_probe = now
        self._desk_task = asyncio.create_task(self.probe_desk())

    def _wait_for(self, now: float) -> float:
        """How long the loop may sleep: at most 1 s, less when an idle pick ends
        sooner (the clock drops it the moment the clear arrives)."""
        ends = self.brain.pet.pick_ends()
        return 1.0 if ends is None else max(0.05, min(1.0, ends - now))

    @staticmethod
    def key(d: dict) -> tuple:
        return (d["a"], tuple(d.get("seq") or ()), d.get("rep", False), tuple(d.get("v") or ()),
                d["txt"], d["dim"], d["focus_until"])

    async def step(self) -> bool:
        now = self.clock()
        if now - self._last_idle_check >= 10:
            self._last_idle_check = now
            idle = await asyncio.to_thread(self.idle)
            self.brain.pet.observe(now, idle, self.brain.usage)
        d = self.brain.desired(now)
        k = self.key(d)
        live = bool(d["a"]) or d["dim"] >= 0
        if k == self.last_key and not (live and now - self.last_sent >= RESEND_EVERY):
            return False
        targets = [state_url(t) for t in self.targets()]
        if not targets:
            return False
        if k != self.last_key:
            log(f"Clock state: {d['a'] or '(usage)'}" + (f" - {d['txt']}" if d["txt"] else ""))
        if await self._send(targets, wire(d)):
            self.last_key, self.last_sent = k, now
            return True
        return False

    async def _send(self, urls: list[str], payload: dict) -> bool:
        body = json.dumps(payload, separators=(",", ":"))
        async with httpx.AsyncClient(timeout=PUSH_TIMEOUT, transport=self.transport) as http:
            res = await asyncio.gather(*(self._post(http, u, body) for u in urls))
        return any(res)

    async def _post(self, http: httpx.AsyncClient, url: str, body: str) -> bool:
        try:
            r = await http.post(url, content=body, headers={"Content-Type": "application/json"})
            ok, why = r.status_code == 200, f"HTTP {r.status_code}"
        except httpx.HTTPError as e:
            ok, why = False, str(e) or type(e).__name__
        if self._ok.get(url) != ok:
            log(f"State push to {url}: ok" if ok else f"State push to {url} failed: {why}")
        self._ok[url] = ok
        return ok

    async def run(self, stop: asyncio.Event) -> None:
        while not stop.is_set():
            try:
                self._maybe_probe_desk(self.clock())
                await self.step()
            except Exception as e:      # keep the loop alive
                log(f"State push cycle failed: {e!r}")
            try:
                await asyncio.wait_for(self.brain.wake.wait(), timeout=self._wait_for(self.clock()))
                await asyncio.sleep(COALESCE)
            except asyncio.TimeoutError:
                pass
            self.brain.wake.clear()


# =============================================================================
# Local event server
# =============================================================================
class EventServer:
    """Tiny HTTP/1.1 server on 127.0.0.1 for the hooks, the pi extension and
    the CLI. Requests carrying an Origin header are refused, so a web page in
    a browser cannot drive the clock through localhost."""

    def __init__(self, brain: Brain, host: str = EVENT_HOST, port: int = EVENT_PORT,
                 clock: Callable[[], float] = time.time) -> None:
        self.brain, self.host, self.port, self.clock = brain, host, port, clock
        self.server: asyncio.AbstractServer | None = None

    async def start(self) -> None:
        self.server = await asyncio.start_server(self._client, self.host, self.port)
        if self.server.sockets:
            self.port = self.server.sockets[0].getsockname()[1]
        log(f"Event server on http://{self.host}:{self.port}")

    async def close(self) -> None:
        if self.server:
            self.server.close()
            await self.server.wait_closed()

    async def _client(self, r: asyncio.StreamReader, w: asyncio.StreamWriter) -> None:
        try:
            status, body = await asyncio.wait_for(self._handle(r), timeout=10)
        except (asyncio.TimeoutError, asyncio.IncompleteReadError, ConnectionError, ValueError):
            status, body = 400, {"ok": False}
        try:
            data = json.dumps(body).encode()
            head = (f"HTTP/1.1 {status} {'OK' if status < 300 else 'Error'}\r\n"
                    f"Content-Type: application/json\r\nContent-Length: {len(data)}\r\n"
                    "Connection: close\r\n\r\n")
            w.write(head.encode() + data)
            await w.drain()
        except ConnectionError:
            pass
        finally:
            w.close()

    async def _handle(self, r: asyncio.StreamReader):
        parts = (await r.readline()).decode("latin1").split()
        if len(parts) < 2:
            return 400, {"ok": False}
        method, path = parts[0], parts[1].split("?", 1)[0]
        headers = {}
        while True:
            h = (await r.readline()).decode("latin1")
            if h in ("\r\n", "\n", ""):
                break
            k, _, v = h.partition(":")
            headers[k.strip().lower()] = v.strip()
        if "origin" in headers:
            return 403, {"ok": False, "error": "browser requests refused"}
        n = int(headers.get("content-length") or 0)
        if n > MAX_BODY:
            return 413, {"ok": False}
        raw = await r.readexactly(n) if n else b""
        now = self.clock()
        if method == "GET" and path == "/state":
            b = self.brain
            return 200, {"desired": b.desired(now), "sessions": b.tracker.snapshot(now),
                         "turns": b.pet.turns, "away": b.pet.away,
                         "focus_left": b.pokes.focus_left(now)}
        if method != "POST":
            return 404, {"ok": False}
        try:
            p = json.loads(raw or b"{}")
        except ValueError:
            return 400, {"ok": False, "error": "bad json"}
        if path == "/poke":
            res = self.brain.pokes.handle(p if isinstance(p, dict) else {}, now)
            if res.get("ok"):
                self.brain.wake.set()
            return (200 if res.get("ok") else 400), res
        if path == "/hook/claude":
            ev = claude_event(p)
        elif path == "/hook/pi":
            ev = pi_event(p)
        else:
            return 404, {"ok": False}
        if ev and self.brain.tracker.ingest(ev, now):
            self.brain.wake.set()
        return 200, {"ok": True}


def read_event_port(read_value: Callable[[str], str | None]) -> int:
    raw = os.environ.get("CLAWD_EVENT_PORT") or read_value("event_port")
    try:
        port = int(str(raw).strip()) if raw else EVENT_PORT
    except ValueError:
        port = EVENT_PORT
    return port if 0 <= port < 65536 else EVENT_PORT   # 0 = any free port (tests)


def read_pet_config(read_value: Callable[[str], str | None]) -> PetConfig:
    c = PetConfig()

    def hm(name: str, default: int) -> int:
        m = re.fullmatch(r"\s*(\d{1,2}):(\d{2})\s*", read_value(name) or "")
        return (int(m.group(1)) % 24) * 60 + int(m.group(2)) % 60 if m else default

    def num(name: str, default: int) -> int:
        try:
            return int(str(read_value(name)).strip())
        except (TypeError, ValueError):
            return default

    c.quiet_start = hm("quiet_start", c.quiet_start)
    c.quiet_end = hm("quiet_end", c.quiet_end)
    c.quiet_dim = max(0, min(100, num("quiet_dim", c.quiet_dim)))
    c.away_after = max(30, num("away_after", c.away_after))
    c.milestone = max(0, num("milestone_turns", c.milestone))
    return c
