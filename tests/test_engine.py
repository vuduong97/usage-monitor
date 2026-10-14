"""Engine wiring: usage pushes feed the finder, payloads update the status."""
import asyncio
import json

import httpx

from clawd_desktop import engine as eng, usage
from clawd_desktop.discovery import Clock, ClockFinder, ClockStore


class NoBrowser:
    changed = 0

    def start(self):
        return False

    def stop(self):
        pass

    def names(self):
        return []

    def resolve(self, name):
        return None


def test_tracking_sinks_report_each_push():
    results = []
    seen = []

    def handler(req):
        seen.append(json.loads(req.content))
        return httpx.Response(200 if req.url.host == "ok" else 500)
    sinks = eng.TrackingSinks(lambda url, ok: results.append((url, ok)),
                              transport=httpx.MockTransport(handler))
    ok = asyncio.run(sinks.send(["http://ok/api/usage", "http://bad/api/usage"], {"s": 1}))
    assert ok and sorted(results) == [("http://bad/api/usage", False), ("http://ok/api/usage", True)]
    assert seen == [{"s": 1}, {"s": 1}]


def test_engine_pushes_usage_to_the_found_clock(tmp_path, monkeypatch):
    store = ClockStore(tmp_path / "clock.json")
    store.save({"host": "10.0.0.7", "name": "clawd"})
    finder = ClockFinder(store, browser=NoBrowser())
    e = eng.Engine(finder)
    monkeypatch.setattr(usage, "read_onoff_setting", lambda name, default="off": "off" if name == "events" else default)
    monkeypatch.setattr(eng, "has_claude_token", lambda: True)

    async def fake_poll():
        return {"s": 42, "w": 17, "ok": True}, False
    monkeypatch.setattr(usage, "poll_active", fake_poll)
    pushed = []

    async def fake_send(self, targets, payload):
        pushed.append((targets, payload))
        for t in targets:
            self.on_result(t, True)
        return True
    monkeypatch.setattr(eng.TrackingSinks, "send", fake_send)

    async def go():
        stop = asyncio.Event()
        task = asyncio.create_task(e.run(stop))
        for _ in range(50):
            await asyncio.sleep(0.02)
            if pushed:
                break
        stop.set()
        await task
    asyncio.run(go())
    assert pushed and pushed[0][0] == ["http://10.0.0.7/api/usage"]
    snap = e.snapshot()
    assert snap["payload"]["s"] == 42 and snap["token"] is True and snap["last_ok"]
    assert snap["clock"] == Clock(host="10.0.0.7", name="clawd", source="saved")


def test_no_token_payload_marks_token_missing(tmp_path):
    e = eng.Engine(ClockFinder(ClockStore(tmp_path / "clock.json"), browser=NoBrowser()))
    e._on_payload({"ok": False})
    assert e.token is False
    e._on_payload({"s": 1, "ok": True})
    assert e.token is True



def test_codex_or_no_usage_says_nothing_about_a_claude_login(tmp_path):
    e = eng.Engine(ClockFinder(ClockStore(tmp_path / "clock.json"), browser=NoBrowser()))
    e._on_payload({"ok": False, "src": "off"})
    assert e.token is None
    e._on_payload({"ok": False, "src": "codex"})
    assert e.token is None
    e._on_payload({"s": 3, "ok": True, "src": "codex"})
    assert e.token is None


def test_daemon_running_checks_the_state_endpoint(monkeypatch):
    def fake_get(url, timeout):
        assert url == "http://127.0.0.1:5555/state"
        return httpx.Response(200, text='{"desired": {}}')
    monkeypatch.setattr(httpx, "get", fake_get)
    assert eng.daemon_running(5555)

    def refused(url, timeout):
        raise httpx.ConnectError("refused")
    monkeypatch.setattr(httpx, "get", refused)
    assert not eng.daemon_running(5555)


def test_a_settings_pause_empties_the_targets(tmp_path):
    from clawd_desktop import pause
    store = ClockStore(tmp_path / "clock.json")
    store.save({"host": "10.0.0.7", "name": "clawd"})
    now = [1000.0]
    p = tmp_path / "pause.json"
    e = eng.Engine(ClockFinder(store, browser=NoBrowser()),
                   pause=pause.PauseReader(p, now=lambda: now[0]))
    e.finder.current = Clock(host="10.0.0.7", name="clawd", source="saved")
    assert e.targets() == ["http://10.0.0.7/api/usage"]
    pause.hold(60, "firmware update", path=p, now=lambda: now[0])
    assert e.targets() == [] and e.snapshot()["paused"] == "firmware update"
    now[0] += 61                                     # the window crashed: the lease runs out
    assert e.targets() == ["http://10.0.0.7/api/usage"] and e.snapshot()["paused"] is None


def test_read_clock_screen_uses_the_clocks_config(tmp_path, monkeypatch):
    from clawd_desktop import engine as eng_mod
    from clawd_desktop.discovery import Clock

    class Api:
        def __init__(self, base, auth=None):
            self.base = base
        def config(self):
            return {"usage": {"screen": 1}}
        def close(self):
            pass
    monkeypatch.setattr(eng_mod, "ClockApi", Api)
    monkeypatch.setattr(eng_mod.creds, "get", lambda key: None)
    e = eng_mod.Engine(ClockFinder(ClockStore(tmp_path / "clock.json"), browser=NoBrowser()))
    assert e.read_clock_screen() is None                 # no clock yet
    e.finder.current = Clock(host="10.0.0.5")
    assert e.read_clock_screen() == 1
