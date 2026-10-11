#!/usr/bin/env python3
"""Token lookup in the Anthropic credentials store (Claude Code storage v5).

Claude Code 2.1.29x moved the default login out of the Keychain item
("Claude Code-credentials" then holds only {"mcpOAuth": {}}) into
~/.config/anthropic/{active_config, credentials/<profile>.json}.

Run: python -m pytest daemon/tests/test_token_store.py -q
"""
import json

import pytest

import daemon.claude_usage_daemon as mod

NOW = 1_800_000_000.0


def _store(tmp_path, cred, profile="default", active=True):
    if active:
        (tmp_path / "active_config").write_text(profile + "\n")
    (tmp_path / "credentials").mkdir(exist_ok=True)
    (tmp_path / "credentials" / f"{profile}.json").write_text(json.dumps(cred))
    return tmp_path


def _cred(**kw):
    base = {"version": "1.0", "type": "oauth_token", "access_token": "sk-ant-oat-LIVE",
            "refresh_token": "sk-ant-ort-R", "expires_at": int((NOW + 3600) * 1000)}
    base.update(kw)
    return base


def test_reads_live_token(tmp_path):
    assert mod._read_token_store(_store(tmp_path, _cred()), now=NOW) == "sk-ant-oat-LIVE"


def test_follows_active_profile(tmp_path):
    _store(tmp_path, _cred(access_token="OTHER"), profile="work")
    assert mod._read_token_store(tmp_path, now=NOW) == "OTHER"


def test_missing_active_config_means_default(tmp_path):
    _store(tmp_path, _cred(), active=False)
    assert mod._read_token_store(tmp_path, now=NOW) == "sk-ant-oat-LIVE"


@pytest.mark.parametrize("cred", [
    _cred(expires_at=int((NOW - 1) * 1000)),          # expired (ms)
    _cred(expires_at=int(NOW - 1)),                   # expired (s)
    _cred(type="api_key"),                            # not an OAuth login
    _cred(access_token=""),
])
def test_unusable_entries_return_none(tmp_path, cred):
    assert mod._read_token_store(_store(tmp_path, cred), now=NOW) is None


def test_absent_or_corrupt_store(tmp_path):
    assert mod._read_token_store(tmp_path / "nope", now=NOW) is None
    (tmp_path / "credentials").mkdir()
    (tmp_path / "credentials" / "default.json").write_text("{not json")
    assert mod._read_token_store(tmp_path, now=NOW) is None


def test_profile_cannot_escape_store(tmp_path):
    (tmp_path / "active_config").write_text("../../etc/x")
    assert mod._read_token_store(tmp_path, now=NOW) is None


def test_default_dir_prefers_store_over_keychain(tmp_path, monkeypatch):
    (tmp_path / "store").mkdir()
    store = _store(tmp_path / "store", _cred(expires_at=4_102_444_800_000))
    monkeypatch.setattr(mod, "ANTHROPIC_STORE_DIR", store)
    monkeypatch.setattr(mod, "DEFAULT_CONFIG_DIR", tmp_path / "claude")
    monkeypatch.setattr(mod.sys, "platform", "darwin")
    monkeypatch.setattr(mod, "_read_token_keychain", lambda: "KEYCHAIN")
    assert mod.read_token_for(tmp_path / "claude") == "sk-ant-oat-LIVE"


def test_default_dir_falls_back_to_keychain_when_store_expired(tmp_path, monkeypatch):
    (tmp_path / "store").mkdir()
    store = _store(tmp_path / "store", _cred(expires_at=1000))
    monkeypatch.setattr(mod, "ANTHROPIC_STORE_DIR", store)
    monkeypatch.setattr(mod, "DEFAULT_CONFIG_DIR", tmp_path / "claude")
    monkeypatch.setattr(mod.sys, "platform", "darwin")
    monkeypatch.setattr(mod, "_read_token_keychain", lambda: "KEYCHAIN")
    assert mod.read_token_for(tmp_path / "claude") == "KEYCHAIN"


def test_other_dirs_ignore_store(tmp_path, monkeypatch):
    (tmp_path / "store").mkdir()
    monkeypatch.setattr(mod, "ANTHROPIC_STORE_DIR", _store(tmp_path / "store", _cred()))
    monkeypatch.setattr(mod.sys, "platform", "linux")
    (tmp_path / "work").mkdir()
    assert mod.read_token_for(tmp_path / "work") is None
