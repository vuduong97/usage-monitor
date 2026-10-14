"""Codex CLI usage for the clock, read from Codex's own session logs. No network.

Codex writes a `token_count` event into its session file
(~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl, CODEX_HOME moves it) after each
model reply, carrying the account's rate limits as the server reported them:

  {"type": "event_msg", "payload": {"type": "token_count", "rate_limits": {
     "limit_id": "codex", "primary": {"used_percent": 1.0, "window_minutes": 300,
     "resets_at": 1783032649}, "secondary": {...10080...}, "plan_type": "plus",
     "rate_limit_reached_type": null}}}

So the numbers are as fresh as the user's last Codex turn: a window whose reset
time has passed reads as 0 %. Plans without such limits (usage-based enterprise
plans) carry null windows: no data.
"""
from __future__ import annotations

import json
import os
import time
from pathlib import Path

TAIL = 256 * 1024            # read this much from the end of a session file
FILES = 8                    # look at this many of the newest session files
DAY = 24 * 60

_cache: dict[tuple, dict | None] = {}


def sessions_dir(home: Path | None = None, env: dict | None = None) -> Path:
    env = os.environ if env is None else env
    raw = env.get("CODEX_HOME")
    base = Path(raw).expanduser() if raw else (home or Path.home()) / ".codex"
    return base / "sessions"


def newest_files(root: Path, limit: int = FILES) -> list[Path]:
    """Newest session files, walking YYYY/MM/DD folders newest first (no full scan)."""
    out: list[Path] = []

    def subdirs(p: Path) -> list[Path]:
        try:
            return sorted((d for d in p.iterdir() if d.is_dir()), key=lambda d: d.name, reverse=True)
        except OSError:
            return []
    for y in subdirs(root):
        for m in subdirs(y):
            for d in subdirs(m):
                try:
                    files = sorted(d.glob("*.jsonl"), key=lambda f: f.stat().st_mtime, reverse=True)
                except OSError:
                    continue
                out.extend(files)
                if len(out) >= limit:
                    return out[:limit]
    return out


def _tail_lines(path: Path) -> list[str]:
    try:
        with path.open("rb") as f:
            f.seek(0, os.SEEK_END)
            size = f.tell()
            f.seek(max(0, size - TAIL))
            data = f.read()
    except OSError:
        return []
    lines = data.decode("utf-8", "replace").splitlines()
    return lines[1:] if size > TAIL else lines          # the first may be cut


def _limits_in(path: Path) -> tuple[dict, float] | None:
    """The last rate_limits with a window in one file, and when they were written."""
    for line in reversed(_tail_lines(path)):
        if '"rate_limits"' not in line:
            continue
        try:
            row = json.loads(line)
        except ValueError:
            continue
        p = row.get("payload") if isinstance(row, dict) else None
        rl = p.get("rate_limits") if isinstance(p, dict) else None
        if not isinstance(rl, dict) or rl.get("limit_id") not in (None, "codex"):
            continue
        if isinstance(rl.get("primary"), dict) or isinstance(rl.get("secondary"), dict):
            return rl, _when(row.get("timestamp"), path)
    return None


def _when(ts, path: Path) -> float:
    if isinstance(ts, str):
        try:
            import datetime
            return datetime.datetime.fromisoformat(ts.replace("Z", "+00:00")).timestamp()
        except ValueError:
            pass
    try:
        return path.stat().st_mtime
    except OSError:
        return time.time()


def _window(w, written: float, now: float) -> tuple[int, int | None, int] | None:
    """(percent, minutes to reset or None, window minutes)."""
    if not isinstance(w, dict):
        return None
    try:
        pct = float(w.get("used_percent") or 0)
        span = int(w.get("window_minutes") or 0)
    except (TypeError, ValueError):
        return None
    reset = w.get("resets_at")
    if not isinstance(reset, (int, float)) and isinstance(w.get("resets_in_seconds"), (int, float)):
        reset = written + w["resets_in_seconds"]          # older Codex versions
    left: int | None = None
    if isinstance(reset, (int, float)):
        if reset <= now:
            return 0, None, span                          # that window has started over
        left = int(round((reset - now) / 60))
    return max(0, min(100, int(round(pct)))), left, span


def payload_from(rl: dict, written: float, now: float | None = None) -> dict | None:
    """Map Codex rate limits to the clock's usage fields (s sr w wr st acct ok src)."""
    now = time.time() if now is None else now
    wins = [w for w in (_window(rl.get("primary"), written, now), _window(rl.get("secondary"), written, now)) if w]
    if not wins:
        return None
    short = [w for w in wins if w[2] and w[2] < DAY]
    long_ = [w for w in wins if not w[2] or w[2] >= DAY]
    out: dict = {"s": 0, "w": 0, "acct": "pro", "ok": True, "src": "codex"}
    if short:
        out["s"] = short[0][0]
        if short[0][1] is not None:
            out["sr"] = short[0][1]
    if long_:
        out["w"] = long_[0][0]
        if long_[0][1] is not None:
            out["wr"] = long_[0][1]
    out["st"] = "rejected" if rl.get("rate_limit_reached_type") else (
        "allowed_warning" if max(out["s"], out["w"]) >= 90 else "allowed")
    return out


def read(home: Path | None = None, env: dict | None = None, now: float | None = None) -> dict | None:
    """The newest Codex usage on this machine, or None (no Codex, no limits seen)."""
    for f in newest_files(sessions_dir(home, env)):
        try:
            st = f.stat()
        except OSError:
            continue
        key = (str(f), st.st_mtime_ns, st.st_size)
        if key not in _cache:
            if len(_cache) > 64:
                _cache.clear()
            _cache[key] = (lambda r: {"rl": r[0], "at": r[1]} if r else None)(_limits_in(f))
        hit = _cache[key]
        if hit is not None:
            return payload_from(hit["rl"], hit["at"], now)
    return None


def available(home: Path | None = None, env: dict | None = None) -> bool:
    """Codex has written session files here (offered in the wizard's usage step)."""
    return bool(newest_files(sessions_dir(home, env), 1))
