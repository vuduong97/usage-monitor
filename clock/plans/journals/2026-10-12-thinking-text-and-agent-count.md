---
title: thinking shows "Thinking" and the clock counts agents
date: 2026-10-12
summary: thinking animation gets the text Thinking; sessions, subagents and child processes are counted together as agents (2 headphones, 3+ juggling, building dropped)
---

# thinking shows "Thinking" and the clock counts agents

## What happened
The user wanted the text `Thinking` under clawd-working-thinking from the prompt to the first tool. In the advise session they also changed the multi-agent rule. Before, subagents (headphones 1, juggling 2+) and sessions (headphones 2, building 3+) were counted separately. Now there is one agent count. The user confirmed the table at the confirmation step. For the case that table did not cover (no main session running and only subagents or children left), the advice proposed 1 → headphones and 2+ → juggling. The user did not object, so that is what shipped.

The work waited for a parallel session (the "question" view, commit cd28df3) that was editing the same three files. It started only after that session committed.

## Changes (commit 3b578ba)
- `clawd_events.py` `_session()`: thinking returns `View("thinking", "Thinking")`.
- `view()`: agents = active main sessions + `subs` + live `sub` child sessions. With 1 agent, that session's view shows. Otherwise the latest main session's ultrathink, debugger or reading shows through as `"<text> (N)"`, now also when the extra agents are subagents. If nothing shows through, it is headphones "2 agents" or juggling "N agents". With no main session running, `extra = agents`, so 1 → headphones "1 agent" and 2 → juggling.
- `building` is no longer chosen. It stays in desk.pak and in the `clawd play` list.
- Tests: 288 daemon tests pass. New tests cover agent counts, a subagent counted as an agent, and children outliving the main session. The old building/sessions/subagent expectations were rewritten. The priority test now expects `Debugger (2)` after a failed tool, because that session's debug turn shows through.
- README agent priority updated.
- Live check after kickstart: `thinking - Thinking`, `headphones - 2 agents`, `juggling - 3 agents`, `reading - Reading (4)`. This pi session was counted as well.

## Limits / next steps
- The clock no longer tells subagents apart from other sessions.
- A short turn shows Thinking for only about 1 s (`MIN_DISPLAY_WORKING`).
- Pyright still reports the existing `dict | None` / `View | None` warnings in the test file. They come from the pattern the tests already used and were not touched.
