#!/usr/bin/env python3
"""Golden exporter for the `config` area (phase 06, crate `clawd-config`).

Runs the REAL `clawd_desktop.usage` readers (config file, config dirs, Claude token sources,
Keychain blob decode, CLIProxy settings, hour format) inside the isolating harness and records
`{input, output}` fixtures under `desktop-rs/fixtures/config/`. The Rust tests replay `input`
through `clawd-config` and must reproduce `output`.

Never touches the real home, the real Keychain or the network: HOME is a temp dir, `security` /
`defaults` are replaced by a scripted `subprocess.run`, and every token is `sk-ant-test-*`.
`sys.platform` is switched to `darwin` / `linux` around a case, like the Python tests do.

    python tools/golden/export_config.py
"""
from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
from pathlib import Path
from types import SimpleNamespace

sys.path.insert(0, str(Path(__file__).resolve().parent))
import _harness as h  # noqa: E402

ctx = h.isolate()
os.environ["LOGNAME"] = "clawd-test-user"      # getpass.getuser(): the Keychain account
from clawd_desktop import usage  # noqa: E402  (after isolate())

USER = "clawd-test-user"
HOME = ctx.home
CONFIG = ctx.config_dir / "config"
PREFS = ctx.config_dir / "desktop.json"
FUTURE_MS = 4_102_444_800_000
NOW = 1_800_000_000.0


def pth(value) -> str:
    """A path as `<home>/...` with forward slashes (the fixture must not depend on the OS)."""
    return h.rel(ctx, str(value)).replace("\\", "/")


def clear(path: Path) -> None:
    if path.is_dir():
        shutil.rmtree(path)
    elif path.exists():
        path.unlink()


def write_bytes(path: Path, text: str | None) -> None:
    clear(path)
    if text is not None:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(text.encode("ascii"))     # no newline translation on Windows


def blob(token: str) -> str:
    return json.dumps({"claudeAiOauth": {"accessToken": token, "expiresAt": FUTURE_MS}})


def cred(**kw) -> dict:
    base = {"version": "1.0", "type": "oauth_token", "access_token": "sk-ant-test-LIVE",
            "refresh_token": "sk-ant-fake-R", "expires_at": FUTURE_MS}
    base.update(kw)
    return base


def hexed(text: str) -> str:
    return text.encode("utf-8").hex()


# ---------------------------------------------------------------- config file readers

CONFIG_CASES = [
    {"name": "missing", "config": None},
    {"name": "empty", "config": ""},
    {"name": "comments_case_spacing", "config": (
        "# top\n  Clock =  24  # trailing\nChime=ON\nbroken\n=novalue\n"
        "http_targets = 192.168.0.125, ,bad://x, 192.168.0.125 # dup\nEVENT_PORT = 0\n")},
    {"name": "first_line_wins", "config": "clock = 12\nclock = 24\nchime = on\nchime = off\n"},
    {"name": "clock_invalid_skipped", "config": "clock = bogus\nclock = Off\nclock = 12\n"},
    {"name": "clock_all_invalid", "config": "clock = 7\n"},
    {"name": "config_dirs_list", "config": "config_dirs = ~/.claude, ~/.claude-work  # two plans\n"},
    {"name": "config_dirs_last_wins", "config": "config_dirs = ~/a\nCONFIG_DIRS = ~, ~/b/, rel/dir, ./c\n"},
    {"name": "config_dirs_blank", "config": "config_dirs = , ,\n"},
    {"name": "cliproxy_bare", "config": "cliproxy_url = 127.0.0.1:9999/\n"},
    {"name": "cliproxy_https", "config": "cliproxy_url = https://proxy.example//\n"},
    {"name": "cliproxy_empty", "config": "cliproxy_url =\n"},
    {"name": "line_separators", "config": "clock = 12\rchime = on\r\nevent_port = 5\x0cconfig_dirs=~/x\n"},
    {"name": "http_targets_mixed", "config": "http_targets = b.local, a.local:81, http://c/x\n"},
    {"name": "equals_in_value", "config": "http_targets=a=b\nble = off\n"},
]
VALUE_NAMES = ["clock", "chime", "config_dirs", "http_targets", "cliproxy_url", "event_port",
               "ble", "events", "nope"]
ONOFF = [["chime", "off"], ["ble", "on"], ["events", "off"], ["nope", "on"]]


