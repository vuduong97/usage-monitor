"""Register (or remove) the Claude Code hooks that let Clawd react to agents.

Merges, never replaces: every existing hook stays; only our entries (a command
running clawd-event, or curl posting to /hook/claude) are added or removed. The
settings file is backed up first and rewritten atomically. Never touches a file
that is not valid JSON.

The hook must never slow Claude Code down or fail it:
- macOS/Linux: a tiny sh + curl script (~10 ms, always exits 0), copied into the
  config dir so moving or updating the app cannot break it.
- Windows: curl.exe (built into Windows 10+) in exec form, run async so a stopped
  app shows no hook error and never blocks a tool call.
"""
from __future__ import annotations

import json
import os
import shutil
import stat
import sys
import time
from pathlib import Path

from clawd_desktop import events, paths, usage

EVENTS = list(events.CLAUDE_HOOK_EVENTS)
MARKS = ("clawd-event", "/hook/claude")

HOOK_SH = """#!/bin/sh
# clawd-event.sh: Claude Code hook. Forwards the hook payload (stdin JSON) to the
# Clawd desktop app on 127.0.0.1, which turns it into what the clock shows.
# Never blocks or fails the agent: app not running -> curl gives up at once
# (refused) or after 0.3 s (stuck), and the script always exits 0 silently.
port=${CLAWD_EVENT_PORT:-%(port)d}
curl -s -o /dev/null --max-time 0.3 --connect-timeout 0.2 \\
  -H 'Content-Type: application/json' --data-binary @- \\
  "http://127.0.0.1:${port}/hook/claude" >/dev/null 2>&1
exit 0
"""


def event_port() -> int:
    return events.read_event_port(usage.read_config_value)


def settings_files() -> list[Path]:
    """settings.json of every Claude config dir the usage poller reads."""
    out: list[Path] = []
    for d in usage.read_config_dirs():
        p = Path(d) / "settings.json"
        if p not in out:
            out.append(p)
    return out


def handler(platform: str = sys.platform, port: int | None = None,
            script: Path | None = None) -> dict:
    port = port or event_port()
    if platform == "win32":
        return {"type": "command", "command": "curl.exe",
                "args": ["-s", "-o", "NUL", "--max-time", "0.3", "--connect-timeout", "0.2",
                         "-H", "Content-Type: application/json", "--data-binary", "@-",
                         f"http://127.0.0.1:{port}/hook/claude"],
                "async": True, "timeout": 5}
    script = script or paths.HOOK_SCRIPT
    return {"type": "command", "command": f"'{script}'", "timeout": 2}


def write_script(path: Path | None = None, port: int | None = None) -> Path:
    path = path or paths.HOOK_SCRIPT
    path.parent.mkdir(parents=True, exist_ok=True)
    text = HOOK_SH % {"port": port or event_port()}
    if not path.exists() or path.read_text() != text:
        path.write_text(text)
    path.chmod(path.stat().st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)
    return path


def _ours(group: object) -> bool:
    if not isinstance(group, dict):
        return False
    for h in group.get("hooks", []):
        if not isinstance(h, dict):
            continue
        text = " ".join([str(h.get("command", "")), *map(str, h.get("args") or [])])
        if any(m in text for m in MARKS):
            return True
    return False


def merge(settings: dict, hook: dict | None) -> dict:
    """Settings with our hook groups removed and, when `hook` is given, re-added."""
    out = dict(settings)
    hooks = dict(out.get("hooks") or {})
    for event in list(hooks):
        groups = [g for g in hooks[event] if not _ours(g)]
        if groups:
            hooks[event] = groups
        else:
            del hooks[event]
    if hook is not None:
        for event in EVENTS:
            hooks.setdefault(event, []).append({"matcher": "*", "hooks": [dict(hook)]})
    if hooks:
        out["hooks"] = hooks
    else:
        out.pop("hooks", None)
    return out


def _read(path: Path) -> dict | None:
    """The settings object, {} when the file is missing, None when unusable."""
    if not path.exists():
        return {}
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return None
    return data if isinstance(data, dict) else None


def _write(path: Path, data: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    if path.exists():
        bak = path.with_name(f"{path.name}.bak-clawd-{time.strftime('%y%m%d-%H%M%S')}")
        shutil.copy2(path, bak)
    tmp = path.with_name(path.name + ".clawd-tmp")
    tmp.write_text(json.dumps(data, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    if path.exists():
        os.chmod(tmp, path.stat().st_mode & 0o777)
    tmp.replace(path)


def is_installed(files: list[Path] | None = None) -> bool:
    for f in files if files is not None else settings_files():
        data = _read(f)
        if data and any(_ours(g) for e in (data.get("hooks") or {}).values() for g in e):
            return True
    return False


def apply(enable: bool, files: list[Path] | None = None, platform: str = sys.platform,
          hook: dict | None = None) -> list[str]:
    """Install (enable) or remove our hooks in every settings file. Returns messages."""
    msgs: list[str] = []
    if enable and hook is None:
        if platform != "win32":
            write_script()
        hook = handler(platform)
    for f in files if files is not None else settings_files():
        if not enable and not f.exists():
            continue
        if enable and not f.parent.exists():
            continue        # that Claude config dir does not exist: Claude Code not set up there
        current = _read(f)
        if current is None:
            msgs.append(f"{f}: not valid JSON, left alone")
            continue
        new = merge(current, hook if enable else None)
        if new == current:
            msgs.append(f"{f}: already up to date")
            continue
        _write(f, new)
        msgs.append(f"{f}: {'hooks installed' if enable else 'hooks removed'}")
    for m in msgs:
        print(f"[{time.strftime('%H:%M:%S')}] {m}", flush=True)
    return msgs
