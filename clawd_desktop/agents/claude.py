"""Claude Code: command hooks in settings.json of every configured Claude config dir.

The edit itself lives in hooks.py (older versions installed it from there, and
`Clawd hooks on|off` still does); this adapter puts it behind the common face.
"""
from __future__ import annotations

from pathlib import Path
from typing import Any

from clawd_desktop import events, hooks
from clawd_desktop.agents.base import Agent


class Claude(Agent):
    id = "claude"
    name = "Claude Code"
    signals = ("think", "tool", "subagent", "error", "attention")
    tested = frozenset({"darwin", "win32"})

    def dirs(self) -> list[Path]:
        return [f.parent for f in hooks.settings_files()]

    def config_dir(self) -> Path:
        dirs = self.dirs()
        return next((d for d in dirs if d.is_dir()), dirs[0] if dirs else Path.home() / ".claude")

    def found(self) -> bool:
        return any(d.is_dir() for d in self.dirs())

    def where(self) -> str:
        return ", ".join(str(d) for d in self.dirs() if d.is_dir()) or str(self.config_dir())

    def is_installed(self) -> bool:
        return hooks.is_installed()

    def install(self) -> str:
        hooks.apply(True, platform=self.platform)
        return "Đã nối." if hooks.is_installed() else "Không ghi được settings.json của Claude Code."

    def uninstall(self) -> str:
        hooks.apply(False, platform=self.platform)
        return "Đã ngắt."

    def normalize(self, p: Any, hint: str = "") -> list[dict]:
        ev = events.claude_event(p)
        return [ev] if ev else []
