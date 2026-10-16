# macOS migration and update procedure (phase 14)

Copy-paste procedure for the **owner**, to run later on an **Apple-silicon Mac, macOS 15**, ideally
a fresh user account or a VM-like clean Mac (a second macOS user is enough: separate `~`, separate
login keychain, separate LaunchAgents). Nothing here is run by the agents.

**mac x64 is the approved exception** (no machine): only the CI build and signature checks of section 7.

Companions: [`windows.md`](windows.md) (same cases, same numbering, same evidence rules),
[`fork-release.md`](fork-release.md) (how the test releases are made), `reports/migration-report.md`,
`reports/parity-checklist.md`.

## 0. Ground rules

* Test releases live on **your fork** (never the main repo): see `fork-release.md`. 0.1.4 is pointed
  at the fork by `update_repo` in `~/.config/claude-usage-monitor/config`.
* The Rust build must be signed with the **same Developer ID Team ID as 0.1.4**: `swap.sh`/the
  updater compare Team IDs and refuse anything else (an ad-hoc build can never be an update).
  Get the Team ID from the installed 0.1.4: `codesign -dv --verbose=2 /Applications/Clawd.app 2>&1 | grep TeamIdentifier`.
* Fixed names (frozen contracts): bundle id `vn.clawd.desktop`, binary `Clawd.app/Contents/MacOS/Clawd`,
  LaunchAgent `~/Library/LaunchAgents/vn.clawd.desktop.plist` (byte-copy of the Python one:
  `RunAtLoad`, `KeepAlive {SuccessfulExit=false}`, `ThrottleInterval 30`, `ProcessType Interactive`, no arguments),
  keychain service `vn.clawd.desktop`, config `~/.config/claude-usage-monitor/`, event port 47913.
* Record prompts: every **Keychain** prompt and every **Local Network** prompt (macOS 15 asks the
  first time the app browses mDNS) goes into the report with a screenshot. Prompts are not failures
  by themselves; a prompt that did not exist in 0.1.4 is noted for the release notes.

## 1. Read-only verification block (paste into Terminal; changes nothing)

```sh
verify() {   # usage: verify <label>      -> prints a block; diff two blocks with: diff before.txt after.txt
  echo "== $1 $(date -u +%FT%TZ)"
  APP=/Applications/Clawd.app
  echo "-- bundle";    /usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' -c 'Print :CFBundleShortVersionString' -c 'Print :CFBundleExecutable' "$APP/Contents/Info.plist"
  echo "-- signature"; codesign --verify --deep --strict --verbose=2 "$APP" 2>&1 | tail -3
  codesign -dv --verbose=2 "$APP" 2>&1 | grep -E 'Identifier|TeamIdentifier|Authority|flags|Timestamp'
  echo "-- gatekeeper"; spctl -a -t exec -vv "$APP" 2>&1
  echo "-- notarization ticket"; xcrun stapler validate "$APP" 2>&1 | tail -1
  echo "-- one bundle?"; mdfind 'kMDItemCFBundleIdentifier == "vn.clawd.desktop"' 2>/dev/null
  echo "-- launch agent"; PL=~/Library/LaunchAgents/vn.clawd.desktop.plist; ls -l "$PL" && shasum -a 256 "$PL" && plutil -p "$PL"
  launchctl print "gui/$(id -u)/vn.clawd.desktop" 2>&1 | grep -E 'state|program|arguments|path' | head -8
  echo "-- keychain (attributes only, no secret, no prompt)"; security find-generic-password -s vn.clawd.desktop 2>&1 | grep -E 'svce|acct|class|keychain'
  echo "-- prefs"; C=~/.config/claude-usage-monitor; shasum -a 256 $C/desktop.json $C/clock.json $C/config $C/pet.json 2>&1
  python3 -c "import json,sys;d=json.load(open('$C/desktop.json'));print(sorted(d), d.get('usage_source'))" 2>&1
  echo "-- hooks"; grep -c -E '/hook/claude|clawd-event' ~/.claude/settings.json 2>&1; ls -l ~/.config/claude-usage-monitor/clawd-event.sh 2>&1
  echo "-- event server"; curl -s --max-time 3 http://127.0.0.1:47913/state | head -c 300; echo
  echo "-- processes"; pgrep -lf 'Clawd.app/Contents/MacOS/Clawd'
  echo "-- update stage"; ls -la ~/.config/claude-usage-monitor/app-update 2>&1; cat ~/.config/claude-usage-monitor/app-update/apply.log 2>&1 | tail -20
  echo "-- update loop (applying lines in the log)"; grep -c 'applying [0-9]' ~/.config/claude-usage-monitor/clawd-desktop.log 2>&1
}
```

