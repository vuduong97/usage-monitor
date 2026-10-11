# clawd-home

Private monorepo for the desk clock (JUZIPi SmallTV SD Pro, ESP8266, 240×240):
it shows Claude usage, and its Clawd now reacts to what Claude Code and pi are
doing, plus a few pet behaviours and `clawd` CLI pokes.

**Never pushed anywhere.** Every remote has its push URL set to `DISABLED`.
The animations in `desk.pak` are rendered from clawd-on-desk art (All Rights
Reserved, personal use only): they are generated on this Mac and never committed.

```
clawdmeter/      git subtree: Clawdmeter (macOS daemon in clawdmeter/daemon/)
smalltv/         git subtree: smalltv-mod fork running the Clawdmeter UI (env smalltv_clawd)
tools/desk/      clawd-on-desk SVG -> build/desk/desk.pak (gitignored)
integrations/
  claude/        Claude Code hook (clawd-event.sh) + settings.json merge installer
  pi/            pi extension (clawd-home.ts) + symlink installer
  cli/clawd      the `clawd` command
plans/journals/  session journals
```

## How it fits together

```
Claude Code hooks ─┐
pi extension ──────┼─ POST 127.0.0.1:47913 ─> daemon (LaunchAgent) ──> clock 192.168.1.48
clawd CLI ─────────┘                          ├─ /api/usage  usage bars (every 60 s)
                                              └─ /api/state  desk anim + text + dim
```

- The daemon owns all logic (`clawdmeter/daemon/clawd_events.py`). The clock
  only plays a named animation from `/desk.pak` with an optional text line,
  `MM:SS` countdown and brightness cap. It returns to the usage screens 60 s
  after the last `/api/state` (the daemon resends every 20 s while something is up).
- Layers, highest first: `clawd` poke > agent activity > focus timer > pet moment > sleeping.
- Agent priority: needs you (`notification`) > error > compacting (`sweeping`) >
  2+ subagents (`juggling`) > 1 subagent or 2 sessions (`headphones`) >
  3+ sessions (`building`) > tool running (`typing`) > `thinking` > done (`happy`, 4 s).
- The clock names the action, never the raw tool: Running (bash), Reading, Editing,
  Searching, Browsing, Planning, otherwise Working; a failed tool shows `Run failed`,
  `Edit failed`... or `Tool failed`. The table is `TOOL_ACTIONS` in `clawd_events.py`;
  `clawd status` still lists the raw tool name.
- pi "needs you" clears as soon as you go to that session (focus its terminal tab, click or
  press a key), even before answering, and stays off until the next question. Claude Code
  has no focus/key hook, so its "needs you" clears only on an answer.
- Pet: 23:00–07:00 brightness cap 15%; 5 min without keyboard/mouse → yawn → sleeping, back → wake + greeting;
  evening yawn 22:00–23:00; session ≥ 80% → `low battery`; quota reset → `double jump`;
  every 10 finished turns a day → `heart eyes`.
- Hooks are fire-and-forget: the daemon down costs each hook ~10 ms and changes nothing.

## Setup (this Mac)

```sh
# 1. daemon venv (Homebrew Python: it already has Downloads-folder access)
cd clawdmeter && /opt/homebrew/opt/python@3.12/bin/python3.12 -m venv daemon/.venv \
  && daemon/.venv/bin/pip install bleak httpx
#    LaunchAgent: ~/Library/LaunchAgents/com.user.claude-usage-daemon.plist -> clawd-home/clawdmeter/daemon/
launchctl unload ~/Library/LaunchAgents/com.user.claude-usage-daemon.plist
launchctl load -w ~/Library/LaunchAgents/com.user.claude-usage-daemon.plist

# 2. hooks + extension + CLI
python3 integrations/claude/install.py        # --uninstall to remove; backs up settings.json
sh integrations/pi/install.sh                 # symlink into ~/.pi/agent/extensions/, then /reload
ln -sfn "$PWD/integrations/cli/clawd" ~/.local/bin/clawd

# 3. animations (needs the clawd-on-desk checkout + Google Chrome)
cd tools/desk && npm install && cd ../..
CLAWD_ON_DESK_DIR=/tmp/clawd-on-desk node tools/desk/desk_to_pak.js   # -> build/desk/desk.pak
curl -u admin:<pass> -F "file=@build/desk/desk.pak" http://192.168.1.48/api/deskpak
```

Config (`~/.config/claude-usage-monitor/config`, all optional): `events = off`,
`event_port`, `quiet_start = 23:00`, `quiet_end = 07:00`, `quiet_dim = 15`,
`away_after = 300`, `milestone_turns = 10`.

## clawd

```sh
clawd status                 # what the clock shows + live sessions
clawd wave | clawd say "drink water" | clawd play "double jump"
clawd focus 25 | clawd focus stop
clawd anims
```

## Firmware

```sh
tools/flash.sh               # build + size check + OTA to /update + wait for the reboot (~35 s)
tools/flash.sh --no-build    # re-flash the last build
#  clock: CLAWD_CLOCK, else http_targets in ~/.config/claude-usage-monitor/config
#  web password (if enabled): CLAWD_AUTH=user:pass

# by hand:
cd smalltv && pio run -e smalltv_clawd                       # image must stay <= 524,288 B
curl -F "firmware=@.pio/build/smalltv_clawd/firmware.bin" http://192.168.1.48/update
tools/clawd_preview/run.sh                                   # desktop render of the screens
```

The flash layout stays stock 4m3m (1 MB sketch + 3 MB LittleFS, never formatted).
`/update` accepts the stock image for rollback; see `smalltv/CLAWDMETER.md`.
The clawd env drops newlib's float printf/scanf (`smalltv/no_float_io.py`):
`%f` prints nothing in this build.

## Tests

```sh
cd clawdmeter && uv run -q --python 3.12 --with bleak --with httpx --with pytest --with pystray --with Pillow python -m pytest daemon/tests -q
uv run -q --python 3.12 --with httpx --with pytest python -m pytest integrations/tests -q
node --test integrations/pi/clawd-home.test.ts
```

## Upstreams (pull only)

```sh
git subtree pull --prefix=clawdmeter upstream-clawdmeter main
git subtree pull --prefix=smalltv    upstream-smalltv    main
```
