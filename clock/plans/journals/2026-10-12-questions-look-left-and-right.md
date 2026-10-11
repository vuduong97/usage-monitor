---
title: questions look left and right
date: 2026-10-12
summary: a question from an agent (pi ask_user_question/ask_user/question, Claude AskUserQuestion) cycles react left and react right under the question; other "needs you" states keep notification
---

# questions look left and right

## What happened
The user wanted `clawd-react-left`/`clawd-react-right` when an agent asks a question. Both SVGs pop a blue pixel `?` and were already in desk.pak, where pokes used them. Scope was set in an advise session. At the first question the user picked alternating notification and react, then corrected it to react only. They also limited it to question tools (permissions, pi dialogs and Claude notifications keep `notification`) and kept desk.pak as it is, about 5.5 s a side: a 2.5 s one-shot with the `?`, then a 3 s blink loop.

## Changes (commit cd28df3)
- `clawd_events.py`: `QUESTION_TOOLS` + `is_question_tool()` (lowercase, MCP prefix stripped). `Sess.asking` is set on `attention` from a question tool. A tool-less `attention` (a Claude `Notification`) while a question is open keeps it. The flag is cleared on `attention_done`, on any state outside attention/waiting (`prompt`, `tool_done`, `stop`, `abort`...), and on TTL expiry.
- `Tracker.view()`: if the latest attention session is asking, it returns `View("question", msg or "Has a question", seq=("react left", "react right"), rep=True, urgent=True)`. Otherwise it returns `notification` as before. `question` has its own name, so holds and `clawd status` do not mix with pokes.
- `MIN_DISPLAY["question"] = 5.0` and `question` is in `FOCUS_KEEPS_TEXT`. Without these the question would hold only 1 s, and focus mode would replace its text with "Focus".
- No pi extension or Claude hook change: both already send `tool` with the attention. No pak or firmware change.
- Tests: 286 daemon (+3), 7 integration. README agent-priority notes updated.
- Live check after kickstart, through `/hook/pi`: `/state` returned `question`, `seq ["react left","react right"]`, `rep true`, `txt "Which DB?"`. The clock's `/api/heap` reported `anim: "react right"` seconds later.

## Limits / next steps
- Not observed: whether Claude Code fires a `Notification` while `AskUserQuestion` is open. If it does, the question view stays because of the tool-less rule.
- The `?` is visible only about 1.3 s of every 5.5 s. If questions are easy to miss, add separate `ask left`/`ask right` pak entries without the blink loop. Pokes keep their animations, but the change needs a desk.pak rebuild and upload.
- pyright reports `Optional` narrowing on every `ingest(claude(...))` in the test file (an existing pattern, 212 before this change). The project does not run pyright.

> Historical work record — not durable authority. Prefer docs/specs/ADRs for current decisions.
