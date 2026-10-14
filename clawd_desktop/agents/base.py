"""What every agent adapter shares: hook commands, safe config edits, payload mapping.

An adapter connects one coding agent to the app's event server
(http://127.0.0.1:<event port>/hook/<agent id>): `detect` says whether the
agent is on this machine, `install` / `uninstall` add or remove only our own
entries in the agent's config (merged, backed up, written atomically, never
touching a file that does not parse), and `normalize` turns one hook payload
into tracker events (events.EVENTS).

Hooks never slow an agent down or fail it:
- macOS/Linux: the shared clawd-event.sh (sh + curl, ~10 ms, always exits 0),
  called as `clawd-event.sh <agent> <event>`.
- Windows: `cmd /c "curl.exe ... || exit 0"`, which works whether the agent runs
  hook commands through cmd, PowerShell or directly, and always exits 0.
The event name also rides in the URL (`?e=<event>`) for agents whose payload
does not name it.
"""
from __future__ import annotations

import os
import shutil
import sys
from pathlib import Path
from typing import Any, Callable

from clawd_desktop import events, hooks, paths

read_json = hooks._read          # {} when missing, None when it does not parse
write_json = hooks._write        # backup + atomic replace

SCRIPT_MARK = "clawd-event"      # our hook script's file name
HTTP_MARK = "/hook/"             # every hook we write posts to .../hook/<agent>


class AgentError(Exception):
    """A config we must not touch (it does not parse, or a file that is not ours is in the way)."""


def handler_text(h: Any) -> str:
    """Everything that can hold a command in one hook entry, across agent formats."""
    if not isinstance(h, dict):
        return ""
    keys = ("command", "commandWindows", "command_windows", "bash", "powershell", "exec", "url")
    return " ".join([*(str(h.get(k) or "") for k in keys), *map(str, h.get("args") or [])])


class Agent:
    """One coding agent. Subclasses set the class attributes and the config edits."""

    id = ""
    name = ""
    signals: tuple[str, ...] = ()        # what reaches the clock: think tool subagent error attention
    tested: frozenset[str] = frozenset()  # platforms checked end to end with the real agent
    note = ""                             # one line for the settings window, "" = none

    def __init__(self, home: Path | None = None, platform: str = sys.platform,
                 port: int | None = None, script: Path | None = None,
                 which: Callable[[str], str | None] = shutil.which,
                 env: dict[str, str] | None = None) -> None:
        self._home = home
        self.platform = platform
        self._port = port
        self._script = script
        self._which = which
        self._env = os.environ if env is None else env

    # ---- where -----------------------------------------------------------------------
    @property
    def home(self) -> Path:
        return self._home if self._home is not None else Path.home()

    def env_dir(self, var: str, default: Path) -> Path:
        raw = self._env.get(var)
        return Path(raw).expanduser() if raw else default

    def config_dir(self) -> Path:
        raise NotImplementedError

    def found(self) -> bool:
        return self.config_dir().is_dir()

    def where(self) -> str:
        return str(self.config_dir())

    @property
    def beta(self) -> bool:
        return self.platform not in self.tested

    # ---- the hook command --------------------------------------------------------------
    @property
    def port(self) -> int:
        return self._port or hooks.event_port()

    def url(self, event: str = "") -> str:
        return f"http://127.0.0.1:{self.port}/hook/{self.id}" + (f"?e={event}" if event else "")

    def posix_command(self, event: str) -> str:
        return f"'{self._script or paths.HOOK_SCRIPT}' {self.id} {event}"

    def windows_command(self, event: str) -> str:
        return ('cmd /c "curl.exe -s -o NUL --max-time 0.3 --connect-timeout 0.2 '
                f'--data-binary @- {self.url(event)} || exit 0"')

    def command(self, event: str) -> str:
        return self.windows_command(event) if self.platform == "win32" else self.posix_command(event)

    def prepare(self) -> None:
        """The hook script must exist before any posix hook points at it."""
        if self.platform != "win32":
            hooks.write_script(self._script, port=self._port)

    def ours(self, h: Any) -> bool:
        text = handler_text(h)
        return f"{HTTP_MARK}{self.id}" in text or (SCRIPT_MARK in text and f" {self.id} " in text + " ")

    # ---- what subclasses implement --------------------------------------------------------
    def is_installed(self) -> bool:
        raise NotImplementedError

    def install(self) -> str:
        raise NotImplementedError

    def uninstall(self) -> str:
        raise NotImplementedError

    def normalize(self, p: Any, hint: str = "") -> list[dict]:
        raise NotImplementedError

    # ---- for the settings window -----------------------------------------------------------
    def info(self) -> dict:
        try:
            installed = self.is_installed()
            error = ""
        except AgentError as e:
            installed, error = False, str(e)
        return {"id": self.id, "name": self.name, "found": self.found(), "installed": installed,
                "where": self.where(), "beta": self.beta, "signals": list(self.signals),
                "note": self.note, "error": error}


