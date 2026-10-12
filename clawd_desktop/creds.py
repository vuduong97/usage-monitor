"""The clock's web password, kept in the OS keychain (macOS Keychain, Windows
Credential Manager). Keyed by the clock's name, so a new IP from the router
keeps it. Nothing is written to the config files."""
from __future__ import annotations

import sys

SERVICE = "vn.clawd.desktop"
DEFAULT_USER = "admin"


def _keyring():
    import keyring
    # A frozen app may lose keyring's entry-point metadata and silently fall
    # back to the "fail" backend: name the platform backend explicitly.
    try:
        if sys.platform == "darwin":
            from keyring.backends import macOS
            keyring.set_keyring(macOS.Keyring())
        elif sys.platform == "win32":
            from keyring.backends import Windows
            keyring.set_keyring(Windows.WinVaultKeyring())
    except Exception:
        pass
    return keyring


def get(clock_key: str) -> tuple[str, str] | None:
    try:
        raw = _keyring().get_password(SERVICE, clock_key)
    except Exception:
        return None
    if not raw:
        return None
    user, sep, pw = raw.partition("\n")
    return (user, pw) if sep else (DEFAULT_USER, raw)


def put(clock_key: str, user: str, password: str) -> bool:
    try:
        _keyring().set_password(SERVICE, clock_key, f"{user}\n{password}")
        return True
    except Exception:
        return False


def forget(clock_key: str) -> None:
    try:
        _keyring().delete_password(SERVICE, clock_key)
    except Exception:
        pass