## 2. Case A: 0.1.4 -> Rust over the same config

### 2.1 Install 0.1.4 and set it up completely

```sh
mkdir -p ~/migration && cd ~/migration
gh release download v0.1.4 -R vuduong97/usage-monitor -p 'Clawd-0.1.4-mac-arm64.zip'
ditto -x -k Clawd-0.1.4-mac-arm64.zip . && rm -rf /Applications/Clawd.app && mv Clawd.app /Applications/
spctl -a -t exec -vv /Applications/Clawd.app        # must say: accepted, source=Notarized Developer ID
open /Applications/Clawd.app
```

By hand, as on Windows (`windows.md` 2.1): finish the wizard with the **real clock** and its web
password (stored in the login keychain), connect **3 agents**, switch **start at login ON** (writes the
LaunchAgent), choose a **usage source**, change two or three preferences you can recognise, prove
the hook: `curl -s -X POST 'http://127.0.0.1:47913/hook/claude?e=UserPromptSubmit' -d '{"hook_event_name":"UserPromptSubmit","session_id":"mig1"}'`
-> `{"ok": true}` and the clock reacts.

```sh
verify "A-before" | tee ~/migration/before.txt
cp ~/Library/LaunchAgents/vn.clawd.desktop.plist ~/migration/before.plist
```

### 2.2 Stage and apply

