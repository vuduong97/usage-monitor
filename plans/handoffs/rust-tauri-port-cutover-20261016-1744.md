# Handoff: Rust + Tauri port of Clawd, from "phase 14 real-machine tests" to the release cutover

Written 2026-10-16 17:44 (+07). Reader: a fresh coding agent (or the owner at home). The owner writes in
Vietnamese; reply to them in Vietnamese, short and concrete.

## 1. Mission and current status

**Outcome:** the Python "Clawd" tray app (`clawd_desktop/`) is ported to Rust + Tauri v2 in `desktop-rs/`, shipped as
`rust-v*` releases that 0.1.4 clients can update to, with no loss of config, hooks, autostart or clock link.
Plan: `plans/261015-0726-rust-tauri-port/plan.md` (15 phases).

**Done and merged into `main`:** phases 01-13 and the headless part of 14 (e2e tests, migration docs, test mapping,
parity checklist, migration report). Workspace tests: **742 passed, 0 failed, 1 ignored** (the manual Python<->Rust
prefs-lock test `two_process.rs:84`). fmt and `clippy -D warnings` clean on Windows **and on the macOS CI runner**.

**Done in this last stretch (all on `main`):**

- Tray was ~1.07 s late. Cause: `clawd_events::daemon_running` waited out a 1 s HTTP probe because a connect to a
  closed loopback port takes ~2 s to fail on this Windows. Fix `3e6aba6` (`crates/clawd-events/src/probe.rs`): 250 ms TCP
  connect first. Re-measured: tray **327 ms**.
- Test that the updater asset names match `rust-release.yml` (`tests-e2e/tests/release_names.rs`).
- `py_api` / `popover_msg` window-label gate is a pure function `only_for_window` with a unit test (`app/src/commands/mod.rs`).
- Two macOS-only clippy errors fixed (`08ede70` `tm_gmtoff` cast, `b4c2d4f` unused `Autostart` import in a windows-only test).
  macOS CI job is now green on `b4c2d4f`.
- `desktop-rs/Cargo.lock` committed (`275ae04`, and removed from the local `.git/info/exclude`). `rust-release.yml` refuses a
  tag build without it.
- Release signing set up (see section 5).
- Perf report `plans/261015-0726-rust-tauri-port/reports/perf-final.md`.
- **Real migration on the owner's machine** (0.1.4 -> Rust 0.2.0, installed over the real app): PASS. Recorded in
  `reports/migration-report.md`.

**Perf against the goals:** installer **5.5 MB** (goal <= 8); idle RAM **3 MB** (<= 20); tray **327 ms** (<= 1 s);
Settings first paint **655 / 663 ms** cold/warm on a quiet machine (goal 300 ms, owner accepted <= 0.6-0.7 s), 731-896 ms
when the machine was busy.

**Not done:** the rest of phase 14 (see section 8) and all of phase 15 (cutover).

## 2. Scope and guardrails

- Repo `D:\vu\usage-monitor`, GitHub `vuduong97/usage-monitor` (**public**), remote `origin`. Branch `main`.
- **Nothing has been tagged or released.** Never put a `v*` test tag on the main repo: 0.1.4 clients read `/releases/latest`
  and would be offered it. Test releases go on the owner's **fork** (see `tools/migration/fork-release.md`). Real Rust releases use
  `rust-v*` tags and are draft + prerelease + `--latest=false`.
- Do not touch the owner's real config `~/.config/claude-usage-monitor`, their Run key, or Credential Manager without a reason and
  consent. The owner allowed running the GUI on their machine, and allowed installing over their real app once (done, with backup).
- Do not delete the owner's global caches. `--auto` only auto-approves plan steps.
- Mac can only be verified through GitHub Actions (the owner has no Mac with them). The owner has a Windows PC and a real Clawd clock.
- **Do not freeze the owner's machine.** It hung earlier because of the 50-cycle window loops, parallel stress runs and parallel builds. Use
  `CARGO_BUILD_JOBS=4` (or 2 with low priority), no parallel stress loops, and ask before any run that opens many windows.
