#!/usr/bin/env python3
"""Usage sources: the read-only /api/oauth/usage endpoint and CLIProxyAPI.

Covers the response mapping, the Haiku-probe fallback, CLIProxyAPI account
discovery and polling through its management `api-call` (the Claude token is
substituted proxy-side as `$TOKEN$`), proxy failures, plan selection across
sources, and the non-auth 4xx backoff.

Run: python -m pytest daemon/tests/test_usage_sources.py -q
"""
import asyncio
import datetime
import json
from pathlib import Path
from unittest.mock import AsyncMock, patch

import httpx
import pytest

import daemon.claude_usage_daemon as mod

FIXTURE = json.loads((Path(__file__).parent / "fixtures" / "oauth_usage_max.json").read_text())
# 2026-10-11 09:40 UTC: 2h20m before the 5h reset, 6d0h20m before the weekly one.
NOW = datetime.datetime(2026, 10, 11, 9, 40, tzinfo=datetime.timezone.utc).timestamp()
KEY = "mgmt-key-for-tests"
IDX = "2864549a21dbb562"


def _run(coro):
    return asyncio.run(coro)


@pytest.fixture(autouse=True)
def _isolated(tmp_path, monkeypatch):
    cfg = tmp_path / "config"
    cfg.write_text("")
    monkeypatch.setattr(mod, "CONFIG_FILE", cfg)
    monkeypatch.setattr(mod, "ANTHROPIC_STORE_DIR", tmp_path / "no-store")
    mod._BACKOFF.clear()
    yield cfg
    mod._BACKOFF.clear()


# ---------------------------------------------------------------------------
# usage_from_oauth_usage
# ---------------------------------------------------------------------------

def test_maps_max_account_response():
    p = mod.usage_from_oauth_usage(FIXTURE, now=NOW)
    assert p == {"s": 7, "sr": 140, "w": 8, "wr": 6 * 1440 + 20, "st": "allowed",
                 "acct": "pro", "ok": True}


def test_locked_window_is_rejected():
    body = json.loads(json.dumps(FIXTURE))
    body["five_hour"]["locked_reason"] = "session_limit"
    assert mod.usage_from_oauth_usage(body, now=NOW)["st"] == "rejected"


def test_non_normal_severity_is_a_warning():
    body = json.loads(json.dumps(FIXTURE))
    body["limits"][1]["severity"] = "warning"
    assert mod.usage_from_oauth_usage(body, now=NOW)["st"] == "allowed_warning"


def test_missing_weekly_window_reads_zero():
    body = dict(FIXTURE, seven_day=None)
    p = mod.usage_from_oauth_usage(body, now=NOW)
    assert (p["s"], p["w"], p["wr"]) == (7, 0, 0)


def test_utilization_is_clamped_and_past_resets_are_zero():
    body = json.loads(json.dumps(FIXTURE))
    body["five_hour"]["utilization"] = 130.4
    body["five_hour"]["resets_at"] = "2020-01-01T00:00:00Z"
    p = mod.usage_from_oauth_usage(body, now=NOW)
    assert (p["s"], p["sr"]) == (100, 0)


@pytest.mark.parametrize("body", [
    {}, [], None, {"five_hour": None}, {"five_hour": {"utilization": None}},
])
def test_no_five_hour_window_is_not_applicable(body):
    with pytest.raises(mod.UsageNotApplicable):
        mod.usage_from_oauth_usage(body, now=NOW)


# ---------------------------------------------------------------------------
# _usage_result: HTTP outcome classification (shared by direct + proxy)
# ---------------------------------------------------------------------------

@pytest.mark.parametrize("status", [401, 403])
def test_auth_errors_mean_expired_token(status):
    with pytest.raises(mod.TokenExpired):
        mod._usage_result(status, "{}")


@pytest.mark.parametrize("status,text", [
    (404, "not found"),
    (400, '{"error":{"message":"Usage limits are not applicable to API organizations"}}'),
])
def test_not_applicable_answers(status, text):
    with pytest.raises(mod.UsageNotApplicable):
        mod._usage_result(status, text)


