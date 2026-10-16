# Windows migration and update procedure (phase 14)

> **Since the cutover:** make the test releases with the local fake update API instead of a fork
> ([`local-release.md`](local-release.md)): the case numbers and checks below are unchanged, only
> the way the release is served differs. Never push a `v*` test tag to the main repo.

Copy-paste procedure for the **owner**, to run later on a **clean Windows VM** (Win11 for the
main run, Win10 for the no-WebView2 case). Nothing here is run by the agents. Never run any of it
on your everyday machine: it installs, updates and rolls back the real app, and the Run value /
Credential Manager entry names are the production ones.

Companions: [`fork-release.md`](fork-release.md) (how the test releases are made),
[`run_windows.ps1`](run_windows.ps1) (read-only verification), `reports/migration-report.md`
(where results go), `reports/parity-checklist.md` (what each result counts for).

## 0. Ground rules

* VM with checkpoints. Take one **before every case** and revert to it afterwards (Hyper-V:
  `Checkpoint-VM -Name "<case>-start"`). A case that needs a previous case's end state says so.
* Test releases live on **your fork**, never on the main repo (`vuduong97/usage-monitor`): a
  release there would be offered to every 0.1.4 user. The 0.1.4 app is pointed at the fork with
  the `update_repo` setting (section 2.2).
* Test versions are plain `vX.Y.Z` tags above 0.1.4 and above what the VM already runs:
  `v0.2.1` (migration), `v0.2.2` (Rust to Rust), `v0.2.9` (rollback, built from Python).
  Tags with a suffix (`v0.2.1-test.1`) are **not** offered by 0.1.4 (`parse_version` wants
  digits only); the workflow tag is `rust-v0.2.1` and the hand-made release tag is `v0.2.1`.
* The binary's version must equal the tag (`v0.2.1` -> app 0.2.1), otherwise it offers itself
  again forever. `fork-release.md` stamps both from the same commit.
* Record every result in `reports/migration-report.md` (PASS / FAIL / not run, date, VM, build,
  evidence file). A FAIL is an integration bug: write down the minimal repro, do not patch the VM.

Tools on the VM: PowerShell 5.1+, `gh` (GitHub CLI, logged in to the fork account), `git`,
`curl.exe` (built in). On your build machine: `minisign`, `gh` (see fork-release.md).

Fixed names used below (production values, from the frozen contracts):

| What | Value |
|---|---|
| Install folder | `%LOCALAPPDATA%\Programs\Clawd` (`Clawd.exe`) |
| Inno AppId / uninstall key | `{6C9B4E2A-1F7D-4C3B-9A51-C1A7D0E5B9F3}` -> `HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\{6C9B4E2A-1F7D-4C3B-9A51-C1A7D0E5B9F3}_is1` |
| Autostart | `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`, value `Clawd` = `"C:\Users\<u>\AppData\Local\Programs\Clawd\Clawd.exe"` (quoted, no arguments) |
| Config folder | `%USERPROFILE%\.config\claude-usage-monitor\` (`desktop.json`, `clock.json`, `config`, `pet.json`, `clawd-desktop.log`, `app-update\`) |
| Clock web password | Credential Manager, target `vn.clawd.desktop` (or `<user>@vn.clawd.desktop`) |
| Event server | `http://127.0.0.1:47913` (`/state`, `/poke`, `/hook/<agent>?e=`) |
| Silent installer flags | `/SILENT /SUPPRESSMSGBOXES /NORESTART` |

## 1. Prepare the VM

```powershell
# 1. Get the 0.1.4 installer (the last Python release) and the verification script
mkdir C:\migration; cd C:\migration
gh release download v0.1.4 -R vuduong97/usage-monitor -p "Clawd-0.1.4-windows-setup.exe"
git clone https://github.com/<you>/usage-monitor-fork.git repo   # for tools\migration\run_windows.ps1
Copy-Item .\repo\tools\migration\run_windows.ps1 C:\migration\
# 2. The script is read-only; check it parses before first use
[System.Management.Automation.Language.Parser]::ParseFile("C:\migration\run_windows.ps1",[ref]$null,[ref]$e) | Out-Null; $e
# 3. WebView2 present? (Win11: yes. Win10 no-WebView2 case: this must print nothing)
reg query "HKLM\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}" /v pv
```