Publish `v0.2.1` on the fork (`fork-release.md`; the mac zip is Developer-ID signed and notarized,
Team ID == 0.1.4's). Then:

```sh
printf 'update_repo = <you>/usage-monitor-fork\n' >> ~/.config/claude-usage-monitor/config
osascript -e 'quit app "Clawd"'; sleep 2; open /Applications/Clawd.app      # read the setting
```

Settings > App > **Kiem tra ban moi** (or wait ~90 s). Expected: downloaded, unpacked and checked
(bundle id, version, `codesign`, same Team ID); tray item **"● Khoi dong lai de cap nhat Clawd 0.2.1"**;
`ls ~/.config/claude-usage-monitor/app-update` shows `ready.json` and the staged `Clawd.app`. Nothing installed yet.

Click the tray item. `swap.sh` waits for the app to quit, keeps the old bundle until the new one is
in place, swaps, opens the new app. After ~20 s:

```sh
verify "A-after" | tee ~/migration/after.txt
diff ~/migration/before.txt ~/migration/after.txt
diff ~/migration/before.plist ~/Library/LaunchAgents/vn.clawd.desktop.plist && echo "LaunchAgent byte-identical"
```

Expected differences **only**: version, signature timestamp, process id, `apply.log`.
Must hold (record each PASS/FAIL):

| Check | Expect |
|---|---|
| bundle | `vn.clawd.desktop`, executable `Clawd`, version `0.2.1` |
| signature | `codesign --verify --deep --strict` OK; `TeamIdentifier` equal to before; `spctl`: accepted, `source=Notarized Developer ID`; stapler validate OK |
| one bundle | `mdfind` lists a single `/Applications/Clawd.app` (no `Clawd.app.old`, no copy left in `app-update`) |
| launch agent | byte-identical to `before.plist`; still loaded; **no argument** in `ProgramArguments` besides the executable |
| keychain | same `svce`/`acct` entry; opening Settings > clock tab does **not** ask for the web password again |
| prefs | `desktop.json` key list and `usage_source` unchanged; `clock.json` unchanged |
| hooks | `/hook/claude` count unchanged; the hook `curl` of 2.1 works; the clock reacts to a real Claude Code prompt |
| no update loop | 10 minutes later Settings > App says up to date; the log has one `applying 0.2.1`; `app-update` has no `failed` file |
| prompts | list every Keychain / Local Network prompt seen during 2.2 |

### 2.3 Variants (each from a fresh 0.1.4 install)

| Variant | Do | Expect |
|---|---|---|
| A2 failed swap | after staging, make `/Applications` unwritable for the swap (or `chflags uchg /Applications/Clawd.app`), click | the old app is left or reopened; `app-update/failed` written; the same version is **not** offered again in a loop |
| A3 firmware busy | `date +%s > ~/.config/claude-usage-monitor/ota.busy`, click | refused (firmware update in progress); remove the file, click again -> applies |
| A4 ad-hoc running app | run an ad-hoc signed build and check | "only ad-hoc signature, no Team ID": does not self-update (Settings explains) |
| A5 different Team | stage a build signed with another Team | refused by the Team ID check before the swap |

## 3. Case B: Rust -> Rust and signature cases

Same table as `windows.md` section 3 (B1-B8), with the mac zip. Start from Rust `v0.2.1` (test-key
build, `update_repo` set). Extra mac checks after B2 (`v0.2.2`): `verify "B2"` as above, **LaunchAgent
unchanged**, Gatekeeper still `Notarized Developer ID`, one bundle. A refused update (B3-B8) leaves
`/Applications/Clawd.app` untouched: `shasum -a 256 /Applications/Clawd.app/Contents/MacOS/Clawd` before and after.

## 4. Case C: rollback to Python

As `windows.md` section 4 with `Clawd-0.2.9-mac-arm64.zip` built from `python-0.1.x`
(Developer ID, same Team, `SHA256SUMS.minisig` from the test key). Expect: applied by the Rust
updater, the Python app runs (`pgrep -lf Clawd`; `Contents/Frameworks` exists again), the LaunchAgent
still byte-identical and the app starts at login, prefs/keychain/hooks intact.

## 5. Case D: GUI checks of phase 12 (macOS 15)

| # | Check | How | Pass |
|---|---|---|---|
| D1 | tray icon <= 1 s | `sh repo/tools/perf/measure.sh --kind tauri --exe /Applications/Clawd.app/Contents/MacOS/Clawd --runs 5 --out tray.json` (see `tools/perf/README.md`; **not yet run on macOS**) | `summary.tray_ms` median <= 1000 |
| D2 | **Thoat Clawd** quits cleanly | menu bar > Thoat Clawd; `pgrep -lf Clawd`; `curl http://127.0.0.1:47913/state` | gone in 5 s; port free; LaunchAgent does **not** restart it (`KeepAlive SuccessfulExit=false`: a clean exit stays quit) |
| D2b | crash restarts | `pkill -9 -f 'Clawd.app/Contents/MacOS/Clawd'`, wait 35 s | launchd restarts it (ThrottleInterval 30) |
| D3 | popover placement | click the menu-bar icon on the built-in display, on an external display, with the Dock at the bottom and on the side, in full-screen Spaces | popover opens under the icon, fully on screen, closes on focus loss |
| D4 | 50 Settings open/close cycles | `measure.sh` cycle mode or Activity Monitor private memory before/after | idle <= 20 MB at T+10 min; no growth; no stray `Clawd Web Content` processes |
| D5 | single instance | `open -n /Applications/Clawd.app` while running | second instance exits at once; one menu-bar icon |
| D6 | mDNS on macOS 15 | fresh user, first launch with the clock on the LAN; System Settings > Privacy & Security > Local Network | the prompt text is the app's `NSLocalNetworkUsageDescription`; after Allow the clock is found by mDNS within ~10 s (`/Applications/Clawd.app/Contents/MacOS/Clawd find --seconds 10` lists it); **Deny**: the app says so and typing the IP (menu "Nhap dia chi dong ho...") still works |
| D7 | IP dialog | menu > "Nhap dia chi dong ho...": AppleScript dialog opens, a bad answer is reported, Cancel is ignored | as described |
| D8 | size | `ls -l Clawd-0.2.1-mac-*.zip *.dmg` | each <= 8,000,000 bytes |

`Clawd find` is a console subcommand (no window); the others are GUI by nature.

## 6. After the run

Fill `reports/migration-report.md` and `reports/parity-checklist.md`, delete the test user or the
fork releases, remove the `update_repo` line from the config file.

## 7. mac x64 (approved exception: CI build + signature only)

On any Mac, with the `Clawd-0.2.1-mac-x86_64.zip` from the fork release:

```sh
ditto -x -k Clawd-0.2.1-mac-x86_64.zip x64 && cd x64
lipo -archs Clawd.app/Contents/MacOS/Clawd                     # x86_64
codesign --verify --deep --strict --verbose=2 Clawd.app
codesign -dv --verbose=2 Clawd.app 2>&1 | grep -E 'TeamIdentifier|Identifier'   # same Team ID
spctl -a -t exec -vv Clawd.app                                 # accepted, Notarized Developer ID
xcrun stapler validate Clawd.app
shasum -a 256 -c <(grep mac-x86_64.zip ../SHA256SUMS)          # listed in the signed SHA256SUMS
```

Real-hardware behaviour of the x64 build is **not verified** (accepted risk, plan.md Validation 1).