- Worker/integration rules: build with `-p <crate>`; only the integrator runs `--workspace` (target `D:/ct/main`); env
  `CARGO_HOME=D:/cargo-home CARGO_TARGET_DIR=D:/ct/<name> TMP=D:/tmp TEMP=D:/tmp`; keep builds on D: (C: is tight);
  write files with the edit tools; frozen contracts change only by additive CR; fixtures use only `sk-ant-test-*` / `sk-ant-fake-*` tokens.
- Gate for a merge: fmt, clippy `-D warnings`, tests, 0 `todo!()`, `python tools/check_public.py`, diff inside owned paths, a reviewer subagent.
- Python: `D:/vu/usage-monitor/.venv/Scripts/python.exe`. PowerShell `$var:` in a string breaks parsing, use `${var}`. `MSYS_NO_PATHCONV=1` for `reg query` from bash.
- The pi runtime has no `kongming`; `oracle`/`ask_advisor` is used for `--advice` checkpoints.

## 3. Current state (observed 2026-10-16 17:44)

- Repo `D:/vu/usage-monitor`, branch `main`, HEAD `f7672c458dec8508581377bea94fca0b4e4ac3dd`.
- Working tree clean (this handoff file is the only new, untracked file).
- **`main` is 2 commits ahead of `origin/main`:** `fb237ca` (perf-final) and `f7672c4` (migration report). They are not pushed.
  Everything before them is on GitHub. **Push before working from another machine.**
- 13 old git worktrees still exist at `D:/vu/usage-monitor.wt/{03..14}-*` (all merged); target dirs `D:/ct/{03..14,gui,main}`. Safe to remove.
- On the owner's PC right now: the **Rust Clawd 0.2.0 is installed over their old 0.1.4** and running (the Python app was replaced).
  Inno Setup 6.7.3 is at `D:\tools\InnoSetup6`. The installer is `dist/Clawd-0.2.0-windows-setup.exe` (git-ignored).
- Files that live only on the owner's PC (copy them if you work elsewhere): `D:\clawd-minisign\`, `D:\backup-clawd-0.1.4\`.

## 4. Decisions and rationale

- Owner decisions: no Python window baseline, judge goal 3 by absolute numbers; accept ~0.7 s settings paint if 300 ms is unreachable
  with RAM-free tricks; `SessionSlot` folded into `AppState`; `pause::hold(&AppPaths, ...)`; `Clawd Print & Paper.html` allow-listed in `tools/check_public.py`.
- Integration policy: merge when the reviewer finds no blockers, send fixes as follow-up commits; resume timed-out workers, never restart them.
- Release safety: `MINISIGN_PUBKEY` reaches the build through the repo variable -> `CLAWD_MINISIGN_PUBKEY`; the placeholder in
  `clawd-contracts/src/consts.rs` fails closed. `sign`, build and publish run only on `rust-v*` tags.
- Test flakes are fixed at the root (compare decoded bodies, widen margins), not retried.
- `popover_msg`/`py_api` are bound to the `Wry` runtime, so the mock Tauri runtime cannot reach them; the label check was made a pure function instead.
- The installer shares the AppId, folder and silent flags with the Python one on purpose (`packaging-rs/windows/clawd.iss`), so 0.1.x self-update installs it over the old app.

## 5. Work performed (signing and release setup)

- GitHub environment `release` exists: tag policy `rust-v*` only, required reviewer `vuduong97`.
- Secrets in env `release`: `MINISIGN_KEY`, `MINISIGN_PASSWORD`. Repo variables: `MINISIGN_PUBKEY` =
  `RWS5L6BddmFHhUHXRlL3sBgcUcwsIiFP4ttSnLnLuiZHgXVv6uxvqXLB`, `EXPECTED_TEAM_ID` = `TYX86KUPYQ`.
  Older repo secrets (2026-10-14): `APPLE_ID`, `APPLE_TEAM_ID`, `APPLE_APP_PASSWORD`, `MACOS_CERT_P12`, `MACOS_CERT_PASSWORD`,
  `MACOS_SIGN_IDENTITY`, `CLAWD_FEED_URL`, `CLAWD_FEED_TOKEN`.
