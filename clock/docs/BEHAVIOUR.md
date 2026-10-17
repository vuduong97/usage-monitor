# How the clock behaves under the Clawd desktop app

The app (repo `clawd-desktop`, `clawd_desktop/events.py`) owns all the logic; the clock only plays what it is sent.


```
Claude Code hooks ─┐
pi extension ──────┼─ POST 127.0.0.1:47913 ─> Clawd desktop app ──> clock (_clawd._tcp)
clawd CLI ─────────┘                          ├─ /api/usage  usage bars (every 60 s)
                                              └─ /api/state  desk anim(s) + text + dim
```
- The clock only plays animations from `/desk.pak` (31 of them, full screen at 1:1, 8–20 fps)
  with an optional text line, `MM:SS` countdown and brightness cap. It returns to
  the usage screens 60 s after the last `/api/state` (the app resends every 20 s
  while something is up).
- `/api/state` `a` is one name or a sequence of up to 4: each plays one full pass and
  the last one loops (`"rep":true` cycles the whole list). A new state switches at the
  end of the current pass (at most ~2 s later); `"u":true` switches at once.
- Layers, highest first: `clawd` poke > agent activity > focus timer > pet moment >
  idle pick > usage screens. When nothing else is up the clock shows usage, and every
  3–6 min (random) Clawd slips in one idle pick for one pass, never the same twice in a
  row: `idle`, `look`, `bubble`, `coffee hand`, `shy`, `outlaw`, or `yawn` → `sleeping`
  → `wake`. Nothing tied to an agent action. Any other layer cancels the pick and the
  gap starts again once the screen is free. `coffee hand`/`shy`/`outlaw` are only sent
  when the clock's pak has them (`desk` ≥ 31 in `/api/heap`, read every 5 min).
- Agent priority: needs you > `dizzy` (3 errors in 2 min) > error >
  compacting (`sweeping`) > agents > done (`happy`, 7 s, then usage). Agents are the running main
  sessions plus their subagents plus live child processes (`pi -p` from a subagent): one
  shows that session's animation; 2 show `headphones` ("2 agents"); 3+ show `juggling`
  ("3 agents"). With no main session running, 1 leftover agent shows `headphones` and 2+
  `juggling`. `building` is only played on request.
- Needs you: a question (pi `ask_user_question`/`ask_user`/`question`, Claude
  `AskUserQuestion`) cycles `react left` ⇄ `react right` (Clawd looks around with a `?`,
  ~5.5 s a side) under the question; a permission request, a pi dialog or a Claude
  notification shows `notification`. `clawd status` names the question view `question`.
- One session, highest first: `ultrathink` ("Thinking hard": no tool running for 30 s,
  counted from the prompt or the last tool to finish) > `thinking` ("Thinking", before the first tool) >
  `debugger` ("Testing" while a test command runs; "Debugging" for the rest of a turn after an
  error and the whole next one, a turn whose prompt says `/ak:debug`, `/ak:fix`, debug, fix,
  bug or `lỗi`, or once the agent loads the ak-debug/ak-fix skill) > `reading` (the last tool
  read or searched: Reading/Searching) > `typing boss` (10 edits in a row) > `wizard`
  ("Long task": a turn running 30 min) > `typing`. The text always names what the
  animation shows, and a variant mixed in (`v`) shows the same thing: a keyboard
  action (Running/Editing...) only swaps `typing` and `typing boss`; plain "Working"
  also mixes in `building`/`carrying`.
  With 2+ agents, the latest main session's `ultrathink`/`debugger`/`reading` still shows,
  with the agent count: `Reading (3)`. Prompt text never goes further than the hook: the app
  (Claude) or the pi extension only keeps whether it asks for debugging.
- Holds (clawd-on-desk's `minDisplay`): an agent animation stays at least 1 s
  (needs you / error / dizzy 5 s, compacting 5.5 s, done 4 s) unless a poke comes.
- The clock names the action, never the raw tool: Running (bash), Reading, Editing,
  Searching, Browsing, Planning, otherwise Working; a failed tool shows `Run failed`,
  `Edit failed`... or `Tool failed`. The table is `TOOL_ACTIONS` in `events.py` in clawd-desktop;
  `clawd status` still lists the raw tool name.
- pi "needs you" clears as soon as you go to that session (focus its terminal tab, click or
  press a key), even before answering, and stays off until the next question. Claude Code
  has no focus/key hook, so its "needs you" clears only on an answer.
- Pet: 23:00–07:00 brightness cap 15%; 5 min without keyboard/mouse (or a locked Mac) changes
  nothing on screen (usage + idle picks), back → wake → happy + greeting;
  `clawd poke` → `react left`/`react right`, 3 pokes in 5 s → `annoyed`;
  evening yawn 22:00–23:00; session ≥ 80% → `low battery`; quota reset → `double jump`;
  every 10 finished turns a day → `heart eyes`.
- Hooks are fire-and-forget: the app being down costs each hook ~10 ms and changes nothing.