@pytest.mark.parametrize("status,text", [(500, "boom"), (429, "slow down"), (200, "<html>")])
def test_transient_answers_return_none(status, text):
    assert mod._usage_result(status, text) is None


def test_ok_answer_carries_clock_fields(_isolated):
    _isolated.write_text("clock = 24\n")
    p = mod._usage_result(200, json.dumps(FIXTURE))
    assert p is not None
    assert p["s"] == 7 and p["tf"] == 24 and "t" in p


# ---------------------------------------------------------------------------
# poll_token: usage endpoint first, Haiku probe only as the fallback
# ---------------------------------------------------------------------------

def test_poll_token_uses_endpoint_without_haiku():
    haiku = AsyncMock()
    with patch.object(mod, "poll_usage_endpoint", new=AsyncMock(return_value={"s": 7})), \
         patch.object(mod, "poll_api", new=haiku):
        assert _run(mod.poll_token("tok")) == {"s": 7}
    haiku.assert_not_awaited()


def test_poll_token_falls_back_when_not_applicable():
    with patch.object(mod, "poll_usage_endpoint",
                      new=AsyncMock(side_effect=mod.UsageNotApplicable("HTTP 404"))), \
         patch.object(mod, "poll_api", new=AsyncMock(return_value={"s": 3, "acct": "ent"})):
        assert _run(mod.poll_token("tok")) == {"s": 3, "acct": "ent"}


def test_poll_token_does_not_fall_back_on_expired_token():
    haiku = AsyncMock()
    with patch.object(mod, "poll_usage_endpoint", new=AsyncMock(side_effect=mod.TokenExpired())), \
         patch.object(mod, "poll_api", new=haiku), \
         pytest.raises(mod.TokenExpired):
        _run(mod.poll_token("tok"))
    haiku.assert_not_awaited()


# ---------------------------------------------------------------------------
# CliProxySource
# ---------------------------------------------------------------------------

AUTH_FILES = {"files": [
    {"provider": "claude", "type": "claude", "auth_index": IDX, "status": "active",
     "disabled": False, "unavailable": False},
    {"provider": "claude", "auth_index": "dddddddd00000000", "disabled": True},
    {"provider": "claude", "auth_index": "uuuuuuuu00000000", "unavailable": True},
    {"provider": "gemini", "auth_index": "gggggggg00000000"},
]}


class FakeProxy:
    """A CLIProxyAPI management API behind httpx.MockTransport."""

    def __init__(self, files=AUTH_FILES, upstream=None, key=KEY):
        self.files = files
        self.upstream = upstream or {}           # auth index -> (status, body)
        self.key = key
        self.list_calls = 0
        self.calls: list[dict] = []

    def handler(self, request: httpx.Request) -> httpx.Response:
        if request.headers.get("authorization") != f"Bearer {self.key}":
            return httpx.Response(401, json={"error": "bad key"})
        if request.url.path == mod.CLIPROXY_AUTH_FILES_PATH:
            self.list_calls += 1
            return httpx.Response(200, json=self.files)
        if request.url.path == mod.CLIPROXY_API_CALL_PATH:
            req = json.loads(request.content)
            self.calls.append(req)
            status, body = self.upstream.get(req["authIndex"], (200, FIXTURE))
            return httpx.Response(200, json={"status_code": status, "header": {},
                                             "body": json.dumps(body)})
        return httpx.Response(404)

    def source(self):
        return mod.CliProxySource(transport=httpx.MockTransport(self.handler))


@pytest.fixture
def key_in_keychain(monkeypatch):
    monkeypatch.setattr(mod, "read_cliproxy_key", lambda: KEY)


