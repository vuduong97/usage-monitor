# Parity checklist (phase 14)

The README feature list of Clawd 0.1.x as a table: what is proven by automated tests, what still
needs a human on a clean Windows VM and on a macOS 15 Apple-silicon Mac, and where the owner
records the result. Written 2026-10-16 from `main` a0c59d6 + the phase 14 e2e crate.

How to read it

* **automated** = names of Rust tests (`crate/file::test`, or just `file::test` where the file name is unique)
  that run headless in `cargo test --workspace` (669 tests + 26 in `tests-e2e`, one of them an `#[ignore]`d bug repro). They ran on **Windows
  only**: nothing macOS-specific (Keychain, plist, `flock`, `swap.sh`, `codesign`) has run in CI yet.
  `e2e/...` = `desktop-rs/tests-e2e/tests/...` (phase 14).
* **manual Win / manual mac** = the case id in `tools/migration/windows.md` / `mac.md` that proves it.
  **Every manual cell is NOT RUN** until the owner fills `migration-report.md`.
* **status**: `auto` = fully proven headless; `auto+human` = headless part proven, human part pending;
  `human` = only a human can prove it; `partial` = headless coverage has a hole (named); `gap` = nobody covers it.
* Python test counts and gaps: `reports/test-mapping.md` (413 functions / 536 cases; 349 ported, 6 partial,
  40 gaps, mostly the settings bridge commands).

## 1. Install, lifecycle, tray

