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
moment > pet away/sleep > nothing (the clock shows its usage screens).
"""
from __future__ import annotations

import asyncio
import json
import os
import re
import subprocess
import time
import unicodedata
import urllib.parse
from dataclasses import dataclass
from pathlib import Path
from typing import Callable

import httpx

EVENT_HOST = "127.0.0.1"
EVENT_PORT = 47913
STATE_PATH = "/api/state"
STATE_TTL = 60            # seconds the clock keeps a state without a resend
RESEND_EVERY = 20         # resend an unchanged live state this often
PUSH_TIMEOUT = 1.5
COALESCE = 0.3            # gather an event burst before pushing
MAX_BODY = 4 * 1024 * 1024

SESSION_TTL = 600         # a working/thinking session silent this long went away
ATTENTION_TTL = 1800      # how long "needs you" stays up without an answer
IDLE_FORGET = 3600        # drop idle sessions after this
ERROR_SHOW = 4.0
DONE_SHOW = 4.0
TEXT_MAX = 90


def log(msg: str) -> None:
    print(f"[{time.strftime('%H:%M:%S')}] {msg}", flush=True)


def ascii_text(text, limit: int = TEXT_MAX) -> str:
    """The clock's fonts are ASCII: fold accents (d-stroke too), collapse
    whitespace, cap the length."""
    text = str(text or "").replace("\u0111", "d").replace("\u0110", "D")
    text = unicodedata.normalize("NFKD", text).encode("ascii", "ignore").decode()
    text = " ".join(text.split())
    return text if len(text) <= limit else text[: limit - 3].rstrip() + "..."


@dataclass(frozen=True)
class View:
    anim: str
    text: str = ""
    layer: str = ""           # poke | agent | focus | pet


# =============================================================================
# Agent sessions
# =============================================================================
@dataclass
class Sess:
    sid: str
    src: str
    sub: bool = False          # a child agent process (e.g. `pi -p` run by a subagent)
    state: str = "idle"        # idle | thinking | working | attention | sweeping
    tool: str = ""
    msg: str = ""
    subs: int = 0              # live subagents this session started
    last: float = 0.0
    error_until: float = 0.0
    error_tool: str = ""
    done_until: float = 0.0


EVENTS = {"start", "prompt", "tool", "tool_done", "tool_error", "subagent_start",
          "subagent_stop", "attention", "attention_done", "compact", "compact_done",
          "stop", "stop_error", "abort", "end"}


class Tracker:
    """Per-session agent state, resolved into one desk animation."""

    def __init__(self, on_turn: Callable[[float], None] | None = None) -> None:
        self.sessions: dict[str, Sess] = {}
        self.on_turn = on_turn

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
                                          sub=bool(ev.get("sub")))
        s.last = now
        tool = ascii_text(ev.get("tool") or "", 24)
        if kind == "prompt":
            s.state, s.msg = "thinking", ""
        elif kind == "tool":
            s.state, s.tool, s.msg = "working", tool or s.tool, ""
        elif kind == "tool_done":
            s.state, s.msg = "working", ""
        elif kind == "attention_done":
            if s.state == "attention":
                s.state, s.msg = "thinking", ""
        elif kind == "tool_error":
            if s.state != "attention":
                s.state = "working"
            s.error_until, s.error_tool = now + ERROR_SHOW, tool or s.tool
        elif kind == "subagent_start":
            s.subs = min(s.subs + 1, 8)
        elif kind == "subagent_stop":
            s.subs = max(s.subs - 1, 0)
        elif kind == "attention":
            s.state, s.msg = "attention", ascii_text(ev.get("msg") or "")
        elif kind == "compact":
            s.state = "sweeping"
        elif kind == "compact_done":
            s.state = "thinking"
        elif kind == "stop":
            s.state, s.subs, s.msg = "idle", 0, ""
            s.done_until = now + DONE_SHOW
            if not s.sub and self.on_turn is not None:
                self.on_turn(now)
        elif kind == "abort":                   # the user cancelled: back to idle, quietly
            s.state, s.subs, s.msg = "idle", 0, ""
        elif kind == "stop_error":
            s.state, s.subs, s.msg = "idle", 0, ""
            s.error_until, s.error_tool = now + ERROR_SHOW, ""
        return True

    def expire(self, now: float) -> None:
        for sid, s in list(self.sessions.items()):
            age = now - s.last
            if s.state == "attention" and age > ATTENTION_TTL:
                s.state, s.msg = "idle", ""
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
            return View("notification", latest(attn).msg or "Needs you", "agent")
        errs = [s for s in main if s.error_until > now]
        if errs:
            t = latest(errs).error_tool
            return View("error", f"{t} failed" if t else "Error", "agent")
        if any(s.state == "sweeping" for s in main):
            return View("sweeping", "Compacting", "agent")
        subs = sum(s.subs for s in main) + sum(
            1 for s in live if s.sub and s.state in ("thinking", "working"))
        if subs >= 2:
            return View("juggling", f"{subs} subagents", "agent")
        if subs == 1:
            return View("headphones", "1 subagent", "agent")
        active = [s for s in main if s.state in ("thinking", "working")]
        if len(active) >= 3:
            return View("building", f"{len(active)} sessions", "agent")
        if len(active) == 2:
            return View("headphones", "2 sessions", "agent")
        working = [s for s in active if s.state == "working"]
        if working:
            return View("typing", latest(working).tool, "agent")
        if active:
            return View("thinking", "", "agent")
        if any(s.done_until > now for s in main):
            return View("happy", "Done!", "agent")
        return None

    def snapshot(self, now: float) -> list[dict]:
        return [{"sid": s.sid, "src": s.src, "sub": s.sub, "state": s.state,
                 "tool": s.tool, "subs": s.subs, "age": round(now - s.last)}
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
    tool = p.get("tool_name") if isinstance(p.get("tool_name"), str) else ""
    ev = {"src": "claude", "sid": f"claude:{sid}", "tool": tool}
    if name in _CLAUDE_SIMPLE:
        return {**ev, "ev": _CLAUDE_SIMPLE[name]}
    if name == "PreToolUse":
        if tool == "AskUserQuestion":
            q = ""
            inp = p.get("tool_input") if isinstance(p.get("tool_input"), dict) else {}
            qs = inp.get("questions")
            if isinstance(qs, list) and qs and isinstance(qs[0], dict):
                q = str(qs[0].get("question") or "")
            return {**ev, "ev": "attention", "msg": q or "Claude has a question"}
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
            "sub": bool(p.get("sub"))}


# =============================================================================
# Pet
# =============================================================================
@dataclass
class Moment:
    anim: str
    text: str
    secs: float
    start: float = 0.0


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
            return View(m.anim, m.text, layer)
        queue.pop(0)
    return None


class Pet:
    def __init__(self, cfg: PetConfig | None = None, store: Path | None = None,
                 localtime: Callable[[float], time.struct_time] = time.localtime) -> None:
        self.cfg = cfg or PetConfig()
        self.store = store
        self.localtime = localtime
        self.moments: list[Moment] = []
        self.away = False
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
                self.away = True
                self.moments = [Moment("yawn", "", 4)]
            elif not away and self.away:
                self.away = False
                self.moments = [Moment("wake", "", 2)]
                today, minute = self._today(now), self._minute(now)
                if 5 * 60 <= minute < 11 * 60 and self.greeted != today:
                    self.greeted = today
                    self._save()
                    self.push("happy", "Good morning!", 3)
                else:
                    self.push("happy", "Welcome back!", 3)
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

    def continuous(self, now: float) -> View | None:
        return View("sleeping", "", "pet") if self.away else None

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
        self.moments = [Moment(ascii_text(anim, 15), ascii_text(text), s, now)]

    def moment(self, now: float) -> View | None:
        if self.focus_until and now >= self.focus_until:
            self.focus_until = 0.0
            self.moments = [Moment("happy", "Focus done!", 6, now)]
        return _play(self.moments, now, "poke")

    def focus_left(self, now: float) -> int:
        return int(self.focus_until - now + 0.999) if self.focus_until > now else 0


# =============================================================================
# Resolution + push
# =============================================================================
FOCUS_KEEPS_TEXT = {"notification", "error"}


class Brain:
    """Owns every layer and decides the one state the clock should show."""

    def __init__(self, pet: Pet | None = None) -> None:
        self.pet = pet or Pet()
        self.tracker = Tracker(on_turn=self.pet.on_turn)
        self.pokes = Pokes()
        self.wake = asyncio.Event()
        self.usage: dict | None = None          # latest usage payload (Publisher)

    def desired(self, now: float) -> dict:
        """{'a', 'txt', 'cd', 'dim', 'focus_until'} for this instant."""
        v = self.pokes.moment(now) or self.tracker.view(now)
        focus = self.pokes.focus_left(now)
        cd = 0
        if focus and (v is None or (v.layer == "agent" and v.anim not in FOCUS_KEEPS_TEXT)):
            v = View(v.anim if v else "reading", "Focus", v.layer if v else "focus")
            cd = focus
        if v is None:
            v = self.pet.moment(now) or self.pet.continuous(now)
        return {"a": v.anim if v else "", "txt": v.text if v else "", "cd": cd,
                "dim": self.pet.dim(now), "focus_until": int(self.pokes.focus_until) if cd else 0}


def state_url(target: str) -> str:
    """An http_targets push URL (…/api/usage) -> that clock's /api/state URL."""
    parts = urllib.parse.urlsplit(target)
    return urllib.parse.urlunsplit((parts.scheme, parts.netloc, STATE_PATH, "", ""))


def wire(d: dict) -> dict:
    out = {"a": d["a"], "ttl": STATE_TTL}
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

    @staticmethod
    def key(d: dict) -> tuple:
        return (d["a"], d["txt"], d["dim"], d["focus_until"])

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
                await self.step()
            except Exception as e:      # keep the loop alive
                log(f"State push cycle failed: {e!r}")
            try:
                await asyncio.wait_for(self.brain.wake.wait(), timeout=1.0)
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
