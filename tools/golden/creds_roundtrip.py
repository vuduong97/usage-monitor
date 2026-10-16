#!/usr/bin/env python3
"""Credential round trip Python <-> Rust (phase 03, step 3). Windows: python-keyring's
WinVaultKeyring writes -> `clawd_store::creds` (Rust) reads -> Rust writes -> Python reads,
including the same-service collision (second user moves the first to `user@service`) and
deletes in both directions.

It uses the REAL Credential Manager / Keychain, so it only ever uses a throwaway service
`vn.clawd.desktop.test-<uuid>` (never `vn.clawd.desktop`) and deletes everything it made. Run it
on a CI VM or a scratch user, and pass `--yes` to confirm:

    cargo build -p clawd-store --example creds_cli
    python tools/golden/creds_roundtrip.py --yes [--exe path/to/creds_cli(.exe)]

Not part of `cargo test` or the golden export (those never touch an OS store).
"""
from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import uuid
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO))

from clawd_desktop import creds  # noqa: E402

PRODUCTION = "vn.clawd.desktop"


def default_exe() -> Path:
    target = Path(os.environ.get("CARGO_TARGET_DIR", REPO / "desktop-rs" / "target"))
    return target / "debug" / "examples" / ("creds_cli.exe" if os.name == "nt" else "creds_cli")


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--yes", action="store_true", help="confirm: use the real OS keyring, throwaway service")
    ap.add_argument("--exe", type=Path, default=default_exe())
    args = ap.parse_args()
    if not args.yes:
        print(__doc__)
        return 2
    service = f"{PRODUCTION}.test-roundtrip-{uuid.uuid4().hex[:12]}"
    if service == PRODUCTION:
        raise SystemExit("refusing the production service")
    creds.SERVICE = service                       # the real Python code path, throwaway service
    keys = ["k-py", "k-first", "k-second", "k-rs", "k-rs-first", "k-rs-second", "Đồng hồ"]

    def rust(action: str, key: str, *rest: str):
        out = subprocess.run([str(args.exe), service, action, key, *rest], capture_output=True,
                             text=True, encoding="utf-8", check=True).stdout.strip()
        return json.loads(out)

    failures: list[str] = []

    def check(name: str, got, want) -> None:
        ok = got == want
        print(("ok   " if ok else "FAIL ") + name + ("" if ok else f"  got={got!r} want={want!r}"))
        if not ok:
            failures.append(name)

    try:
        # Python writes -> Rust reads
        creds.put("k-py", "root", "pä\U0001F600\nline2")
        check("py put -> rust get", rust("get", "k-py"), ["root", "pä\U0001F600\nline2"])
        # collision written by Python (different users, same service): both readable by Rust
        creds.put("k-first", "admin", "pw-first")
        creds.put("k-second", "admin", "pw-second")
        check("py collision -> rust get first", rust("get", "k-first"), ["admin", "pw-first"])
        check("py collision -> rust get second", rust("get", "k-second"), ["admin", "pw-second"])
        # Rust writes -> Python reads
        check("rust put", rust("put", "k-rs", "ü", "pa ss"), True)
        check("rust put -> py get", creds.get("k-rs"), ("ü", "pa ss"))
        check("rust put unicode key", rust("put", "Đồng hồ", "u", "pw"), True)
        check("rust unicode key -> py get", creds.get("Đồng hồ"), ("u", "pw"))
        check("rust collision put 1", rust("put", "k-rs-first", "a", "pw1"), True)
        check("rust collision put 2", rust("put", "k-rs-second", "b", "pw2"), True)
        check("rust collision -> py get 1", creds.get("k-rs-first"), ("a", "pw1"))
        check("rust collision -> py get 2", creds.get("k-rs-second"), ("b", "pw2"))
        # deletes cross over
        rust("forget", "k-rs")
        check("rust forget -> py get none", creds.get("k-rs"), None)
        creds.forget("k-py")
        check("py forget -> rust get none", rust("get", "k-py"), None)
    finally:
        for key in keys:
            creds.forget(key)
        if os.name == "nt":
            # drop whatever compound targets the collisions left behind
            for key in keys:
                for target in (service, f"{key}@{service}", f"admin@{service}", f"a@{service}", f"b@{service}"):
                    subprocess.run(["cmdkey", f"/delete:{target}"], capture_output=True, check=False)
    print("roundtrip:", "FAILED " + ", ".join(failures) if failures else "all ok")
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
