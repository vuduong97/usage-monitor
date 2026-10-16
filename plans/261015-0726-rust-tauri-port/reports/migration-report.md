# Migration report (phase 14)

Status of the 0.1.4 -> Rust migration, the Rust -> Rust update and the rollback rehearsal.
**Skeleton written 2026-10-16 by the phase 14 headless worker.** Section 1 is what automated
tests already prove; section 2 are the human runs, all `NOT RUN`; section 3 the integration bugs
found so far; section 4 everything still unverified.

Procedures: `tools/migration/{windows.md, mac.md, fork-release.md}`; verification:
`tools/migration/run_windows.ps1` (read-only). Parity rows: `reports/parity-checklist.md`.
Python test mapping: `reports/test-mapping.md`.

How to fill: replace `NOT RUN` by `PASS` or `FAIL`, add date, VM/Mac and the exact build
(tag + sha256 of the installer), and a pointer to the evidence (`before.json`, `after.txt`,
log, screenshots). Any `FAIL` needs a minimal repro in section 3 (the fix goes to the phase
branch that owns the file, then the case is repeated from its checkpoint).

Fork under test: `<owner>/<fork>`; test minisign public key: `RW...` (fingerprint) - **fill in**.
Builds: `v0.2.1` = ____ , `v0.2.2` = ____ , Python rollback `v0.2.9` = ____ .

## 1. Already proven by automated tests

All run headless at library level over temp homes and loopback fakes (no window, no tray, no WebView,
no real keychain/registry/port 47913). Windows only; nothing here has run on macOS.

| Claim | Evidence | Result |
|---|---|---|
| The updater trusts only `releases/latest` of the configured repo, a plain `vX.Y.Z`, never draft/prerelease; `rust-v*` tags are not a release for it | `clawd-appupdate` `source::test_latest_finds_the_asset_and_its_checksum`, `::test_a_draft_a_prerelease_or_a_odd_tag_is_not_offered`, `::a_rust_prefixed_tag_is_not_a_release_of_this_updater`, `::test_nothing_newer_nothing_for_this_platform_or_no_checksum_yet` | PASS (phase 10, rerun in the workspace gate) |
| `SHA256SUMS` must carry a valid minisign signature under the key baked into the app; missing, wrong-key, mismatched or malformed signatures and the placeholder key are refused (fail closed) | `source::the_check_without_a_signature_or_with_a_wrong_one_is_refused`, `::verify_sums_accepts_only_the_exact_bytes_under_the_key`, `updater::a_restart_refuses_missing_wrong_or_swapped_signatures` | PASS |
| Every redirect hop and download URL is re-checked (GitHub hosts / https or loopback only); a redirect to a foreign host is refused; at most 4 hops for the sums | `source::test_a_redirect_to_a_foreign_host_is_refused`, `::the_sums_hop_loop_checks_every_hop_and_stops_after_four`, `transport::the_real_client_does_not_follow_a_redirect_to_a_foreign_host` | PASS |
| The expected installer hash lives in memory; a rewritten `ready.json` cannot change it; after a restart `ready.json` is trusted only when the signature verifies again | `updater::the_hash_in_memory_wins_over_a_rewritten_ready_json`, `::a_restart_trusts_ready_json_only_when_the_signature_verifies_again`, `::windows_ready_json_must_name_this_versions_installer`, `::mac_ready_json_must_name_clawd_app` | PASS |
| Windows apply runs the installer silently (`/SILENT /SUPPRESSMSGBOXES /NORESTART`, detached); changed file refused; refused while the firmware is being installed (`ota.busy`) | `updater::test_apply_on_windows_runs_the_installer_silently`, `::test_apply_refuses_while_firmware_is_installing_and_a_changed_file`, `launch::windows_flags_are_detached_process_and_new_process_group` | PASS |
| macOS bundle checks (id, version, `codesign`, same Team ID, ad-hoc never self-updates); `swap.sh` byte-identical to Python's | `updater::test_mac_*`, `golden::golden_swap_script_is_byte_identical_to_pythons`, `golden::golden_check_bundle` | PASS for the logic; `swap_script::*` (3, `cfg(unix)`) **never ran** |
| Frozen contracts are byte-copies of Python: LaunchAgent plist, Run value, creds blob format, prefs/pause/ota_busy/payload files | `golden_store::*` (16), `autostart::*`, `wincred::*` | PASS |
| Credentials round trip Python <-> Rust (Windows Credential Manager, throwaway service names) | phase 03 store run, re-run **2026-10-16: passed** | PASS (Windows). macOS Keychain code never built |
| `desktop.json` shared between Python and Rust processes loses no update | `prefs::concurrent_updates_lose_nothing`, `two_process::two_rust_processes_lose_no_update`; `python_and_rust_share_the_prefs_lock` is `#[ignore]`, passed by hand in phase 03 | PASS (ignored test by hand) |
| `cleanup` (what the uninstaller runs) removes the hooks of every agent and the autostart entry, keeps foreign settings, is idempotent, continues when autostart fails | `e2e/cleanup::*` (3, run on a temp home + sandbox Run key / plist) | PASS |
| Engine: fake Anthropic -> usage poll -> fake clock `/api/usage` payload equals the Python golden, byte for byte incl. key order | `e2e/chain::usage_poll_reaches_the_clock_byte_for_byte_like_the_python_golden` | PASS |
| Engine: hook POST -> brain -> pusher -> clock `/api/state` in wire order; `/state`, `/poke`; every agent route; payload file kept; `events=off`; port already taken | `e2e/chain::*`, `e2e/engine::*` | PASS |
| Event server refuses bad Host / Origin, oversized (declared, chunked, lying) bodies, slow heads and bodies, full house of stalled clients | `e2e/hostile::*` (7), `e2e/engine::the_engine_server_refuses_hostile_requests_with_the_production_limits` | PASS |
| A panic inside one engine loop is contained, the loop restarts with back-off, siblings and the server keep working, usage poll panic costs one cycle | `e2e/fault_injection::*` (5) | PASS |

