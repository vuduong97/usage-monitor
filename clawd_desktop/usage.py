#!/usr/bin/env python3
"""Claude Usage Tracker Daemon (BLE) — macOS port of claude-usage-daemon.sh.

Polls Claude API rate-limit headers and writes a JSON payload to the
ESP32 "Clawdmeter" peripheral over a custom GATT service. Uses
bleak (CoreBluetooth backend on macOS).

The same payload can also be pushed over WiFi: every `http_targets` entry in
the config gets an HTTP POST of the JSON (e.g. a SmallTV running the
smalltv-clawdmeter firmware, which has no Bluetooth). One poll feeds every
destination, so adding a WiFi screen does not add API calls.
"""

import asyncio
import calendar
import datetime
import getpass
import json
import os
import re
import shutil
import signal
import subprocess
import sys
import time
import unicodedata
import urllib.parse
from pathlib import Path
from typing import Callable

import httpx

try:                       # Bluetooth is optional: the desktop app only pushes over WiFi
    from bleak import BleakClient
    from bleak.exc import BleakError
except ImportError:        # pragma: no cover - exercised by the packaged app
    BleakClient = None     # type: ignore[assignment,misc]

    class BleakError(Exception):  # type: ignore[no-redef]
        """Stand-in so the BLE code paths still parse without bleak."""

from clawd_desktop import events as clawd_events

DEVICE_NAME = "Clawdmeter"
SERVICE_UUID = "4c41555a-4465-7669-6365-000000000001"
RX_CHAR_UUID = "4c41555a-4465-7669-6365-000000000002"
REQ_CHAR_UUID = "4c41555a-4465-7669-6365-000000000004"

POLL_INTERVAL = 60
TICK = 5
CONNECT_TIMEOUT = 20.0

# macOS: token lives in Keychain (service "Claude Code-credentials").
# Linux: token lives in ~/.claude/.credentials.json.
KEYCHAIN_SERVICE = "Claude Code-credentials"
DEFAULT_CONFIG_DIR = Path.home() / ".claude"
# Claude Code 2.1.29x+ ("storage v5") keeps the default login in a shared
# Anthropic credentials store instead: <dir>/active_config names the profile and
# <dir>/credentials/<profile>.json holds {"type": "oauth_token",
# "access_token", "expires_at" (ms), ...}. The Keychain item is then left with
# only MCP state, so it must be checked first.
ANTHROPIC_STORE_DIR = Path.home() / ".config" / "anthropic"
SAVED_ADDR_FILE = Path.home() / ".config" / "claude-usage-monitor" / "ble-address"
CONFIG_FILE = Path.home() / ".config" / "claude-usage-monitor" / "config"
# Written by Claude Code hooks: `attention` holds {"m", "s"} (message, session
# id) when a session needs the user, and `clear-<session id>` is touched when
# that session moves on.
ATTENTION_DIR = Path.home() / ".config" / "claude-usage-monitor"
ATTENTION_FLAG = ATTENTION_DIR / "attention"
ATTENTION_MAX_AGE = 60
DEFAULT_MAX_WRITE = 180

# WiFi push (`http_targets`): path appended to a bare host, and the per-request
# bound so an offline screen never stalls the BLE device sharing the poll.
HTTP_PUSH_PATH = "/api/usage"
HTTP_PUSH_TIMEOUT = 5.0
RESEND_TIMEOUT = 15.0     # a clock that just booted answers HTTP slowly for a while

API_URL = "https://api.anthropic.com/v1/messages"
API_HEADERS_TEMPLATE = {
    "anthropic-version": "2023-06-01",
    "anthropic-beta": "oauth-2025-04-20",
    "Content-Type": "application/json",
    "User-Agent": "claude-code/2.1.5",
}
API_BODY = {
    "model": "claude-haiku-4-5-20251001",
    "max_tokens": 1,
    "messages": [{"role": "user", "content": "hi"}],
}


# Read-only usage endpoint (what Claude Code's /usage shows). Costs no quota,
# unlike the Haiku probe above, which stays as the fallback for accounts where
# it reports nothing usable (Enterprise spend limits, API organizations).
USAGE_URL = "https://api.anthropic.com/api/oauth/usage"
USAGE_HEADERS_TEMPLATE = {
    "anthropic-beta": "oauth-2025-04-20",
    "Content-Type": "application/json",
    "User-Agent": "claude-code/2.1.5",
}

# CLIProxyAPI (https://github.com/router-for-me/CLIProxyAPI) as a usage source:
# the daemon asks the proxy's management API to make the usage request with an
# account's token (`$TOKEN$` is substituted proxy-side), so the Claude token
# never leaves the proxy. The management key lives only in the macOS Keychain.
CLIPROXY_DEFAULT_URL = "http://localhost:8317"
CLIPROXY_KEYCHAIN_SERVICE = "clawdmeter-cliproxy"
CLIPROXY_AUTH_FILES_PATH = "/v0/management/auth-files"
CLIPROXY_API_CALL_PATH = "/v8/management/requests/api-call"
CLIPROXY_ACCOUNTS_TTL = 300      # re-list proxy accounts every 5 minutes
CLIPROXY_TIMEOUT = 20.0


class UsageNotApplicable(Exception):
    """The usage endpoint has no 5h/7d windows for this account (API
    organization, Enterprise spend limit, endpoint gone): use the fallback."""


class TokenExpired(Exception):
    """Raised by poll_api on a 401/403 — the access token is dead. The daemon never
    refreshes (pure free-ride: Claude Code owns refreshing), so the caller just
    signals "No data" to the device until the CLI re-seeds the token."""


def log(msg: str) -> None:
    print(f"[{time.strftime('%H:%M:%S')}] {msg}", flush=True)


def _extract_access_token(blob: str) -> str | None:
    """Pull the accessToken out of a credentials blob.

    Claude Code stores credentials as a JSON object; the blob may also be
    nested ({"claudeAiOauth": {"accessToken": "..."}}). Fall back to a
    regex match so unexpected shapes still work, and finally treat the
    blob as a raw token if nothing else matches.
    """
    blob = blob.strip()
    if not blob:
        return None
    try:
        data = json.loads(blob)
    except json.JSONDecodeError:
        data = None
    if isinstance(data, dict):
        # direct: {"accessToken": "..."}
        tok = data.get("accessToken")
        if isinstance(tok, str) and tok.strip():
            return tok
        # nested: {"claudeAiOauth": {"accessToken": "..."}}
        for v in data.values():
            if isinstance(v, dict):
                tok = v.get("accessToken")
                if isinstance(tok, str) and tok.strip():
                    return tok
    m = re.search(r'"accessToken"\s*:\s*"([^"]+)"', blob)
    if m:
        return m.group(1)
    # Raw token (no JSON wrapper) — must look plausible (sk-ant-... etc.)
    if re.fullmatch(r"[A-Za-z0-9_\-.~+/=]{20,}", blob):
        return blob
    return None


def _decode_keychain_blob(raw: str) -> str:
    """Transparently decode a hex-dumped Keychain secret back to text.

    ``security … -w`` prints the password as a continuous hex string whenever
    the stored bytes aren't cleanly printable (e.g. an embedded newline). A
    normal credentials blob is JSON, which is never valid hex (it contains
    '{', '"', …), so all-hex detection is unambiguous and safe.
    """
    s = raw.strip()
    if s and len(s) % 2 == 0 and re.fullmatch(r"[0-9a-fA-F]+", s):
        try:
            return bytes.fromhex(s).decode("utf-8")
        except (ValueError, UnicodeDecodeError):
            return raw
    return raw


