---
title: "Release 0.2.2: Windows test, sign gate and the clock-firmware artifact"
date: 2026-10-30
summary: "0.2.2 is the first tag with clock/ firmware; the sign job merged the clock-firmware artifact and failed its exactly-three-files gate (fixed in 75b8a2b, tag moved); draft built and installed on Windows"
---

# Release 0.2.2: Windows test, sign gate and the clock-firmware artifact

## What happened
The goal was to run the latest app (Rust) and the latest clock firmware (fwSeq 9) on the owner's Windows PC. No release contained them: v0.2.0 and v0.2.1 predate `clock/`, the v0.2.1 Release run had waited 309 h for the `release` approval, and CI on `main` was red since 03f8c05.

## Changes
- `9bfff17` test(events): `save_load_watch_and_find` created a folder `ak:plan`; `:` is illegal in Windows file names (os error 267). Windows now uses `ak-plan`; other systems keep `ak:plan`.
- `89b33c6` chore(release): 0.2.2 (Cargo.toml, tauri.conf.json, Cargo.lock, release notes).
- `75b8a2b` ci(release): the `sign` job downloads only `{windows,mac-*}`. Without a pattern, `merge-multiple` also pulled `clock-firmware` (clawd-firmware.bin, clawd.pak, desk.pak, firmware.json) and the "exactly the expected files" gate failed. 77a6f41 added the artifact; 0.2.2 was the first tag run since.
- Tag v0.2.2 moved from 89b33c6 to 75b8a2b (no release existed for it). v0.2.1 run 39008969553 cancelled.

## Evidence
- CI 41000389444 (89b33c6): success, all 5 jobs.
- Release 41021425041 (v0.2.2 @ 75b8a2b): all jobs success; draft with dmg, zip, windows-setup.exe, SHA256SUMS, SHA256SUMS.minisig. `releases/latest` is still v0.1.4.
- Installed from the draft: `sha256sum -c SHA256SUMS` OK for the setup exe; log `Clawd 0.2.2 starting (windows)`; `Programs/Clawd/firmware/firmware.json` fwSeq 9, commit 75b8a2b.

## Decisions
- Version 0.2.2, not 0.2.2-rc.1: `clawd-appupdate::parse_version` rejects pre-release tags, so an rc build could never update itself. The draft is the "test only" channel: updaters ignore drafts.
- Full workspace build left to CI: the local `cargo test --workspace` filled the disk and froze the PC; `target/` (9.9 GB) was cleaned.

## Open
- The clock (10.97.41.158 / 10.97.40.207) did not answer from this PC, so fwSeq 9 is not on the clock yet; the app installs it once they share a network.
- The draft is not published. When ready: `gh release edit v0.2.2 --draft=false --latest`. This is the first Python 0.1.4 -> Rust update for real users; see `tools/migration/local-release.md`.
