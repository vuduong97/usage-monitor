---
title: Firmware feed (Worker + R2 + app install)
date: 2026-10-13
---

# Firmware feed (Worker + R2 + app install)

## What
- Clawd desktop can now install new clock firmware and packs from a private feed with one click. No new app release is needed.
- Branches are `feat/fw-feed` in two worktrees; neither is merged.
  - clawd-home-feed: 94da798, 5a8c353, ed80826
  - clawd-desktop-feed: 75fc3c4, 49790de, 661f444, bf1542b

## Design
- **Firmware (fwSeq 6, 501,520 B):** `/api/heap` reports `pakCrc`/`deskCrc`, stored in `.crc` sidecars and computed in the background 30 s after boot. The pack uploads return a `crc`.
- **Worker:** `/v1/latest` and `/v1/grant` require a Bearer token. Grant returns the per-release AES key, read from KV, plus HMAC-signed URLs valid for 300 s. `/v1/file` streams the `.enc` files from a private R2 bucket.
- **`tools/release.sh`:**
  - Runs the build and the tests (host, worker, preview).
  - Encrypts each file with AES-256-GCM.
  - Uploads the manifest last.
  - Guards the seq and supports beta and `--promote`.
  - Refuses to publish to Cloudflare unless NOTICE has a `Feed licence:` line.
- **App:**
  - `feed.py` decrypts and verifies in RAM.
  - `ota.install_release` installs the firmware if it is newer, then only the packs whose CRC differs.
  - The Cập nhật tab, the tray item "● Có firmware mới" and a 6 h check are wired in.
  - The URL and token come from CI secrets, written into a git-ignored `_feed_build.py`.

## Verification
- pytest: 376 passed, 2 skipped.
- Worker node tests: 8 passed.
- Firmware host tests and preview checks: 0 failures.
- `release.sh --local`: one run took 3 m 40 s.
- `tools/feed_e2e.py`, run against `wrangler dev` with a fake HTTP clock:
  - Requests were refused with 401, 403 and 404 as expected.
  - The files are served as ciphertext.
  - The firmware, `clawd.pak` and `desk.pak` arrived byte-identical.

## Lessons
- #lesson workerd rejects an entry module that exports anything besides handlers, so the logic lives in `feed.js`.
- #lesson `clawd_preview/run.sh` exits 0 even when its checks fail, because of a pipe. Check its `FAIL` lines instead. It also needs `DESK_PACK_PATH` in a worktree.

## Open
- Deploy needs a Cloudflare account and domain, the KV namespace id, the `APP_TOKENS`/`URL_SECRET` secrets, and the licence line.
- The app token can be extracted from the public app.
- Not tested on hardware: the CRC timing, the 150 s wait and the tray loop.

> Historical work record — not durable authority. Prefer docs/specs/ADRs for current decisions.
