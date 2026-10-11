---
title: reading, debugger and ultrathink show when they apply
date: 2026-10-12
summary: daemon shows reading for read/search tools, debugger for debug prompts/skills (text Debugger), ultrathink after 30 s with no tool running even between tools, and lets these through the many-sessions animation with a count
---

# reading, debugger and ultrathink show when they apply

## What happened
The user asked for clawd-idle-reading while reading files, clawd-working-debugger while debugging, and clawd-working-ultrathink when thinking past 30 s. All three were already in desk.pak and partly wired, but the log since 23:01 showed them almost never (debugger 7, ultrathink 1, reading 0 out of ~540 state changes). There were three causes. 2+ sessions always showed `headphones`/`building` (215 of the 540). Ultrathink was measured from the last state change, and `tool_done` keeps the state `working`, so it only fired before the first tool of a turn. No rule mapped a read tool to `reading`. Scope was set in an advise session; at confirmation the user changed the debugger text to `Debugger` (keeping `Testing` for test runs).

## Changes (commits b8aa766, ce171cf)
- `clawd_events.py`: `Sess.running` counts started-but-unfinished tools (pi runs them in parallel), and `Sess.quiet_since` is when the count last reached 0, or the prompt / answer / compaction. `_session()` replaces `_working()`. Its order is ultrathink > thinking (before the first tool) > debugger (`Testing` / `Debugger`) > reading (`read` and `search` actions, text Reading/Searching) > typing boss > wizard > typing. `reading` sticks after the tool ends because `s.action` does.
- With 2+ active sessions, if the latest session's view is one of `SESSION_SHOWS_THROUGH`, it shows as `"<text> (N)"`. Subagents still win.
- Debug turns: `is_debug_prompt()` matches `/ak:debug`, `/ak:fix`, `/skill:ak-debug|fix`, `debug*`, `fix`, `bug(s)` and the accented `lỗi`. The unaccented `loi` is also `lời` (as in "trả lời"), so it does not count. pi expands skills into the prompt as `<skill name=...>…</skill>`. Those blocks are stripped and count only by name, because any skill body mentions fixes. `is_debug_skill_load()` catches the Claude `Skill` tool and reads of `ak-(debug|fix)/SKILL.md`. Claude prompts are classified in `claude_event`. The pi extension has the same rules (`isDebugPrompt`, `isDebugSkillRead`) and sends only `debug: true`.
- `snapshot()` adds `running` and `debug` for `/state`.
- Tests: 283 daemon (+26), 8 pi extension, 7 integration. Old expectations changed: Read/Grep now show `reading`, `Debugging` became `Debugger`, the event-server Grep case shows `reading`.
- Live check after kickstart, through `/hook/pi`: `reading - Reading`, `reading - Searching`, `debugger - Debugger`, `ultrathink - Thinking hard` after 31 s quiet, and `reading - Reading (4)` alongside this session.

## Limits / next steps
- If Claude Code fires `PreToolUse` without a matching `PostToolUse`/`PostToolUseFailure` (for example a denied permission), `running` stays above 0 and ultrathink cannot fire until the next prompt.
- The Claude `Skill` tool input key (`skill`) is assumed, not observed. Reads of SKILL.md are covered either way.
- "fix" in any prompt (for example "fix typo") makes the whole turn a debugger turn. This was the user's choice; drop `fix` from both regexes if it is too noisy.
- With many busy sessions the clock follows whichever session sent the last event, so it can switch every second (`MIN_DISPLAY_WORKING`).
- Running pi sessions need `/reload` to send the debug flag.

> Historical work record — not durable authority. Prefer docs/specs/ADRs for current decisions.