def _read_token_keychain() -> str | None:
    """Read the OAuth access token from the macOS Keychain, or None.

    ``security … -w`` may hex-dump the stored secret (see _decode_keychain_blob),
    so decode before extracting the access token.
    """
    try:
        out = subprocess.run(
            [
                "security",
                "find-generic-password",
                "-s",
                KEYCHAIN_SERVICE,
                "-a",
                getpass.getuser(),
                "-w",
            ],
            check=True,
            capture_output=True,
            text=True,
            timeout=10,
        )
    except subprocess.CalledProcessError as e:
        log(f"Keychain read failed (rc={e.returncode}): {e.stderr.strip()}")
        return None
    except (FileNotFoundError, subprocess.TimeoutExpired) as e:
        log(f"Keychain access error: {e}")
        return None
    return _extract_access_token(_decode_keychain_blob(out.stdout))


def read_config_dirs() -> list[Path]:
    """Claude config dirs to poll, from the `config_dirs` option (comma list).

    Defaults to [~/.claude] so existing single-plan setups are unchanged. ~ is
    expanded. Mirrors the Linux bash daemon's read_config_dirs.
    """
    raw = ""
    try:
        if CONFIG_FILE.exists():
            for line in CONFIG_FILE.read_text().splitlines():
                line = line.split("#", 1)[0].strip()
                if "=" not in line:
                    continue
                key, val = line.split("=", 1)
                if key.strip().lower() == "config_dirs":
                    raw = val.strip()
    except OSError:
        pass
    if not raw:
        return [DEFAULT_CONFIG_DIR]
    dirs = [Path(p.strip()).expanduser() for p in raw.split(",") if p.strip()]
    return dirs or [DEFAULT_CONFIG_DIR]


def read_token_for(config_dir: Path) -> str | None:
    """Read the OAuth token for one config dir.

    Linux: each dir keeps its own ``<dir>/.credentials.json``. macOS: the default
    install stores the token in Keychain with no file, so for the default dir we
    fall back to Keychain when no file is present — preserving existing
    single-plan macOS behavior. Additional macOS dirs are read from their files;
    a work plan whose token lives only in the single Keychain entry can't be told
    apart there (documented follow-up). For the default dir, a live token in the
    Anthropic credentials store (newer Claude Code) wins over both.
    """
    if config_dir == DEFAULT_CONFIG_DIR:
        tok = _read_token_store()
        if tok:
            return tok
    cred = config_dir / ".credentials.json"
    try:
        if cred.exists():
            return _extract_access_token(cred.read_text())
    except OSError as e:
        log(f"Error reading credentials in {config_dir}: {e}")
    if sys.platform == "darwin" and config_dir == DEFAULT_CONFIG_DIR:
        return _read_token_keychain()
    return None


def _read_token_store(store: Path | None = None, now: float | None = None) -> str | None:
    """Unexpired OAuth access token from the Anthropic credentials store, or None.

    Expired or non-OAuth entries return None so older storage (file/Keychain)
    still gets a chance; Claude Code refreshes this file whenever it runs.
    """
    store = store or ANTHROPIC_STORE_DIR
    try:
        profile = (store / "active_config").read_text().strip() or "default"
    except OSError:
        profile = "default"
    if "/" in profile or profile.startswith("."):
        return None
    try:
        data = json.loads((store / "credentials" / f"{profile}.json").read_text())
    except (OSError, ValueError):
        return None
    if not isinstance(data, dict) or data.get("type") != "oauth_token":
        return None
    tok = data.get("access_token")
    if not isinstance(tok, str) or not tok.strip():
        return None
    exp = data.get("expires_at")
    if isinstance(exp, (int, float)) and exp > 0:
        exp_s = exp / 1000 if exp > 1e11 else exp
        if exp_s <= (now if now is not None else time.time()):
            return None
    return tok.strip()


def load_cached_address() -> str | None:
    if not SAVED_ADDR_FILE.exists():
        return None
    addr = SAVED_ADDR_FILE.read_text().strip()
    # Accept both Linux MAC (AA:BB:CC:DD:EE:FF) and macOS CoreBluetooth UUID
    # (E621E1F8-C36C-495A-93FC-0C247A3E6E5F).
    if re.fullmatch(r"(?:[0-9A-Fa-f]{2}:){5}[0-9A-Fa-f]{2}", addr) or re.fullmatch(
        r"[0-9A-Fa-f]{8}-(?:[0-9A-Fa-f]{4}-){3}[0-9A-Fa-f]{12}", addr
    ):
        return addr
    log("Cached address malformed, discarding")
    SAVED_ADDR_FILE.unlink(missing_ok=True)
    return None


# --- macOS: recover a device the OS already holds as an HID keyboard --------
#
# The firmware advertises as a BLE HID keyboard so its buttons type into the
# Mac. macOS auto-connects to that HID, and CoreBluetooth then EXCLUDES the
# peripheral from BleakScanner.discover() results (already-connected devices
# never appear in scans). bleak's connect-by-address path also scans
# internally, so a cached address can't help either. The documented escape
# hatch is retrieveConnectedPeripheralsWithServices_, which returns
# peripherals the system is already connected to. We wrap the result in a
# BLEDevice carrying the live (peripheral, manager) details so BleakClient
# connects to it directly without scanning. CoreBluetooth shares the single
# physical link, so this rides the existing HID connection — the keyboard
# keeps working.
_cb_manager = None  # reused CentralManagerDelegate (CoreBluetooth)


async def _get_cb_manager():
    """Lazily create and ready a shared CoreBluetooth central manager."""
    global _cb_manager
    if _cb_manager is None:
        from bleak.backends.corebluetooth.CentralManagerDelegate import (
            CentralManagerDelegate,
        )

        mgr = CentralManagerDelegate()
        await mgr.wait_until_ready()  # raises if Bluetooth is unauthorized/off
        _cb_manager = mgr
    return _cb_manager


async def retrieve_connected_macos(skip_addr: str | None = None):
    """Return a BLEDevice for a system-connected 'Clawdmeter', or None.

    Two-step lookup, strongest signal first:

    1. Peripherals connected under our CUSTOM service UUID. Membership in
       that service is unambiguous (no other device exposes it), so we accept
       by service alone — the peripheral's name can be None on macOS.
    2. Fall back to the generic HID service 0x1812, but ONLY trust a
       peripheral whose name matches DEVICE_NAME. 0x1812 also matches
       unrelated keyboards/mice, so picking blindly here could grab the
       wrong device.

    ``skip_addr`` skips a peripheral whose UUID just failed to connect, so a
    stale CoreBluetooth handle can't trap us into never trying a fresh scan.
    """
    from CoreBluetooth import CBUUID
    from bleak.backends.device import BLEDevice

    try:
        manager = await _get_cb_manager()
    except Exception as e:  # BleakBluetoothNotAvailableError etc.
        log(f"CoreBluetooth unavailable: {e}")
        return None

    cm = manager.central_manager

    def _wrap(p):
        addr = p.identifier().UUIDString()
        log(f"Found system-connected peripheral: {p.name()!r} [{addr}]")
        return BLEDevice(addr, p.name(), (p, manager))

    def _ok(p) -> bool:
        return not (skip_addr and p.identifier().UUIDString() == skip_addr)

    # 1. Custom service — accept by service membership alone.
    custom = cm.retrieveConnectedPeripheralsWithServices_(
        [CBUUID.UUIDWithString_(SERVICE_UUID)]
    )
    for p in custom or []:
        if _ok(p):
            return _wrap(p)

    # 2. Generic HID service — require an exact name match.
    hid = cm.retrieveConnectedPeripheralsWithServices_(
        [CBUUID.UUIDWithString_("1812")]
    )
    for p in hid or []:
        if _ok(p) and p.name() == DEVICE_NAME:
            return _wrap(p)

    return None