def read_all() -> dict:
    return {
        "values": {n: usage.read_config_value(n) for n in VALUE_NAMES},
        "onoff": [usage.read_onoff_setting(n, d) for n, d in ONOFF],
        "chime": usage.read_chime_setting(),
        "clock": usage.read_clock_setting(),
        "config_dirs": [pth(d) for d in usage.read_config_dirs()],
        "http_targets": usage.read_http_targets(),
        "cliproxy_url": usage.read_cliproxy_url(),
    }


outputs = []
for case in CONFIG_CASES:
    write_bytes(CONFIG, case["config"])
    outputs.append({"name": case["name"], **read_all()})
write_bytes(CONFIG, None)
h.write_fixture("config", "config_file", ctx=ctx,
                input={"names": VALUE_NAMES, "onoff": ONOFF, "cases": CONFIG_CASES},
                output=outputs)

# ---------------------------------------------------------------- usage_source (desktop.json)

SOURCE_CASES = [None, "{broken", "[1, 2]", "{}"] + [
    json.dumps({"usage_source": v}) for v in
    ["codex", "off", "claude", "claude:/x/.claude-work", "claude:", "gemini", 3, None, "", True]]
outputs = []
for text in SOURCE_CASES:
    write_bytes(PREFS, text)
    outputs.append(usage.read_usage_source())
write_bytes(PREFS, None)
h.write_fixture("config", "usage_source", ctx=ctx, input=SOURCE_CASES, output=outputs)

# ---------------------------------------------------------------- normalize_http_target

TARGETS = ["192.168.0.125", " clawd.local:8080 ", "http://10.0.0.5/", "http://10.0.0.5/custom/push",
           "", "   ", "ftp://host", "HTTPS://Host:8443?x=1#frag", "http://user:pw@10.0.0.5/p?q=1&r=2",
           "http://[::1]:80", "http:///path", "http://:80", "http://[::1", "host/path/", "host?a=b",
           "10.0.0.5:80/x y", "http://h\tost/", "wss://host", "http://HOST.Local", "://x", "1http://x",
           "http://a@/p"]
outputs = []
for raw in TARGETS:
    try:
        outputs.append(usage.normalize_http_target(raw))
    except ValueError:
        outputs.append({"error": "ValueError"})          # Rust: None
h.write_fixture("config", "normalize_http_target", ctx=ctx, input=TARGETS, output=outputs)

# ---------------------------------------------------------------- Anthropic credentials store

STORE_CASES = [
    {"name": "live", "profiles": {"default": json.dumps(cred())}, "active": "default\n"},
    {"name": "active_profile", "profiles": {"work": json.dumps(cred(**{"access_token": "sk-ant-test-WORK"}))},
     "active": "work\n"},
    {"name": "no_active_config", "profiles": {"default": json.dumps(cred())}, "active": None},
    {"name": "blank_active_config", "profiles": {"default": json.dumps(cred())}, "active": "  \n"},
    {"name": "expired_ms", "profiles": {"default": json.dumps(cred(expires_at=int((NOW - 1) * 1000)))},
     "active": "default"},
    {"name": "expired_s", "profiles": {"default": json.dumps(cred(expires_at=int(NOW - 1)))},
     "active": "default"},
    {"name": "future_s", "profiles": {"default": json.dumps(cred(expires_at=int(NOW + 5)))},
     "active": "default"},
    {"name": "expires_now", "profiles": {"default": json.dumps(cred(expires_at=NOW))},
     "active": "default"},
    {"name": "expires_zero", "profiles": {"default": json.dumps(cred(expires_at=0))}, "active": "default"},
    {"name": "expires_negative", "profiles": {"default": json.dumps(cred(expires_at=-5))},
     "active": "default"},
    {"name": "expires_string", "profiles": {"default": json.dumps(cred(expires_at="soon"))},
     "active": "default"},
    {"name": "expires_true", "profiles": {"default": json.dumps(cred(expires_at=True))},
     "active": "default"},
    {"name": "no_expiry", "profiles": {"default": json.dumps({k: v for k, v in cred().items()
                                                              if k != "expires_at"})},
     "active": "default"},
    {"name": "not_oauth", "profiles": {"default": json.dumps(cred(type="api_key"))}, "active": "default"},
    {"name": "blank_token", "profiles": {"default": json.dumps(cred(**{"access_token": ""}))},
     "active": "default"},
    {"name": "spaces_token", "profiles": {"default": json.dumps(cred(**{"access_token": "   "}))},
     "active": "default"},
    {"name": "padded_token", "profiles": {"default": json.dumps(cred(**{"access_token": " sk-ant-test-X \n"}))},
     "active": "default"},
    {"name": "token_not_string", "profiles": {"default": json.dumps(cred(**{"access_token": 5}))},
     "active": "default"},
    {"name": "corrupt", "profiles": {"default": "{not json"}, "active": "default"},
    {"name": "not_an_object", "profiles": {"default": "[1]"}, "active": "default"},
    {"name": "no_profile_file", "profiles": {}, "active": "default"},
    {"name": "profile_escapes", "profiles": {"default": json.dumps(cred())}, "active": "../../etc/x"},
    {"name": "profile_hidden", "profiles": {".hidden": json.dumps(cred())}, "active": ".hidden"},
    {"name": "profile_slash", "profiles": {"default": json.dumps(cred())}, "active": "a/b"},
]


