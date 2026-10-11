# 2026-10-11 — clawd-home: Clawd on the clock reacts to Claude Code / pi, pet moods, `clawd` CLI

## What changed
- New private monorepo `~/Downloads/code/clawd-home`: `clawdmeter/` and `smalltv/` as full-history
  `git subtree`s (subtree pull from both GitHub upstreams tested, rc=0). All push URLs disabled.
- `tools/desk/desk_to_pak.js`: renders clawd-on-desk SVGs (CSS keyframes, paused + stepped in system
  Chrome via puppeteer-core) into a 60×60-cell, ≤16-colour, RLE `desk.pak` (CLDK v1). 19 animations,
  136,400 B. Output gitignored (art is All Rights Reserved; personal use only).
- Firmware: full-screen desk view (`POST /api/state {a,txt,cd,dim,ttl}`, `/desk.pak`,
  `POST /api/deskpak`), brightness cap, fs stats in `/api/heap`. Splash + mascot share one frame pool.
- Daemon `clawd_events.py`: 127.0.0.1:47913 event server (`/hook/claude`, `/hook/pi`, `/poke`, `GET /state`,
  Origin header → 403), per-session tracker with TTLs and the priority order, pet (quiet hours, HID idle →
  sleep/wake/greeting, low battery, quota reset, 10-turn milestone), pokes + focus timer, latest-wins
  state pusher (0.3 s coalesce, 20 s keep-alive, 1.5 s timeout).
- Integrations: `clawd-event.sh` (curl 0.3 s, always exit 0) + `install.py` merge into
  `~/.claude/settings.json` (backup, idempotent, atomic); pi extension `clawd-home.ts` (symlinked);
  `clawd` CLI (symlinked to `~/.local/bin`).
- LaunchAgent now runs `clawd-home/clawdmeter/daemon` with a Homebrew-Python venv.

## Problems hit
- **OTA ours→ours "Not Enough Space"** at 521,712 B: `/update` reserved `freeSketchSpace − 4 KB`, so a
  running ~521 KB image cannot accept itself in the 1 MB 4m3m slot (limit 520,192). Fixed by dropping the
  core's forced `-u _printf_float -u _scanf_float` (`smalltv/no_float_io.py`, no `%f` in the clawd build)
  → **504,992 B**, and using the full free slot (Updater::begin already refuses overlap). Verified:
  old→new and new→new OTA both OK. Lesson: budget = what the *running* image's handler accepts, not 1 MB/2.
- **Daemon hung at Python startup** after moving the LaunchAgent: macOS TCC "access Downloads folder"
  prompt for uv's python binary blocked `open(pyvenv.cfg)`; later requests (even Homebrew python, even the
  old path) queued behind it. Visible only via `sample <pid>` (stuck in `getpath_readlines`) and
  `log show --predicate 'subsystem == "com.apple.TCC"'`. User clicked Allow; venv rebuilt on Homebrew python.

## Verified
- Daemon 213 passed / 2 skipped; integrations 7 passed; pi extension 3 passed (node --test).
- Hook with daemon down ≈10 ms; prompt → `thinking` on the clock in 0.75 s; permission → `notification`
  with text; `SessionEnd` → back to usage; `clawd say` works end to end.
- All 19 animations played on hardware via `/api/state`; minHeap 23,096 B (≥ 20 KB), no resets.
- `git ls-files` contains no desk.pak / clawd-on-desk output.

## Not verified (hardware / user)
- Visual quality and frame rate on the panel; 24 h heap soak; stock rollback with the new image size.
- Real Claude Code session through the installed hooks; pi extension after `/reload` (pi-subagents children).
- Pet behaviours that need time: sleep after 5 min idle, morning greeting, low battery, quota reset, milestone.