def test_discovers_active_claude_accounts_only(key_in_keychain):
    proxy = FakeProxy()
    payloads, live = _run(proxy.source().poll())
    assert live and list(payloads) == [f"cliproxy:{IDX[:8]}"]
    assert payloads[f"cliproxy:{IDX[:8]}"]["s"] == 7
    assert [c["authIndex"] for c in proxy.calls] == [IDX]


def test_api_call_lets_the_proxy_substitute_the_token(key_in_keychain):
    proxy = FakeProxy()
    _run(proxy.source().poll())
    call = proxy.calls[0]
    assert call["method"] == "GET" and call["url"] == mod.USAGE_URL
    assert call["header"]["Authorization"] == "Bearer $TOKEN$"
    assert KEY not in json.dumps(call)          # the management key never goes upstream


def test_account_list_is_cached(key_in_keychain):
    proxy = FakeProxy()
    src = proxy.source()
    _run(src.poll())
    _run(src.poll())
    assert proxy.list_calls == 1 and len(proxy.calls) == 2


def test_account_list_refreshes_after_ttl(key_in_keychain, monkeypatch):
    proxy = FakeProxy()
    src = proxy.source()
    _run(src.poll())
    src._listed_at -= mod.CLIPROXY_ACCOUNTS_TTL + 1
    _run(src.poll())
    assert proxy.list_calls == 2


def test_wrong_key_yields_nothing_and_is_dead(monkeypatch):
    monkeypatch.setattr(mod, "read_cliproxy_key", lambda: "wrong")
    assert _run(FakeProxy().source().poll()) == ({}, False)


def test_no_key_in_keychain_skips_the_proxy(monkeypatch):
    monkeypatch.setattr(mod, "read_cliproxy_key", lambda: None)
    proxy = FakeProxy()
    assert _run(proxy.source().poll()) == ({}, False)
    assert proxy.list_calls == 0


def test_proxy_not_running_is_dead(key_in_keychain):
    def refuse(request):
        raise httpx.ConnectError("connection refused", request=request)
    src = mod.CliProxySource(transport=httpx.MockTransport(refuse))
    assert _run(src.poll()) == ({}, False)


def test_expired_account_is_dead_but_others_live(key_in_keychain):
    other = "bbbbbbbb11111111"
    files = {"files": [{"provider": "claude", "auth_index": IDX},
                       {"provider": "claude", "auth_index": other}]}
    proxy = FakeProxy(files=files, upstream={IDX: (401, {"error": "expired"})})
    payloads, live = _run(proxy.source().poll())
    assert live and list(payloads) == [f"cliproxy:{other[:8]}"]


def test_all_accounts_expired_is_dead(key_in_keychain):
    proxy = FakeProxy(upstream={IDX: (401, {"error": "expired"})})
    assert _run(proxy.source().poll()) == ({}, False)


def test_transient_upstream_error_stays_live(key_in_keychain):
    proxy = FakeProxy(upstream={IDX: (529, {"error": "overloaded"})})
    assert _run(proxy.source().poll()) == ({}, True)


def test_transient_account_error_backs_off(key_in_keychain):
    proxy = FakeProxy(upstream={IDX: (529, {"error": "overloaded"})})
    src = proxy.source()
    _run(src.poll())
    assert _run(src.poll()) == ({}, True)        # next 5 s tick: not asked again
    assert len(proxy.calls) == 1
    src._backoff.clear()                          # interval elapsed
    _run(src.poll())
    assert len(proxy.calls) == 2


def test_source_failure_is_logged_once(monkeypatch):
    lines = []
    monkeypatch.setattr(mod, "log", lines.append)
    monkeypatch.setattr(mod, "read_cliproxy_key", lambda: None)
    src = mod.CliProxySource()
    for _ in range(3):
        _run(src.poll())
    assert len(lines) == 1 and "no management key" in lines[0]


def test_cliproxy_url_from_config(_isolated):
    assert mod.read_cliproxy_url() == "http://localhost:8317"
    _isolated.write_text("cliproxy_url = 127.0.0.1:9999/\n")
    assert mod.read_cliproxy_url() == "http://127.0.0.1:9999"