# ---- config edits ----------------------------------------------------------------------
def load(path: Path) -> dict:
    data = read_json(path)
    if data is None:
        raise AgentError(f"{path} không đọc được (JSON lỗi), Clawd không sửa file này.")
    return data


def merge_groups(data: dict, add: dict[str, dict] | None, ours: Callable[[Any], bool],
                 matcher: str | None = None) -> dict:
    """Claude-style `hooks: {Event: [{matcher, hooks: [handler]}]}`: drop our groups, then
    (when `add` is given) append one group per event. Everything else stays as it was."""
    out = dict(data)
    table = dict(out.get("hooks") or {}) if isinstance(out.get("hooks"), dict) else {}
    for event in list(table):
        groups = table[event]
        if not isinstance(groups, list):
            continue
        kept = [g for g in groups
                if not (isinstance(g, dict) and any(ours(h) for h in g.get("hooks") or []))]
        if kept:
            table[event] = kept
        else:
            del table[event]
    for event, h in (add or {}).items():
        group = {"matcher": matcher} if matcher is not None else {}
        table.setdefault(event, []).append({**group, "hooks": [dict(h)]})
    if table:
        out["hooks"] = table
    else:
        out.pop("hooks", None)
    return out


def merge_flat(data: dict, add: dict[str, dict] | None, ours: Callable[[Any], bool]) -> dict:
    """Cursor/Copilot-style `hooks: {event: [handler]}`."""
    out = dict(data)
    table = dict(out.get("hooks") or {}) if isinstance(out.get("hooks"), dict) else {}
    for event in list(table):
        entries = table[event]
        if not isinstance(entries, list):
            continue
        kept = [h for h in entries if not ours(h)]
        if kept:
            table[event] = kept
        else:
            del table[event]
    for event, h in (add or {}).items():
        table.setdefault(event, []).append(dict(h))
    if table:
        out["hooks"] = table
    else:
        out.pop("hooks", None)
    return out


def has_ours(data: dict | None, ours: Callable[[Any], bool]) -> bool:
    table = (data or {}).get("hooks")
    if not isinstance(table, dict):
        return False
    for entries in table.values():
        for e in entries if isinstance(entries, list) else []:
            if ours(e) or (isinstance(e, dict) and any(ours(h) for h in e.get("hooks") or [])):
                return True
    return False


def edit_json(path: Path, change: Callable[[dict], dict], create: bool = True) -> str:
    """Apply `change` to a JSON config. Returns "changed" / "unchanged" / "missing"."""
    if not path.exists() and not create:
        return "missing"
    current = load(path)
    new = change(current)
    if new == current:
        return "unchanged"
    write_json(path, new)
    return "changed"


# ---- payload -> tracker events -----------------------------------------------------------
def first_str(p: dict, *keys: str) -> str:
    for k in keys:
        v = p.get(k)
        if isinstance(v, str) and v:
            return v
    return ""


def base_event(src: str, sid: str, tool: Any = "", inp: Any = None) -> dict:
    ev: dict[str, Any] = {"src": src, "sid": f"{src}:{sid[:160]}",
                          "tool": tool if isinstance(tool, str) else ""}
    if isinstance(inp, dict) and isinstance(inp.get("command"), str):
        ev["cmd"] = inp["command"][:300]
    return ev


def tool_summary(tool: str, inp: Any) -> str:
    return events._tool_summary(tool, inp)


def prepared(src: str, p: Any) -> list[dict]:
    """A payload the agent's own plugin already shaped like the tracker's events (pi, opencode)."""
    ev = events.pi_event(p)
    if ev is None:
        return []
    return [{**ev, "src": src, "sid": f"{src}:{str(p['sid'])[:160]}"}]
