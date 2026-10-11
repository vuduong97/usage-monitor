#!/bin/sh
# clawd-event.sh — Claude Code hook: forward the hook payload (stdin JSON) to
# the clawd-home daemon on 127.0.0.1, which turns it into what the clock shows.
#
# Never blocks or fails the agent: no daemon -> curl gives up at once (refused)
# or after 0.3 s (stuck), and the script always exits 0 with no output.
# Registered for every event by integrations/claude/install.py.
port=${CLAWD_EVENT_PORT:-47913}
curl -s -o /dev/null --max-time 0.3 --connect-timeout 0.2 \
  -H 'Content-Type: application/json' --data-binary @- \
  "http://127.0.0.1:${port}/hook/claude" >/dev/null 2>&1
exit 0