# ---------------------------------------------------------------------------
# poll_active across sources
# ---------------------------------------------------------------------------

DIR = Path("claude-home-test")   # a plan key only; never touched on disk


def _with_proxy(monkeypatch, cfg, payloads, live=True):
    cfg.write_text("cliproxy = on\n")
    monkeypatch.setattr(mod._CLIPROXY, "poll", AsyncMock(return_value=(payloads, live)))


def test_cliproxy_off_never_touches_the_proxy(monkeypatch, _isolated):
    proxy_poll = AsyncMock()
    monkeypatch.setattr(mod._CLIPROXY, "poll", proxy_poll)
    monkeypatch.setattr(mod, "read_config_dirs", lambda: [DIR])
    monkeypatch.setattr(mod, "read_token_for", lambda d: "tok")
    with patch.object(mod, "poll_token", new=AsyncMock(return_value={"s": 5})):
        assert _run(mod.poll_active(mod.PlanSelector())) == ({"s": 5}, False)
    proxy_poll.assert_not_awaited()


def test_proxy_account_wins_when_it_is_the_one_in_use(monkeypatch, _isolated):
    monkeypatch.setattr(mod, "read_config_dirs", lambda: [DIR])
    monkeypatch.setattr(mod, "read_token_for", lambda d: "tok")
    sel = mod.PlanSelector()
    with patch.object(mod, "poll_token", new=AsyncMock(return_value={"s": 30})):
        _with_proxy(monkeypatch, _isolated, {"cliproxy:2864549a": {"s": 7}})
        assert _run(mod.poll_active(sel))[0] == {"s": 30}        # startup: highest %
        _with_proxy(monkeypatch, _isolated, {"cliproxy:2864549a": {"s": 9}})
        assert _run(mod.poll_active(sel))[0] == {"s": 9}         # proxy account rose


def test_proxy_alone_carries_the_device_when_login_is_dead(monkeypatch, _isolated):
    monkeypatch.setattr(mod, "read_config_dirs", lambda: [DIR])
    monkeypatch.setattr(mod, "read_token_for", lambda d: None)
    _with_proxy(monkeypatch, _isolated, {"cliproxy:2864549a": {"s": 7}})
    assert _run(mod.poll_active(mod.PlanSelector())) == ({"s": 7}, False)


def test_all_sources_dead_signals_no_data(monkeypatch, _isolated):
    monkeypatch.setattr(mod, "read_config_dirs", lambda: [DIR])
    monkeypatch.setattr(mod, "read_token_for", lambda d: None)
    _with_proxy(monkeypatch, _isolated, {}, live=False)
    assert _run(mod.poll_active(mod.PlanSelector())) == (None, True)


def test_proxy_down_with_live_but_silent_login_stays_quiet(monkeypatch, _isolated):
    monkeypatch.setattr(mod, "read_config_dirs", lambda: [DIR])
    monkeypatch.setattr(mod, "read_token_for", lambda d: "tok")
    _with_proxy(monkeypatch, _isolated, {}, live=False)
    with patch.object(mod, "poll_token", new=AsyncMock(return_value=None)):
        assert _run(mod.poll_active(mod.PlanSelector())) == (None, False)


def test_unanswered_dir_backs_off_for_a_poll_interval(monkeypatch, _isolated):
    monkeypatch.setattr(mod, "read_config_dirs", lambda: [DIR])
    monkeypatch.setattr(mod, "read_token_for", lambda d: "tok")
    poll = AsyncMock(return_value=None)          # e.g. "credit balance too low"
    with patch.object(mod, "poll_token", new=poll):
        sel = mod.PlanSelector()
        assert _run(mod.poll_active(sel)) == (None, False)
        assert _run(mod.poll_active(sel)) == (None, False)   # next 5 s tick
        assert poll.await_count == 1
        mod._BACKOFF[DIR] = 0                                  # interval elapsed
        _run(mod.poll_active(sel))
        assert poll.await_count == 2
