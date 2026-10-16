"""Hand-curated part of reports/test-mapping.md: the status of every Python test function that is
not matched to a Rust test by name. Read by build_test_mapping.py (which validates every Rust
reference against the real test names, so a renamed test breaks the build of the report).

Reference shorthand (see build_test_mapping.py ALIASES): `events/tests/golden.rs::fn` is
`desktop-rs/crates/clawd-events/tests/golden.rs`, `app/...` is `desktop-rs/app/...`, `e2e/...` is
`desktop-rs/tests-e2e/...`. A reference may end in `[scenario]`: the golden scenario (or fixture
case) the test is replayed as.

Statuses: ported | partial (pieces covered, the integrated behaviour is not) | dropped (with a
reason) | python-only (stays a Python test, with a reason) | gap (nobody covers it: needs work).
Sources: the phase worker reports (2026-10-15/16) re-checked against the Rust tree at main a0c59d6
plus the phase 14 e2e crate.
"""

GOLD_EV = "events/tests/golden.rs"
GOLD_AG = "agents/tests/golden.rs"
SC = "events/tests/golden.rs::every_brain_scenario_matches_python"


def P(*refs, note=""):
    return ("ported", list(refs), note)


def PA(*refs, note=""):
    return ("partial", list(refs), note)


def DROP(reason, *refs):
    return ("dropped", list(refs), reason)


def PY(reason, *refs):
    return ("python-only", list(refs), reason)


def GAP(reason, *refs):
    return ("gap", list(refs), reason)


def scen(name, note=""):
    return P(f"{SC}[{name}]", note=note)


