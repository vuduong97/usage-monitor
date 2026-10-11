---
title: Clear pi needs-you when the user goes to the session
date: 2026-10-11
summary: pi extension watches stdin passively; first focus-in/click/key sends attention_seen; daemon hides that session until the next question
---

# Clear pi needs-you when the user goes to the session

## What happened
"pi needs you" on the desk clock stayed up until the question was answered (or 30 min). The user wanted it gone as soon as they switch to that session's terminal tab, click, or press a key, and to stay off for that question.

## Key finding
`ctx.ui.onTerminalInput` cannot see focus-in or clicks in fullscreen pi: the viewport input listener is registered first and consumes `ESC[I` and SGR mouse events. pi itself reads input with `process.stdin.on("data")`, so a passive extra listener sees every raw chunk without touching pi's input. Verified in a PTY with real `pi` fullscreen: `?1004h` is on, hover and focus-out are ignored, focus-in sends exactly one `attention_seen`, closing the dialog sends `attention_done`.

## Changes (commit 0a64ed2)
- `integrations/pi/clawd-home.ts`: `isUserInput()` whitelist (focus-in, SGR press without motion/wheel bits, keys, typed text); terminal replies (OSC/DCS/APC, `?`-private CSI, cell size `t`, CPR `R`) do not count. Listener only in TUI with TTY stdin, added on the first attention, removed on `end`; one `attention_seen` per question.
- `clawdmeter/daemon/clawd_events.py`: new `attention_seen` event and hidden `waiting` state (expires like attention; `attention_done` → thinking).
- Tests: 6/6 node tests, 216 daemon tests pass; live daemon restarted and checked over /hook/pi + /state.

## Limits / next steps
- Depends on pi reading stdin in flowing mode (undocumented). If that changes the feature silently falls back to old behaviour; switch to an official focus event if pi adds one.
- No focus-in if the question appears while the tab is already focused: a key or click is needed.
- Claude Code is unchanged (no focus/key hook).
- Running pi sessions need `/reload` to load the new extension.

> Historical work record — not durable authority. Prefer docs/specs/ADRs for current decisions.
