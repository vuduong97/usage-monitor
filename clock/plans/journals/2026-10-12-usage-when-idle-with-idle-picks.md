---
title: usage when idle, with an idle pick every 3–6 minutes
date: 2026-10-12
summary: idle screen is now usage; one non-action animation (or yawn→sleeping→wake) slips in every 180–360 s; desk.pak gets coffee hand, shy, outlaw and loses doze/collapse
---

# Usage when idle, with an idle pick every 3–6 minutes

## What happened
In the advise session, the user asked for the clock to mostly show usage when nothing runs, with Clawd's animations slipped in now and then. Animations tied to an action (reading, typing, …) must not show. The user picked the animations by their clawd-on-desk SVG names: idle-living, idle-look, idle-bubble, coffee-hand, aegyo-shy, outlaw-bender, plus yawn → sleeping → wake as one chain. coffee-head-flip was dropped. Other decisions:
- the gap is 3–6 min, random;
- "Done!" shows for 5–10 s, then usage;
- being away looks the same as idle, and coming back still plays wake.

## Changes
- clawd-desktop 26ba32d, clawd-home 22342dd (`events.py` / `clawd_events.py`, same code):
  - `IDLE_ROTATION`, `SLEEP_CHAIN` and `idle_linger` are gone.
  - `Pet.continuous()` schedules one pick (`IDLE_PICKS`) every `IDLE_GAP` = 180–360 s, never the same pick twice in a row.
  - Each pick lasts exactly `PASS_SECS` (one pass, measured from desk.pak), then `a=""`.
  - Any other layer calls `hold_off()`, so the gap restarts.
  - `DONE_SHOW` is 7 s.
  - The push loop sleeps only until the pick ends, so the clear goes out right on time. The clock drops a desk view as soon as it gets `a=""`.
- New names are gated: the clock plays animation 0 (`thinking`) for a name it does not have. `StatePusher.probe_desk()` reads `desk` from `/api/heap` every 5 min. coffee hand, shy and outlaw are sent only when every answering clock reports ≥ 31.
- b7e37d1 desk.pak:
  - Added `coffee hand` (12 fps), `shy`, and `outlaw` (8 fps).
  - Removed `doze` and `collapse`.
  - 31 animations, 2,854,285 B; LittleFS has 3,096,576 / 3,121,152 B used.
- clawd-coffee-hand is animated with SMIL `<animate>`, and the renderer only stepped CSS animations, so the first build gave 1 frame. The new `smil` flag (per animation) also steps the SVG timeline.
  - The flag is not on by default: turning it on for every animation changed reading and typing boss, and reading failed `--verify` at 2.07%.
- Tests: clawd-desktop 183 and clawd-home 304 pass. The new tests cover:
  - picks over 3 h: one pass each, then usage; gaps inside 180–360 s; no repeats; no action-tied names;
  - an old pak skips the new names;
  - Done 7 s, then usage, and an agent restarts the gap;
  - away gives no endless sleeping, and coming back plays wake;
  - `probe_desk`, and the push loop's wake-up timing;
  - every idle pick exists in `desk_anims.js`.
- README and PROTOCOL updated in both repos (layers, the 31 names, the example sequence).

## Verified live
- Upload to 10.14.237.36: `{"ok":true,"anims":31}`, which took 147 s. `/api/heap` shows `desk` 31.
- Clawd.app rebuilt and reinstalled. Its log says "Clock desk.pak has 31 animations: all idle picks on".
- Posting `coffee hand`, `shy`, `outlaw` and `["yawn","sleeping","wake"]` to `/api/state` put that name in `deskAnim`, with no fallback to thinking; `loopMaxMs` was 306.

## Limits / next steps
- Not yet seen on the clock: a full pick from the app with no agent running (it needs 3–6 min of quiet), and whether ending at exactly one pass looks clean. If the end flickers, the fix is a firmware "one pass, then usage" flag.
- outlaw has alcohol and a cigarette. Reconsider before giving clocks to friends.
- Only ~24 KB is left on LittleFS. Another animation needs something else removed.
- One unexplained single test failure in the clawd-home suite did not come back in 50+ reruns. It has no name, because it happened before `-rf` was on.
- `plans/reports/261012-advise-idle-anim-pool-preview/` holds clawd-on-desk SVG copies (All Rights Reserved). It stays uncommitted.