async def discover_target(skip_addr: str | None = None):
    """Return a connectable target, or None.

    The daemon only ever targets the device this system already holds — it
    never scans for a nearby device by name, so it can't grab a stranger's or
    the wrong nearby unit. On macOS that's the system-connected peripheral (the
    firmware advertises as an HID keyboard, so once paired the OS auto-connects
    and holds it — HID-grabbed devices are invisible to scans anyway). On other
    platforms it's a previously-pinned address in the cache file. If the device
    isn't held/pinned, we log and wait rather than scanning. ``skip_addr`` skips
    a peripheral whose handle just failed to connect.
    """
    if sys.platform == "darwin":
        dev = await retrieve_connected_macos(skip_addr=skip_addr)
        if dev is None:
            log("Device not held by OS; waiting (not scanning by name)")
        return dev

    address = load_cached_address()
    if not address:
        log("No pinned address cached; waiting (not scanning by name)")
    return address


def read_config_value(name: str) -> str | None:
    """Raw value of `name = value` in the config file (comments stripped), or None."""
    try:
        if CONFIG_FILE.exists():
            for line in CONFIG_FILE.read_text().splitlines():
                line = line.split("#", 1)[0].strip()
                if "=" not in line:
                    continue
                key, val = line.split("=", 1)
                if key.strip().lower() == name:
                    return val.strip()
    except OSError:
        pass
    return None


def read_onoff_setting(name: str, default: str = "off") -> str:
    """Read an off|on option from the config file.

    Defaults to "off" (the device stays silent) so existing setups are
    unaffected until the user opts in.
    """
    val = (read_config_value(name) or "").lower()
    return val if val in ("off", "on") else default


def normalize_http_target(raw: str) -> str | None:
    """Turn an `http_targets` entry into a push URL, or None if unusable.

    A bare host or IP (`192.168.0.125`, `smalltv.local:80`) becomes
    `http://<host>/api/usage`; a full URL keeps its own path.
    """
    raw = raw.strip()
    if not raw:
        return None
    if "://" not in raw:
        raw = "http://" + raw
    parts = urllib.parse.urlsplit(raw)
    if parts.scheme not in ("http", "https") or not parts.hostname:
        return None
    path = parts.path if parts.path not in ("", "/") else HTTP_PUSH_PATH
    return urllib.parse.urlunsplit((parts.scheme, parts.netloc, path, parts.query, ""))


def read_http_targets() -> list[str]:
    """Push URLs from the comma-separated `http_targets` option (empty = WiFi push off)."""
    targets = []
    for raw in (read_config_value("http_targets") or "").split(","):
        url = normalize_http_target(raw)
        if url is None:
            if raw.strip():
                log(f"Ignoring invalid http_targets entry: {raw.strip()!r}")
            continue
        if url not in targets:
            targets.append(url)
    return targets


def read_chime_setting() -> str:
    """Read the `chime` option (session-reset chime). One of: off|on."""
    return read_onoff_setting("chime")


def read_clock_setting() -> str:
    """Read the `clock` option from the config file. One of: off|auto|12|24.

    Defaults to "off" (no clock; the device keeps showing "Usage") so existing
    setups are unaffected until the user opts in.
    """
    try:
        if CONFIG_FILE.exists():
            for line in CONFIG_FILE.read_text().splitlines():
                line = line.split("#", 1)[0].strip()
                if "=" not in line:
                    continue
                key, val = line.split("=", 1)
                if key.strip().lower() == "clock":
                    val = val.strip().lower()
                    if val in ("off", "auto", "12", "24"):
                        return val
    except OSError:
        pass
    return "off"


def add_chime_field(payload: dict) -> None:
    """Add "c":1 to the payload when the config opts in, so the firmware may
    sound the session-reset chime. Omitted entirely when chime is off."""
    if read_chime_setting() == "on":
        payload["c"] = 1


def detect_hour_format() -> int:
    """Best-effort 12h/24h detection for the host. Returns 12 or 24 (default 24)."""
    # macOS: the explicit System Settings toggle lives in NSGlobalDomain.
    for key, result in (("AppleICUForce24HourTime", 24), ("AppleICUForce12HourTime", 12)):
        try:
            out = subprocess.run(["defaults", "read", "-g", key],
                                 capture_output=True, text=True, timeout=3)
            if out.stdout.strip() == "1":
                return result
        except (OSError, subprocess.SubprocessError):
            pass
    # Fallback to the C locale's time format (may be C/24h under launchd).
    try:
        import locale
        locale.setlocale(locale.LC_TIME, "")
        fmt = locale.nl_langinfo(locale.T_FMT)
        if "%p" in fmt or "%r" in fmt or "%I" in fmt:
            return 12
    except (ImportError, locale.Error, AttributeError):
        pass
    return 24


def add_clock_fields(payload: dict) -> None:
    """Add wall-clock fields to the payload when the config opts in.

    "t"  = local wall-clock epoch (UTC epoch shifted by the tz offset) so the
           device can show the time without an RTC.
    "tf" = 12 or 24, the hour format the device should render.
    """
    clock = read_clock_setting()
    if clock == "off":
        return
    tf = 24 if clock == "24" else 12 if clock == "12" else detect_hour_format()
    payload["t"] = int(time.time()) + time.localtime().tm_gmtoff
    payload["tf"] = tf


async def poll_api(token: str) -> dict | None:
    headers = dict(API_HEADERS_TEMPLATE)
    headers["Authorization"] = f"Bearer {token}"
    try:
        async with httpx.AsyncClient(timeout=20.0) as http:
            resp = await http.post(API_URL, headers=headers, json=API_BODY)
    except httpx.HTTPError as e:
        log(f"API call failed: {e}")
        return None
    if resp.status_code in (401, 403):
        log(f"API HTTP {resp.status_code} (token expired/invalid)")
        raise TokenExpired()
    if resp.status_code >= 400:
        log(f"API HTTP {resp.status_code}: {resp.text[:200]}")
        return None

    def hdr(name: str, default: str = "0") -> str:
        return resp.headers.get(name, default)

    now = time.time()

    def reset_minutes(reset_ts: str) -> int:
        try:
            r = float(reset_ts)
        except ValueError:
            return 0
        mins = (r - now) / 60.0
        return int(round(mins)) if mins > 0 else 0

    def pct(util: str) -> int:
        try:
            return int(round(float(util) * 100))
        except ValueError:
            return 0

    # Pro/Max accounts expose 5h/7d windows; Enterprise/overage use a single
    # spending-limit model reported via overage-utilization.
    if resp.headers.get("anthropic-ratelimit-unified-5h-utilization"):
        payload = {
            "s": pct(hdr("anthropic-ratelimit-unified-5h-utilization")),
            "sr": reset_minutes(hdr("anthropic-ratelimit-unified-5h-reset")),
            "w": pct(hdr("anthropic-ratelimit-unified-7d-utilization")),
            "wr": reset_minutes(hdr("anthropic-ratelimit-unified-7d-reset")),
            "st": hdr("anthropic-ratelimit-unified-5h-status", "unknown"),
            "acct": "pro",
            "ok": True,
        }
    else:
        reset_ts = hdr("anthropic-ratelimit-unified-overage-reset")
        payload = {
            "s": pct(hdr("anthropic-ratelimit-unified-overage-utilization")),
            "sr": reset_minutes(reset_ts),
            "w": 0,
            "wr": 0,
            "st": hdr("anthropic-ratelimit-unified-status", "unknown"),
            "acct": "ent",
            **_billing_period_info(now, reset_ts),
            "ok": True,
        }
    add_chime_field(payload)   # adds "c":1 iff the config opts in
    add_clock_fields(payload)   # adds "t" + "tf" iff the config opts in
    return payload