def make_store(root: Path, case: dict) -> None:
    clear(root)
    root.mkdir(parents=True)
    if case["active"] is not None:
        (root / "active_config").write_bytes(case["active"].encode("ascii"))
    for profile, text in case["profiles"].items():
        (root / "credentials").mkdir(exist_ok=True)
        (root / "credentials" / f"{profile}.json").write_bytes(text.encode("ascii"))


outputs = []
scratch = HOME / "store-case"
for case in STORE_CASES:
    make_store(scratch, case)
    outputs.append(usage._read_token_store(scratch, now=NOW))
clear(scratch)
h.write_fixture("config", "token_store", ctx=ctx, input={"now": NOW, "cases": STORE_CASES},
                output=outputs)

# ---------------------------------------------------------------- blob extraction / decode

BLOBS = [
    json.dumps({"accessToken": "sk-ant-test-5678"}, indent=2),
    json.dumps({"claudeAiOauth": {"accessToken": "sk-ant-test-1234", "refreshToken": "sk-ant-fake-5678",
                                  "expiresAt": 9999999999000, "scopes": ["user:inference"]}}),
    '{"a": {"x": 1}, "b": {"accessToken": "B"}, "accessToken": "TOP"}',
    '{"a": {"x": 1}, "b": {"accessToken": "B"}}',
    '{"a": {"b": {"accessToken" :  "DEEP"}}}',
    '{"accessToken": "", "accessToken" : "SECOND", oops',
    '{"accessToken": "  "}',
    '{"accessToken":""}',
    '{"claudeAiOauth":{"accessToken":"","expiresAt":0}}',
    '{"mcpOAuth": {}}',
    '{"accessToken": 5}',
    '  sk-ant-test-abcdefghijkl\n',
    'sk-ant-test-abcdefghijkl',
    'short', 'not a token at all, really no', '', '  \n', '[]', 'null', '"just a string........"',
    '{"accessToken" : "pad ded"}',
    'accessToken: "x"',
    'prefix "accessToken"\n:\n "sk-ant-test-LINES" suffix',
]
h.write_fixture("config", "extract_access_token", ctx=ctx, input=BLOBS,
                output=[usage._extract_access_token(b) for b in BLOBS])

DECODE = [
    '{"claudeAiOauth": {"accessToken": "x"}}',
    hexed('{"claudeAiOauth": {"accessToken": "x"}}\n'),
    hexed('{"claudeAiOauth": {"accessToken": "x"}}\n').upper(),
    hexed('{"a": 1}\n') + "\n",
    "abc", "ff00", "", "  ", "00", "zz", "6162", " 6162 ", "6",
]
h.write_fixture("config", "decode_keychain_blob", ctx=ctx, input=DECODE,
                output=[usage._decode_keychain_blob(b) for b in DECODE])

# ---------------------------------------------------------------- scripted `security` / `defaults`


class Script:
    """Replaces `subprocess.run`: each call is answered from `replies` (keyed by a function of the
    command) and recorded. A reply: {"rc": 0, "stdout": ".."} | {"rc": 44} | {"raises": "..."}."""

    def __init__(self, key, replies):
        self.key, self.replies, self.calls = key, replies, []

    def __call__(self, cmd, **kw):
        self.calls.append(list(cmd))
        reply = self.replies.get(self.key(cmd), {"raises": "FileNotFoundError"})
        kind = reply.get("raises")
        if kind == "FileNotFoundError":
            raise FileNotFoundError(2, "No such file")
        if kind == "TimeoutExpired":
            raise subprocess.TimeoutExpired(cmd, 10)
        if reply.get("rc", 0) != 0:
            if kw.get("check"):
                raise subprocess.CalledProcessError(reply["rc"], cmd, output="", stderr="item not found")
            return SimpleNamespace(returncode=reply["rc"], stdout=reply.get("stdout", ""), stderr="")
        return SimpleNamespace(returncode=0, stdout=reply.get("stdout", ""), stderr="")


