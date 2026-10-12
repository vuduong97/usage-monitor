"""A clock that restarts gets its usage and Clawd state back without waiting
for the next poll (BootWatch + Publisher.resend)."""
import asyncio
import json
from unittest.mock import AsyncMock, patch

import httpx

from clawd_desktop import bootwatch as bw, usage as d

URL = "http://10.0.0.5/api/usage"
PAYLOAD = {"s": 42, "sr": 133, "w": 67, "wr": 3000, "st": "allowed", "acct": "pro", "ok": True}


class Fake:
    """Fake clock: answers /api/heap with `uptime` (None = no answer)."""

    def __init__(self):
        self.uptime: int | None = 5000
        self.heap_hits = 0

    def transport(self):
        def handler(req: httpx.Request) -> httpx.Response:
            assert req.url.path == "/api/heap"
            self.heap_hits += 1
            if self.uptime is None:
                raise httpx.ConnectTimeout("down", request=req)
            return httpx.Response(200, json={"uptime": self.uptime, "version": "2.16.0"})
        return httpx.MockTransport(handler)


def _watch(fake, now, targets=lambda: [URL], delivered=lambda: True):
    hits = []

    async def on_restart(url, up):
        hits.append((url, up))
        return delivered()
    w = bw.BootWatch(targets, on_restart, transport=fake.transport(), clock=lambda: now[0])
    return w, hits


def test_heap_url():
    assert bw.heap_url("http://10.0.0.5/api/usage") == "http://10.0.0.5/api/heap"
    assert bw.heap_url("http://clawd.local:8080/custom") == "http://clawd.local:8080/api/heap"


def test_uptime_drop_is_a_restart_reported_once():
    fake, now = Fake(), [1000.0]
    w, hits = _watch(fake, now)
    asyncio.run(w.step())                    # first look, booted long ago: not a restart
    now[0] += 5
    fake.uptime += 5
    asyncio.run(w.step())                    # still running
    assert hits == []
    now[0] += 40
    fake.uptime = 12                         # rebooted
    asyncio.run(w.step())
    assert hits == [(URL, 12)]
    now[0] += 5
    fake.uptime = 17
    asyncio.run(w.step())                    # same boot: no second report
    assert hits == [(URL, 12)]


def test_slow_answers_are_not_restarts():
    """Request latency shifts the boot estimate a little; that is not a restart."""
    fake, now = Fake(), [1000.0]
    w, hits = _watch(fake, now)
    asyncio.run(w.step())
    now[0] += 20                             # a slow reply: 20 s passed, uptime +8
    fake.uptime += 8
    asyncio.run(w.step())
    assert hits == []


def test_unreachable_then_back_after_reboot():
    fake, now = Fake(), [1000.0]
    w, hits = _watch(fake, now)
    asyncio.run(w.step())
    fake.uptime, now[0] = None, now[0] + 5   # rebooting: no answer
    asyncio.run(w.step())
    assert hits == []
    fake.uptime, now[0] = 9, now[0] + 30
    asyncio.run(w.step())
    assert hits == [(URL, 9)]


def test_new_address_that_just_booted_counts():
    """DHCP gave the rebooted clock a new IP: first look at it, freshly booted."""
    fake, now = Fake(), [1000.0]
    fake.uptime = 30
    w, hits = _watch(fake, now)
    asyncio.run(w.step())
    assert hits == [(URL, 30)]


def test_failed_delivery_retries_on_next_probe_only_while_fresh():
    fake, now = Fake(), [1000.0]
    ok = [False]
    w, hits = _watch(fake, now, delivered=lambda: ok[0])
    asyncio.run(w.step())
    now[0] += 40
    fake.uptime = 10
    asyncio.run(w.step())                    # restart seen, data not taken
    now[0] += 5
    fake.uptime = 15
    ok[0] = True
    asyncio.run(w.step())                    # retried, delivered
    now[0] += 5
    fake.uptime = 20
    asyncio.run(w.step())                    # done: no more retries
    assert hits == [(URL, 10), (URL, 15)]


def test_retries_stop_after_the_fresh_window():
    fake, now = Fake(), [1000.0]
    fake.uptime = 100
    w, hits = _watch(fake, now, delivered=lambda: False)
    asyncio.run(w.step())                    # new clock, booted 100 s ago: try
    now[0] += 30
    fake.uptime = 130
    asyncio.run(w.step())                    # past FRESH_BOOT: leave it to the poll
    assert hits == [(URL, 100)]


def test_no_targets_no_probe():
    fake, now = Fake(), [1000.0]
    w, hits = _watch(fake, now, targets=lambda: [])
    asyncio.run(w.step())
    assert fake.heap_hits == 0 and hits == []


# ---- Publisher.resend -----------------------------------------------------------

def _recording():
    seen = []

    def handler(req):
        seen.append((str(req.url), json.loads(req.content)))
        return httpx.Response(200, json={"ok": True})
    return seen, httpx.MockTransport(handler)


def _env(tmp_path, cfg="http_targets = 10.0.0.5\nbeep = on\n"):
    (tmp_path / "config").write_text(cfg)
    return patch.multiple(d, CONFIG_FILE=tmp_path / "config", ATTENTION_DIR=tmp_path,
                          ATTENTION_FLAG=tmp_path / "attention")


def test_resend_replays_last_payload_without_polling(tmp_path):
    seen, transport = _recording()
    pub = d.Publisher(http=d.HttpSinks(transport=transport))
    poll = AsyncMock(side_effect=lambda: (dict(PAYLOAD), False))
    with _env(tmp_path), patch.object(d, "poll_active", new=poll):
        (tmp_path / "attention").write_text('{"m": "Needs you", "s": "sess1"}')
        asyncio.run(pub.step())                     # poll + alert with beep
        assert seen[-1][1]["b"] == 1
        assert asyncio.run(pub.resend(URL)) is True
    assert poll.await_count == 1                    # the resend never polls
    assert seen[-1] == (URL, PAYLOAD)               # no beep / alert replayed


def test_resend_refreshes_wall_clock(tmp_path):
    seen, transport = _recording()
    pub = d.Publisher(http=d.HttpSinks(transport=transport))
    pub.last_payload = {**PAYLOAD, "t": 1, "tf": 24}
    with _env(tmp_path, "clock = 24\n"):
        asyncio.run(pub.resend(URL))
    body = seen[-1][1]
    assert body["t"] > 1_000_000_000 and body["tf"] == 24


def test_resend_with_nothing_cached_asks_for_a_poll():
    seen, transport = _recording()
    pub = d.Publisher(http=d.HttpSinks(transport=transport))
    assert asyncio.run(pub.resend(URL)) is True      # the forced poll delivers it
    assert pub.force and pub.wake.is_set() and seen == []