def _billing_period_info(now: float, reset_ts: str) -> dict:
    """Fraction of billing period elapsed (tp, 0-100) and period length in days (pd).

    Billing periods are assumed calendar-monthly: period_end is the reset
    timestamp, period_start is the same day/time one calendar month earlier.

    The rate-limit headers expose only the reset timestamp, not the period
    length, so the monthly window is an assumption — but a documented one:
    Enterprise spend-limit `period` "the only value today is monthly"
    (Claude Enterprise Admin API reference). The doc notes period is an open
    string that may gain other values later; revisit this if so.
    """
    try:
        period_end = float(reset_ts)
    except ValueError:
        return {"tp": 0, "pd": 30}
    if period_end <= 0:
        # reset_ts defaults to "0" when the overage-reset header is absent.
        # fromtimestamp(0) is 1970; stepping a month back lands in 1969, and
        # datetime.timestamp() raises OSError for pre-1970 dates on Windows.
        # Benign on macOS/Linux, but guard here too to keep the daemons parallel.
        return {"tp": 0, "pd": 30}
    dt_end = datetime.datetime.fromtimestamp(period_end)
    prev_month = dt_end.month - 1 or 12
    prev_year = dt_end.year if dt_end.month > 1 else dt_end.year - 1
    prev_day = min(dt_end.day, calendar.monthrange(prev_year, prev_month)[1])
    dt_start = dt_end.replace(year=prev_year, month=prev_month, day=prev_day)
    period_start = dt_start.timestamp()
    period_len = period_end - period_start
    if period_len <= 0:
        return {"tp": 0, "pd": 30}
    pct_val = (now - period_start) / period_len * 100
    total_days = int(round(period_len / 86400))
    rd = f"{dt_end.strftime('%b')} {dt_end.day}"
    return {
        "tp": max(0, min(100, int(round(pct_val)))),
        "pd": total_days,
        "rd": rd,
    }


def _minutes_until(iso_ts, now: float) -> int:
    """Whole minutes from `now` until an ISO-8601 timestamp (0 if past/invalid)."""
    if not isinstance(iso_ts, str) or not iso_ts:
        return 0
    try:
        ts = datetime.datetime.fromisoformat(iso_ts.replace("Z", "+00:00")).timestamp()
    except ValueError:
        return 0
    mins = (ts - now) / 60.0
    return int(round(mins)) if mins > 0 else 0


def _window_pct(window: dict | None) -> int:
    try:
        return max(0, min(100, int(round(float((window or {}).get("utilization") or 0)))))
    except (TypeError, ValueError):
        return 0


def usage_from_oauth_usage(body, now: float | None = None) -> dict:
    """Map an /api/oauth/usage response to the device payload (s sr w wr st acct ok).

    Raises UsageNotApplicable when the response carries no 5h window, so the
    caller can fall back to the rate-limit-header probe.
    """
    if not isinstance(body, dict):
        raise UsageNotApplicable("not a JSON object")
    five = body.get("five_hour")
    if not isinstance(five, dict) or five.get("utilization") is None:
        raise UsageNotApplicable("no five_hour window")
    week = body.get("seven_day") if isinstance(body.get("seven_day"), dict) else None
    now = time.time() if now is None else now
    status = "allowed"
    if five.get("locked_reason") or (week or {}).get("locked_reason"):
        status = "rejected"
    else:
        for lim in body.get("limits") or []:
            if (isinstance(lim, dict) and lim.get("kind") in ("session", "weekly")
                    and lim.get("severity") not in (None, "normal")):
                status = "allowed_warning"
                break
    return {
        "s": _window_pct(five),
        "sr": _minutes_until(five.get("resets_at"), now),
        "w": _window_pct(week),
        "wr": _minutes_until((week or {}).get("resets_at"), now),
        "st": status,
        "acct": "pro",
        "ok": True,
    }


def _usage_result(status: int, text: str) -> dict | None:
    """Shared handling of a usage-endpoint answer, direct or via the proxy.

    401/403 -> TokenExpired; 404 or a "not applicable" 400 -> UsageNotApplicable;
    other errors -> None (try again next poll); 200 -> device payload.
    """
    if status in (401, 403):
        raise TokenExpired()
    if status == 404 or (status == 400 and "not applicable" in text.lower()):
        raise UsageNotApplicable(f"HTTP {status}")
    if status >= 400:
        log(f"Usage endpoint HTTP {status}: {text[:200]}")
        return None
    try:
        body = json.loads(text)
    except ValueError:
        log("Usage endpoint returned non-JSON")
        return None
    payload = usage_from_oauth_usage(body)
    add_chime_field(payload)
    add_clock_fields(payload)
    return payload


async def poll_usage_endpoint(token: str) -> dict | None:
    """Usage for a Claude Code token via the read-only /api/oauth/usage."""
    headers = dict(USAGE_HEADERS_TEMPLATE)
    headers["Authorization"] = f"Bearer {token}"
    try:
        async with httpx.AsyncClient(timeout=20.0) as http:
            resp = await http.get(USAGE_URL, headers=headers)
    except httpx.HTTPError as e:
        log(f"Usage endpoint call failed: {e}")
        return None
    return _usage_result(resp.status_code, resp.text)


async def poll_token(token: str) -> dict | None:
    """Payload for one Claude Code token: the usage endpoint first, the Haiku
    rate-limit-header probe (poll_api) only when the endpoint has no 5h/7d
    windows for this account."""
    try:
        return await poll_usage_endpoint(token)
    except UsageNotApplicable as e:
        log(f"Usage endpoint not applicable ({e}); using the rate-limit headers")
        return await poll_api(token)


def read_cliproxy_key() -> str | None:
    """The CLIProxyAPI management key from the Keychain, or None."""
    if sys.platform != "darwin":
        return None
    try:
        out = subprocess.run(
            ["security", "find-generic-password", "-s", CLIPROXY_KEYCHAIN_SERVICE, "-w"],
            check=True, capture_output=True, text=True, timeout=10,
        )
    except subprocess.CalledProcessError:
        return None
    except (FileNotFoundError, subprocess.TimeoutExpired) as e:
        log(f"cliproxy: Keychain access error: {e}")
        return None
    key = _decode_keychain_blob(out.stdout).strip()
    return key or None


def read_cliproxy_url() -> str:
    url = (read_config_value("cliproxy_url") or CLIPROXY_DEFAULT_URL).strip().rstrip("/")
    return url if url.startswith(("http://", "https://")) else "http://" + url


