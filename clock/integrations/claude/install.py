#!/usr/bin/env python3
"""Register (or remove) the clawd-home hook in Claude Code's settings.json.

Merges, never replaces: every existing hook stays as it is; only entries whose
command runs clawd-event.sh are added or removed. The file is backed up first
(settings.json.bak-clawd-<timestamp>) and rewritten atomically.

  python3 integrations/claude/install.py            # install / refresh
  python3 integrations/claude/install.py --uninstall
  python3 integrations/claude/install.py --settings /path/to/settings.json --dry-run
"""
from __future__ import annotations

import argparse
import json
import os
import shutil
import sys
import time
from pathlib import Path

HOOK = Path(__file__).resolve().parent / "clawd-event.sh"
MARK = "clawd-event.sh"
# Kept in step with daemon/clawd_events.py CLAUDE_HOOK_EVENTS (a test checks it).
EVENTS = [
    "Notification", "PermissionRequest", "PostCompact", "PostToolUse", "PostToolUseFailure",
    "PreCompact", "PreToolUse", "SessionEnd", "SessionStart", "Stop", "StopFailure",
    "SubagentStart", "SubagentStop", "UserPromptSubmit",
]


def _ours(group: dict) -> bool:
    return any(MARK in str(h.get("command", "")) for h in group.get("hooks", []) if isinstance(h, dict))


def merge(settings: dict, hook_cmd: str, install: bool = True) -> dict:
    """Return settings with our hook groups removed and (if install) re-added."""
    out = dict(settings)
    hooks = dict(out.get("hooks") or {})
    for event in list(hooks):
        groups = [g for g in hooks[event] if not (isinstance(g, dict) and _ours(g))]
        if groups:
            hooks[event] = groups
        else:
            del hooks[event]
    if install:
        for event in EVENTS:
            hooks.setdefault(event, []).append(
                {"matcher": "*", "hooks": [{"type": "command", "command": hook_cmd, "timeout": 2}]})
    if hooks:
        out["hooks"] = hooks
    else:
        out.pop("hooks", None)
    return out


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--settings", type=Path, default=Path.home() / ".claude" / "settings.json")
    ap.add_argument("--uninstall", action="store_true")
    ap.add_argument("--dry-run", action="store_true")
    a = ap.parse_args(argv)

    try:
        current = json.loads(a.settings.read_text()) if a.settings.exists() else {}
    except ValueError as e:
        print(f"{a.settings}: not valid JSON ({e}); leaving it alone", file=sys.stderr)
        return 1
    if not isinstance(current, dict):
        print(f"{a.settings}: not a JSON object; leaving it alone", file=sys.stderr)
        return 1
    HOOK.chmod(HOOK.stat().st_mode | 0o111)
    cmd = f"'{HOOK}'"
    new = merge(current, cmd, install=not a.uninstall)
    if new == current:
        print("already up to date")
        return 0
    if a.dry_run:
        print(json.dumps(new.get("hooks", {}), indent=2))
        return 0
    a.settings.parent.mkdir(parents=True, exist_ok=True)
    if a.settings.exists():
        bak = a.settings.with_name(f"{a.settings.name}.bak-clawd-{time.strftime('%y%m%d-%H%M%S')}")
        shutil.copy2(a.settings, bak)
        print(f"backup: {bak}")
    tmp = a.settings.with_name(a.settings.name + ".clawd-tmp")
    tmp.write_text(json.dumps(new, indent=2, ensure_ascii=False) + "\n")
    if a.settings.exists():
        os.chmod(tmp, a.settings.stat().st_mode & 0o777)
    tmp.replace(a.settings)
    n = sum(1 for e in new.get("hooks", {}).values() for g in e if _ours(g))
    print(f"{'removed clawd hooks from' if a.uninstall else f'{n} clawd hook entries in'} {a.settings}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