class Scripted:
    """`with Scripted(platform, key, replies) as s:` -> `s.calls`."""

    def __init__(self, platform, key, replies):
        self.platform, self.script = platform, Script(key, replies)

    def __enter__(self):
        self.saved = (sys.platform, subprocess.run)
        sys.platform, subprocess.run = self.platform, self.script
        return self.script

    def __exit__(self, *exc):
        sys.platform, subprocess.run = self.saved


# ---------------------------------------------------------------- read_token_for

SECURITY = {"rc": 0, "stdout": blob("sk-ant-test-KEYCHAIN") + "\n"}
TOKEN_FOR_CASES = [
    {"name": "other_dir_file_nested", "platform": "linux", "dir": "other",
     "files": {".credentials.json": blob("sk-ant-test-FILE")}},
    {"name": "other_dir_no_file_linux", "platform": "linux", "dir": "other"},
    {"name": "other_dir_no_file_darwin", "platform": "darwin", "dir": "other", "keychain": SECURITY},
    {"name": "other_dir_ignores_store", "platform": "linux", "dir": "other",
     "store": {"profiles": {"default": json.dumps(cred())}, "active": "default"}},
    {"name": "other_dir_blank_file", "platform": "linux", "dir": "other",
     "files": {".credentials.json": '{"claudeAiOauth":{"accessToken":"","refreshToken":"","expiresAt":0}}'}},
    {"name": "default_keychain_darwin", "platform": "darwin", "dir": "default", "keychain": SECURITY},
    {"name": "default_no_keychain_linux", "platform": "linux", "dir": "default", "keychain": SECURITY},
    {"name": "default_file_wins_over_keychain", "platform": "darwin", "dir": "default",
     "files": {".credentials.json": '{"accessToken": "sk-ant-test-FILE"}'}, "keychain": SECURITY},
    {"name": "default_blank_file_blocks_keychain", "platform": "darwin", "dir": "default",
     "files": {".credentials.json": '{"accessToken": ""}'}, "keychain": SECURITY},
    {"name": "default_raw_token_file", "platform": "linux", "dir": "default",
     "files": {".credentials.json": "sk-ant-test-abcdefghijkl\n"}},
    {"name": "default_store_live_wins", "platform": "darwin", "dir": "default", "keychain": SECURITY,
     "files": {".credentials.json": '{"accessToken": "sk-ant-test-FILE"}'},
     "store": {"profiles": {"default": json.dumps(cred())}, "active": "default"}},
    {"name": "default_store_active_profile", "platform": "linux", "dir": "default",
     "store": {"profiles": {"work": json.dumps(cred(**{"access_token": "sk-ant-test-WORK"}))}, "active": "work"}},
    {"name": "default_store_expired_falls_to_keychain", "platform": "darwin", "dir": "default",
     "keychain": SECURITY,
     "store": {"profiles": {"default": json.dumps(cred(expires_at=1000))}, "active": "default"}},
    {"name": "default_store_expired_linux", "platform": "linux", "dir": "default",
     "store": {"profiles": {"default": json.dumps(cred(expires_at=1000))}, "active": "default"}},
    {"name": "keychain_hex_blob", "platform": "darwin", "dir": "default",
     "keychain": {"rc": 0, "stdout": hexed(blob("sk-ant-test-HEX") + "\n") + "\n"}},
    {"name": "keychain_not_found", "platform": "darwin", "dir": "default", "keychain": {"rc": 44}},
    {"name": "keychain_no_security_binary", "platform": "darwin", "dir": "default",
     "keychain": {"raises": "FileNotFoundError"}},
    {"name": "keychain_timeout", "platform": "darwin", "dir": "default",
     "keychain": {"raises": "TimeoutExpired"}},
    {"name": "keychain_mcp_only", "platform": "darwin", "dir": "default",
     "keychain": {"rc": 0, "stdout": '{"mcpOAuth": {}}\n'}},
    {"name": "keychain_empty", "platform": "darwin", "dir": "default", "keychain": {"rc": 0, "stdout": ""}},
]
OTHER_DIR = HOME / ".claude-work"
outputs = []
for case in TOKEN_FOR_CASES:
    for d in (usage.DEFAULT_CONFIG_DIR, OTHER_DIR, usage.ANTHROPIC_STORE_DIR):
        clear(d)
    target = usage.DEFAULT_CONFIG_DIR if case["dir"] == "default" else OTHER_DIR
    target.mkdir(parents=True)
    for name, text in case.get("files", {}).items():
        (target / name).write_bytes(text.encode("ascii"))
    if "store" in case:
        make_store(usage.ANTHROPIC_STORE_DIR, case["store"])
    reply = {"find-generic-password": case["keychain"]} if "keychain" in case else {}
    with Scripted(case["platform"], lambda cmd: cmd[1], reply) as script:
        token = usage.read_token_for(target)
    outputs.append({"token": token, "commands": script.calls})