class CliProxySource:
    """Every active Claude account in CLIProxyAPI, each polled as its own plan.

    Plan keys are "cliproxy:<auth index>". Proxy-level failures (not running,
    wrong key, timeout) make the whole source yield nothing this cycle; a
    401/403 for one account makes only that account dead.
    """

    def __init__(self, transport: httpx.AsyncBaseTransport | None = None) -> None:
        self._transport = transport   # tests inject httpx.MockTransport
        self._problem = ""            # last source-level failure, logged once
        self._backoff: dict[str, float] = {}   # auth index -> retry time
        self._accounts: list[str] = []
        self._listed_at = 0.0
        self._listed_url = ""

    def _report(self, problem: str) -> None:
        """Log a source-level failure once, not on every 5 s retry tick."""
        if problem and problem != self._problem:
            log(problem)
        self._problem = problem

    async def _list_accounts(self, http: httpx.AsyncClient, url: str, key: str) -> list[str] | None:
        now = time.time()
        if url == self._listed_url and now - self._listed_at < CLIPROXY_ACCOUNTS_TTL:
            return self._accounts
        try:
            resp = await http.get(url + CLIPROXY_AUTH_FILES_PATH,
                                  headers={"Authorization": f"Bearer {key}"})
        except httpx.HTTPError as e:
            self._report(f"cliproxy: {url} unreachable ({type(e).__name__})")
            return None
        if resp.status_code != 200:
            self._report(f"cliproxy: listing accounts failed (HTTP {resp.status_code}"
                         f"{', check the management key' if resp.status_code in (401, 403) else ''})")
            return None
        try:
            data = resp.json()
        except ValueError:
            self._report("cliproxy: account list is not JSON")
            return None
        files = data.get("files") if isinstance(data, dict) else data
        accounts = []
        for f in files if isinstance(files, list) else []:
            if not isinstance(f, dict):
                continue
            if (f.get("provider") or f.get("type")) != "claude":
                continue
            if f.get("disabled") or f.get("unavailable"):
                continue
            idx = f.get("auth_index") or f.get("authIndex")
            if isinstance(idx, str) and idx:
                accounts.append(idx)
        if accounts != self._accounts or url != self._listed_url:
            log(f"cliproxy: {len(accounts)} claude account(s)")
        self._accounts, self._listed_at, self._listed_url = accounts, now, url
        return accounts

    async def _poll_account(self, http: httpx.AsyncClient, url: str, key: str, idx: str) -> dict | None:
        req = {
            "authIndex": idx,
            "method": "GET",
            "url": USAGE_URL,
            "header": {**USAGE_HEADERS_TEMPLATE, "Authorization": "Bearer $TOKEN$"},
        }
        try:
            resp = await http.post(url + CLIPROXY_API_CALL_PATH, json=req,
                                   headers={"Authorization": f"Bearer {key}"})
        except httpx.HTTPError as e:
            log(f"cliproxy:{idx[:8]} call failed ({type(e).__name__})")
            return None
        if resp.status_code != 200:
            log(f"cliproxy:{idx[:8]} api-call HTTP {resp.status_code}")
            if resp.status_code in (401, 403):     # management key rejected
                self._listed_at = 0.0
            return None
        try:
            wrap = resp.json()
        except ValueError:
            log(f"cliproxy:{idx[:8]} api-call returned non-JSON")
            return None
        body = wrap.get("body") if isinstance(wrap, dict) else None
        if not isinstance(body, str):
            body = json.dumps(body) if body is not None else ""
        try:
            status = int(wrap.get("status_code") or wrap.get("status") or 0)
        except (TypeError, ValueError):
            status = 0
        if status == 0:
            log(f"cliproxy:{idx[:8]} api-call without an upstream status")
            return None
        return _usage_result(status, body)

    async def poll(self) -> tuple[dict[str, dict], bool]:
        """({plan key: payload}, any_live) for this cycle."""
        key = read_cliproxy_key()
        if not key:
            self._report(f"cliproxy: no management key in Keychain (service {CLIPROXY_KEYCHAIN_SERVICE})")
            return {}, False
        url = read_cliproxy_url()
        payloads: dict[str, dict] = {}
        any_live = False
        async with httpx.AsyncClient(timeout=CLIPROXY_TIMEOUT, transport=self._transport) as http:
            accounts = await self._list_accounts(http, url, key)
            if accounts is not None:
                self._report("")
            now = time.time()
            for idx in accounts or []:
                plan = f"cliproxy:{idx[:8]}"
                if self._backoff.get(idx, 0) > now:
                    any_live = True      # answered recently, just not usefully
                    continue
                try:
                    payload = await self._poll_account(http, url, key, idx)
                except TokenExpired:
                    log(f"{plan}: token expired/invalid in the proxy; skipping")
                    continue
                except UsageNotApplicable as e:
                    log(f"{plan}: usage not applicable ({e}); skipping")
                    self._backoff[idx] = now + CLIPROXY_ACCOUNTS_TTL
                    any_live = True
                    continue
                any_live = True
                if payload is None:
                    self._backoff[idx] = now + POLL_INTERVAL
                else:
                    self._backoff.pop(idx, None)
                    payloads[plan] = payload
                    log(f"{plan} s={payload['s']} w={payload['w']}")
        return payloads, any_live


_CLIPROXY = CliProxySource()


class PlanSelector:
    """Decide which config dir's plan is "active" across polls.

    "Active" = the plan whose session % rose most recently (recent API activity).
    A rise stamps a monotonic poll counter, so the choice is sticky and a window
    reset (a drop to 0) isn't mistaken for use. Before any rise is seen (startup)
    the highest current session % wins. Mirrors the Linux bash daemon.
    """

    def __init__(self) -> None:
        # Keys: a config dir Path or a "cliproxy:<auth>" string.
        self.prev_s: dict = {}
        self.last_active: dict = {}
        self.seq = 0

    def choose(self, sessions: dict) -> object:
        """Update state from this cycle's {plan: session_pct} and return the active plan."""
        self.seq += 1
        for d, s in sessions.items():
            if d in self.prev_s and s > self.prev_s[d]:
                self.last_active[d] = self.seq
            self.prev_s[d] = s
        # Most recent activity wins; ties (and the startup case) break by highest %.
        return max(sessions, key=lambda d: (self.last_active.get(d, 0), sessions[d]))


# Module-level so the active-plan state survives reconnects.
_SELECTOR = PlanSelector()


# Sources that answered with a non-auth client error (e.g. an API organization's
# "credit balance too low"): plan key -> time before which they are not retried.
# Without this the publisher's 5 s retry tick would hammer the API.
_BACKOFF: dict = {}


async def _poll_dir(d: Path) -> tuple[dict | None, bool]:
    """(payload, live) for one Claude Code config dir."""
    token = read_token_for(d)
    if not token:
        log(f"No token in {d}; skipping")
        return None, False
    try:
        payload = await poll_token(token)
    except TokenExpired:
        log(f"Token in {d} expired/invalid; skipping")
        return None, False
    if payload is None:
        # Authenticated but no answer: wait a full poll interval before asking
        # this dir again rather than retrying every tick.
        _BACKOFF[d] = time.time() + POLL_INTERVAL
    return payload, True