MANUAL = {
    # ---- test_agents.py -------------------------------------------------------------------------
    "test_hook_agents_install_and_remove_cleanly": P(f"{GOLD_AG}::configs_match_python"),
    "test_codex_hooks_run_in_the_background_with_a_windows_command": P(f"{GOLD_AG}::configs_match_python"),
    "test_windows_commands_are_cmd_wrapped_curl": P(
        f"{GOLD_AG}::configs_match_python", "agents/src/json_edit.rs::handler_text_covers_every_agent_format"
    ),
    "test_cursor_uses_only_observing_hooks": P(f"{GOLD_AG}::configs_match_python"),
    "test_broken_config_is_never_touched": P(
        "agents/tests/safety.rs::an_unparsable_config_is_never_touched",
        "agents/src/hooks.rs::apply_leaves_broken_or_missing_dirs_alone",
    ),
    "test_user_hooks_keep_their_place_and_a_shared_group_survives": P(
        "agents/src/json_edit.rs::removal_keeps_key_order_and_shared_groups", f"{GOLD_AG}::configs_match_python"
    ),
    "test_gemini_groups_have_no_matcher": P(f"{GOLD_AG}::configs_match_python"),
    "test_gemini_needs_more_than_the_antigravity_folder": P(f"{GOLD_AG}::registry_matches_python"),
    "test_copilot_writes_its_own_file_and_removes_it": P(f"{GOLD_AG}::configs_match_python"),
    "test_copilot_never_overwrites_a_foreign_file": P(f"{GOLD_AG}::configs_match_python"),
    "test_env_dirs_move_the_config": P(f"{GOLD_AG}::configs_match_python"),
    "test_plugin_agents_copy_update_and_remove": P(
        f"{GOLD_AG}::configs_match_python", f"{GOLD_AG}::bundled_assets_equal_the_integrations_python_ships"
    ),
    "test_plugin_agent_leaves_a_foreign_file_and_keeps_a_dev_symlink": PA(
        f"{GOLD_AG}::configs_match_python",
        note="foreign-file half ported; the dev-symlink branch is untested (symlinks need a privilege on Windows, nobody ran it on macOS)",
    ),
    "test_uninstall_all_reports_every_agent": P(
        "e2e/tests/cleanup.rs::cleanup_removes_hooks_and_the_launch_agent_on_macos_layout",
        f"{GOLD_AG}::registry_matches_python",
        note="phase 14: uninstall_all over every agent of a temp home, via the same cleanup_with the CLI uses",
    ),
    "test_codex_payloads": P(f"{GOLD_AG}::normalize_matches_python"),
    "test_gemini_payloads": P(f"{GOLD_AG}::normalize_matches_python"),
    "test_cursor_payloads": P(f"{GOLD_AG}::normalize_matches_python"),
    "test_copilot_payloads_in_both_casings": P(f"{GOLD_AG}::normalize_matches_python"),
    "test_claude_hook_drops_cursor_payloads": P(
        f"{GOLD_EV}::claude_and_pi_normalisers_match_python_bytes", f"{GOLD_AG}::normalize_matches_python_all"
    ),
    "test_new_tool_names_read_as_actions": P(f"{GOLD_EV}::vocab_helpers_match_python"),
    "test_event_server_routes_every_agent": P(
        "e2e/tests/engine.rs::the_engine_server_takes_a_hook_from_every_agent",
        "agents/src/lib.rs::normalize_fits_the_event_servers_alias",
        note="phase 14: five agents + an unknown id through the engine's own server",
    ),
    "test_the_hook_script_passes_agent_and_event": PY(
        "runs `sh` + curl against a live server (skipped on Windows even in Python); the script text is byte-checked by hooks_match_python",
        f"{GOLD_AG}::hooks_match_python",
    ),
    # ---- test_appupdate.py ----------------------------------------------------------------------
    "test_the_build_names_match_the_release_workflow": PY(
        "reads the Python `packaging/` + release.yml; the Rust release has packaging-rs/test_check_version.py (8 tests, run by CI); desktop-rs/tests-e2e/tests/release_names.rs ties the asset names of release.yml to appupdate's name functions"
    ),
    "test_versions_agree": PY(
        "pyproject vs __version__; the Rust version is the single `workspace.package.version`, checked by packaging-rs/check_version.py in CI"
    ),
    "test_can_apply": P(
        "appupdate/tests/updater.rs::test_can_apply_on_mac_paths",
        "appupdate/tests/updater.rs::test_can_apply_needs_a_writable_applications_folder",
    ),
    "test_ota_busy_marker": P("store/src/paths.rs::ota_busy_follows_the_file_and_goes_stale_after_900_s"),
    "test_config_can_turn_the_background_check_off": P("appupdate/tests/golden.rs::golden_settings"),
    # ---- test_attention.py ----------------------------------------------------------------------
    "test_attention_flag": P(
        "usage/tests/attention_codex.rs::attention_flag_lifecycle", "usage/tests/golden.rs::attention_flag_matches_python"
    ),
    "test_clears": P("usage/tests/attention_codex.rs::clears_are_consumed"),
    # ---- test_bootwatch.py ----------------------------------------------------------------------
    "test_heap_url": P("clock/src/bootwatch.rs::heap_url_matches_python", "clock/tests/golden.rs::bootwatch_steps_match_python"),
    "test_resend_replays_last_payload_without_polling": P(
        "usage/tests/publisher.rs::resend_pushes_the_cached_payload_without_polling"
    ),
    "test_resend_refreshes_wall_clock": P(
        "usage/tests/publisher.rs::resend_pushes_the_cached_payload_without_polling",
        note="same Rust test asserts the refreshed `t`",
    ),
    # ---- test_clockapi.py / test_discovery.py ----------------------------------------------------
    "test_get_retries_a_slow_clock_then_gives_up": P(
        "clock/tests/clockapi.rs::get_retries_a_slow_clock",
        "clock/tests/clockapi.rs::get_gives_up_after_two_retries_a_retry_gap_apart",
    ),
    "test_manual_clock_parsing": P("clock/src/clock.rs::typed_addresses", "clock/tests/golden.rs::manual_clock_and_valid_host_match_python"),
    "test_manual_clock_rejects_malformed_hosts": P(
        "clock/src/clock.rs::malformed_hosts_are_rejected", "clock/tests/golden.rs::manual_clock_and_valid_host_match_python"
    ),
    # ---- test_codex_usage.py --------------------------------------------------------------------
    "test_maps_windows_by_length": P("usage/tests/golden.rs::codex_payload_from_matches_python"),
    "test_a_past_reset_reads_as_zero_and_limits_reached_as_rejected": P("usage/tests/golden.rs::codex_payload_from_matches_python"),
    "test_older_resets_in_seconds": P("usage/tests/golden.rs::codex_payload_from_matches_python"),
    "test_usage_based_plan_has_no_windows": P("usage/tests/golden.rs::codex_payload_from_matches_python"),
    "test_reads_the_newest_session_with_limits": P(
        "usage/tests/golden.rs::codex_read_over_session_logs_matches_python",
        "usage/tests/attention_codex.rs::codex_only_looks_at_the_newest_files",
    ),
    "test_codex_home_moves_it": P("usage/tests/attention_codex.rs::codex_home_moves_the_sessions_dir"),
    # ---- test_engine.py -------------------------------------------------------------------------
    "test_tracking_sinks_report_each_push": P("usage/tests/publisher.rs::http_sinks_report_each_push_to_the_hook"),
    "test_engine_pushes_usage_to_the_found_clock": P(
        "app/tests/shell_engine_wiring.rs::engine_run_pushes_usage_to_the_found_clock",
        "e2e/tests/engine.rs::engine_run_turns_a_hook_post_into_pushes_on_the_clock",
    ),
    "test_daemon_running_checks_the_state_endpoint": P(
        "events/tests/server.rs::daemon_running_sees_this_server_and_nothing_else",
        "app/tests/shell_engine_wiring.rs::engine_run_pushes_usage_to_the_found_clock",
    ),
    "test_read_clock_screen_uses_the_clocks_config": P(
        "app/tests/shell_engine.rs::the_clocks_main_screen_is_an_integer_in_its_config",
        "app/tests/shell_engine.rs::the_password_key_is_the_name_else_the_address",
    ),
    "test_stop_twice_after_the_loop_closed_is_harmless": P(
        "app/tests/shell_engine_wiring.rs::stop_twice_and_poll_now_after_stop_are_harmless"
    ),
    # ---- test_feed.py / test_firmware_ota.py ------------------------------------------------------
    "test_manifest_checks": P(
        "firmware/src/feed/manifest.rs::manifest_checks_refuse_what_python_refuses", "firmware/tests/golden_fw.rs::manifest_matches_python"
    ),
    "test_manifest_load_and_damage_check": P("firmware/src/firmware.rs::load_and_damage_check", "firmware/tests/golden_fw.rs::bundles_on_disk_match_python"),
    "test_update_states": P("firmware/src/firmware.rs::update_states_like_python", "firmware/tests/golden_fw.rs::firmware_heap_readers_match_python"),
    "test_pause_lease": P(
        "firmware/tests/ota_pause_lease.rs::the_lease_is_held_renewed_and_removed_even_on_error",
        "store/src/pause.rs::lease_is_capped_at_300_seconds",
    ),
    # ---- test_firstrun.py -----------------------------------------------------------------------
    "test_fresh_install_sees_every_step": P("store/src/firstrun.rs::pending_lists_steps_in_order", "store/tests/golden_store.rs::firstrun_matches_python"),
    "test_upgrade_from_0_1_sees_only_the_new_steps": P("store/src/firstrun.rs::migrate_derives_setup_done_from_first_run_done", "store/tests/golden_store.rs::firstrun_matches_python"),
    "test_steps_done_and_opened_once_per_version": P("store/src/firstrun.rs::should_open_once_per_version_while_steps_remain", "store/src/firstrun.rs::mark_done_ignores_unknown_steps_and_keeps_junk"),
    "test_a_new_agent_is_offered_once": P("store/src/firstrun.rs::new_agents_skips_seen_and_installed", "store/src/firstrun.rs::seen_and_mark_seen_sort_and_dedupe"),
    "test_first_run_installs_no_hooks": P("app/tests/shell_lifecycle.rs::first_run_enables_autostart_once_and_installs_no_hooks"),
    "test_cleanup_disconnects_every_agent": P(
        "app/tests/shell_cli.rs::cleanup_over_a_temp_home_removes_the_hook_and_the_autostart_entry",
        "e2e/tests/cleanup.rs::cleanup_removes_hooks_and_the_launch_agent_on_macos_layout",
        "e2e/tests/cleanup.rs::cleanup_removes_hooks_and_the_run_value_sandbox_on_windows",
    ),
    "test_daemon_has_no_refresh_path": PY("a `hasattr` guard on a Python module; the Rust token code has no refresh function to guard"),
    # ---- test_hooks.py --------------------------------------------------------------------------
    "test_merge_keeps_existing_hooks_and_is_idempotent": P(f"{GOLD_AG}::hooks_match_python"),
    "test_merge_replaces_the_old_clawd_home_hook": P(f"{GOLD_AG}::hooks_match_python"),
    "test_windows_hook_is_async_curl_in_exec_form": P(f"{GOLD_AG}::hooks_match_python"),
    "test_hook_is_fast_and_silent_without_app": PY("runs the `sh` hook script against nothing; the script text is byte-checked by hooks_match_python", f"{GOLD_AG}::hooks_match_python"),
    "test_hook_forwards_payload_to_app": PY("runs the `sh` hook script with curl against a live server; Rust side of the wire is e2e/tests/chain.rs", "e2e/tests/chain.rs::a_claude_hook_post_becomes_a_state_push_in_wire_order"),
    "test_cli_builds_requests": PY("the `clawd` poke script stays a Python script in integrations/; its server side (/poke, /state) is covered by e2e/tests/chain.rs", "e2e/tests/chain.rs::state_and_poke_work_end_to_end"),
    "test_cli_end_to_end": PY("same: the Python `clawd` script; the say -> state round trip is covered by e2e/tests/chain.rs", "e2e/tests/chain.rs::state_and_poke_work_end_to_end"),
    # ---- test_http_push.py ----------------------------------------------------------------------
    "test_normalize_http_target": P("config/tests/readers.rs::normalize_http_target_cases", "config/tests/golden_config.rs::golden_normalize_http_target"),
    "test_read_http_targets": P("config/tests/readers.rs::read_http_targets_dedups_and_skips_invalid"),
    "test_ble_setting_defaults_on": P("config/tests/config_file.rs::onoff_setting_defaults_and_is_case_insensitive"),
    "test_clock_time_sent_by_default": P("usage/tests/publisher.rs::clock_time_is_sent_by_default"),
    "test_http_sinks_delivers_to_every_target": P("usage/tests/publisher.rs::http_sinks_deliver_to_every_target"),
    "test_publisher_pushes_without_ble": P("usage/tests/publisher.rs::publisher_pushes_to_a_wifi_clock"),
    "test_publisher_idle_without_destinations": P("usage/tests/publisher.rs::publisher_is_idle_without_destinations"),
    "test_publisher_respects_poll_interval": P("usage/tests/publisher.rs::publisher_respects_the_poll_interval"),
    "test_publisher_no_data_beat": P("usage/tests/publisher.rs::publisher_no_data_beat_is_login_expired"),
    "test_publisher_feeds_ble_and_http_from_one_poll": DROP("BLE path removed (owner decision Q3)"),
    # ---- test_macos_multidir.py -----------------------------------------------------------------
    "test_discover_target_darwin_uses_os_held_device": DROP("BLE discovery removed (owner decision Q3)"),
    "test_discover_target_darwin_returns_none_when_not_held": DROP("BLE discovery removed (owner decision Q3)"),
    "test_discover_target_non_darwin_uses_pinned_address": DROP("BLE discovery removed (owner decision Q3)"),
    "test_discover_target_non_darwin_returns_none_without_pin": DROP("BLE discovery removed (owner decision Q3)"),
    # ---- test_prefs.py --------------------------------------------------------------------------
    "test_concurrent_updates_keep_every_key": P("store/src/prefs.rs::concurrent_updates_lose_nothing", "store/tests/two_process.rs::two_rust_processes_lose_no_update"),
    "test_update_never_overwrites_a_file_it_could_not_read": P("store/src/prefs.rs::unreadable_file_is_an_error_not_an_empty_prefs"),
    "test_a_short_sharing_violation_is_waited_out": P("store/src/prefs.rs::short_permission_errors_are_waited_out"),
    "test_missing_or_corrupt_file_is_a_fresh_start": P("store/src/prefs.rs::missing_file_reads_empty_and_update_creates_it", "store/src/prefs.rs::corrupt_or_non_object_reads_empty"),
    "test_unchanged_update_does_not_write": P("store/src/prefs.rs::unchanged_update_does_not_rewrite"),
    # ---- test_preview.py ------------------------------------------------------------------------
    "test_payload_roundtrip_keeps_only_the_screen_keys": P("store/src/paths.rs::payload_round_trips_only_the_persisted_keys"),
    "test_unreadable_payload_file_is_none": P("store/src/paths.rs::unusable_payload_files_read_as_none"),
    "test_missing_payload_file_is_none": P("store/src/paths.rs::unusable_payload_files_read_as_none"),
    "test_engine_keeps_every_payload_for_the_settings_window": P(
        "e2e/tests/engine.rs::the_engine_keeps_the_pushed_payload_for_the_settings_window",
        note="phase 14; reveals the first-payload hook race (ignored repro in the same file)",
    ),
    "test_engine_survives_an_unwritable_payload_file": P("e2e/tests/engine.rs::an_unwritable_payload_file_does_not_stop_the_pushes"),
    "test_live_payload_moves_the_wall_clock_to_now_only_when_there_is_one": P(
        "firmware/src/preview.rs::live_payload_moves_the_wall_clock_to_the_minute",
        "firmware/tests/golden_fw.rs::live_payload_frame_key_and_commits_match_python",
    ),
    "test_load_renderer_needs_every_file": P("firmware/src/preview.rs::load_renderer_needs_every_file_and_trusts_only_its_commit"),
    "test_renderer_built_from_another_firmware_is_not_trusted": P("firmware/src/preview.rs::load_renderer_needs_every_file_and_trusts_only_its_commit"),
    "test_a_broken_clip_raises": P("firmware/tests/preview_run.rs::a_broken_clip_or_frame_raises"),
    "test_a_broken_renderer_raises": P("firmware/tests/preview_run.rs::a_missing_program_is_an_io_error", "firmware/tests/preview_run.rs::a_renderer_that_hangs_is_killed_at_the_timeout"),
    "test_real_renderer_is_deterministic_and_draws_each_style": DROP("skipped in Python too unless CLAWD_GOLDEN is set and a built C++ renderer exists (never in the repo)"),
    "test_real_clips_loop_each_style": DROP("skipped in Python too unless CLAWD_GOLDEN is set and a built C++ renderer exists (never in the repo)"),
    # ---- test_renderer_feed.py ------------------------------------------------------------------
    "test_manifest_lists_renderers_per_platform": P("firmware/src/feed/manifest.rs::renderers_are_kept_apart_and_validated_apart", "firmware/tests/golden_fw.rs::manifest_matches_python"),
    "test_a_release_without_renderers_parses_as_before": P("firmware/src/feed/manifest.rs::renderers_are_kept_apart_and_validated_apart", "firmware/tests/golden_fw.rs::manifest_matches_python"),
    "test_a_bad_renderer_row_drops_the_renderers_never_the_release": P("firmware/src/feed/manifest.rs::renderers_are_kept_apart_and_validated_apart", "firmware/tests/golden_fw.rs::manifest_matches_python"),
    "test_two_renderers_for_one_platform_are_refused": P("firmware/src/feed/manifest.rs::renderers_are_kept_apart_and_validated_apart", "firmware/tests/golden_fw.rs::manifest_matches_python"),
    "test_a_renderer_row_with_an_unhashable_kind_drops_only_the_renderers": P("firmware/src/feed/manifest.rs::renderers_are_kept_apart_and_validated_apart", "firmware/tests/golden_fw.rs::manifest_matches_python"),
    "test_unpacked_program_is_executable": GAP("only the `chmod 755` code on unix exists; no test on any OS (suite ran on Windows only): verify on macOS"),
    "test_not_a_zip_at_all": P("firmware/src/preview.rs::a_zip_that_is_not_a_renderer_is_refused_and_writes_nothing"),
    "test_window_previews_with_the_releases_renderer_and_reuses_it": P("firmware/tests/preview_run.rs::the_window_previews_with_the_releases_renderer_and_reuses_it"),
    "test_window_keeps_its_fallback_when_the_renderer_is_missing_or_unusable": P("firmware/tests/preview_run.rs::the_window_keeps_its_fallback_when_the_renderer_is_missing_or_unusable"),
    "test_window_without_a_worker_that_signs_renderers_still_gets_the_packs": PA("firmware/tests/feed_client.rs::a_worker_that_does_not_sign_renderers_is_a_feed_error", note="the feed client half; the settings window glue (packs still installed) is untested"),
    "test_same_commit": P("firmware/src/preview.rs::same_commit_cases"),
    "test_windows_style_names_never_leave_the_folder": DROP("platform-dependent in Python (needs backslash names); the zip-name guard is covered by the outside-the-folder tests", "firmware/src/preview.rs::a_marker_naming_a_file_outside_the_folder_is_not_trusted"),
    "test_size_and_count_limits": P("firmware/src/preview.rs::a_zip_over_its_limits_is_refused"),
    # ---- test_settings_api.py (phase 13: Bridge command behaviour mostly deferred, never written) --
    "test_scan_keeps_the_strongest_of_each_name": P("app/src/commands/clock.rs::a_scan_keeps_the_strongest_row_per_ssid_sorted_by_signal", "app/tests/settings_golden.rs::a_scan_matches_python"),
    "test_set_clock_by_hand_reconnects_and_tells_the_tray": P("app/tests/settings_bridge.rs::a_typed_address_goes_to_the_tray_and_is_used"),
    "test_private_state_is_not_exposed_to_the_page": PA("app/tests/settings_api.rs::private_state_is_not_exposed_to_the_page", note="asserts the 32 command names and that state() never holds the password; Python's public-method introspection has no Rust equivalent"),
    "test_feed_release_is_offered_installed_with_its_packs_and_style": PA("app/tests/settings_api.rs::feed_release_is_offered_installed_with_its_packs_and_style", note="after-heap has pak 0 so the CRC wait is skipped (covered by firmware ota tests)"),
    "test_state_does_not_wait_for_a_slow_feed": PA("app/tests/settings_api.rs::state_does_not_wait_for_a_slow_feed", note="FakeFeed routes are first-match: only /v1/latest is delayed, the release is kept without its packs"),
    # ---- test_setup_api.py ----------------------------------------------------------------------
    "test_agents_lists_found_and_marks_them_offered": PA("app/tests/settings_setup.rs::agents_lists_found_and_marks_them_offered", note="only checks wsl is a boolean: the Rust check reads the machine PATH"),
    # ---- test_tray.py ---------------------------------------------------------------------------
    "test_icons_have_a_status_dot": P("app/tests/shell_tray.rs::icons_are_three_distinct_pngs", note="PNG checks, not Pillow pixel probes"),
    "test_launch_agent_restarts_only_after_a_crash": P("store/tests/golden_store.rs::launch_agent_plist_is_a_byte_copy_of_python"),
    "test_mac_autostart_writes_and_removes_the_plist": P("store/tests/golden_store.rs::mac_autostart_flow_writes_the_python_bytes_under_a_temp_home", "e2e/tests/cleanup.rs::cleanup_removes_hooks_and_the_launch_agent_on_macos_layout"),
    "test_nothing_pops_up_by_itself": DROP("structural: the Rust tray has no auto-popup code; the explicit dialogs are tested in shell_dialogs.rs", "app/tests/shell_dialogs.rs::menu_dialog_reports_a_bad_answer_and_ignores_cancel"),
    "test_settings_window_retry_keeps_the_address": P("app/tests/shell_lifecycle.rs::an_empty_host_is_search_again", "app/tests/shell_lifecycle.rs::a_typed_clock_pins_the_finder_and_refreshes"),
    # ---- test_usage_sources.py ------------------------------------------------------------------
    "test_maps_max_account_response": P("usage/tests/golden.rs::oauth_mapping_matches_python"),
    "test_locked_window_is_rejected": P("usage/tests/golden.rs::oauth_mapping_matches_python"),
    "test_non_normal_severity_is_a_warning": P("usage/tests/golden.rs::oauth_mapping_matches_python"),
    "test_missing_weekly_window_reads_zero": P("usage/tests/golden.rs::oauth_mapping_matches_python"),
    "test_utilization_is_clamped_and_past_resets_are_zero": P("usage/tests/golden.rs::oauth_mapping_matches_python"),
    "test_no_five_hour_window_is_not_applicable": P("usage/tests/golden.rs::oauth_mapping_matches_python"),
    "test_ok_answer_carries_clock_fields": P("usage/tests/golden.rs::usage_result_classification_matches_python"),
    "test_auth_errors_mean_expired_token": P("usage/tests/golden.rs::usage_result_classification_matches_python"),
    "test_not_applicable_answers": P("usage/tests/golden.rs::usage_result_classification_matches_python"),
    "test_transient_answers_return_none": P("usage/tests/golden.rs::usage_result_classification_matches_python"),
    "test_source_failure_is_logged_once": PY("asserts on a log line; the quiet-after-first-failure behaviour is in the code (`quiet` flag) but not asserted"),
    # ---- test_clawd_events.py -------------------------------------------------------------------
    "test_single_session_flow_prompt_tool_stop": scen("single_session_flow"),
    "test_priority_attention_over_error_over_subagents": scen("priority_attention_over_error_over_subagents"),
    "test_tool_shows_an_action_word": scen("tool_word:*"),
    "test_tool_error_shows_the_failed_action": scen("tool_error_word:*"),
    "test_tool_error_without_a_name_uses_the_running_tool_and_stop_error_says_error": scen("tool_error_without_a_name"),
    "test_agent_counts_pick_the_session_headphones_juggling": scen("agent_counts"),
    "test_a_subagent_counts_as_an_agent": scen("a_subagent_counts_as_an_agent"),
    "test_children_still_running_after_the_main_session_stops": scen("children_without_a_main_session"),
    "test_pi_child_processes_count_as_subagents_and_never_celebrate": scen("pi_children_never_celebrate"),
    "test_compact_and_end_and_stale_sessions": scen("compact_end_and_stale_sessions"),
    "test_attention_expires_after_ttl": scen("attention_expires_after_ttl"),
    "test_attention_seen_hides_needs_you_until_the_next_question": scen("attention_seen_hides_until_next_question"),
    "test_questions_look_left_and_right_other_attention_stays_notification": scen("questions_look_left_and_right"),
    "test_a_question_expires_and_stopping_forgets_it": scen("question_expires_and_stopping_forgets_it"),
    "test_seen_session_alone_shows_nothing_and_expires_like_attention": scen("seen_session_alone_shows_nothing"),
    "test_claude_mapping": P(f"{GOLD_EV}::claude_and_pi_normalisers_match_python_bytes", "events/src/source_events.rs::claude_mapping_matches_python"),
    "test_pi_event_validation": P(f"{GOLD_EV}::claude_and_pi_normalisers_match_python_bytes", "events/src/source_events.rs::pi_event_validates_and_prefixes"),
    "test_ascii_text_folds_accents": P(f"{GOLD_EV}::vocab_helpers_match_python", "events/src/vocab.rs::ascii_text_folds_accents_and_caps_length"),
    "test_pet_falls_asleep_and_wakes_with_greeting": scen("pet_sleeps_and_wakes_with_greeting"),
    "test_pet_quiet_hours_dim_wraps_midnight": scen("pet_dim:*"),
    "test_pet_usage_low_and_reset": scen("pet_usage_low_and_reset"),
    "test_pet_milestone_every_n_turns_and_new_day": scen("pet_milestone_every_n_turns"),
    "test_pet_evening_yawn_once_per_interval": scen("pet_evening_yawn_once_per_interval"),
    "test_poke_say_beats_agent_then_agent_returns": scen("poke_say_beats_agent_then_agent_returns"),
    "test_clock_text_starts_with_a_capital": scen("clock_text_starts_with_a_capital"),
    "test_poke_validation": scen("poke_validation"),
    "test_a_question_during_focus_keeps_its_phrase_and_holds_like_notification": scen("question_during_focus_holds_like_notification"),
    "test_focus_countdown_reading_then_done": scen("focus_countdown_reading_then_done"),
    "test_pet_layers_below_agent_and_dim_always": scen("pet_layers_below_agent_and_dim_always"),
    "test_wire_and_state_url": P(f"{GOLD_EV}::wire_and_clock_urls_match_python_bytes", "events/src/pusher.rs::state_and_heap_urls_keep_only_scheme_and_host"),
    "test_long_agent_animations_carry_same_meaning_variants": scen("long_animation_variants:*"),
    "test_thinking_ultrathink_and_wizard_variants": scen("thinking_ultrathink_wizard_variants"),
    "test_the_state_carries_variants_and_a_variant_change_is_pushed": P(f"{SC}[state_carries_variants]", "events/tests/pusher.rs::a_variant_change_is_pushed"),
    "test_read_pet_config_and_port": P(f"{GOLD_EV}::config_readers_match_python", "events/src/config.rs::event_port_order_and_fallbacks", "events/src/config.rs::pet_config_parses_and_clamps"),
    "test_pusher_sends_on_change_resends_and_clears": P("events/tests/pusher.rs::sends_on_change_resends_and_clears"),
    "test_pusher_retries_after_failed_delivery_and_no_targets": P("events/tests/pusher.rs::retries_after_failed_delivery_and_does_nothing_without_targets"),
    "test_event_server_routes": P("events/tests/server.rs::routes_match_test_event_server_routes", "e2e/tests/chain.rs::state_and_poke_work_end_to_end"),
    "test_daemon_wires_events": P("e2e/tests/engine.rs::engine_run_turns_a_hook_post_into_pushes_on_the_clock", "app/tests/shell_engine_wiring.rs::engine_run_pushes_usage_to_the_found_clock", note="phase 14: hook POST through the engine's own wiring to the fake clock"),
    "test_abort_returns_to_idle_without_celebrating": scen("abort_returns_to_idle_without_celebrating"),
    "test_long_thinking_becomes_ultrathink": scen("long_thinking_becomes_ultrathink"),
    "test_ultrathink_between_tools_counts_from_the_last_tool_and_parallel_tools": P(f"{SC}[ultrathink_counts_from_the_last_tool]", f"{SC}[ultrathink_parallel_tools_and_resets]"),
    "test_reading_and_searching_tools_read": scen("reading_tool:*"),
    "test_debug_prompts": P(f"{GOLD_EV}::vocab_helpers_match_python", "events/src/vocab.rs::debug_prompts_and_skill_loads"),
    "test_debug_skill_loads": P(f"{GOLD_EV}::vocab_helpers_match_python", "events/src/vocab.rs::debug_prompts_and_skill_loads"),
    "test_debug_turns_from_the_prompt_or_the_skill_show_the_debugger": scen("debug_turns_from_prompt_or_skill"),
    "test_many_agents_let_the_latest_sessions_reading_debugger_ultrathink_through": scen("many_agents_let_the_latest_session_show_through"),
    "test_running_tests_and_fixing_after_a_failure_show_the_debugger": scen("running_tests_and_fixing_after_a_failure"),
    "test_three_errors_in_two_minutes_make_clawd_dizzy": P(f"{SC}[three_errors_make_clawd_dizzy]", f"{SC}[spread_out_errors_never_dizzy]"),
    "test_edit_streak_types_like_a_boss_and_long_turns_cast_wizard": scen("edit_streak_and_wizard"),
    "test_min_display_holds_agent_animations_but_not_against_a_poke": scen("min_display_holds_agent_animations_but_not_against_a_poke"),
    "test_pokes_alternate_and_three_quick_ones_annoy": scen("pokes_alternate_and_three_quick_ones_annoy"),
    "test_idle_picks_skip_new_animations_on_an_older_desk_pak": P("events/tests/behaviour.rs::an_older_desk_pak_skips_the_new_animations", f"{SC}[idle_picks_old_desk]"),
    "test_done_then_usage_and_agent_activity_restarts_the_gap": scen("done_then_usage_restarts_the_gap"),
    "test_away_keeps_usage_and_picks_then_wakes_on_return": scen("away_keeps_usage_and_picks_then_wakes_on_return"),
    "test_pusher_wakes_up_right_when_an_idle_pick_ends": P("events/src/pusher.rs::wait_for_follows_the_idle_pick_end"),
    "test_usage_screen_blocks_animations_but_not_what_needs_you": scen("usage_screen_blocks_animations_but_not_what_needs_you"),
}