for d in (usage.DEFAULT_CONFIG_DIR, OTHER_DIR, usage.ANTHROPIC_STORE_DIR):
    clear(d)
h.write_fixture("config", "token_for", ctx=ctx, input={"user": USER, "cases": TOKEN_FOR_CASES},
                output=outputs)

# ---------------------------------------------------------------- read_cliproxy_key

NEW, OLD = usage.CLIPROXY_KEYCHAIN_SERVICE, usage.CLIPROXY_LEGACY_SERVICE
KEY_CASES = [
    {"name": "new_service_wins", "platform": "darwin",
     "replies": {NEW: {"rc": 0, "stdout": "new-key\n"}, OLD: {"rc": 0, "stdout": "old-key\n"}}},
    {"name": "legacy_fallback", "platform": "darwin",
     "replies": {NEW: {"rc": 44}, OLD: {"rc": 0, "stdout": "old-key\n"}}},
    {"name": "none_in_either", "platform": "darwin", "replies": {NEW: {"rc": 44}, OLD: {"rc": 44}}},
    {"name": "hex_secret", "platform": "darwin", "replies": {NEW: {"rc": 0, "stdout": hexed("hex-key\n")}}},
    {"name": "blank_then_legacy", "platform": "darwin",
     "replies": {NEW: {"rc": 0, "stdout": "  \n"}, OLD: {"rc": 0, "stdout": "old-key"}}},
    {"name": "security_missing", "platform": "darwin",
     "replies": {NEW: {"raises": "FileNotFoundError"}, OLD: {"rc": 0, "stdout": "old-key\n"}}},
    {"name": "security_timeout", "platform": "darwin",
     "replies": {NEW: {"raises": "TimeoutExpired"}, OLD: {"rc": 0, "stdout": "old-key\n"}}},
    {"name": "not_macos", "platform": "linux", "replies": {NEW: {"rc": 0, "stdout": "new-key\n"}}},
]
outputs = []
for case in KEY_CASES:
    with Scripted(case["platform"], lambda cmd: cmd[3], case["replies"]) as script:
        key = usage.read_cliproxy_key()
    outputs.append({"key": key, "commands": script.calls})
h.write_fixture("config", "cliproxy_key", ctx=ctx, input={"cases": KEY_CASES}, output=outputs)

# ---------------------------------------------------------------- detect_hour_format

K24, K12 = "AppleICUForce24HourTime", "AppleICUForce12HourTime"
HOUR_CASES = [
    {"name": "force_24", "replies": {K24: {"rc": 0, "stdout": "1\n"}}},
    {"name": "force_12", "replies": {K24: {"rc": 0, "stdout": "0\n"}, K12: {"rc": 0, "stdout": "1\n"}}},
    {"name": "both_one_24_first", "replies": {K24: {"rc": 0, "stdout": "1\n"}, K12: {"rc": 0, "stdout": "1\n"}}},
    {"name": "defaults_missing", "replies": {}},
    {"name": "defaults_hangs", "replies": {K24: {"raises": "TimeoutExpired"}, K12: {"rc": 0, "stdout": "1"}}},
    {"name": "keys_absent_nonzero_exit", "replies": {K24: {"rc": 1}, K12: {"rc": 1}}},
]
outputs = []
for case in HOUR_CASES:
    # (platform stays real: only the `defaults` answers decide; with none of them printing 1 the
    # result is Python's locale fallback, 24 on Windows where `nl_langinfo` does not exist)
    saved = subprocess.run
    script = Script(lambda cmd: cmd[3], case["replies"])
    subprocess.run = script
    try:
        value = usage.detect_hour_format()
    finally:
        subprocess.run = saved
    outputs.append({"hour_format": value, "commands": script.calls})
h.write_fixture("config", "hour_format", ctx=ctx, input={"cases": HOUR_CASES}, output=outputs)

print("export_config: ok")