async def poll_active(selector: PlanSelector = _SELECTOR) -> tuple[dict | None, bool]:
    """Poll every source; return ``(active_payload, all_dead)``.

    Sources are each configured Claude Code config dir and, with
    ``cliproxy = on``, every active Claude account in CLIProxyAPI. Each is a
    plan for the selector. A single config dir (the default) collapses to
    exactly the old single-poll path.

    ``all_dead`` — True when *every* source lacked a usable token this cycle
    (file/Keychain empty, a 401/expired token, proxy unreachable), so the caller
    can signal "No data". False when at least one token authenticated —
    including a transient non-auth poll failure worth retrying silently.

    Pure free-ride: a 401 (TokenExpired) means that token has expired and only
    its owner (Claude Code, CLIProxyAPI) can re-seed it — we never refresh it.
    """
    dirs = read_config_dirs()
    payloads: dict = {}
    any_live = False
    now = time.time()
    for d in dirs:
        if _BACKOFF.get(d, 0) > now:
            any_live = True          # answered recently, just not usefully
            continue
        payload, live = await _poll_dir(d)
        any_live = any_live or live
        if payload is not None:
            _BACKOFF.pop(d, None)
            payloads[d] = payload
    if read_onoff_setting("cliproxy") == "on":
        proxy_payloads, proxy_live = await _CLIPROXY.poll()
        any_live = any_live or proxy_live
        payloads.update(proxy_payloads)
    if not payloads:
        return None, not any_live
    sessions = {k: int(p.get("s", 0) or 0) for k, p in payloads.items()}
    active = selector.choose(sessions)
    if len(payloads) > 1 or len(dirs) > 1:
        log(f"Active plan: {active} (s={sessions[active]})")
    return payloads[active], False


async def poll_active_payload(selector: PlanSelector = _SELECTOR) -> dict | None:
    """The active plan's payload, or None when no dir yields one this cycle.

    Thin wrapper over :func:`poll_active` for callers that don't need the
    all-dead flag.
    """
    payload, _dead = await poll_active(selector)
    return payload


def to_ascii(text: str, limit: int) -> str:
    """The device fonts are ASCII only: drop accents, collapse whitespace, cap."""
    text = unicodedata.normalize("NFKD", text).encode("ascii", "ignore").decode()
    text = " ".join(text.split())
    return text if len(text) <= limit else text[: limit - 3].rstrip() + "..."


def take_attention_flag() -> dict | None:
    """Consume the attention file. Only a fresh one counts, so a file written
    while the device was away doesn't beep on reconnect. A bare `touch` with no
    JSON still beeps, just without a card."""
    try:
        mtime = ATTENTION_FLAG.stat().st_mtime
        raw = ATTENTION_FLAG.read_text()
        ATTENTION_FLAG.unlink()
    except FileNotFoundError:
        return None
    if time.time() - mtime >= ATTENTION_MAX_AGE:
        return None
    try:
        data = json.loads(raw)
    except ValueError:
        data = None
    if not isinstance(data, dict):
        data = {}
    return {
        "m": to_ascii(str(data.get("m") or ""), 120),
        "s": str(data.get("s") or ""),
        "t": mtime,
    }


def take_clears() -> dict[str, float]:
    """Consume every clear-<session id> file and return {session id: mtime}."""
    clears = {}
    for f in ATTENTION_DIR.glob("clear-*"):
        try:
            clears[f.name.removeprefix("clear-")] = f.stat().st_mtime
            f.unlink()
        except FileNotFoundError:
            pass
    return clears


def encode_payload(payload: dict, limit: int) -> bytes:
    """JSON-encode for one write-without-response of at most `limit` bytes.
    Shortens the alert message, then drops it, until it fits."""
    data = json.dumps(payload, separators=(",", ":")).encode()
    msg = payload.get("m", "")
    while len(data) > limit and msg:
        msg = msg[: max(0, len(msg) - (len(data) - limit) - 3)].rstrip()
        payload = {**payload, "m": msg + "..." if msg else ""}
        data = json.dumps(payload, separators=(",", ":")).encode()
    if len(data) > limit:
        payload = {k: v for k, v in payload.items() if k != "m"}
        data = json.dumps(payload, separators=(",", ":")).encode()
    return data


class Session:
    def __init__(self, client: BleakClient) -> None:
        self.client = client
        self.refresh_requested = asyncio.Event()
        self.on_refresh: Callable[[], None] | None = None   # Publisher hook: poll now
        self.data_writes = 0      # usage payloads delivered on this link
        rx = client.services.get_characteristic(RX_CHAR_UUID)
        self.max_write = rx.max_write_without_response_size if rx else DEFAULT_MAX_WRITE
        log(f"Max write size: {self.max_write} bytes")

    def _on_refresh(self, _char, _data: bytearray) -> None:
        log("Refresh requested by device")
        self.refresh_requested.set()
        if self.on_refresh is not None:
            self.on_refresh()

    async def setup_refresh_subscription(self) -> None:
        # start_notify awaits CoreBluetooth's CCCD-write confirmation, which
        # never arrives if the peripheral doesn't ACK the subscribe (a
        # half-open link after the OS auto-connects the HID). Unbounded, that
        # await wedges the whole daemon between "Connected" and the first poll
        # — the device then shows nothing until a manual restart. Bound it: the
        # subscription is only an optional device-initiated refresh nudge (we
        # poll every POLL_INTERVAL regardless), so on timeout we proceed.
        try:
            await asyncio.wait_for(
                self.client.start_notify(REQ_CHAR_UUID, self._on_refresh),
                timeout=10,
            )
        except (BleakError, ValueError) as e:
            log(f"Refresh subscription unavailable: {e}")
        except asyncio.TimeoutError:
            log("Refresh subscription timed out; polling without it")

    async def write_payload(self, payload: dict) -> bool:
        data = encode_payload(payload, self.max_write)
        log(f"Sending: {data.decode()}")
        try:
            await self.client.write_gatt_char(RX_CHAR_UUID, data, response=False)
        except BleakError as e:
            log(f"Write failed: {e}")
            return False
        if payload.get("ok"):
            self.data_writes += 1
        return True


class HttpSinks:
    """POSTs each payload to every `http_targets` URL (WiFi screens).

    Logs only when a target's reachability changes, so an unplugged screen
    doesn't add a line every poll.
    """

    def __init__(self, transport: httpx.AsyncBaseTransport | None = None) -> None:
        self._transport = transport
        self._ok: dict[str, bool] = {}

    async def send(self, targets: list[str], payload: dict,
                   timeout: float = HTTP_PUSH_TIMEOUT) -> bool:
        """True if at least one target accepted the payload."""
        body = json.dumps(payload, separators=(",", ":"))
        async with httpx.AsyncClient(timeout=timeout,
                                     transport=self._transport) as http:
            results = await asyncio.gather(
                *(self._post(http, url, body) for url in targets))
        return any(results)

    async def _post(self, http: httpx.AsyncClient, url: str, body: str) -> bool:
        try:
            resp = await http.post(url, content=body,
                                   headers={"Content-Type": "application/json"})
            ok, why = resp.status_code == 200, f"HTTP {resp.status_code}"
        except httpx.HTTPError as e:
            ok, why = False, str(e) or type(e).__name__
        if url not in self._ok or self._ok[url] != ok:
            log(f"WiFi push to {url}: ok" if ok else f"WiFi push to {url} failed: {why}")
        self._ok[url] = ok
        return ok