Flake report of `cargo test -p clawd-e2e` (3 consecutive runs): see section 5.

## 2. Human runs

Legend: `NOT RUN` / `PASS` / `FAIL`. Case ids are those of `windows.md` / `mac.md`.

### 2.1 Windows (clean Win11 VM; D on a Win10 VM without WebView2)

| Case | What | Result | Date | Build / VM | Evidence | Notes |
|---|---|---|---|---|---|---|
| A | 0.1.4 -> Rust `v0.2.1` via `update_repo`, user clicks "● Khởi động lại để cập nhật" | NOT RUN | | | | |
| A.run | Run value unchanged, no arguments (`run-exact`, `run-unchanged`) | NOT RUN | | | | |
| A.addremove | exactly 1 Add/Remove entry, version 0.2.1 | NOT RUN | | | | |
| A.internal | `_internal\` gone | NOT RUN | | | | |
| A.prefs | prefs intact (`desktop.json` keys, `usage_source`, files) | NOT RUN | | | | |
| A.password | clock password not asked again | NOT RUN | | | | |
| A.hooks | hook works end to end (curl + real Claude Code prompt) | NOT RUN | | | | |
| A.noloop | no update loop after 10 min; one `applying` in the log | NOT RUN | | | | |
| A.uninstall | uninstall runs `cleanup` (Run value, hooks, Add/Remove gone) | NOT RUN | | | | |
| A2-A5 | not clicked / firmware busy / installer changed / no network | NOT RUN | | | | |
| B1-B2 | Rust `v0.2.1` -> `v0.2.2`, Run value and prefs unchanged | NOT RUN | | | | |
| B3 | bad signature (other key) refused | NOT RUN | | | | |
| B4 | tampered `SHA256SUMS` refused | NOT RUN | | | | |
| B5 | missing `.minisig` refused | NOT RUN | | | | |
| B6 | installer hash mismatch refused | NOT RUN | | | | |
| B7 | draft / prerelease never offered | NOT RUN | | | | |
| B8 | swapped stage refused or re-verified | NOT RUN | | | | |
| C | rollback: Python `v0.2.9` installed by the Rust updater; Run value unchanged; Python app runs and autostarts | NOT RUN | | | | |
| D | Win10 without WebView2: tray runs, "Nhập địa chỉ đồng hồ…" native dialog works | NOT RUN | | | | |
| E1 | tray <= 1 s | NOT RUN | | | | |
| E2 | Thoát exits cleanly | NOT RUN | | | | |
| E3 | 50 Settings cycles: RAM; idle <= 20 MB | NOT RUN | | | | |
| E4 | single instance | NOT RUN | | | | |
| E5 | Settings first paint | NOT RUN | | | | |
| E6 | installer <= 8 MB | NOT RUN | | | | |
| OTA | real OTA on the real clock, once | NOT RUN | | | | |

### 2.2 macOS 15, Apple silicon

| Case | What | Result | Date | Build / Mac | Evidence | Notes |
|---|---|---|---|---|---|---|
| A | 0.1.4 (Developer ID) -> Rust `v0.2.1` via `swap.sh`, same Team ID | NOT RUN | | | | |
| A.sig | `codesign --verify`, `spctl` = Notarized Developer ID, stapler | NOT RUN | | | | |
| A.agent | LaunchAgent byte-identical, loaded, no arguments | NOT RUN | | | | |
| A.keychain | keychain entry kept; password not asked again; prompts recorded | NOT RUN | | | | |
| A.prefs / A.hooks / A.noloop | prefs, hooks, no update loop | NOT RUN | | | | |
| A2-A5 | failed swap / firmware busy / ad-hoc / foreign Team | NOT RUN | | | | |
| B | Rust -> Rust (valid) and signature cases (B3-B8) | NOT RUN | | | | |
| C | rollback to Python build | NOT RUN | | | | |
| D1 | tray/menu-bar icon <= 1 s (`measure.sh` first macOS run) | NOT RUN | | | | |
| D2, D2b | Thoát clean; launchd restarts a crash after 30 s | NOT RUN | | | | |
| D3 | popover placement (built-in, external, Dock bottom/side, full-screen Space) | NOT RUN | | | | |
| D4 | 50 Settings cycles RAM | NOT RUN | | | | |
| D5 | single instance | NOT RUN | | | | |
| D6 | mDNS + Local Network prompt on macOS 15 (Allow and Deny paths) | NOT RUN | | | | |
| D7 | IP dialog | NOT RUN | | | | |
| D8 | file sizes <= 8 MB | NOT RUN | | | | |
| 7 | mac x64 (exception): `lipo`, `codesign`, `spctl`, stapler, SHA256SUMS | NOT RUN | | | | |

### 2.3 Prompts and surprises to record

| Prompt / observation | Where | Screenshot | Decision |
|---|---|---|---|
| (Keychain, Local Network, SmartScreen, firewall...) | | | |

## 3. Integration bugs found by phase 14 (headless part)

Each has a minimal repro; the fix belongs to the owning phase branch (routed by the integrator).

| # | Bug / finding | Owner | Repro | Severity |
|---|---|---|---|---|
| 1 | **First payload can miss the `on_payload` hook.** `engine.rs::run` spawns the usage publisher *before* `publisher.set_on_payload(..)` (the event server is started in between), so a fast first poll (usage source `off`, Codex, a cached answer) is pushed before the hook exists: `Status.payload`/`token`, `Brain.usage` and the payload file the settings window draws stay empty until the next poll (`POLL_INTERVAL` = 60 s). 4 of 5 engines missed it in the repro. A slow Claude poll (network) is probably safe. Fix: install the hook before spawning the loops. | phase 12 (`app/src/engine.rs`) | `cargo test -p clawd-e2e --test engine -- --ignored` (`the_first_payload_reaches_the_on_payload_hook_without_a_second_poll`); 4/5 misses | Low-Medium |
| 2 | **Settings/setup bridge command behaviour is untested**: 39 Python tests (`test_settings_api` 22, `test_setup_api` 5, `test_preview` `usage_preview_*`/`clawd_pak_*` 11, plus 1) have no Rust counterpart; phase 13 deferred them to B'' and never wrote them. Not a defect found, a hole in the safety net. List: `test-mapping.md` "Gaps". | phase 13 | `python tools/migration/build_test_mapping.py` | Medium |
| 3 | `EngineConfig` cannot be pointed at a fake Anthropic (`run` builds `UsageEnv::new`, production URLs). The e2e rig therefore re-assembles the engine's graph for the Anthropic -> clock golden and uses `usage_source = off` through the real `EngineHandle`. A `usage_url`/`api_url` field (or an env override honoured only when `CLAWD_EVENT_PORT`-style test env is set) would let e2e run the true wiring. | phase 12 | n/a (testability) | Low |
| 4 | `rust-release.yml`'s asset names are not checked against `clawd_appupdate::asset_name` by any test (Python had `test_the_build_names_match_the_release_workflow`). A rename in either place would break updates silently. | phase 11 / 10 | n/a | Low |
| 5 | `unpacked_program_is_executable` (unix `chmod 755` of the downloaded renderer) has no test on any OS. | phase 09 | n/a | Low |
| 6 | Runtime test that a popover-labelled caller is denied `py_api` still needs the tauri `test` feature (known CR, not done). | phase 12/13 | n/a | Low |

## 4. Not verified by anything yet

* `iscc` on `packaging-rs/windows/clawd.iss`, installer size, the install-over-0.1.4 behaviour (`_internal` removal).
* Developer ID signing, notarization, stapling, `EXPECTED_TEAM_ID` check in CI; the mac zip layout; `build_mac.sh`.
* `minisign` signing/verification step in `rust-release.yml`; the placeholder `MINISIGN_PUBKEY` in `consts.rs`
  (CI injects `CLAWD_MINISIGN_PUBKEY`); the GitHub environment `release` and its secrets/variables do not exist yet.
* Any macOS execution: `clawd-store` macOS Keychain / plist / `flock` paths, `swap_script` tests, `measure.sh`,
  mDNS under macOS 15 local-network privacy.
* The GUI itself: tray, popover, Settings window, dialogs, WebView2 absence, memory after 50 cycles, paint time on the real app.
* The 0.1.4 side of the migration (it can only be exercised with the real 0.1.4 app).
* `tests-e2e` is not wired into `rust-ci.yml` yet (integrator adds the job: `cargo test -p clawd-e2e`, Win + mac).
* Reports owed by later steps: `reports/perf-final.md` (4 goals on the real app, same conditions as `perf-baseline.md`).

## 5. Gate log of this worker

Env: `CARGO_HOME=D:/cargo-home CARGO_TARGET_DIR=D:/ct/14 TMP=D:/tmp`, Windows 11, 2026-10-16.

| Gate | Result |
|---|---|
| `cargo fmt --check -p clawd-e2e` | clean |
| `cargo clippy -p clawd-e2e --all-targets -- -D warnings` | clean |
| `cargo test -p clawd-e2e` x3 | 25 passed, 1 ignored (the bug repro), 0 failed, in each of 3 consecutive runs. One outlier: `fault_injection` took 6.25 s in run 1 (0.27 s in runs 2-3; every wait in it has a 5-15 s margin), so no flake, but do not tighten those margins |
| `cargo test -p clawd-e2e --test engine -- --ignored` | fails as intended (4/5 engines missed the first payload hook) |
| `python tools/check_public.py` | `check_public: 472 files ok` |
