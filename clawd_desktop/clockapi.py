"""The clock's settings API, for the settings window.

A small synchronous client for the clawd firmware's web API: read and save the
settings (`/api/config`, a POST carries only what changed), scan WiFi, reboot,
factory reset, backup/restore, and push a firmware image (`/update`, the same
upload tools/flash.sh does).

The web password is HTTP digest auth. One `httpx.DigestAuth` lives as long as
the client, so after the first challenge every request (the 0.5 MB firmware
upload too) is signed up front instead of being sent once to collect a 401.
pywebview calls the bridge from worker threads, so every request holds a lock.
"""
from __future__ import annotations

import threading
import time
from typing import Any, Callable

import httpx

TIMEOUT = 20.0          # the clock answers slowly for a few minutes after a boot
UPLOAD_TIMEOUT = 180.0
RETRIES = 2             # extra tries for a GET that timed out or could not connect
RETRY_GAP = 1.5
OTA_MAX = 524_288       # clawd -> clawd OTA budget (firmware/docs/FIRMWARE.md in the firmware repo)


class ClockError(Exception):
    """The clock did not answer, or answered with an error."""


class AuthNeeded(ClockError):
    """The clock has a web password and ours is missing or wrong."""


def config_diff(old: Any, new: Any) -> dict:
    """What to POST to /api/config to turn `old` into `new`.

    Nested objects are compared key by key and only changed keys are sent.
    Lists go whole: the firmware treats a present `wifi` list as the complete
    list (order = priority, a missing row = deletion). Keys only in `old` are
    left alone; the firmware applies only the keys that are present.
    """
    out: dict = {}
    if not isinstance(new, dict):
        return out
    old = old if isinstance(old, dict) else {}
    for k, v in new.items():
        if k not in old:
            out[k] = v
        elif isinstance(v, dict) and isinstance(old[k], dict):
            sub = config_diff(old[k], v)
            if sub:
                out[k] = sub
        elif v != old[k]:
            out[k] = v
    return out


class ClockApi:
    def __init__(self, base: str, auth: tuple[str, str] | None = None,
                 transport: httpx.BaseTransport | None = None,
                 sleep: Callable[[float], None] = time.sleep) -> None:
        self.base = base.rstrip("/")
        self._lock = threading.Lock()
        self._sleep = sleep
        self._auth = httpx.DigestAuth(*auth) if auth else None
        self._http = httpx.Client(timeout=TIMEOUT, transport=transport)

    def close(self) -> None:
        self._http.close()

    def set_auth(self, auth: tuple[str, str] | None) -> None:
        with self._lock:
            self._auth = httpx.DigestAuth(*auth) if auth else None

    # ---- plumbing --------------------------------------------------------------
    def _send(self, method: str, path: str, *, retries: int = 0, timeout: float = TIMEOUT,
              **kw) -> httpx.Response:
        last: Exception | None = None
        for attempt in range(retries + 1):
            if attempt:
                self._sleep(RETRY_GAP)
            try:
                with self._lock:
                    r = self._http.request(method, self.base + path, auth=self._auth or httpx.USE_CLIENT_DEFAULT,
                                           timeout=timeout, **kw)
            except (httpx.ConnectError, httpx.TimeoutException) as e:
                last = e
                continue
            except httpx.HTTPError as e:
                raise ClockError(f"{type(e).__name__}: {e}") from e
            if r.status_code == 401:
                raise AuthNeeded("the clock wants its web password")
            if r.status_code >= 400:
                raise ClockError(f"HTTP {r.status_code}: {r.text[:200]}")
            return r
        raise ClockError(f"no answer from {self.base} ({type(last).__name__})")

    def _json(self, method: str, path: str, **kw) -> Any:
        r = self._send(method, path, **kw)
        try:
            return r.json()
        except ValueError as e:
            raise ClockError(f"not JSON from {path}") from e

    # ---- endpoints ---------------------------------------------------------------
    def heap(self) -> dict:
        """`GET /api/heap`: no login; uptime, version, fwSeq, layouts, packs."""
        d = self._json("GET", "/api/heap", retries=RETRIES)
        if not isinstance(d, dict):
            raise ClockError("odd /api/heap answer")
        return d

    def config(self) -> dict:
        d = self._json("GET", "/api/config", retries=RETRIES)
        if not isinstance(d, dict):
            raise ClockError("odd /api/config answer")
        return d

    def save(self, changes: dict) -> dict:
        """POST only `changes`; {"ok": true, "reboot": <WiFi changed>}."""
        if not changes:
            return {"ok": True, "reboot": False, "unchanged": True}
        d = self._json("POST", "/api/config", json=changes)
        if not isinstance(d, dict) or not d.get("ok"):
            raise ClockError(f"save refused: {d!r}")
        return d

    def scan(self) -> list[dict]:
        """Nearby WiFi networks [{ssid, rssi, enc}]; the clock blocks ~3 s scanning."""
        d = self._json("GET", "/api/scan", retries=1, timeout=30.0)
        return d if isinstance(d, list) else []

    def reboot(self) -> None:
        self._send("POST", "/api/reboot")

    def factory_reset(self) -> None:
        self._send("POST", "/api/factory")

    def export(self) -> dict:
        """The saved config.json, WiFi passwords included (it is a backup)."""
        d = self._json("GET", "/api/export", retries=RETRIES)
        if not isinstance(d, dict):
            raise ClockError("odd backup")
        return d

    def restore(self, data: dict) -> None:
        """Apply a backup; the clock reboots afterwards."""
        self._send("POST", "/api/import", json=data)

    def upload_firmware(self, image: bytes) -> None:
        """POST an app image to /update; the clock reboots ~1 s after the 200."""
        if not image or len(image) > OTA_MAX:
            raise ClockError(f"firmware size {len(image)} B is outside 1..{OTA_MAX}")
        if self._auth is not None:
            self.config()          # collect the digest challenge with a small request first
        r = self._send("POST", "/update", timeout=UPLOAD_TIMEOUT,
                       files={"firmware": ("firmware.bin", image, "application/octet-stream")})
        if r.text.strip() != "OK":
            raise ClockError(f"update refused: {r.text[:200]}")

    def wait_back(self, booted_before: float, deadline: float = 150.0,
                  clock: Callable[[], float] = time.monotonic,
                  tick: Callable[[], None] | None = None) -> dict:
        """Poll /api/heap until the clock answers with an uptime that started after
        `booted_before` (a monotonic time); returns that heap. `tick` runs every
        poll (the caller renews its push pause there)."""
        end = clock() + deadline
        while clock() < end:
            if tick:
                tick()
            try:
                h = self._json("GET", "/api/heap")
            except ClockError:
                h = None
            up = h.get("uptime") if isinstance(h, dict) else None
            if isinstance(up, (int, float)) and not isinstance(up, bool):
                if clock() - up > booted_before:
                    return h
            self._sleep(2.0)
        raise ClockError("the clock did not come back after the update")
