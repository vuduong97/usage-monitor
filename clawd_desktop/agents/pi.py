"""pi: the clawd-home.ts extension in pi's extensions directory.

The extension shapes its events like the tracker's own (integrations/pi), so
`normalize` only validates them.
"""
from __future__ import annotations

from pathlib import Path
from typing import Any

from clawd_desktop.agents.base import prepared
from clawd_desktop.agents.plugin import PluginAgent


class Pi(PluginAgent):
    id = "pi"
    name = "pi"
    signals = ("think", "tool", "subagent", "error", "attention")
    tested = frozenset({"darwin"})
    mark = "pi extension: report agent lifecycle to the clawd-home daemon"
    file_name = "clawd-home.ts"
    bundled = "pi/clawd-home.ts"
    note = "Khởi động lại pi (hoặc gõ /reload) sau khi nối."

    def config_dir(self) -> Path:
        return self.env_dir("PI_CODING_AGENT_DIR", self.home / ".pi" / "agent")

    def found(self) -> bool:
        return self.config_dir().is_dir() or self._which("pi") is not None

    def plugin_dir(self) -> Path:
        return self.config_dir() / "extensions"

    def normalize(self, p: Any, hint: str = "") -> list[dict]:
        return prepared("pi", p)
