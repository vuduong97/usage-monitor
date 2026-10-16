#!/usr/bin/env python3
"""A local stand-in for the GitHub Releases API, to test the app's self-update without a release.

Both updaters (Python 0.1.x `appupdate.py` and Rust `clawd-appupdate`) read
`<api>/repos/<owner>/<name>/releases/latest` and accept plain-http downloads from 127.0.0.1 when
the API itself is loopback. So, with this server running:

    python tools/migration/fake_api.py --dir rel-0.2.0              # serves rel-0.2.0/* as v0.2.0
    CLAWD_UPDATE_API=http://127.0.0.1:8765 /Applications/Clawd.app/Contents/MacOS/Clawd    # mac
    $env:CLAWD_UPDATE_API = "http://127.0.0.1:8765"; & "$env:LOCALAPPDATA\\Programs\\Clawd\\Clawd.exe"  # Windows

the app sees `--dir`'s files as the newest published release. `--dir` holds what a release would
carry: `Clawd-X.Y.Z-windows-setup.exe`, `Clawd-X.Y.Z-mac-<arch>.zip` (and `.dmg`), `SHA256SUMS`,
`SHA256SUMS.minisig`. For the release candidate, download the draft:
`gh release download vX.Y.Z -D rel-X.Y.Z` (works on a draft for the repo owner).

`--case` serves a broken variant without touching the files on disk (each must be REFUSED by the
updater, and the app must keep running the installed version):

    ok               the files as they are
    bad-sig          SHA256SUMS.minisig with one byte of the signature changed
    tampered-sums    one hex digit of every SHA256SUMS line changed (signature kept)
    no-minisig       SHA256SUMS.minisig left out of the release
    wrong-installer  installers/zips of the announced size with their last byte changed
    prerelease       the release is marked prerelease (never offered)
    draft            the release is marked draft (never offered)

A signature made with another key: `minisign -S -s other.key -m SHA256SUMS -x other.minisig`,
then `--minisig other.minisig`.

Every request is printed, so the log shows what the app asked for and downloaded. Standard
library only (Python 3.9+). Nothing is written to disk.
"""
from __future__ import annotations

import argparse
import base64
import contextlib
import json
import re
import sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

CASES = ("ok", "bad-sig", "tampered-sums", "no-minisig", "wrong-installer", "prerelease", "draft")
SUMS, SIG = "SHA256SUMS", "SHA256SUMS.minisig"
VERSION_IN_NAME = re.compile(r"^Clawd-([0-9]+\.[0-9]+\.[0-9]+)-")
INSTALLER = re.compile(r"-(windows-setup\.exe|mac-[a-z0-9_]+\.zip)$")


def flip_hex(c: str) -> str:
    return "0" if c != "0" else "1"


def tamper_sums(data: bytes) -> bytes:
    out = []
    for line in data.decode("utf-8").splitlines(keepends=True):
        out.append(flip_hex(line[0]) + line[1:] if line[:1].strip() else line)
    return "".join(out).encode("utf-8")


def tamper_sig(data: bytes) -> bytes:
    """Change one byte of the signature itself (line 2 of a minisign file)."""
    lines = data.decode("utf-8").splitlines(keepends=True)
    if len(lines) < 2:
        raise SystemExit(f"{SIG} does not look like a minisign signature")
    raw = bytearray(base64.b64decode(lines[1].strip()))
    raw[-1] ^= 0x01
    lines[1] = base64.b64encode(bytes(raw)).decode("ascii") + "\n"
    return "".join(lines).encode("utf-8")


def load(args: argparse.Namespace) -> tuple[str, dict[str, bytes]]:
    folder = Path(args.dir)
    files = {p.name: p.read_bytes() for p in sorted(folder.iterdir()) if p.is_file()}
    if args.minisig:
        files[SIG] = Path(args.minisig).read_bytes()
    versions = {m[1] for name in files if (m := VERSION_IN_NAME.match(name))}
    tag = args.tag or (f"v{versions.pop()}" if len(versions) == 1 else "")
    if not tag:
        raise SystemExit(f"cannot tell the version from {sorted(files)}: pass --tag vX.Y.Z")
    if args.case == "bad-sig":
        files[SIG] = tamper_sig(files[SIG])
    elif args.case == "tampered-sums":
        files[SUMS] = tamper_sums(files[SUMS])
    elif args.case == "no-minisig":
        files.pop(SIG, None)
    elif args.case == "wrong-installer":
        for name in [n for n in files if INSTALLER.search(n)]:
            data = bytearray(files[name])
            data[-1] ^= 0xFF
            files[name] = bytes(data)
    return tag, files


def make_handler(tag: str, files: dict[str, bytes], case: str, base: str):
    release = {
        "tag_name": tag,
        "name": f"Clawd {tag[1:]} (local test)",
        "draft": case == "draft",
        "prerelease": case == "prerelease",
        "html_url": f"{base}/release/{tag}",
        "body": f"Local test release ({case}).",
        "assets": [
            {"name": name, "size": len(data), "browser_download_url": f"{base}/dl/{name}"}
            for name, data in files.items()
        ],
    }
    page = json.dumps(release).encode("utf-8")

    class Handler(BaseHTTPRequestHandler):
        def _send(self, code: int, body: bytes, ctype: str) -> None:
            self.send_response(code)
            self.send_header("Content-Type", ctype)
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            if self.command != "HEAD":
                self.wfile.write(body)

        def do_GET(self) -> None:  # noqa: N802 (http.server API)
            path = self.path.split("?", 1)[0]
            if re.fullmatch(r"/repos/[^/]+/[^/]+/releases/latest", path):
                self._send(200, page, "application/json")
            elif path.startswith("/dl/") and path[4:] in files:
                self._send(200, files[path[4:]], "application/octet-stream")
            else:
                self._send(404, b'{"message":"Not Found"}', "application/json")

        do_HEAD = do_GET

        def log_message(self, format: str, *args) -> None:  # noqa: A002 (base class name)
            print(f"{self.log_date_time_string()} {self.address_string()} {format % args}", flush=True)

    return Handler


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=(__doc__ or "").split("\n\n")[0])
    ap.add_argument("--dir", required=True, help="folder with the release files")
    ap.add_argument("--tag", default="", help="tag to announce (default: v<version in the file names>)")
    ap.add_argument("--case", choices=CASES, default="ok")
    ap.add_argument("--minisig", default="", help="serve this file as SHA256SUMS.minisig")
    ap.add_argument("--port", type=int, default=8765)
    args = ap.parse_args(argv)
    tag, files = load(args)
    base = f"http://127.0.0.1:{args.port}"
    server = ThreadingHTTPServer(("127.0.0.1", args.port), make_handler(tag, files, args.case, base))
    print(f"{tag} ({args.case}) at {base}: {', '.join(files)}", flush=True)
    print(f"set CLAWD_UPDATE_API={base} for the app under test; Ctrl+C stops", flush=True)
    with contextlib.suppress(KeyboardInterrupt):
        server.serve_forever()
    return 0


if __name__ == "__main__":
    sys.exit(main())
