---
title: random variants for long agent animations
date: 2026-10-12
summary: the clock plays the current action's animation 3 passes, then one same-meaning variant at random (new state key v); a debugging turn keeps the action's animation and mixes the debugger in. Flashed, but the clock dropped off the office WiFi right after, so the effect is not seen on the device yet
---

# random variants for long agent animations

## What happened
The user said Running and Debugger sat on one animation for a long time. The scope was locked in an advise session first:
- The animation for the current action stays primary. It plays 3 passes, then one same-meaning variant picked at random.
- The clock does the picking (firmware), so a pass is never cut. The user chose this over a daemon-only option.
- Reading and thinking get no variants, because they are short (the user's correction).
- In a debugging turn, the current action wins: reading shows `reading` with the text "Debugger", and `debugger` becomes its variant. A test run still shows `debugger`.
- headphones/juggling are unchanged. Friends' clocks are not reflashed, because the firmware is still in development.

## Changes
- **Daemon** (clawd-desktop `83b25f3`, clawd-home `e58bd49`):
  - `View.variants` is sent as `v`. `VARIANTS`: typing → building/carrying/typing boss; typing boss → typing/building; wizard → typing/building/carrying; debugger (Testing) → typing/building; ultrathink → thinking.
  - `DEBUG_VARIANTS = (debugger,)`.
  - `View.debug` keeps a debugging session showing through many agents.
  - The `_hold` reset and the `StatePusher.key` comparison now include `v`.
- **Firmware** (`4ae603e`):
  - `ClawdState.var`/`varCount` are parsed from `v` (at most 3).
  - The `DeskSeq` comparison includes the variants, so a change of `v` alone is taken over.
  - `deskPassEnd` counts passes of a single non-rep animation and picks a variant with `clawdRandom` (Arduino `random()` = RANDOM_REG32 without randomSeed), never the previous one. The variant plays one pass, then the primary restarts from frame 0.
  - `/api/heap` gains `deskAnim`. Image is 512,240 B.
- `PROTOCOL.md` documents `v` and `deskAnim` in both repos.

## Verification
- clawd-desktop: full suite passes. clawd-home daemon tests: 298 passed, 2 skipped (needs `--with bleak --with pillow`). A new test checks every variant name against `tools/desk/desk_anims.js`.
- `tools/clawd_preview`, new `checkVariants()`:
  - One typing pass is 9,990 ms. Each of 20 cycles plays typing for 30,000 ms, then a variant.
  - There are no repeats in a row, and all 3 variants appear.
  - Without `v`, typing just loops. A rep sequence ignores `v`.
- `desk.pak` flags: only wake, double jump and annoyed are one-shot. Every primary loops, so it always reaches its variant.
- Pass lengths (desk_check): typing 10 s, debugger 14 s, ultrathink 3.4 s, typing boss 5.6 s, building 6 s, carrying 2.5 s, thinking 12 s.
- Live Clawd.app after the reinstall, through `/hook/pi`:
  - Running: `typing [building, carrying, typing boss]`.
  - Testing: `debugger [typing, building]`.
  - Debug + read/bash: `reading`/`typing [debugger]` with the text "Debugger".

## Open
- Flashed with `CLAWD_CLOCK=10.97.44.122 sh tools/flash.sh`. The clock replied "OK, rebooting", then never came back on 10.97.44.0/23 (ARP incomplete, no host answers `/api/heap`). This looks like the office WiFi isolation seen this morning, but it is not proven that the new image boots.
- Next time it is reachable:
  - Check that `version`/`uptime` are fresh and there is no `crash`.
  - Poll `deskAnim` during a long Running turn: expect about 30 s of typing, then a variant.
  - Look at the screen.
