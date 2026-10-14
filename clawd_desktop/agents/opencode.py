"""opencode: a plugin in opencode's global plugin directory (~/.config/opencode/plugins).

opencode loads every .js/.ts file there at startup (https://opencode.ai/docs/plugins).
The plugin (integrations/opencode/clawd.js) shapes its events like the tracker's
own, so `normalize` only validates them.
"""
from __future__ import annotations

from pathlib import Path
from typing import Any

from clawd_desktop.agents.base import prepared
from clawd_desktop.agents.plugin import PluginAgent


class OpenCode(PluginAgent):
    id = "opencode"
    name = "opencode"
    signals = ("think", "tool", "subagent", "error", "attention")
    tested: frozenset[str] = frozenset()
    mark = "opencode plugin: report agent lifecycle to the Clawd desktop app"
    file_name = "clawd.js"
    bundled = "opencode/clawd.js"
    note = "Khởi động lại opencode sau khi nối."

    def config_dir(self) -> Path:
        xdg = self._env.get("XDG_CONFIG_HOME")
        return (Path(xdg).expanduser() if xdg else self.home / ".config") / "opencode"

    def found(self) -> bool:
        return self.config_dir().is_dir() or self._which("opencode") is not None

    def plugin_dir(self) -> Path:
        return self.config_dir() / "plugins"

    def normalize(self, p: Any, hint: str = "") -> list[dict]:
        return prepared("opencode", p)
