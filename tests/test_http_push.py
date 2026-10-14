"""WiFi push (`http_targets`) on the macOS daemon: target parsing, delivery,
and the shared Publisher that feeds BLE and HTTP from one poll."""

import asyncio
import json
from unittest.mock import AsyncMock, MagicMock, patch

import httpx

from clawd_desktop import usage as d

PAYLOAD = {"s": 42, "sr": 133, "w": 67, "wr": 3000, "st": "allowed", "acct": "pro", "ok": True}


def _config(tmp_path, text):
    cfg = tmp_path / "config"
    cfg.write_text(text)
    return patch.object(d, "CONFIG_FILE", cfg)


def _attention(tmp_path):
    return patch.multiple(d, ATTENTION_DIR=tmp_path, ATTENTION_FLAG=tmp_path / "attention")


def _recording_transport(status=200):
    seen = []

    def handler(request: httpx.Request) -> httpx.Response:
        seen.append((str(request.url), json.loads(request.content)))
        return httpx.Response(status, json={"ok": status == 200})

    return seen, httpx.MockTransport(handler)


def test_normalize_http_target():
    n = d.normalize_http_target
    assert n("192.168.0.125") == "http://192.168.0.125/api/usage"
    assert n(" clawd.local:8080 ") == "http://clawd.local:8080/api/usage"
    assert n("http://10.0.0.5/") == "http://10.0.0.5/api/usage"
    assert n("http://10.0.0.5/custom/push") == "http://10.0.0.5/custom/push"
    assert n("") is None
    assert n("ftp://host") is None


def test_read_http_targets(tmp_path):
    with _config(tmp_path, "http_targets = 192.168.0.125, ,bad://x, 192.168.0.125 # dup\n"):
        assert d.read_http_targets() == ["http://192.168.0.125/api/usage"]
    with _config(tmp_path, "clock = auto\n"):
        assert d.read_http_targets() == []


def test_ble_setting_defaults_on(tmp_path):
    with _config(tmp_path, ""):
        assert d.read_onoff_setting("ble", default="on") == "on"
    with _config(tmp_path, "ble = off\n"):
        assert d.read_onoff_setting("ble", default="on") == "off"


def test_clock_time_sent_by_default(tmp_path):
    """The clock has no RTC and NTP may be blocked: without a config the payload carries the time."""
    missing = tmp_path / "none" / "config"
    with patch.object(d, "CONFIG_FILE", missing):
        p = {}
        d.add_clock_fields(p)
    assert isinstance(p.get("t"), int) and p.get("tf") in (12, 24)
    with _config(tmp_path, "clock = 12\n"):
        p = {}
        d.add_clock_fields(p)
    assert p["tf"] == 12
    with _config(tmp_path, "clock = off\n"):
        p = {}
        d.add_clock_fields(p)
    assert p == {}


def test_http_sinks_delivers_to_every_target():
    seen, transport = _recording_transport()
    sinks = d.HttpSinks(transport=transport)
    targets = ["http://a/api/usage", "http://b/api/usage"]
    assert asyncio.run(sinks.send(targets, PAYLOAD)) is True
    assert sorted(url for url, _ in seen) == targets
    assert all(body == PAYLOAD for _, body in seen)


def test_http_sinks_failure_is_reported():
    _seen, transport = _recording_transport(status=400)
    assert asyncio.run(d.HttpSinks(transport=transport).send(["http://a/api/usage"], PAYLOAD)) is False

    def boom(request):
        raise httpx.ConnectError("unreachable", request=request)

    sinks = d.HttpSinks(transport=httpx.MockTransport(boom))
    assert asyncio.run(sinks.send(["http://a/api/usage"], PAYLOAD)) is False


def _publisher(transport):
    return d.Publisher(http=d.HttpSinks(transport=transport))


def test_publisher_pushes_without_ble(tmp_path):
    """A WiFi-only setup polls and pushes with no Bluetooth device at all."""
    seen, transport = _recording_transport()
    pub = _publisher(transport)
    with _config(tmp_path, "http_targets = 192.168.0.125\n"), _attention(tmp_path), \
         patch.object(d, "poll_active", new=AsyncMock(return_value=(dict(PAYLOAD), False))):
        asyncio.run(pub.step())
    assert seen == [("http://192.168.0.125/api/usage", PAYLOAD)]
    assert pub.last_poll > 0