- The Team ID was read from the signature of the public 0.1.4 mac build ("Developer ID Application: LUMINADE COMMERCE COMPANY LIMITED (TYX86KUPYQ)").
- The minisign key pair lives in `D:\clawd-minisign\` (`.key`, `.pub`, `.password.txt`). **The password is only in that file; it is not in this handoff.**
  The C: copy and the temporary `.env` were deleted. **The owner has not backed this folder up yet.** Losing it means a new key and a new build for every client.
- Scripts: `packaging-rs/set-release-secrets.ps1` (dry run by default, `-Apply` writes), `packaging-rs/build_windows.ps1`
  (cargo build + ISCC; needs `PATH` to include `D:\tools\InnoSetup6` and `PYTHON`).
- Real install test: backed up the install folder, config, the Uninstall and Run registry keys, hook files and the 0.1.4 installer to
  `D:\backup-clawd-0.1.4\`, then ran `Clawd-0.2.0-windows-setup.exe /SILENT /SUPPRESSMSGBOXES /NORESTART`.
  Result: old tray closed, `_internal` removed, Uninstall entry 0.2.0, Run value unchanged, config folder byte-identical (105 files),
  Claude/Codex/Gemini hook files byte-identical, event server `:47913` answered, clock accepted state and usage pushes.
  A second silent install of the same version over the running app also worked.

## 6. Verification

Run and passed:

- Windows: `cargo fmt --check`; `cargo clippy --workspace --all-targets -- -D warnings` (exit 0); workspace tests 742/0/1.
- GitHub Actions `Rust CI` on `b4c2d4f`: macos-14 (fmt, clippy, tests 742/0/1, release build), windows-latest, ubuntu-check, check-public, golden-harness all green
  (windows-latest was still building when last read; re-check).
- `python tools/check_public.py`: 479 files ok. Credential roundtrip `tools/golden/creds_roundtrip.py --yes` passed both directions (earlier).
- Perf: `D:/tmp/perf-window.json`, `D:/tmp/perf-idle.json`, `D:/tmp/perf-tray.json` (temp, not committed); summary is in `perf-final.md`.

Not run: Rust -> Rust update with `.minisig` / tamper cases; rollback to 0.1.4; tray "Thoat" exit; Win10 without WebView2 (IP dialog);
the real-clock firmware flow; mDNS on macOS 15; mac keychain / `swap.sh` / notarize / minisign on a real runner; the in-process tray-click open timing; `iscc` in CI.

Known flaky / watch: `clawd-firmware` test `feed_security::a_grant_may_only_name_relative_feed_paths` failed once in a full workspace run, 0/120 in parallel alone.

## 7. Open risks and blockers

- **Unexplained early exit:** in the first install, the Rust app started by the installer exited cleanly (`Stopped`, `Bye`) about 16 s after start,
  right after one `Clock state: error` line. Not reproduced (a hand start and a second install both stayed up). Check `~/.config/claude-usage-monitor/clawd-desktop.log`
  if it recurs; look at what in `lifecycle.rs` calls `quit()` / `app.exit(0)` (tray QUIT, `restart_for_update`).
- The Rust log timestamps are UTC while the Python app logged local time (cosmetic, but confusing when comparing logs).
- The minisign key folder has no backup.
- 11 leftover Credential Manager entries named `vn.clawd.desktop.test-wincred-collision-*` (junk from the wincred tests; `desktop-rs/crates/clawd-store/src/creds/wincred.rs`
  and `tools/golden/creds_roundtrip.py`). The owner has not yet agreed to delete them. A test that leaves entries behind should clean up.
- Mac behaviour beyond CI is unverified; `EngineConfig` cannot point at a fake Anthropic server (a test gap).
- Phase 15 touches `README`, `release.yml` (Python release), `ci.yml`, version fields: do not break the 0.1.4 update path while doing it.

## 8. Exact next actions

**First safe step:** `git push origin main` from `D:\vu\usage-monitor` (two doc commits, no tag), then `gh run list --limit 3` and confirm the Rust CI run is green on all 5 jobs.

Then, in order:

1. Back up `D:\clawd-minisign\` (the owner, to a safe place that is not the repo). Do not paste the password anywhere.
2. Owner: right-click the tray icon and click "Thoat" (quit); check the process exits and no `msedgewebview2` of Clawd is left. This is the first live test of the `EngineHandle::stop()` fix. Report the log tail.
3. Rollback test (owner agrees first, it replaces the running app): run `D:\backup-clawd-0.1.4\Clawd-0.1.4-windows-setup.exe /SILENT /SUPPRESSMSGBOXES /NORESTART`, confirm the old app works,
   then install `dist\Clawd-0.2.0-windows-setup.exe` again. Compare config sha256 with the earlier list (`D:/tmp/pre-config.sha`, if it still exists).
4. Rust -> Rust update with minisign tamper cases, on the owner's **fork** only: follow `tools/migration/fork-release.md` and `tools/migration/windows.md` (cases B and the `.minisig` cases),
   with a test key, not the production one. Record each result in `reports/migration-report.md`.
5. Mac: full dry run on the fork with a test tag and test key (notarize, `swap.sh`, keychain, `LaunchAgent`). Never a `v*` tag on the main repo.
6. Phase 15 cutover (`plans/261015-0726-rust-tauri-port/phase-15-*.md`): README, `release.yml` / `ci.yml`, version fields, then the first real `rust-v0.2.0` draft release,
   approved through the `release` environment reviewer. Re-read the plan's "Tồn đọng" line first.
7. Housekeeping: remove merged worktrees and `D:/ct/*` target dirs; delete the junk credential entries (with consent); update `plan.md` statuses
   (01-13 completed, 14 `in-progress`, 15 `pending`) as each item lands.

## 8b. Working from the owner's MacBook (added at push time)

The owner continues on a **MacBook**, not the Windows PC. Consequences:

- The Windows-only items (section 8 steps 2-4: tray "Thoat", rollback, Rust->Rust on Windows, Win10 no-WebView2) need the Windows PC; leave them for later.
- `D:\clawd-minisign\` and `D:ackup-clawd-0.1.4\` stay on the Windows PC. The CI only needs the GitHub secrets (already set), so the Mac does not need the key file.
- On the Mac you can do for real what CI could only approximate: `cargo test --workspace` on macOS, the keychain / LaunchAgent / popover placement checks in
  `tools/migration/mac.md`, and the mac packaging with `packaging-rs/build_mac.sh` (needs the Apple certificate; the CI has it as secrets). Run the Python checks with your own venv.
- Setup on the Mac: Rust stable, `gh auth login`, `git clone` the repo, `cargo test --workspace` from `desktop-rs/`. Paths in this file that start with `D:` or `C:` are Windows-only.
- Do not push a `v*` tag from the Mac either.

## 9. Source pointers

- Plan: `plans/261015-0726-rust-tauri-port/plan.md`, phase files `phase-01..15-*.md`.
- Reports: `plans/261015-0726-rust-tauri-port/reports/{perf-final,migration-report,parity-checklist,scout-report,spike-decision}.md`.
- Migration procedures: `tools/migration/{windows.md,mac.md,fork-release.md,run_windows.ps1}` (the script is read-only).
- Release: `.github/workflows/rust-release.yml`, `.github/workflows/rust-ci.yml`, `packaging-rs/` (`build_windows.ps1`, `build_mac.sh`, `set-release-secrets.ps1`, `check_version.py`, `windows/clawd.iss`).
- Code: `desktop-rs/app/src/{lifecycle,engine,popover,windows}.rs`, `desktop-rs/app/src/commands/`, `desktop-rs/crates/clawd-*`, `desktop-rs/tests-e2e/`.
- Perf tool: `tools/perf/measure.ps1` (isolated profile, scrubbed env, own port; opens Settings with `Clawd.exe settings`).
- Worker/reviewer outputs: `C:\Users\User\.pi\agent\sessions\--D--vu-usage-monitor--\subagent-artifacts\<id>_{worker,reviewer,oracle}_output.md` (this PC only).
- Installed app on the owner's PC: `C:\Users\User\AppData\Local\Programs\Clawd\`; config `C:\Users\User\.config\claude-usage-monitor\`; rollback bundle `D:\backup-clawd-0.1.4\`.
- Redactions applied to this file: 0 secrets were present; the minisign password and every token were deliberately left out.