| # | Feature (README) | Automated coverage | Manual Win | Manual mac arm64 | Status |
|---|---|---|---|---|---|
| 1 | Windows installer: per-user, no UAC, same AppId, installs over 0.1.4 (`_internal\` removed), uninstall runs `cleanup` | `iscc` never run on a runner yet. `cleanup` half: `e2e/cleanup::cleanup_removes_hooks_and_the_run_value_sandbox_on_windows`, `shell_cli::cleanup_over_a_temp_home_removes_the_hook_and_the_autostart_entry` | windows.md A (2.1-2.3) | n/a | human |
| 2 | macOS app: zip/dmg, Developer ID + notarized, same Team ID as 0.1.4 | none (CI signing job unrun) | n/a | mac.md 2.2 (signature table), 7 (x64 exception: CI + `codesign`/`spctl` only) | human |
| 3 | Start at login: Run value `"<exe>"` / LaunchAgent byte-copy | `golden_store::launch_agent_plist_is_a_byte_copy_of_python`, `::run_value_matches_python`, `::windows_autostart_flow_uses_a_sandbox_registry_key`, `autostart::plist_bytes_match_python_plistlib`, `::windows_run_value_is_the_quoted_exe_and_goes_away_on_disable`, `shell_lifecycle::first_run_enables_autostart_once_and_installs_no_hooks` | windows.md A 2.3.5 (reboot), run_windows `run-exact` | mac.md 2.2 (plist byte-identical), D2b (crash restart) | auto+human |
| 4 | Tray icon with dot (green/yellow/red) | `shell_tray::icons_are_three_distinct_pngs`, `::connected_with_usage_detail`, `::problems_come_first`, `::only_a_difference_is_redrawn` | look at it (windows.md E1) | look at it (mac.md D1) | auto+human |
| 5 | Tray menu: status line, Cài đặt…, Thoát; `● Hoàn tất thiết lập…`, `● Tìm thấy …`, `● Khởi động lại để cập nhật`, `● Có firmware mới`, Nhập địa chỉ đồng hồ… | `shell_tray::the_plain_menu_is_head_settings_quit`, `::the_full_menu_is_tray_py_192_to_208_in_order`, `::the_address_item_shows_while_no_push_gets_through`, `::lost_clock_points_at_typing_its_ip` | windows.md 2.2, 5 | mac.md 2.2, D7 | auto+human |
| 6 | **Tray icon appears <= 1 s after launch** (phase 12 GUI item) | spike measured 72 ms (`reports/spike-decision.md`); the real app was never measured | windows.md E1 | mac.md D1 (`measure.sh` unrun on mac) | human |
| 7 | **Thoát exits cleanly** (process gone, port freed, no WebView children) (phase 12) | engine half: `shell_engine_wiring::stop_twice_and_poll_now_after_stop_are_harmless`, `::engine_run_pushes_usage_to_the_found_clock` (port freed after stop), `e2e/engine::engine_run_turns_a_hook_post_into_pushes_on_the_clock`; tray/window half has no test | windows.md E2 | mac.md D2 (+ D2b launchd restart after crash only) | auto+human |
| 8 | **Single instance** (a second launch exits) (phase 12) | `e2e/engine::a_second_engine_on_a_taken_port_runs_without_agent_events`, `shell_engine_wiring::engine_run_pushes_usage_to_the_found_clock` (`another_daemon_running`); the second *process* exiting is GUI | windows.md E4 | mac.md D5 | auto+human |
| 9 | **50 open/close Settings cycles: RAM does not grow; idle <= 20 MB** (phase 12, Goal 2) | none (spike: 2.3 MB idle without window; `rss_after_50_cycles_mb` 39.3 MB for the spike's own measure) | windows.md E3 | mac.md D4 | human |
| 10 | **mac popover placement** (under the icon, on every display / Dock position, closes on focus loss, ⌘, and gear open Settings, right-click menu) (phase 12) | view model only: `shell_popover::connected_shows_two_bars_and_no_clock_name`, `::reset_counts_down_from_the_reading`, `::page_messages_are_parsed_like_popover_py`, `::pak_is_read_off_the_clock_once_and_kept`; placement has no test | n/a (Windows has no popover) | mac.md D3 | human |
| 11 | **IP dialog without WebView2** (Win10 without runtime; native dialogs) (phase 12) | `shell_dialogs::menu_dialog_uses_the_typed_ip`, `::menu_dialog_reports_a_bad_answer_and_ignores_cancel`, `::the_windows_input_box_takes_text_from_the_environment`, `::applescript_text_is_escaped_not_spliced` (logic, not the window) | windows.md Case D | mac.md D7 | auto+human |
| 12 | Nothing pops up by itself (no auto dialogs) | by construction (no code); `shell_dialogs::menu_dialog_reports_a_bad_answer_and_ignores_cancel` | watch the first 10 min | same | auto+human |
| 13 | CLI: `--version`, `run`, `find`, `hooks on/off`, `autostart on/off`, `cleanup`, `settings` | `shell_cli::*` (parse, exit codes, `cleanup_with`, `find_line`), `e2e/cleanup::*`; `run` = `EngineHandle::run_until` (`e2e/engine::*`) | `Clawd.exe --version` on the VM | `Clawd --version`, `find --seconds 10` (mac.md D6) | auto+human |

## 2. First run and Settings window

| # | Feature | Automated coverage | Manual Win | Manual mac arm64 | Status |
|---|---|---|---|---|---|
| 14 | First run: 4-step wizard (clock, agents, usage, done); upgraders see 2 steps; new agent offered once | `firstrun` unit tests (`pending_lists_steps_in_order`, `migrate_derives_setup_done_from_first_run_done`, `new_agents_skips_seen_and_installed`), `golden_store::firstrun_matches_python`, `shell_lifecycle::first_run_enables_autostart_once_and_installs_no_hooks`; **wizard commands (`test_setup_api`) have no tests: 5 gaps** | windows.md 2.1 | mac.md 2.1 | partial |
| 15 | Settings window opens; first paint <= 300 ms (accepted 600 ms; about 700 ms accepted if unreachable) (Goal 3) | spike 632-692 ms cold; nothing on the real app; `shell_lifecycle::the_window_shows_the_dark_background_before_the_page_paints`, `::two_clicks_in_a_row_build_one_window_and_forward_the_other`, `::webview_arguments_come_from_the_environment_in_debug_builds_only` | windows.md E5 | mac.md D1/D4 (paint on macOS never measured) | human |
| 16 | Only the settings window may call `py_api`; only the popover `popover_msg`; 32-command allow-list | `settings_capabilities::only_the_settings_window_may_call_py_api`, `::only_the_popover_may_call_popover_msg`, `::no_capability_is_granted_to_every_window`, `commands::allowlist_is_the_32_names_the_ui_calls`; **runtime deny test needs tauri `test` feature (CR, not done)** | n/a | n/a | partial |
| 17 | UI markup: no inline style, no meta CSP | `settings_ui_markup::*` | n/a | n/a | auto |
| 18 | Hiển thị tab: brightness, rotation, main screen, animation, time zone, night mode; "Lưu vào đồng hồ" sends only the diff | `clockapi::diff_sends_only_changed_keys_nested`, `::diff_sends_a_changed_list_whole_and_nothing_when_equal`, `::save_posts_the_diff_and_skips_an_empty_one`, `settings_golden::the_save_diff_matches_python`, `schema::clean_drops_unknown_keys_and_checks_values`; **`Bridge::state/save` command tests missing (`test_settings_api` 22 gaps)** | windows.md 2.1 step 4 (real clock) | mac.md 2.1 | partial |
| 19 | Clock web password: asked once, kept in Credential Manager / Keychain, Digest login | `clockapi::digest_login_and_one_signed_upload`, `::password_missing_is_auth_needed`, `creds::put_get_forget_round_trip_with_user_and_newlines`, `wincred::*`, `golden_store::creds_secret_format_matches_python`; Py<->Rust credential round trip run 2026-10-16: passed (phase 03 report); `Bridge::login` untested | windows.md 2.3 "password not asked again" | mac.md 2.2 (keychain row; macOS Keychain code never built) | auto+human |
| 20 | Usage tab: source (Claude all/one, Codex, off), style, preview with real usage and moving Clawd | `usage` golden + `sources` (39) + `publisher` (25); renderer level `preview_run::*`, `preview::*`; **`usage_preview`/`clawd_pak` commands: 11 gaps** | with real clock + feed (windows.md 2.1 step 3) | same | partial |
| 21 | Preview with the feed's renderer (hash-checked per file; fallback to approximate) | `feed_client::fetch_renderer_downloads_decrypts_and_checks`, `preview::unpack_then_load_and_every_file_is_checked`, `preview_run::the_window_keeps_its_fallback_when_the_renderer_is_missing_or_unusable`; the renderer exec bit on unix has **no test** | real feed + token (never in CI) | real feed; check `chmod 755` of the unpacked renderer | auto+human |
| 22 | Wi-Fi tab: up to 4 networks, scan, name; saving reboots the clock | `clockapi::scan_export_restore_reboot`, `commands::clock::a_scan_keeps_the_strongest_row_per_ssid_sorted_by_signal`, `settings_golden::a_scan_matches_python`; wifi save sequencing untested | real clock | real clock | partial |
| 23 | Cập nhật tab: firmware + packs by one button, only changed packs sent, "● Có firmware mới", auto check at start and every 6 h | `ota::install_*`, `feed_client::*`, `feed_security::plan_needed_and_offer`, `golden_fw::plan_needed_offer_and_pack_state_match_python`; `Bridge` update job untested | windows.md (OTA on the real clock **once**, owner decision) | same | partial |
| 24 | Backup / restore / reboot / factory reset | `clockapi::scan_export_restore_reboot`, `backup::the_backup_is_named_after_the_clock_and_the_day`, `backup::only_a_json_object_with_a_hostname_is_a_backup`; factory-reset guard untested | real clock | real clock | partial |
| 25 | Agent tab: 7 agents (Claude Code, pi, Codex, Gemini, Cursor, opencode, Copilot), Beta labels, "Ngắt kết nối tất cả" | `agents/golden::configs_match_python`, `::registry_matches_python`, `::hooks_match_python`, `::normalize_matches_python`, `safety::*`, `e2e/engine::the_engine_server_takes_a_hook_from_every_agent`, `e2e/cleanup::*` | windows.md 2.1 (3 agents; try all 7 where installed) | mac.md 2.1 | auto+human |
| 26 | App tab: start at login, rerun setup, choose clock, open log, web page | `commands::update::only_a_github_release_page_is_opened`, `shell_lifecycle::a_typed_clock_pins_the_finder_and_refreshes`; the rest is page glue | click through | click through | partial |
| 27 | Preferences file `desktop.json`: no key lost, concurrent writers safe (tray + settings window + Python) | `prefs::*` (11), `two_process::two_rust_processes_lose_no_update`, `golden_store::prefs_update_matches_python_byte_for_byte`; `two_process::python_and_rust_share_the_prefs_lock` is `#[ignore]` (passed by hand, phase 03) | run_windows `prefs-keys-kept` | mac.md prefs row | auto+human |

## 3. Engine: usage, agents, clock

| # | Feature | Automated coverage | Manual Win | Manual mac arm64 | Status |
|---|---|---|---|---|---|
| 28 | Usage poll (Claude OAuth endpoint, fallback probe, Codex, CLIProxy) pushed to the clock, payload byte-exact incl. key order | `e2e/chain::usage_poll_reaches_the_clock_byte_for_byte_like_the_python_golden`, `usage/golden::oauth_mapping_matches_python`, `::usage_result_classification_matches_python`, `::codex_payload_from_matches_python`, `sources::*`, `publisher::*` | real account, real clock | same (macOS keychain read of `Claude Code-credentials` untested) | auto+human |
| 29 | Agent hook -> clock state (`POST /hook/<agent>?e=` -> `/api/state`), animations/variants/pet | `e2e/chain::a_claude_hook_post_becomes_a_state_push_in_wire_order`, `e2e/engine::engine_run_turns_a_hook_post_into_pushes_on_the_clock`, `events/golden::every_brain_scenario_matches_python` (90 scenarios), `pusher::*` | real Claude Code prompt | same | auto+human |
| 30 | `/state` and `/poke` (the `clawd` poke script) | `e2e/chain::state_and_poke_work_end_to_end`, `server::routes_match_test_event_server_routes` | `curl.exe` (windows.md 2.1 step 5) | `curl` (mac.md 2.1) | auto |
| 31 | Event server security: Host check, Origin 403, 413 (declared/chunked/lying), slowloris, connection cap | `e2e/hostile::*` (7), `e2e/engine::the_engine_server_refuses_hostile_requests_with_the_production_limits`, `events/server::host_header_must_be_loopback_with_the_bound_port` and 9 more | none | none | auto |
| 32 | Panic in a loop does not take the app down; the loop restarts | `e2e/fault_injection::*` (5), `shell_engine::a_panicking_loop_is_restarted_and_the_app_lives`, `::guard_contains_a_panic_in_a_sync_callback`; release profile keeps `panic = "unwind"` | none | none | auto |
| 33 | Clock found by mDNS (no port scan), remembered, typed IP wins, "Tìm lại", address change detected | `discovery::*` (19) with `FakeMdns`, `golden::finder_sequences_match_python`, `mdns::*` unit tests (fake daemon) | real mDNS on the Win11 VM | real mDNS on macOS 15: **Local Network prompt**, Deny path (mac.md D6) | auto+human |
| 34 | Clock restart detected, state and usage resent | `bootwatch::*` (10), `golden::bootwatch_steps_match_python`, `publisher::resend_*` | reboot the clock | same | auto+human |
| 35 | Pause lease (<= 300 s, firmware update) is not `ota.busy` (900 s, blocks app update) | `pause::*` (7), `shell_engine_wiring::engine_handle_targets_follow_the_pause_lease`, `updater::test_apply_refuses_while_firmware_is_installing_and_a_changed_file`, `golden_store::ota_busy_matches_python` | windows.md A3 | mac.md A3 | auto+human |
| 36 | Settings window keeps the pushed payload; survives an unwritable payload file | `e2e/engine::the_engine_keeps_the_pushed_payload_for_the_settings_window`, `::an_unwritable_payload_file_does_not_stop_the_pushes`; **bug: first payload can miss the hook (`e2e/engine::the_first_payload_reaches_the_on_payload_hook_without_a_second_poll`, ignored repro)** | none | none | partial |

## 4. Self-update and migration

| # | Feature | Automated coverage | Manual Win | Manual mac arm64 | Status |
|---|---|---|---|---|---|
| 37 | 0.1.4 stages the Rust build (`update_repo`), applies only on the user's click, keeps prefs / password / hooks / Run value, one Add/Remove entry, `_internal\` gone, no update loop | `clawd-appupdate` tests prove the Rust side of the contract (asset names, `releases/latest` rules, trusted hosts, checksums); the 0.1.4 side and the installer are real-world only | windows.md Case A + `run_windows.ps1` | mac.md Case A (`swap.sh`, same Team ID) | human |
| 38 | Rust -> Rust update with minisign: valid applies; wrong key, tampered sums, missing `.minisig`, wrong installer hash, draft/prerelease, swapped stage refused | `source::the_check_without_a_signature_or_with_a_wrong_one_is_refused`, `::verify_sums_accepts_only_the_exact_bytes_under_the_key`, `::the_sums_hop_loop_checks_every_hop_and_stops_after_four`, `updater::the_hash_in_memory_wins_over_a_rewritten_ready_json`, `::a_restart_trusts_ready_json_only_when_the_signature_verifies_again`, `::a_restart_refuses_missing_wrong_or_swapped_signatures`, `transport::the_real_client_does_not_follow_a_redirect_to_a_foreign_host` | windows.md Case B (B1-B8) | mac.md Case B | auto+human |
| 39 | macOS bundle checks before swap (bundle id, version, `codesign`, same Team ID, ad-hoc refused); `swap.sh` | `updater::test_mac_refuses_a_bad_bundle`, `::test_mac_an_ad_hoc_running_app_does_not_self_update`, `::test_mac_apply_checks_the_bundle_again`, `golden::golden_swap_script_is_byte_identical_to_pythons`; `swap_script::*` are `cfg(unix)` and **never ran** | n/a | mac.md Case A2-A5 | human |
| 40 | Rollback: the Rust updater installs a Python build (signed `SHA256SUMS.minisig`); Python app runs, autostart still starts it | none | windows.md Case C | mac.md Case 4 | human |
| 41 | Background check off (`auto_update = off`), `update_repo` / `CLAWD_UPDATE_REPO` override | `golden::golden_settings`, `source::repo_names_follow_the_python_regex`, `from_settings` tests | set it and watch | same | auto |
| 42 | Cleanup on uninstall removes autostart, hooks of all agents; a second run is harmless | `e2e/cleanup::*`, `shell_cli::cleanup_*` | windows.md 2.3.6 | mac.md (Ngắt kết nối tất cả) | auto+human |

## 5. Targets (Goals) measured on the real app

| # | Goal | Automated | Win | mac arm64 | Status |
|---|---|---|---|---|---|
| 43 | Installer / each mac file <= 8 MB | workflow size gate (never ran) | windows.md E6 | mac.md D8 | human |
| 44 | RAM idle <= 20 MB private, no window, T+10 min | none | windows.md E3 | mac.md D4 | human |
| 45 | Settings first paint <= 300 ms (600 ms accepted) | none | windows.md E5 | mac.md D1 | human |
| 46 | Tray <= 1 s | none | windows.md E1 | mac.md D1 | human |
| 47 | Golden fixtures 100 % | `*/golden*` tests (all recorded from Python) | CI | CI | auto |
| 48 | Python test mapping complete | `reports/test-mapping.md` (413/413 rows, 40 gaps listed) | n/a | n/a | partial |
| 49 | mac x64 | CI build + `codesign`/`spctl` only (approved exception) | n/a | mac.md 7 | human (exception) |

## 6. Totals (2026-10-16)

| status | rows |
|---|---|
| auto | 6 (17, 30, 31, 32, 41, 47) |
| auto+human | 19 |
| partial | 10 (14, 16, 18, 20, 22, 23, 24, 26, 36, 48) |
| human | 14 |
| gap | no row is a pure gap; the 40 Python tests nobody covers are listed in `test-mapping.md` |
| **total** | **49** |

Nothing in the manual columns has been run. The headless columns passed on Windows; the same
tests have not run on macOS.
