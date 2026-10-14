---
title: Clock-screen renderer delivered through the firmware feed
status: implemented (not committed; owner steps pending)
created: 2026-10-14
repos: [usage-monitor (public), clawd-home (private)]
---

# Renderer via feed

## Contract
- **Outcome:** a user who only downloads the public app sees real previews (animated, their own
  usage) of every usage style; the author ships new styles + matching renderer from remote.
- **Constraints:** firmware source stays private (only compiled binary leaves the repo); nothing to
  install besides the app; preview matches the release's firmware/style API; offline/old Worker/
  no renderer → today's fallback (canvas/placeholder), never an error.
- **Non-goals:** WASM; JS approximation per style; publishing a release or deploying the Worker
  (owner's step, needs Cloudflare credentials).
- **Backward compat (hard):** shipped apps 0.1.x reject any `files[]` row with an unknown kind,
  so renderers go in a NEW top-level manifest array `renderers` (old apps ignore unknown keys).

## Design
- Release artifact per platform: `renderer/win-x64.zip`, `renderer/mac.zip` = the folder
  `tools/build_preview.py` already makes (exe, clawd.pak, styles/, preview.json).
- Manifest: `"renderers": [{name, kind:"renderer", platform, size, enc, sha256, crc}]`, same
  encryption/AAD as other files.
- Worker grant signs `files` + `renderers`.
- App: after the style packs, fetch this platform's zip (encrypted cache as today), check it,
  extract to `~/.config/claude-usage-monitor/renderer/<sha16>/` with a hash marker, verify every
  file's hash before each load, `load_renderer(folder)`; set it on SettingsApi, drop frame cache.
  Bundle renderer (private build) still wins when the bundle is the source.

## Phases
1. [x] usage-monitor `tools/build_preview.py`: `--zip OUT`, `-s` on Windows, `CXXFLAGS` env.
2. [x] usage-monitor `feed.py`: parse `renderers` (bad rows → ignored, never reject manifest),
       `Feed.renderer(rel, platform)` → folder; zip safety (names, sizes, expected files).
3. [x] usage-monitor `settings_api.py`: use feed renderer for the feed release.
4. [x] usage-monitor tests (feedkit renderers, parse compat, extract, settings integration).
5. [x] clawd-home `feed/release.mjs` `--renderer platform=zip`; Worker grant; worker tests.
6. [x] clawd-home `tools/release.sh` picks `build/renderer/*.zip`, checks commit; CI workflow
       `renderer.yml` (windows + macos) builds the zips; `tools/fetch_renderer.sh`.
7. [x] Docs: feed/README.md, feed.py + preview.py docstrings.
8. [x] Verify: pytest, node tests, real cross-built win exe end-to-end if a compiler is available.

## Success metrics
- Public app + feed release with renderers: log `preview: feed renderer <commit>`,
  `usage_preview()` → `exact: true` for every pack.
- Old manifest (no `renderers`) and old app with new manifest: still parse/update.
- All existing tests green; `check_public.py` passes.

## Result (2026-10-14)
- Verified: pytest green (except 1 test of unrelated parallel prefs work); node 12/12; real MinGW-built
  clawd_render.exe (360 KB zip, KERNEL32+msvcrt only) -> FakeFeed -> unpack -> usage_preview: 47/47
  styles exact + animated; old HEAD feed.py parses a manifest with `renderers`.
- Review (reviewer subagent): no blockers; fixed M1 (unhashable kind), M2 (desktop_ref input +
  renderApi/desktop in preview.json), zip error classes, renderer/frames swap race, check_public bans,
  smoke test covers --clip and x86_64, docs wording, --clear honours --out.
- Owner steps: commit+push both repos; `npx wrangler deploy` (feed/worker); tools/fetch_renderer.sh;
  next tools/release.sh. Accepted risk: unsigned manifest -> feed/Cloudflare compromise = code exec.
- Not done: renderer download still runs under the feed lock (zip ~360 KB, acceptable).
