---
title: hide question text, show a random phrase per question
date: 2026-10-12
summary: the clock no longer shows what Claude or pi asks; each new question picks one of 9 phrases, and the question text no longer leaves the hook
---

# Hide question text, show a random phrase per question

## What happened
The user did not want the clock to show the questions Claude or pi ask. In the advise session the scope was locked:
- Only question tools change: Claude `AskUserQuestion`, pi `ask_user_question` / `ask_user` / `question`.
- Permission requests ("Allow Bash: …") and notifications keep their text.
- The react left/right "?" animation stays.
- One phrase per question, picked at random (uniform), held until the question is answered, never the same twice in a row.

The user picked the phrases: "Hey! Got a question", plus groups B (Your turn! · Hey! Over to you · Your call, boss · Help me pick?) and C (Psst... a question · Knock knock! · Hmm, help me out? · Pick one for me?). The user's own idea, "Hey! I has a question." (22 chars), was replaced: the status line is mono 18 at 10.8 px per char in 224 px, so more than 20 chars scrolls.

## Changes
- clawd-desktop ad890ee, clawd-home 63eb5c0 (`events.py` / `clawd_events.py`, same code):
  - Added `QUESTION_TEXTS`.
  - Added `Sess.ask_text`, plus `Tracker(rng=…)` and `last_ask_text`.
  - The phrase is picked only when a question opens (`asking and not open_q`). A generic nudge, `attention_seen`, or a repeat call while the question is open keeps the same phrase.
  - When asking, `msg` is dropped, so an old extension that still sends the question text shows nothing of it either.
- `claude_event()` no longer reads `tool_input.questions`.
- pi extension `integrations/pi/clawd-home.ts`: `firstQuestion()` was removed, and the attention event sends only `{tool}`. The clawd-desktop copy is identical.
- Tests: clawd-desktop 229 pass and clawd-home 305 pass (2 skipped); the pi tests (8) pass in both repos.
  - A new test runs 40 questions with seed 7. It checks: no back-to-back repeat, every phrase shows up, and the phrase holds through a nudge or seen.
  - The pi test checks that "Which DB?" is not in any event that goes out.

## Verified live
Clawd.app was rebuilt from a clean worktree of HEAD and reinstalled. The worktree was used because clawd-desktop has uncommitted pywebview/keyring deps from the settings-window work. Fake events sent to 127.0.0.1:47913 gave these `/state` texts, and the "SECRET" question text never appeared:

| Fake event | `/state` text |
|---|---|
| 3 pi questions | Your turn! → Help me pick? → Psst... a question |
| 1 Claude question | Hey! Got a question |

## Notes
- The pi sessions that are already open run the old extension until `/reload`. The app ignores their text anyway.
- A `git stash` / `stash pop` in clawd-home briefly moved another session's uncommitted smalltv work. It came back intact, but next time do not stash in a shared tree; use a worktree or `git show HEAD:file` instead.