class Publisher:
    """One poll loop feeding every destination: the BLE device while it is
    connected, plus the `http_targets` WiFi screens.

    Owns the poll schedule and the attention state, so an alert is consumed
    once and delivered to every destination in the same payload. The firmware
    treats every write as a full payload, so alerts and clears always ride on a
    fresh poll, never on their own.
    """

    def __init__(self, http: HttpSinks | None = None,
                 targets: Callable[[], list[str]] | None = None) -> None:
        self.http = http or HttpSinks()
        self.targets = targets    # None: the `http_targets` config option
        self.ble: Session | None = None
        self.wake = asyncio.Event()
        self.force = False        # poll on the next step regardless of schedule
        self.last_poll = 0.0
        self.alert: dict | None = None   # latest alert, kept to match its session's clear
        self.alert_unsent = False
        self.clear_unsent = False
        self.quiet = False        # "no usable source" already logged
        self.on_payload: Callable[[dict], None] | None = None   # clawd-home pet sees usage
        self.last_payload: dict | None = None   # last delivered payload, for resend()

    def attach_ble(self, session: Session) -> None:
        self.ble = session
        session.on_refresh = self.wake.set
        self.force = True         # a fresh link gets data immediately
        self.wake.set()

    def detach_ble(self, session: Session) -> None:
        if self.ble is session:
            self.ble = None
        session.on_refresh = None

    async def step(self) -> None:
        targets = self.targets() if self.targets is not None else read_http_targets()
        ble = self.ble if self.ble is not None and self.ble.client.is_connected else None
        if ble is None and not targets:
            return    # nobody to show it to: leave attention flags for later

        # One alert slot, latest wins: two sessions alerting in the same tick
        # keep only the last one.
        new_alert = take_attention_flag()
        # A session that is still waiting can notify twice (AskUserQuestion
        # fires PreToolUse, then a permission_prompt). Keep the first card
        # and beep once until that session clears.
        alert = self.alert
        repeat = new_alert and alert and alert["s"] and new_alert["s"] == alert["s"]
        if new_alert is not None and not repeat:
            self.alert, self.alert_unsent, self.clear_unsent = new_alert, True, False
        clears = take_clears()
        if self.alert and clears.get(self.alert["s"], 0) > self.alert["t"]:
            self.clear_unsent = not self.alert_unsent   # never shown, nothing to clear
            self.alert, self.alert_unsent = None, False

        refresh = ble is not None and ble.refresh_requested.is_set()
        due = time.time() - self.last_poll >= POLL_INTERVAL
        if not (self.force or refresh or due or self.alert_unsent or self.clear_unsent):
            return
        self.force = False
        if ble is not None:
            ble.refresh_requested.clear()

        # Pure free-ride: read whatever access token(s) Claude Code currently
        # holds across the configured config dirs and NEVER refresh them
        # ourselves. Claude Code (the token's owner) does all refreshing;
        # refreshing here would race its rotation and feed the OAuth endpoint's
        # rate limit (429).
        payload, dead = await poll_active()
        extra = {}
        if self.alert_unsent and self.alert is not None:
            if read_onoff_setting("beep") == "on":
                extra["b"] = 1
            if self.alert["m"]:
                extra["m"] = self.alert["m"]
        if self.clear_unsent:
            extra["x"] = 1
        if payload is not None:
            self.quiet = False
            out = {**payload, **extra}
        elif dead:
            # No live token in any config dir (missing, or a 401/expired token)
            # -> show "No data" now instead of stale numbers.
            log("No usable token; signalling no-data to device \u2014 run "
                "`claude login` or use the CLI to let Claude Code renew it")
            out = {"ok": False, **extra}
        else:
            # A live token that didn't answer usefully this cycle -> stay
            # silent. poll_active backs that source off for a poll interval,
            # so the next ticks don't hammer the API.
            if not self.quiet:
                log("No usable source this cycle; retrying quietly")
            self.quiet = True
            return

        # Guard last_poll on delivery so a failed write retries next tick
        # instead of throttling a healthy link for a full POLL_INTERVAL.
        if await self.send(out, ble, targets):
            self.last_poll = time.time()
            self.alert_unsent = self.clear_unsent = False
            # One-shot extras (beep, alert line, clear) are not replayed.
            self.last_payload = {k: v for k, v in out.items() if k not in ("b", "m", "x")}

    async def resend(self, url: str) -> bool:
        """Push the last delivered payload again to one clock that just restarted,
        so it leaves its "no data" splash now instead of at the next poll. Never
        calls the usage API; with nothing cached yet it asks for a poll instead
        (which delivers it). False only when the clock did not take the data."""
        if self.last_payload is None:
            self.force = True
            self.wake.set()
            return True
        out = dict(self.last_payload)
        if "t" in out:
            add_clock_fields(out)     # the cached wall-clock time is up to a poll old
        return await self.http.send([url], out, timeout=RESEND_TIMEOUT)

    async def send(self, payload: dict, ble: Session | None, targets: list[str]) -> bool:
        delivered = False
        if self.on_payload is not None:
            self.on_payload(payload)
        if ble is not None:
            delivered = await ble.write_payload(payload)
        if targets:
            if ble is None:
                log(f"Sending: {json.dumps(payload, separators=(',', ':'))}")
            delivered = await self.http.send(targets, payload) or delivered
        return delivered

    async def run(self, stop_event: asyncio.Event) -> None:
        while not stop_event.is_set():
            try:
                await self.step()
            except Exception as e:  # keep the loop alive; the next tick retries
                log(f"Publish cycle failed: {e!r}")
            try:
                await asyncio.wait_for(self.wake.wait(), timeout=TICK)
            except asyncio.TimeoutError:
                pass
            self.wake.clear()


def _is_encryption_error(exc: BaseException) -> bool:
    """True if a connect error is a macOS bonding/encryption mismatch.

    macOS reports a stale bond as CBErrorDomain Code=15 ("Failed to encrypt
    the connection..."). Match on the message text so we don't depend on how
    bleak wraps the underlying CoreBluetooth error.
    """
    s = str(exc).lower()
    return "code=15" in s or "encrypt" in s


# blueutil talks to Bluetooth via IOBluetooth, which on recent macOS needs its
# OWN Bluetooth TCC grant (separate from the daemon's CoreBluetooth grant).
# Without it, blueutil *hangs* instead of erroring — so every call is bounded
# by a timeout and a hang is reported as a permission problem, not a crash.
BLUEUTIL_TIMEOUT = 8


def _blueutil(*args: str) -> str | None:
    """Run `blueutil <args>`, returning stdout, or None on failure/timeout.

    A timeout almost always means blueutil lacks Bluetooth permission (it
    blocks rather than failing), so we surface that cause explicitly.
    """
    try:
        return subprocess.run(
            ["blueutil", *args],
            capture_output=True, text=True,
            timeout=BLUEUTIL_TIMEOUT, check=True,
        ).stdout
    except subprocess.TimeoutExpired:
        log(f"blueutil {' '.join(args)} timed out — it likely lacks Bluetooth "
            "permission. Grant it under System Settings > Privacy & Security > "
            "Bluetooth (run `blueutil --paired` once from Terminal to prompt).")
        return None
    except (subprocess.SubprocessError, OSError) as e:
        log(f"blueutil {' '.join(args)} failed: {e}")
        return None


