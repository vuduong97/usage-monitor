---
title: clawd-desktop tray app for friends, and provision.sh for gift clocks
date: 2026-10-12
summary: new public repo clawd-desktop (Mac + Windows tray app that finds the clock and pushes usage + agent events), tools/provision.sh (stock clock to gift-ready clawd), PROTOCOL.md v1 in both repos; this Mac moved from the LaunchAgent daemon to /Applications/Clawd.app
---

# clawd-desktop and provision.sh

## What happened
The advise session set the goal: friends get a pre-flashed clock and install a `.exe`/`.dmg` from GitHub Releases, with no IP or config to type. That needs two repos. clawd-home stays private (firmware, art, provisioning). The new repo `../clawd-desktop` is meant to be public. It holds only the daemon code, hooks and the tray app. Run with `ak:cook --auto`.

## clawd-desktop (local only, not pushed: repo name/owner still to be chosen)
- `usage.py` / `events.py` are copies of the clawd-home daemon. Changes in the copies:
  - `bleak` is optional.
  - The publisher takes its targets from a callable.
  - Windows idle time comes from `GetLastInputInfo`.
- `discovery.py` (ClockFinder) picks the clock in this order:
  1. `http_targets` from config
  2. an address typed by the user
  3. the remembered clock
  4. mDNS `_clawdmeter._tcp`
  5. a /24 scan of `/api/heap` after 15 s
- ClockFinder failure handling:
  - It drops a clock after 3 failed pushes and holds it off for 60 s.
  - It detects macOS blocking the LAN: every probe fails with EHOSTUNREACH. It then rescans every 30 s instead of every 5 min.
- `engine.py` runs the finder, publisher, EventServer and StatePusher in one thread.
- `tray.py` is pystray with a Vietnamese menu.
- `hooks.py` merges Claude Code hooks into `settings.json` with a backup.
  - macOS: sh + curl script copied to `~/.config/claude-usage-monitor/clawd-event.sh`.
  - Windows: exec-form `curl.exe` with `async: true`. The exec-form `args` and `async` fields were checked against the hooks docs.
  - The merge also replaces the old clawd-home hook entries.
- `autostart.py`: LaunchAgent `vn.clawd.desktop` (KeepAlive only after a crash) on macOS, HKCU Run on Windows.
- Packaging:
  - PyInstaller spec builds a menu-bar-only `.app` with `NSLocalNetworkUsageDescription` and an ad-hoc signature.
  - `build_mac.sh` makes the DMG.
  - Inno Setup does a per-user install. Its uninstaller runs `Clawd cleanup`.
  - CI tests on 3 OSes. The release workflow runs on `v*` tags and builds arm64 + Intel DMGs and the `.exe`.
  - `tools/check_public.py` fails CI on `.bin`/`.pak`/fonts/art/large blobs.
- Tests: 228 pass. Live checks:
  - mDNS found the clock in about 6–10 s.
  - The scan found it in about 2 s.
  - The packaged app, run from a shell, pushed usage through cliproxy.
- 5 commits (ea64c3a..bb0dee3).

## clawd-home (commits ac4db5e, 910ceb5)
- `tools/provision.sh <ip>` steps:
  1. Reads the stock `javascript.js`. It stops unless `/update_ota`, the `update` field and the `SDP` filename check are all there.
  2. Uploads `clawd.pak` and a bench-WiFi `config.json` through the stock `/upload`.
  3. Runs OTA as `SDP_clawd.bin`.
  4. Runs `stock_fs.sh` backup + delete, with a per-clock backup in `.stock-backup/provision/`.
  5. Uploads `desk.pak`.
  6. One `/api/config` POST sets hostname `clawd-<chip id>`, AP `Clawd-<ID>` with a random WPA2 password, admin login with a random password, and `wifi:[]`. The clock reboots into SETUP MODE.
  7. Prints a Vietnamese card and appends it to `cards.tsv` (mode 600).
- Each step checks state first, so a rerun resumes. `--dry-run` was tested against the real clawd clock (resume path) and a fake stock server, including the stop on a changed OTA page. shellcheck is clean.
- PROTOCOL.md v1 is identical in both repos. README has a new "Clocks for friends" section.

## This Mac
- Rollback copies:
  - Old LaunchAgent `com.user.claude-usage-daemon` was booted out. Its plist is kept as `~/Library/LaunchAgents/.com.user.claude-usage-daemon.plist.bak-clawd-desktop`.
  - The stale `http_targets = 192.168.1.48` was commented out. The config backup is `config.bak-clawd-desktop`.
- `/Applications/Clawd.app` is installed and its autostart LaunchAgent is written.
- Hooks in `~/.claude/settings.json` were switched to the new script. A backup was made.
- The pi extension and the `clawd` CLI keep working: same port 47913.
- **Open:** launched by launchd/Finder, the app is blocked by macOS Local Network privacy (EHOSTUNREACH on every LAN address). Launched from a shell, it reached the LAN. The prompt has to be answered at the screen, which was locked during the run. Until then the clock gets no pushes. Fix: allow it in the prompt, or System Settings → Privacy & Security → Local Network → Clawd.

## Limits / next steps
- Not tested on Windows at all: the build, tray, `curl.exe` hooks, idle time and Inno Setup only run in CI.
- No real stock clock was available. The flash path of `provision.sh` (`/upload`, `/update_ota`, first boot) is untested end to end.
- Clawdmeter upstream has no license file. Publishing its daemon code is a grey area, noted in NOTICE.
- Digest auth on the captive portal in iOS/Android mini-browsers is untested. README tells friends to open `http://192.168.4.1` in a normal browser if needed.
- Keychain prompt for "Claude Code-credentials" on another Mac account is not verified. This Mac uses cliproxy.
- To push: choose the GitHub owner/name, `gh repo create`, push, tag `v0.1.0`.