def test_publisher_idle_without_destinations(tmp_path):
    """No BLE link and no targets: no API call, attention flags left in place."""
    (tmp_path / "attention").write_text('{"m": "hi", "s": "abc"}')
    poll = AsyncMock(return_value=(dict(PAYLOAD), False))
    with _config(tmp_path, ""), _attention(tmp_path), patch.object(d, "poll_active", new=poll):
        asyncio.run(_publisher(None).step())
    poll.assert_not_awaited()
    assert (tmp_path / "attention").exists()


def test_publisher_respects_poll_interval(tmp_path):
    seen, transport = _recording_transport()
    pub = _publisher(transport)
    poll = AsyncMock(return_value=(dict(PAYLOAD), False))
    with _config(tmp_path, "http_targets = h\n"), _attention(tmp_path), \
         patch.object(d, "poll_active", new=poll):
        asyncio.run(pub.step())
        asyncio.run(pub.step())        # within POLL_INTERVAL: nothing new
    assert poll.await_count == 1
    assert len(seen) == 1


def test_publisher_alert_rides_once_then_clear(tmp_path):
    seen, transport = _recording_transport()
    pub = _publisher(transport)
    poll = AsyncMock(side_effect=lambda: (dict(PAYLOAD), False))
    with _config(tmp_path, "http_targets = h\nbeep = on\n"), _attention(tmp_path), \
         patch.object(d, "poll_active", new=poll):
        asyncio.run(pub.step())                                  # initial poll
        (tmp_path / "attention").write_text('{"m": "Needs you", "s": "sess1"}')
        asyncio.run(pub.step())                                  # alert forces a poll
        asyncio.run(pub.step())                                  # nothing pending
        (tmp_path / "clear-sess1").touch()
        asyncio.run(pub.step())                                  # clear forces a poll
    bodies = [body for _, body in seen]
    assert len(bodies) == 3
    assert "m" not in bodies[0]
    assert bodies[1]["m"] == "Needs you" and bodies[1]["b"] == 1
    assert bodies[2].get("x") == 1 and "m" not in bodies[2]


def test_publisher_no_data_beat(tmp_path):
    seen, transport = _recording_transport()
    with _config(tmp_path, "http_targets = h\n"), _attention(tmp_path), \
         patch.object(d, "poll_active", new=AsyncMock(return_value=(None, True))):
        asyncio.run(_publisher(transport).step())
    assert [body for _, body in seen] == [{"ok": False}]


def test_publisher_feeds_ble_and_http_from_one_poll(tmp_path):
    seen, transport = _recording_transport()
    pub = _publisher(transport)
    client = MagicMock()
    client.is_connected = True
    client.services.get_characteristic.return_value = None
    client.write_gatt_char = AsyncMock(return_value=None)
    session = d.Session(client)
    poll = AsyncMock(return_value=(dict(PAYLOAD), False))
    with _config(tmp_path, "http_targets = h\n"), _attention(tmp_path), \
         patch.object(d, "poll_active", new=poll):
        pub.last_poll = 10**12         # schedule not due ...
        pub.attach_ble(session)        # ... but a fresh link forces a poll
        asyncio.run(pub.step())
    assert poll.await_count == 1
    client.write_gatt_char.assert_awaited_once()
    assert json.loads(client.write_gatt_char.await_args.args[1]) == PAYLOAD
    assert [body for _, body in seen] == [PAYLOAD]
    assert session.data_writes == 1


def test_publisher_delivered_if_any_destination_ok(tmp_path):
    """A dead WiFi screen must not block the schedule for a working BLE link."""
    _seen, transport = _recording_transport(status=500)
    pub = _publisher(transport)
    client = MagicMock()
    client.is_connected = True
    client.services.get_characteristic.return_value = None
    client.write_gatt_char = AsyncMock(return_value=None)
    with _config(tmp_path, "http_targets = h\n"), _attention(tmp_path), \
         patch.object(d, "poll_active", new=AsyncMock(return_value=(dict(PAYLOAD), False))):
        pub.attach_ble(d.Session(client))
        asyncio.run(pub.step())
    assert pub.last_poll > 0
