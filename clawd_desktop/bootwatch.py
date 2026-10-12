"""Notice when a clock restarts, so it gets its data back at once.

After a restart the clock shows its "no data" splash until the next usage push,
which can be up to a poll interval (60 s) away. BootWatch reads the clock's
`GET /api/heap` (no login, has `uptime` in seconds) every PROBE_EVERY seconds
and estimates when the clock booted (now - uptime). When that estimate moves,
the clock restarted: `on_restart(usage_url, uptime)` runs right away. When it
returns False (the clock was still too slow to take the data), the next probe
tries again while the clock is in its first FRESH_BOOT seconds.

A clock seen for the first time (app start, or a new address after DHCP)
counts as restarted only when it booted less than FRESH_BOOT seconds ago.
"""
from __future__ import annotations

import asyncio
import time
import urllib.parse
from typing import Awaitable, Callable

import httpx

PROBE_EVERY = 5.0
PROBE_TIMEOUT = 12.0     # the clock answers HTTP slowly for a few minutes after boot
BOOT_JITTER = 30.0       # boot-time estimate drift (request latency) that is not a restart
FRESH_BOOT = 120.0
HEAP_PATH = "/api/heap"


def log(msg: str) -> None:
    print(f"[{time.strftime('%H:%M:%S')}] {msg}", flush=True)


def heap_url(target: str) -> str:
    """An http_targets push URL (…/api/usage) -> that clock's /api/heap URL."""
    p = urllib.parse.urlsplit(target)
    return urllib.parse.urlunsplit((p.scheme, p.netloc, HEAP_PATH, "", ""))


class BootWatch:
    def __init__(self, targets: Callable[[], list[str]],
                 on_restart: Callable[[str, int], Awaitable[bool]],
                 transport: httpx.AsyncBaseTransport | None = None,
                 clock: Callable[[], float] = time.monotonic) -> None:
        self.targets, self.on_restart = targets, on_restart
        self.transport, self.clock = transport, clock
        self._booted: dict[str, float] = {}   # usage url -> estimated boot time (monotonic)
        self._retry: set[str] = set()          # restarted, but the data did not get through

    async def _uptime(self, http: httpx.AsyncClient, url: str) -> int | None:
        try:
            r = await http.get(heap_url(url))
            if r.status_code != 200:
                return None
            up = r.json().get("uptime")
        except (httpx.HTTPError, ValueError, AttributeError):
            return None
        return int(up) if isinstance(up, (int, float)) and up >= 0 else None

    async def step(self) -> list[str]:
        """Probe every clock once; returns the usage URLs that restarted."""
        urls = self.targets()
        for gone in set(self._booted) - set(urls):
            del self._booted[gone]
            self._retry.discard(gone)
        restarted: list[str] = []
        if not urls:
            return restarted
        async with httpx.AsyncClient(timeout=PROBE_TIMEOUT, transport=self.transport) as http:
            for url in urls:
                up = await self._uptime(http, url)
                if up is None:
                    continue          # not answering: keep the last estimate
                booted = self.clock() - up
                prev = self._booted.get(url)
                self._booted[url] = booted
                if prev is None:
                    fresh = up < FRESH_BOOT
                else:
                    fresh = booted - prev > BOOT_JITTER
                retry = url in self._retry and up < FRESH_BOOT
                self._retry.discard(url)
                if fresh or retry:
                    if fresh:
                        log(f"Clock {urllib.parse.urlsplit(url).netloc} restarted "
                            f"(up {up} s): sending its data again")
                    restarted.append(url)
                    if not await self.on_restart(url, up):
                        self._retry.add(url)
        return restarted

    async def run(self, stop: asyncio.Event) -> None:
        while not stop.is_set():
            try:
                await self.step()
            except Exception as e:     # keep watching; the next tick retries
                log(f"Restart watch failed: {e!r}")
            try:
                await asyncio.wait_for(stop.wait(), timeout=PROBE_EVERY)
            except asyncio.TimeoutError:
                pass