PowerShell may refuse a downloaded script: `Unblock-File C:\migration\run_windows.ps1` (that
changes the file's zone marker only) or run it as `powershell -ExecutionPolicy Bypass -File ...`.

## 2. Case A: 0.1.4 -> Rust over the same config (the migration that matters)

### 2.1 Install 0.1.4 and set it up completely

```powershell
cd C:\migration
.\Clawd-0.1.4-windows-setup.exe /SILENT /SUPPRESSMSGBOXES /NORESTART    # per-user, no UAC
```

In the app (do each by hand, this is the state the migration must preserve):

1. Tray icon appears. Open **Cai dat** (Settings). Finish the 4-step wizard: pick the **real
   clock** (mDNS) or type its IP; enter the clock's **web password** (it goes to Credential Manager).
2. Settings > Agents: connect **3 agents** (e.g. Claude Code, Codex, Gemini). Claude Code needs
   `%USERPROFILE%\.claude\` to exist: create it by starting Claude Code once, or
   `mkdir $env:USERPROFILE\.claude` before the wizard.
3. Settings > App: **start at login ON** (writes the Run value). Settings > Usage: choose a
   **usage source** (`claude`, `codex`, or `off`).
4. Change two or three preferences you can recognise later (clock screen, brightness, a Wi-Fi
   field untouched but visible) and let the app save them.
5. Prove the hook works before the update: start a Claude Code prompt (or
   `curl.exe -s -X POST "http://127.0.0.1:47913/hook/claude?e=UserPromptSubmit" -d "{\"hook_event_name\":\"UserPromptSubmit\",\"session_id\":\"mig1\"}"`
   -> `{"ok": true}`); the clock reacts and `curl.exe -s http://127.0.0.1:47913/state` lists `claude:mig1`.

Take the baseline (read-only; writes nothing, you redirect stdout yourself):

```powershell
cd C:\migration
.\run_windows.ps1 -ConfirmDisposableVm -Stage Before          # human-readable, expect 0 FAIL
.\run_windows.ps1 -ConfirmDisposableVm -Stage Before -Json > .\before.json
Get-FileHash $env:USERPROFILE\.config\claude-usage-monitor\desktop.json > .\before-prefs.hash.txt
reg query HKCU\Software\Microsoft\Windows\CurrentVersion\Run /v Clawd  > .\before-run.txt
cmdkey /list | Select-String "clawd"                                   > .\before-cred.txt
```

Take a **checkpoint** now ("a-before-update").

### 2.2 Point 0.1.4 at the fork and stage the Rust build

On the build machine publish `v0.2.1` on the fork following `fork-release.md` (Rust build,
`CLAWD_MINISIGN_PUBKEY` = test key, assets + `SHA256SUMS` + `SHA256SUMS.minisig`, release
**not draft, not prerelease**). Check from the VM:

```powershell
curl.exe -s https://api.github.com/repos/<you>/usage-monitor-fork/releases/latest | ConvertFrom-Json | Select tag_name,draft,prerelease
```

Tell 0.1.4 where to look (**before** it checks; it reads this once at start):

```powershell
$cfg = "$env:USERPROFILE\.config\claude-usage-monitor\config"
Add-Content -Path $cfg -Value "update_repo = <you>/usage-monitor-fork"
Get-Content $cfg
# restart the app so it reads it: right-click tray > Thoat Clawd, then
& "$env:LOCALAPPDATA\Programs\Clawd\Clawd.exe"
```

0.1.4 downloads and stages the new installer by itself: first check ~90 s after start, then every
6 h. To not wait: Settings > App > **Kiem tra ban moi**. Expected: a staged update; the tray gets
the item **"● Khoi dong lai de cap nhat Clawd 0.2.1"**. It is staged only: nothing has been
installed yet. Verify while staged:

```powershell
dir $env:USERPROFILE\.config\claude-usage-monitor\app-update      # ready.json + Clawd-0.2.1-windows-setup.exe
```

### 2.3 Apply (the user's click) and verify

Click **"● Khoi dong lai de cap nhat"** in the tray. The app quits, the silent installer runs over
the install (same AppId), the new app starts. Wait ~30 s, then:

```powershell
cd C:\migration
.\run_windows.ps1 -ConfirmDisposableVm -Stage After -ExpectVersion 0.2.1 -BaselineJson .\before.json | Tee-Object .\after.txt
```

What the script proves automatically (all must be PASS):

| Check | Meaning |
|---|---|
| `run-exact`, `run-unchanged` | Run value is still `"<exe>"` with no arguments and equals the baseline |
| `addremove-one`, `version-addremove` | exactly **one** Add/Remove entry, version 0.2.1 |
| `no-_internal`, `files` | the PyInstaller `_internal\` folder is gone |
| `version-exe` | `Clawd.exe` reports 0.2.1 |
| `prefs-keys-kept`, `usage-source`, `file:*` | `desktop.json` keys/`usage_source` kept; the files are byte-identical (WARN = rewritten by the app: diff by hand) |
| `credential-kept` | the `vn.clawd.desktop` Credential Manager entry is still there |
| `hooks-installed`, `hooks-kept`, `event-server` | Claude hook entries kept; `/state` answers 200 |
| `one-process`, `no-failed-marker`, `no-update-loop` | one `Clawd.exe`, no `failed` marker, the log says `applying` at most once |

Then by hand (the script cannot see these; record each in the report):

1. **Password not asked again**: Settings > clock tab opens, shows the clock's values, and does
   **not** ask for the web password.
2. **Prefs intact**: the values you changed in 2.1 step 4 are as you left them.
3. **Hook works**: repeat the hook `curl.exe` of 2.1 step 5 (and one real Claude Code prompt): the
   clock reacts, `/state` lists the session.
4. **No update loop**: wait 10 minutes; Settings > App says "up to date"; no second installer
   run (`dir $env:TEMP -Filter "is-*"` empty; the log has one `applying`):
   `Select-String applying $env:USERPROFILE\.config\claude-usage-monitor\clawd-desktop.log`.
5. **Autostart**: sign out and in (or reboot): exactly one tray icon returns.
6. **Uninstall (last step of the case, after the evidence is saved)**: Settings > Apps > Clawd >
   Uninstall. The Inno uninstaller runs `Clawd.exe cleanup`: afterwards the Run value is gone,
   `%USERPROFILE%\.claude\settings.json` has no Clawd hook, and Add/Remove has no Clawd entry.
   `.\run_windows.ps1 -ConfirmDisposableVm` now FAILs `exe`/`run-present` on purpose.

Keep: `before.json`, `after.txt`, the two `Get-FileHash` files, the log, screenshots of the tray
and Settings.

### 2.4 Variants of case A (each from the "a-before-update" checkpoint)

| Variant | What to do | Expect |
|---|---|---|
| A2 update not clicked | stage as in 2.2 and **do not click**; leave the app running 7 h (or restart it twice) | stays staged; never installs by itself |
| A3 firmware busy | during 2.3, hold an OTA lease: `Set-Content $env:USERPROFILE\.config\claude-usage-monitor\ota.busy ([string][int](Get-Date -UFormat %s))` then click | refuses ("Dang cap nhat firmware..."); remove `ota.busy`, click again -> applies |
| A4 installer changed on disk | after staging, append a byte to the staged `Clawd-0.2.1-windows-setup.exe` | "File cai dat da bi doi, da bo"; nothing installs |
| A5 no network | disable the VM NIC before clicking | the click still applies the staged installer (no download needed) |

## 3. Case B: Rust -> Rust update and signature cases

Start from a VM with the **Rust `v0.2.1`** installed (end state of case A without the uninstall,
or install `Clawd-0.2.1-windows-setup.exe` directly) **built with the test key** and
`update_repo = <you>/usage-monitor-fork` in the `config` file.

| # | Do | Expect |
|---|---|---|
| B1 | Publish `v0.2.2` on the fork (valid `.minisig`). In the app: Settings > App > Kiem tra ban moi | staged; tray item "● Khoi dong lai de cap nhat Clawd 0.2.2" |
| B2 | Click it. After ~30 s run `.\run_windows.ps1 -ConfirmDisposableVm -Stage After -ExpectVersion 0.2.2 -BaselineJson .\after-0.2.1.json` (a `-Json` snapshot taken on 0.2.1) | 0 FAIL; Run value and prefs unchanged; one Add/Remove entry |
| B3 **bad signature**: release `v0.2.3` whose `SHA256SUMS.minisig` was made with **another** key | check | refused: "Khoa ... / khong xac minh duoc"; nothing staged; app stays 0.2.2; `app-update\` has no `ready.json` |
| B4 **tampered SHA256SUMS**: `v0.2.4` with a valid `.minisig` for the *original* `SHA256SUMS`, but the uploaded `SHA256SUMS` has one hash digit changed | check | refused (signature does not match the bytes) |
| B5 **missing .minisig**: `v0.2.5` without the signature asset | check | refused: "Ban phat hanh thieu SHA256SUMS.minisig" |
| B6 **wrong installer**: `v0.2.6` signed correctly, but the `.exe` differs from the hash in `SHA256SUMS` | check | refused after download (hash mismatch); nothing staged |
| B7 **draft / prerelease** `v0.2.7` | check | never offered |
| B8 **swapped stage**: stage `v0.2.8` validly, quit the app, replace `ready.json` or the staged `.exe`, start the app | check | the update is dropped or re-verified (a restart trusts `ready.json` only if the signature verifies again) |

Delete each bad release from the fork after its case (`gh release delete vX -R <you>/usage-monitor-fork --cleanup-tag -y`),
otherwise it stays "latest" and blocks the next case. Between B3-B8 take a checkpoint: a refused
update leaves nothing, but a mistake leaves you on the wrong version.

## 4. Case C: rollback rehearsal (Rust -> Python)

Purpose: if the Rust release is withdrawn, the Rust updater can install a Python build over it.

1. Build the Python app from branch `python-0.1.x` with its version set **above** the latest
   Rust one (e.g. `0.2.9`; all four `__version__`/`pyproject` places) on a Windows build machine:
   `packaging\build_windows.ps1` -> `dist\Clawd-0.2.9-windows-setup.exe`.
2. Publish it on the fork as `v0.2.9` (not draft, not prerelease) with `SHA256SUMS` **and
   `SHA256SUMS.minisig`** signed with the test key (`fork-release.md` section 6).
3. On the VM with Rust `v0.2.2` (test key build) run the update as in B1/B2.
4. Expect: the Rust updater stages and applies it; afterwards `Clawd.exe` is the Python build
   (`_internal\` is back), one Add/Remove entry at 0.2.9, **Run value unchanged and without
   arguments**, tray icon present, prefs/password/hooks intact, the clock still gets pushes.
   `.\run_windows.ps1 -ConfirmDisposableVm -Stage Check` shows `exe-version` 0.2.9 and
   `internalPresent` True in `-Json`.
5. Restart the VM: the Python app starts from the Run value.

## 5. Case D: no WebView2 (Win10 VM)

Win10 VM **without** the WebView2 Evergreen runtime (the registry query in section 1 prints
nothing; uninstall "Microsoft Edge WebView2 Runtime" if present). Install `Clawd-0.2.1-windows-setup.exe`.

* The tray icon still appears and `/state` answers.
* Tray menu has **"Nhap dia chi dong ho..."** (type the IP shown on the clock). Click it: a native
  input box (not a WebView) opens; type the clock IP; the clock is used (tray tooltip says
  connected, usage lands on the clock).
* "Cai dat..." with no WebView2 must not crash the tray (a message or nothing; record what happens).

## 6. Case E: GUI checks of phase 12 (Win11 VM, real clock)

| # | Check | How | Pass |
|---|---|---|---|
| E1 | tray icon <= 1 s after launch | `powershell -ExecutionPolicy Bypass -File repo\tools\perf\measure.ps1 -Kind tauri -Exe "$env:LOCALAPPDATA\Programs\Clawd\Clawd.exe" -Runs 5 -Out tray.json` (see `tools/perf/README.md` for exact parameters), read `summary.tray_ms` | median <= 1000 |
| E2 | **Thoat Clawd** exits cleanly | tray > "Thoat Clawd"; `Get-Process Clawd` | no process within 5 s; port 47913 free (`curl.exe http://127.0.0.1:47913/state` fails); no WebView2 children left (`Get-Process msedgewebview2` of this app) |
| E3 | 50 open/close Settings cycles, RAM | `measure.ps1` cycle mode (`rss_after_50_cycles_mb`) or by hand with Task Manager > Details > Clawd.exe private working set before and after | idle <= 20 MB private at T+10 min; after 50 cycles no growth beyond a few MB; no leak of WebView2 processes |
| E4 | single instance | start `Clawd.exe` a second time while it runs | second one exits at once (event port taken), one tray icon, one process |
| E5 | Settings first paint | `summary.paint_cold_ms` / `paint_warm_ms` from `measure.ps1` | target 300 ms, accepted <= 600 ms (owner accepted about 0.7 s if not reachable; record the number) |
| E6 | installer size | `(Get-Item Clawd-0.2.1-windows-setup.exe).Length` | <= 8,000,000 bytes |

## 7. After the run

* Fill `reports/migration-report.md` and `reports/parity-checklist.md` (every row: result, date, evidence).
* Revert the VM to its first checkpoint.
* Delete the fork's test releases/tags and the test minisign key from the secrets (keep the key
  file offline if you will repeat the run).
* A FAIL anywhere: attach `before.json`, `after.txt`, `clawd-desktop.log` and `app-update\` to the
  bug; the fix goes to the phase branch that owns the file, then repeat the case from its checkpoint.
