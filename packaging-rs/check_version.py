#!/usr/bin/env python3
"""The Rust build's version, and the guard that a release tag says the same thing.

  python packaging-rs/check_version.py                  print the version (after checking
                                                        Cargo.toml == tauri.conf.json)
  python packaging-rs/check_version.py rust-v0.2.0      also require the tag to be rust-v<version>

The self-update compares a release tag with the version baked into the build: a release whose build
says another version would be offered again after every update (red team #2). Three places carry
the version and they must agree: the tag, desktop-rs/app/Cargo.toml (usually inherited from
[workspace.package] in desktop-rs/Cargo.toml) and desktop-rs/app/tauri.conf.json.
Needs Python 3.11+ (tomllib). Exit 0 = agree, 1 = disagree or unreadable.
"""
import json
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
RS = ROOT / "desktop-rs"
TAG_PREFIX = "rust-"


def cargo_version() -> str:
    pkg = tomllib.loads((RS / "app" / "Cargo.toml").read_text(encoding="utf-8"))["package"]
    version = pkg["version"]
    if isinstance(version, dict):  # `version.workspace = true`
        if version.get("workspace") is not True:
            raise ValueError("app/Cargo.toml: version is a table but not `workspace = true`")
        ws = tomllib.loads((RS / "Cargo.toml").read_text(encoding="utf-8"))
        version = ws["workspace"]["package"]["version"]
    return version


def tauri_version() -> str:
    return json.loads((RS / "app" / "tauri.conf.json").read_text(encoding="utf-8"))["version"]


def main(argv: list[str]) -> int:
    if len(argv) > 2:
        print(__doc__, file=sys.stderr)
        return 1
    try:
        cargo, tauri = cargo_version(), tauri_version()
    except (OSError, KeyError, ValueError, tomllib.TOMLDecodeError, json.JSONDecodeError) as err:
        print(f"cannot read the version: {err!r}", file=sys.stderr)
        return 1
    problems = []
    if cargo != tauri:
        problems.append(f"app/Cargo.toml version {cargo!r} != app/tauri.conf.json version {tauri!r}")
    if len(argv) == 2:
        tag = argv[1]
        if not tag.startswith(TAG_PREFIX):
            problems.append(f"tag {tag!r} does not start with {TAG_PREFIX!r}")
        elif tag[len(TAG_PREFIX):] != f"v{cargo}":
            problems.append(f"tag {tag!r} (without {TAG_PREFIX!r}: {tag[len(TAG_PREFIX):]!r}) != v{cargo}")
    if problems:
        for p in problems:
            print(f"version mismatch: {p}", file=sys.stderr)
        return 1
    print(cargo)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
