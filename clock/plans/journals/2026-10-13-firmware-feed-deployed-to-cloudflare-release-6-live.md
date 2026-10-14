---
title: "Firmware feed deployed to Cloudflare, release 6 live"
date: 2026-10-13
---

# Firmware feed deployed to Cloudflare, release 6 live

- Merged main into feat/fw-feed. This brought in the waiting screen; NOTICE is removed at the user's request. The licence gate moved to `feed/LICENCE`. The new `--private` flag allows publishing to Cloudflare without that file, for the owner's own test.
- Cloudflare account 72100c7c:
  - R2 bucket `clawd-fw` (r2.dev URL disabled).
  - KV namespace `clawd-fw-keys` (id 74afa38d…).
  - `URL_SECRET` and `APP_TOKENS` secrets set. The app token lives only in `~/.clawd-feed/app_token`.
  - Worker deployed at https://clawd-fw-feed.thien-vu97ht123.workers.dev.
- Smoke test: no token returns 401, a file without a signature 403, and the raw bucket path 404.
- Published release 6 (a7a9c7f, 50 files) to beta and stable with `--private`. The first attempt failed with "uncommitted changes": the publish started before the wrangler.toml commit had landed.
- Checked the live feed from the app code: `latest` reports 6, and all files fetched and decrypted in 28 s. The firmware is byte-identical to the local build (501,696 B). 46 style packs.
- Built Clawd.app and a .dmg with the feed URL and token. `_feed_build.py` is git-ignored.
- Open: test an install on a real clock (the office network can't reach it). The branches are not merged to main.

> Historical work record — not durable authority. Prefer docs/specs/ADRs for current decisions.