def unpair_macos() -> bool:
    """Forget a stale macOS bond for DEVICE_NAME so the device can re-pair.

    A Code=15 "failed to encrypt" connect error means macOS holds bonding
    keys that no longer match the ESP32's (e.g. after a firmware reflash or
    the on-device bond-clear gesture). The firmware pairs "just works" (no
    MITM), so once the stale bond is gone the next connect re-bonds silently
    with no GUI prompt.

    CoreBluetooth exposes no unpair API, so we shell out to `blueutil`. The
    daemon only knows the peripheral's CoreBluetooth UUID, not the BD_ADDR
    that blueutil needs, so we map by name via `blueutil --paired`. Returns
    True if a bond was removed. Mirrors the Linux daemon's `bluetoothctl
    remove` self-heal.
    """
    if not shutil.which("blueutil"):
        log("Stale bond detected but `blueutil` is not installed; cannot "
            "auto-recover. Run `brew install blueutil`, or forget "
            f"'{DEVICE_NAME}' in System Settings > Bluetooth and reconnect.")
        return False

    out = _blueutil("--paired")
    if out is None:
        return False

    # Each line looks like:
    #   address: 28-84-85-55-5c-3d, ... name: "Clawdmeter", ...
    addr = None
    for line in out.splitlines():
        if f'name: "{DEVICE_NAME}"' in line:
            m = re.search(r"address:\s*([0-9a-fA-F:-]+)", line)
            if m:
                addr = m.group(1)
                break
    if not addr:
        log(f"No paired '{DEVICE_NAME}' found to unpair (already forgotten?)")
        return False

    if _blueutil("--unpair", addr) is None:
        return False
    log(f"Unpaired stale bond for '{DEVICE_NAME}' [{addr}]; re-pairing on "
        "next connect")
    return True


async def connect_and_run(target, stop_event: asyncio.Event,
                          publisher: "Publisher | None" = None) -> bool:
    """Connect to a target and feed it payloads until disconnected or stopped.

    Payloads come from ``publisher`` (the daemon-wide poll loop, shared with
    the WiFi push targets). Without one, a private publisher runs for the
    lifetime of this connection.

    ``target`` is either an address string (Linux) or a BLEDevice carrying
    live CoreBluetooth details (macOS). Returns True if the connection was
    used successfully (so the caller keeps the cached address), False if the
    connection failed and the cache should be invalidated.
    """
    display = target if isinstance(target, str) else target.address
    log(f"Connecting to {display}...")
    client = BleakClient(target)
    try:
        # Bound the connect the same way #84 bounded the refresh subscribe.
        # On macOS the OS auto-connects the firmware's HID link, so
        # CoreBluetooth can hand us a half-open peripheral whose GATT connect
        # handshake never completes. BleakClient's own timeout governs
        # discovery, not connectPeripheral, so an unbounded await here wedges
        # the single-threaded daemon forever at "Connecting..." (observed ~13h,
        # device stuck on stale data). wait_for raises TimeoutError, which the
        # handler below already treats as a connection failure -> drop the
        # cached address and rescan.
        await asyncio.wait_for(client.connect(), timeout=CONNECT_TIMEOUT)
    except (BleakError, asyncio.TimeoutError) as e:
        log(f"Connection failed: {e}")
        if sys.platform == "darwin" and _is_encryption_error(e):
            log("Encryption failed — likely a stale macOS bond; self-healing")
            unpair_macos()
        return False

    if not client.is_connected:
        log("Connection failed (no error but not connected)")
        return False

    log("Connected")
    if not read_http_targets():
        # Drop alerts raised while the device was away. With WiFi screens
        # configured those alerts were (or are about to be) shown there.
        ATTENTION_FLAG.unlink(missing_ok=True)
        take_clears()
    session = Session(client)
    await session.setup_refresh_subscription()

    own_stop = None
    own_task = None
    if publisher is None:
        publisher = Publisher()
        own_stop = asyncio.Event()
        own_task = asyncio.create_task(publisher.run(own_stop))
    publisher.attach_ble(session)
    try:
        while client.is_connected and not stop_event.is_set():
            try:
                await asyncio.wait_for(stop_event.wait(), timeout=TICK)
            except asyncio.TimeoutError:
                pass
    finally:
        publisher.detach_ble(session)
        if own_task is not None and own_stop is not None:
            own_stop.set()
            publisher.wake.set()
            await own_task
        try:
            await client.disconnect()
        except BleakError:
            pass

    log("Device disconnected" if not stop_event.is_set() else "Stopping")
    return session.data_writes > 0


async def main() -> None:
    stop_event = asyncio.Event()
    loop = asyncio.get_running_loop()

    def _stop(*_args: object) -> None:
        log("Daemon stopping")
        stop_event.set()

    for sig in (signal.SIGINT, signal.SIGTERM):
        try:
            loop.add_signal_handler(sig, _stop)
        except NotImplementedError:
            signal.signal(sig, _stop)

    log("=== Claude Usage Tracker Daemon (BLE, macOS) ===")
    log(f"Poll interval: {POLL_INTERVAL}s")
    targets = read_http_targets()
    if targets:
        log(f"WiFi push targets: {', '.join(targets)}")
    ble_on = read_onoff_setting("ble", default="on") == "on"

    publisher = Publisher()
    pub_task = asyncio.create_task(publisher.run(stop_event))
    events_task, events_server = await start_events(publisher, stop_event)
    try:
        if ble_on:
            await ble_loop(stop_event, publisher)
        else:
            log("Bluetooth off (ble = off): WiFi push only")
            if not targets:
                log("No http_targets configured either; nothing will be sent")
            await stop_event.wait()
    finally:
        stop_event.set()
        publisher.wake.set()
        await pub_task
        if events_task is not None:
            await events_task
        if events_server is not None:
            await events_server.close()


async def start_events(publisher: "Publisher", stop_event: asyncio.Event):
    """clawd-home: the local event server (Claude Code hooks, pi extension,
    `clawd` CLI) and the push of the resulting desk state to every WiFi clock.
    `events = off` in the config disables both."""
    if read_onoff_setting("events", default="on") != "on":
        log("Clawd events off (events = off)")
        return None, None
    pet = clawd_events.Pet(clawd_events.read_pet_config(read_config_value),
                           store=ATTENTION_DIR / "pet.json")
    brain = clawd_events.Brain(pet)

    def on_payload(p: dict) -> None:
        brain.usage = p
    publisher.on_payload = on_payload
    server = clawd_events.EventServer(brain, port=clawd_events.read_event_port(read_config_value))
    try:
        await server.start()
    except OSError as e:
        log(f"Event server not started ({e}); agent events disabled")
        return None, None
    pusher = clawd_events.StatePusher(brain, read_http_targets)
    return asyncio.create_task(pusher.run(stop_event)), server


async def ble_loop(stop_event: asyncio.Event, publisher: Publisher) -> None:
    """Find the BLE device, keep it connected, and reconnect with backoff."""
    backoff = 1
    skip_addr: str | None = None  # macOS: a peripheral to skip for one cycle
    while not stop_event.is_set():
        # Apply any pending skip exactly once, then clear it so the next
        # cycle re-tries retrieveConnected (the device may have recovered).
        target = await discover_target(skip_addr=skip_addr)
        skip_addr = None
        if not target:
            log(f"Device not found, retrying in {backoff}s...")
            try:
                await asyncio.wait_for(stop_event.wait(), timeout=backoff)
            except asyncio.TimeoutError:
                pass
            backoff = min(backoff * 2, 60)
            continue

        addr = target if isinstance(target, str) else target.address
        ok = await connect_and_run(target, stop_event, publisher)
        if not ok:
            if sys.platform == "darwin":
                # No string cache to drop; instead skip this stale handle on
                # the next retrieveConnected so the scan fallback is reachable.
                skip_addr = addr
            else:
                log("Invalidating cached address")
                SAVED_ADDR_FILE.unlink(missing_ok=True)
            try:
                await asyncio.wait_for(stop_event.wait(), timeout=backoff)
            except asyncio.TimeoutError:
                pass
            backoff = min(backoff * 2, 60)
        else:
            backoff = 1


if __name__ == "__main__":
    try:
        asyncio.run(main())
    except KeyboardInterrupt:
        sys.exit(0)
