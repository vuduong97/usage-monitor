# 2026-10-12: desk v2, 30 clawd-on-desk animations at 1:1

Commits `00f2c48` (firmware + tools), `e7ab20d` (daemon + integrations), `f053935` (docs). Nothing has been pushed. The artwork, `desk.pak` and the stock backup are not committed.

## What changed

- **The pak format is CLDK v2.** Frames are 240 px at 1:1, 15–20 fps, with full natural loops (≤ 16 s).
  - Each frame is a row delta: skip, literal and run ops, with 16 colours per animation.
  - The pak is 2,818,166 B for 30 animations.
  - Every frame is within 2% of the Chrome render.
  - `desk_check` confirms the firmware decoder matches the encoder's CRCs.
- **The firmware streams the pak.** It reads 128 B at a time and pushes row spans.
  - `/api/state` takes a sequence: `a` (up to 4 names), `rep` and `u`.
  - Switches happen at the end of a pass, or at once when `u` is set.
  - `/api/heap` reports draw p50/p95/max and lateness.
  - The v1 desk path is gone.
- **Stock files are off the clock.** The 30-animation pak did not fit next to the stock files (1.5 MB).
  - New auth-gated `/api/fs/{list,get,raw,put,rm}` plus `tools/stock_fs.sh` back the files up to `.stock-backup/fs-20261012-000754/`. The backup was verified against the raw partition using mklittlefs.
  - The stock files were then deleted from the clock.
  - **Run `restore` before flashing the stock firmware.** The restore path has not been run end to end yet; only `check` has.
- **The daemon now drives behaviour the way clawd-on-desk does:**
  - The sleep chain plays as one sequence, and waking plays `wake` → `happy`.
  - An idle rotation runs for `idle_linger` (300 s) after the last agent event.
  - `minDisplay` holds come from the clawd-on-desk theme.
  - New triggers: ultrathink, debugger (test runs, plus the rest of the failing turn and the next turn), dizzy, typing boss, wizard, and `clawd poke` (react left/right, annoyed).

## Measured on the device

- Draw time p95 ranges from 11 ms (`typing`) to 45 ms (`building`). `lateP95` is 5 ms.
- After a reboot, `minHeap` is at least 22.5 KB, and `contFree` is 2,272 B of the 4 KB `loop()` stack.
- The image is 511,168 B, about 13 KB under the 524,288 B OTA budget.

## Open: one unexplained restart

- The 1 h soak was skipped at the user's request.
- In its first 3 minutes, the clock restarted once at about 00:44:40 while it played `typing`. The reported reason was "Software/System restart".
- On this core, a panic, abort or `cont` stack-guard failure restarts through `system_restart_local`. The next boot then reports the same reason as `ESP.restart()`.
- `ESP.restart()` is only reachable through `scheduleReboot`, and nothing called it.
- Calling `/api/fs/list` (run just before the restart) three times did not reproduce it.
- **Added `CrashLog`** to catch the next one:
  - `custom_crash_callback` stores the reason, exccause, epc, excvaddr and the code addresses on the stack in RTC memory.
  - A planned restart leaves a marker.
  - `/api/heap` then shows `crash`, and `reset` reads `Crash (panic/abort/stack)`.
- **Next time:** read `crash` from `/api/heap` and run the `sp:` addresses through `xtensa-lx106-elf-addr2line -e .pio/build/smalltv_clawd/firmware.elf`.
