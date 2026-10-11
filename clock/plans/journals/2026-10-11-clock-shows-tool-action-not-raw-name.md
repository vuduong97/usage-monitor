---
title: Clock shows the tool's action, not its raw name
date: 2026-10-11
summary: daemon maps tool names to action words (Running/Reading/Editing/...) and failures to "<Action> failed"; raw name kept for clawd status
---

# Clock shows the tool's action, not its raw name

## What happened
While an agent ran a tool, the clock showed the raw tool name (`Bash`, `mcp__cry_opera__slide_bash`), and failures showed `Bash failed`. The user wanted a short, readable word. In an advise session they chose one verb per tool group with a `Working` fallback, and the same groups for failures.

## Changes (commit bdf8df2)
- `clawdmeter/daemon/clawd_events.py`: `ACTION_TEXT` (action -> running/failed text) and `TOOL_ACTIONS` (lower-cased tool name -> action) for both Claude Code and pi names; `tool_action()` strips `mcp__<server>__`. `Sess.action` feeds the `typing` text; `Sess.error_text` replaces `error_tool`. `Sess.tool` keeps the raw name for `clawd status`.
- The lookup uses the raw event field, before `ascii_text(..., 24)` truncation, so long MCP names still match.
- A `tool_error` without a name reuses the running tool's action; `stop_error` still shows `Error`.
- Tests: new parametrized cases; the old `Bash`, `Bash failed` and `Mcp__web__fetch` expectations were updated. 239 daemon + 7 integration tests pass. Live daemon kickstarted; `/hook/pi` tool_error `edit` -> `Edit failed`.

## Limits / next steps
- New tools show `Working` until they are added to `TOOL_ACTIONS`.
- Live `Running` was not seen on the real clock during the check because 3 sessions were open (`building` wins over `typing`); covered by unit tests.

> Historical work record — not durable authority. Prefer docs/specs/ADRs for current decisions.
