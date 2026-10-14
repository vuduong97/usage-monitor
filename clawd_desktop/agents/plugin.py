"""Agents that load a plugin file from a directory (pi extensions, opencode plugins).

The app carries the file (paths.integration) and copies it in, so it works on a
machine that never saw this repo. A copy older than the app's is replaced at the
next install or app start (`refresh`). A symlink there is a developer's link to
the repo (integrations/pi/install.sh): it counts as installed and is left alone.
A file of the same name that is not ours is never overwritten.
"""
from __future__ import annotations

from pathlib import Path

from clawd_desktop.agents.base import Agent, AgentError


class PluginAgent(Agent):
    mark = ""            # a line only our plugin has
    file_name = ""
    bundled = ""         # paths.integration() name of the file the app carries

    def plugin_dir(self) -> Path:
        raise NotImplementedError

    def target(self) -> Path:
        return self.plugin_dir() / self.file_name

    def source(self) -> Path:
        from clawd_desktop import paths
        return paths.integration(self.bundled)

    def _text(self, p: Path) -> str | None:
        try:
            return p.read_text(encoding="utf-8")
        except (OSError, UnicodeDecodeError):
            return None

    def is_installed(self) -> bool:
        t = self.target()
        text = self._text(t) if t.exists() else None
        return bool(text and self.mark in text)

    def needs_update(self) -> bool:
        t = self.target()
        if t.is_symlink() or not self.is_installed():
            return False
        return self._text(t) != self._text(self.source())

    def install(self) -> str:
        t, src = self.target(), self.source()
        body = self._text(src)
        if body is None:
            raise AgentError(f"Bản cài thiếu file {self.bundled}.")
        if t.is_symlink():
            if self.is_installed():
                return "Đã nối (liên kết tới mã nguồn)."
            raise AgentError(f"{t} là một liên kết khác, Clawd không ghi đè.")
        if t.exists():
            current = self._text(t)
            if current is None or self.mark not in current:
                raise AgentError(f"{t} đã có và không phải của Clawd, Clawd không ghi đè.")
            if current == body:
                return "Đã nối."
        t.parent.mkdir(parents=True, exist_ok=True)
        tmp = t.with_name(t.name + ".clawd-tmp")
        tmp.write_text(body, encoding="utf-8")
        tmp.replace(t)
        return "Đã nối."

    def uninstall(self) -> str:
        t = self.target()
        if t.is_symlink() or self.is_installed():
            t.unlink(missing_ok=True)
            return "Đã ngắt."
        return "Chưa nối."

    def refresh(self) -> bool:
        """Replace an older copy with the app's. True when it did."""
        if not self.needs_update():
            return False
        try:
            self.install()
        except (AgentError, OSError):
            return False
        return True
