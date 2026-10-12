"""The background work: find the clock, poll Claude usage, run the agent event
server, push everything to the clock. Runs its own asyncio loop in a thread so
the tray icon can own the main thread (macOS requires it)."""
from __future__ import annotations

import asyncio
import threading
import time

import httpx

from clawd_desktop import events, paths, usage
from clawd_desktop.pause import PauseReader
from clawd_desktop.bootwatch import BootWatch
from clawd_desktop.discovery import ClockFinder, ClockStore

TOKEN_CHECK_EVERY = 60.0


def log(msg: str) -> None:
    print(f"[{time.strftime('%H:%M:%S')}] {msg}", flush=True)


class TrackingSinks(usage.HttpSinks):
    """HttpSinks that tells the finder whether each push reached the clock."""

    def __init__(self, on_result, transport: httpx.AsyncBaseTransport | None = None) -> None:
        super().__init__(transport=transport)
        self.on_result = on_result

    async def _post(self, http: httpx.AsyncClient, url: str, body: str) -> bool:
        ok = await super()._post(http, url, body)
        self.on_result(url, ok)
        return ok


def has_claude_token() -> bool:
    """True when some configured Claude dir holds a token (does not call the API)."""
    try:
        return any(usage.read_token_for(d) for d in usage.read_config_dirs())
    except Exception:
        return False


def daemon_running(port: int | None = None, timeout: float = 1.0) -> bool:
    """Another clawd daemon (this app or the old LaunchAgent) owns the event port."""
    port = port or events.read_event_port(usage.read_config_value)
    try:
        r = httpx.get(f"http://127.0.0.1:{port}/state", timeout=timeout)
        return r.status_code == 200 and "desired" in r.text
    except (httpx.HTTPError, ValueError):
        return False


class Engine:
    def __init__(self, finder: ClockFinder | None = None, pause: PauseReader | None = None) -> None:
        self.finder = finder or ClockFinder(ClockStore(paths.CLOCK_FILE),
                                            manual=usage.read_http_targets)
        self.payload: dict | None = None      # the last usage payload sent
        self.payload_at: float | None = None
        self.token: bool | None = None        # None until checked
        self.events_on = False
        self._loop: asyncio.AbstractEventLoop | None = None
        self._stop: asyncio.Event | None = None
        self._thread: threading.Thread | None = None
        self._ready = threading.Event()
        self.pause = pause or PauseReader()
        self._paused: str | None = None

    def targets(self) -> list[str]:
        """The clocks to push to: none while the settings window holds a pause
        (a firmware update), so nothing competes with its upload."""
        reason = self.pause.reason()
        if reason != self._paused:
            log(f"Pushes paused: {reason}" if reason else "Pushes resumed")
            self._paused = reason
        return [] if reason else self.finder.targets()

    # ---- the async side ------------------------------------------------------
    def _on_payload(self, p: dict) -> None:
        self.payload, self.payload_at = p, time.time()
        if p.get("ok") is False:
            self.token = False
        elif "s" in p or "w" in p:
            self.token = True

    async def _watch_token(self, stop: asyncio.Event) -> None:
        # Only until the first usage payload: after that the payload says whether a
        # token works (it also counts accounts this check cannot see, e.g. cliproxy).
        while not stop.is_set() and self.payload is None:
            found = await asyncio.to_thread(has_claude_token)
            if self.payload is None:
                self.token = found or None   # missing locally is not proof: wait for the poll
            try:
                await asyncio.wait_for(stop.wait(), timeout=TOKEN_CHECK_EVERY)
            except asyncio.TimeoutError:
                pass

    async def run(self, stop: asyncio.Event) -> None:
        log("=== Clawd desktop (usage + agent events over WiFi) ===")
        publisher = usage.Publisher(http=TrackingSinks(self.finder.report),
                                    targets=self.targets)
        tasks = [asyncio.create_task(self.finder.run(stop)),
                 asyncio.create_task(publisher.run(stop)),
                 asyncio.create_task(self._watch_token(stop))]
        server = None
        brain = None
        pusher: events.StatePusher | None = None
        if usage.read_onoff_setting("events", default="on") == "on":
            pet = events.Pet(events.read_pet_config(usage.read_config_value),
                             store=usage.ATTENTION_DIR / "pet.json")
            brain = events.Brain(pet)
            server = events.EventServer(brain, port=events.read_event_port(usage.read_config_value))
            try:
                await server.start()
                self.events_on = True
                pusher = events.StatePusher(brain, self.targets)
                tasks.append(asyncio.create_task(pusher.run(stop)))
            except OSError as e:
                log(f"Event server not started ({e}); agent events disabled")
                server = None

        def on_payload(p: dict) -> None:
            self._on_payload(p)
            if brain is not None:
                brain.usage = p
        publisher.on_payload = on_payload

        async def on_restart(url: str, uptime: int) -> bool:
            # A restarted clock lost both the usage bars and Clawd's state.
            if pusher is not None and brain is not None:
                pusher.last_key = None
                brain.wake.set()
            return await publisher.resend(url)
        tasks.append(asyncio.create_task(BootWatch(self.targets, on_restart).run(stop)))
        try:
            await stop.wait()
        finally:
            publisher.wake.set()
            await asyncio.gather(*tasks, return_exceptions=True)
            if server is not None:
                await server.close()
            log("Stopped")

    # ---- thread control (used by the tray) -------------------------------------
    def start(self) -> None:
        def main() -> None:
            loop = asyncio.new_event_loop()
            asyncio.set_event_loop(loop)
            self._loop = loop
            self._stop = asyncio.Event()
            self._ready.set()
            try:
                loop.run_until_complete(self.run(self._stop))
            finally:
                loop.close()
        self._thread = threading.Thread(target=main, name="clawd-engine", daemon=True)
        self._thread.start()
        self._ready.wait(5)

    def stop(self, timeout: float = 8.0) -> None:
        if self._loop is not None and self._stop is not None:
            self._loop.call_soon_threadsafe(self._stop.set)
        if self._thread is not None:
            self._thread.join(timeout)

    def call(self, fn, *args) -> None:
        """Run a finder action from the tray thread (it is lock-protected)."""
        fn(*args)

    def snapshot(self) -> dict:
        d = self.finder.describe()
        d.update(payload=self.payload, payload_at=self.payload_at, token=self.token,
                 events_on=self.events_on, paused=self._paused)
        return d
