# PORT-MAP: Python public symbol -> Rust path -> owner phase

Source of truth: `clawd_desktop/` (read-only). Rust paths are the stubs created in phase 02; the
owner phase implements them and may add (never remove/rename) public items. Crate DAG:

```text
contracts <- testkit
contracts <- store <- config <- events <- agents      (agents also uses store, config)
                  config <- usage        (contracts, store, config)
                  store  <- clock        (contracts, store)
                  clock  <- firmware     (contracts, store, config, clock)
                  config <- appupdate    (contracts, store, config)
app uses all
```

Conventions: `AppPaths` replaces module globals (`CONFIG_DIR`, `CONFIG_FILE`, ...); `OsStoreNames`
replaces the keyring service / Run value / LaunchAgent label constants; Python `Callable` hooks
become `Arc<dyn Fn>` aliases; `asyncio.Event` stop is `StopRx`; `use` is `use_clock`.
**BLE is dropped (owner decision Q3)**: every BLE symbol below is `dropped (Q3)`.
Legend for phase: 02 contracts, 03 store, 04 events, 05 agents, 06 config, 07 usage, 08 clock,
09 firmware, 10 appupdate, 12 app shell, 13 settings bridge.

## `__init__`
| Python | Rust | Phase |
|---|---|---|
| `__version__` | `clawd_contracts::APP_VERSION` | 02 |
| `APP_NAME` | `clawd_contracts::APP_NAME` | 02 |
| `_feed_build` (generated) | `clawd_contracts::{BUILD_FEED_URL, BUILD_FEED_TOKEN}` (`option_env!`) | 02 |

## `paths` (split: AppPaths, prefs, paths)
| Python | Rust | Phase |
|---|---|---|
| `CONFIG_DIR`, `CONFIG_FILE`, `CLOCK_FILE`, `PREFS_FILE`, `PAYLOAD_FILE`, `LOG_FILE`, `PAK_FILE`, `RENDERER_DIR`, `HOOK_SCRIPT`, `OTA_BUSY_FILE` | `clawd_contracts::AppPaths::{config_dir, config_file, clock_file, prefs_file, payload_file, log_file, pak_file, renderer_dir, hook_script, ota_busy_file}` | 02 |
| `OTA_BUSY_STALE`, `LOG_MAX`, `PREFS_IO_WAIT`, `SCREEN_USAGE`, `PAYLOAD_KEYS` | `clawd_contracts::{OTA_BUSY_STALE, LOG_MAX, PREFS_IO_WAIT, SCREEN_USAGE, PAYLOAD_KEYS}` | 02 |
| `frozen()` | `clawd_store::paths::is_packaged` | 03 |
| `integration(rel)` | `clawd_agents::assets::integration` (+ consts `PI_CLAWD_HOME_TS`, `OPENCODE_CLAWD_JS`) | 05 |
| `_read_prefs`, `_write_prefs`, `_retrying`, `_lock_byte`, `_unlock_byte`, `_prefs_lock` | internal to `clawd_store::{prefs, lock}`; `lock::prefs_lock` public | 03 |
| `load_prefs` | `clawd_store::prefs::load_prefs` (returns `Prefs`, a Map newtype) | 03 |
| `update_prefs` | `clawd_store::prefs::update_prefs` | 03 |
| `save_prefs` | `clawd_store::prefs::save_prefs` | 03 |
| `clock_screen`, `set_clock_screen` | `clawd_store::prefs::{clock_screen, set_clock_screen}` | 03 |
| `set_ota_busy`, `ota_busy` | `clawd_store::paths::{set_ota_busy, ota_busy}` (900 s; app updater only) | 03 |
| `set_usage_payload`, `usage_payload` | `clawd_store::paths::{set_usage_payload, usage_payload}` | 03 |
| `RotatingLog` | `clawd_store::log::RotatingLog` | 03 |
| `open_log` | `clawd_store::log::init_logging` | 03 |

## `pause`
| Python | Rust | Phase |
|---|---|---|
| `PAUSE_FILE` | `AppPaths::pause_file` | 02 |
| `MAX_LEASE` | `clawd_contracts::MAX_LEASE` (300) | 02 |
| `hold`, `release` | `clawd_store::pause::{hold, hold_at, release}` | 03 |
| `PauseReader` | `clawd_store::pause::PauseReader` | 03 |

## `creds`
| Python | Rust | Phase |
|---|---|---|
| `SERVICE` | `clawd_contracts::KEYRING_SERVICE` / `OsStoreNames.keyring_service` | 02 |
| `DEFAULT_USER` | `clawd_contracts::DEFAULT_CLOCK_USER`, `clawd_store::creds::DEFAULT_USER` | 02 |
| `get`, `put`, `forget` | `clawd_store::creds::{get, put, forget}` (+ `wincred`/`mac` backends) | 03 |

## `autostart`
| Python | Rust | Phase |
|---|---|---|
| `LABEL`, `RUN_KEY`, `RUN_VALUE`, `PLIST` | `clawd_contracts::{LAUNCH_AGENT_LABEL, RUN_KEY, RUN_VALUE}`, `AppPaths::launch_agent_plist` | 02 |
| `command` | `clawd_store::autostart::command` | 03 |
| `launch_agent` | `clawd_store::autostart::launch_agent` (+ `run_value`) | 03 |
| `supported`, `is_enabled`, `enable`, `disable`, `set_enabled` | `clawd_store::autostart::Autostart::{supported, is_enabled, enable, disable, set_enabled}` | 03 |

## `firstrun`
| Python | Rust | Phase |
|---|---|---|
| `STEPS` | `clawd_store::firstrun::STEPS` | 03 |
| `migrate`, `load`, `pending`, `mark_done`, `should_open`, `mark_shown`, `seen`, `mark_seen`, `new_agents` | `clawd_store::firstrun::{same names}` | 03 |

## `clockapi`
| Python | Rust | Phase |
|---|---|---|
| `ClockError`, `AuthNeeded` | `clawd_contracts::{ClockError, AuthNeeded}` | 02 |
| `config_diff` | `clawd_clock::config_diff` | 08 |
| `ClockApi` (`heap config save status scan reboot factory_reset export restore get_file upload_firmware upload_pack style install_style clear_style wait_back set_auth`) | `clawd_clock::ClockApi` (async) | 08 |
| `TIMEOUT UPLOAD_TIMEOUT RETRIES RETRY_GAP OTA_MAX STYLE_MAX PACK_MAX PACK_TIMEOUT PACK_PATHS` | `clawd_clock::api::{...}` (`PACK_PATHS` inside `upload_pack`) | 08 |

## `discovery`
| Python | Rust | Phase |
|---|---|---|
| `Clock` | `clawd_contracts::ClockTarget` (re-export `clawd_clock::Clock`) | 02 |
| `manual_clock`, `valid_host` | `clawd_clock::{manual_clock, valid_host}` | 08 |
| `ClockStore`, `stored_clock` | `clawd_clock::{ClockStore, stored_clock}` | 08 |
| `MdnsBrowser` | `clawd_clock::MdnsBrowser` (impl of `MdnsSource`) | 08 |
| `ClockFinder` (`targets describe report set_manual use retry step run`) | `clawd_clock::ClockFinder` (`use` -> `use_clock`) | 08 |
| `SERVICE` | `clawd_contracts::MDNS_SERVICE` | 02 |
| `FAIL_LIMIT BAD_HOLD TICK RESOLVE_TIMEOUT_MS` | `clawd_clock::{finder, mdns}::{...}` | 08 |

## `bootwatch`
| Python | Rust | Phase |
|---|---|---|
| `BootWatch` | `clawd_clock::BootWatch` | 08 |
| `heap_url` (internal duplicate) | `clawd_events::heap_url` | 04 |
| `PROBE_EVERY PROBE_TIMEOUT BOOT_JITTER FRESH_BOOT` | `clawd_clock::bootwatch::{...}` | 08 |

## `events`
| Python | Rust | Phase |
|---|---|---|
| `EVENT_HOST EVENT_PORT STATE_PATH HEAP_PATH MAX_BODY` | `clawd_contracts::{EVENT_HOST, EVENT_PORT, STATE_PATH, HEAP_PATH, MAX_BODY}` | 02 |
| `STATE_TTL` | `clawd_contracts::STATE_TTL` | 02 |
| `View` | `clawd_contracts::View` (re-export `clawd_events::View`) | 02 |
| `PetConfig` | `clawd_contracts::PetConfig` | 02 |
| `desired()` dict / `wire()` dict | `clawd_contracts::{DesiredState, WireState, WireAnim}` | 02 |
| `EVENTS`, `CLAUDE_HOOK_EVENTS` | `clawd_events::{EVENTS, CLAUDE_HOOK_EVENTS}` | 04 |
| `is_test_command`, `is_debug_prompt`, `is_debug_skill_load`, `ascii_text`, `is_question_tool`, `tool_action` | `clawd_events::vocab::{same}` | 04 |
| `_tool_summary` | `clawd_events::tool_summary` (public) | 04 |
| `claude_event`, `pi_event` | `clawd_events::{claude_event, pi_event}` | 04 |
| `read_event_port`, `read_pet_config` | `clawd_events::{read_event_port, read_pet_config}` | 04 |
| `Moment`, `Sess`, `Tracker` | `clawd_events::tracker::{Moment, Sess, Tracker}` | 04 |
| `Pet` | `clawd_events::Pet` | 04 |
| `Pokes` | `clawd_events::Pokes` | 04 |
| `Brain` | `clawd_events::{Brain, SharedBrain}` | 04 |
| `read_hid_idle` (`_windows_idle`) | `clawd_events::read_hid_idle` | 04 |
| `state_url`, `heap_url`, `wire` | `clawd_events::{state_url, heap_url, wire}` | 04 |
| `StatePusher` | `clawd_events::StatePusher` | 04 |
| `EventServer` | `clawd_events::EventServer` + `NormalizeFn` alias | 04 |
| `log` | `tracing` | 03 |

## `agents` (package)
| Python | Rust | Phase |
|---|---|---|
| `AgentError` | `clawd_contracts::AgentError` | 02 |
| `agents.normalize` | `clawd_agents::normalize` (fits `clawd_events::NormalizeFn`) | 05 |
| `registry`, `ORDER`, `IDS` | `clawd_agents::{Registry, IDS}` | 05 |
| `infos`, `found_ids`, `uninstall_all`, `refresh_plugins` | `clawd_agents::{same}` | 05 |
| `base.Agent`, `SCRIPT_MARK`, `HTTP_MARK` | `clawd_agents::{Agent, AgentCtx, AgentInfo, SCRIPT_MARK, HTTP_MARK}` | 05 |
| `base.handler_text`, `read_json`, `write_json`, `load`, `merge_groups`, `merge_flat`, `has_ours`, `edit_json`, `first_str`, `base_event`, `tool_summary`, `prepared` | `clawd_agents::json_edit::{same}` (`tool_summary` re-exports events) | 05 |
| `plugin.PluginAgent` | `clawd_agents::plugin::{is_installed, needs_update, install, uninstall, refresh}` | 05 |
| `claude.Claude`, `codex.Codex`, `copilot.Copilot`, `cursor.Cursor`, `gemini.Gemini`, `opencode.OpenCode`, `pi.Pi` | `clawd_agents::{claude, codex, copilot, cursor, gemini, opencode, pi}::{same}` | 05 |

## `hooks`
| Python | Rust | Phase |
|---|---|---|
| `EVENTS`, `MARKS`, `HOOK_SH` | `clawd_agents::hooks::{EVENTS, MARKS, hook_script_text}` | 05 |
| `event_port`, `settings_files`, `handler`, `write_script`, `merge`, `is_installed`, `apply` | `clawd_agents::hooks::{same}` | 05 |
| `_read`, `_write` | `clawd_agents::json_edit::{read_json, write_json}` | 05 |

## `usage` (read side -> config; poll/publish side -> usage)
| Python | Rust | Phase |
|---|---|---|
| `UsageNotApplicable`, `TokenExpired` | `clawd_contracts::{UsageNotApplicable, TokenExpired}` | 02 |
| `HTTP_PUSH_PATH` | `clawd_contracts::HTTP_PUSH_PATH` | 02 |
| `DEFAULT_CONFIG_DIR`, `ANTHROPIC_STORE_DIR` | `AppPaths::{default_claude_dir, anthropic_store_dir}` | 02 |
| `ATTENTION_DIR`, `ATTENTION_FLAG` | `AppPaths::{attention_dir, attention_flag}` | 02 |
| `CONFIG_FILE` | `AppPaths::config_file` | 02 |
| `KEYCHAIN_SERVICE` | `clawd_config::tokens::KEYCHAIN_SERVICE` | 06 |
| `_extract_access_token`, `_decode_keychain_blob`, `_read_token_keychain`, `_read_token_store` | `clawd_config::tokens::{extract_access_token, decode_keychain_blob, read_token_keychain, read_token_store}` | 06 |
| `read_token_for` | `clawd_config::read_token_for` | 06 |
| `read_config_dirs` | `clawd_config::read_config_dirs` | 06 |
| `read_config_value`, `read_onoff_setting`, `read_chime_setting`, `read_clock_setting`, `read_usage_source` | `clawd_config::{same}` | 06 |
| `normalize_http_target`, `read_http_targets` | `clawd_config::{same}` | 06 |
| `detect_hour_format` | `clawd_config::detect_hour_format` | 06 |
| `read_cliproxy_key`, `read_cliproxy_url`, `CLIPROXY_*` consts | `clawd_config::cliproxy::{same}` | 06 |
| (`engine.has_claude_token`) | `clawd_config::has_claude_token` | 06 |
| `API_URL`, `USAGE_URL`, `API_HEADERS_TEMPLATE`, `API_BODY`, `USAGE_HEADERS_TEMPLATE` | `clawd_usage::oauth::{API_URL, USAGE_URL}` (+ header/body constants added by 07) | 07 |
| `poll_api`, `poll_usage_endpoint`, `poll_token`, `usage_from_oauth_usage`, `_billing_period_info`, `_minutes_until`, `_window_pct`, `_usage_result` | `clawd_usage::oauth::{same}` (private helpers internal) | 07 |
| `CliProxySource` | `clawd_usage::CliProxySource` | 07 |
| `PlanSelector`, `_SELECTOR`, `_BACKOFF`, `_CLIPROXY` | `clawd_usage::{PlanSelector, PlanKey, PollState}` | 07 |
| `_poll_dir`, `poll_active`, `poll_claude`, `poll_active_payload` | `clawd_usage::{poll_claude, poll_active, poll_active_payload}` | 07 |
| `add_chime_field`, `add_clock_fields`, `to_ascii`, `take_attention_flag`, `take_clears`, `encode_payload` | `clawd_usage::payload::{same}` | 07 |
| `POLL_INTERVAL`, `TICK`, `DEFAULT_MAX_WRITE`, `ATTENTION_MAX_AGE`, `HTTP_PUSH_TIMEOUT`, `RESEND_TIMEOUT` | `clawd_usage::{publisher, payload, sinks}::{same}` | 07 |
| `HttpSinks` (and `engine.TrackingSinks`) | `clawd_usage::HttpSinks::new(Some(ResultHook))` | 07 |
| `Publisher` (`step resend send run wake force on_payload`) | `clawd_usage::Publisher` (`request_poll`, `set_on_payload`) | 07 |
| `start_events` | app engine wiring | 12 |
| `log`, `main` | `tracing` / dropped (standalone daemon) | - |
| `BleakError`, `Session`, `DEVICE_NAME`, `SERVICE_UUID`, `RX_CHAR_UUID`, `REQ_CHAR_UUID`, `CONNECT_TIMEOUT`, `SAVED_ADDR_FILE`, `load_cached_address`, `_get_cb_manager`, `retrieve_connected_macos`, `discover_target`, `_is_encryption_error`, `BLUEUTIL_TIMEOUT`, `_blueutil`, `unpair_macos`, `connect_and_run`, `ble_loop`, `Publisher.attach_ble/detach_ble` | dropped (Q3) | - |

## `codex_usage`
| Python | Rust | Phase |
|---|---|---|
| `TAIL`, `FILES`, `sessions_dir`, `newest_files`, `payload_from`, `read`, `available` | `clawd_usage::codex::{same}` | 07 |

## `firmware`
| Python | Rust | Phase |
|---|---|---|
| `CLAWD_VARIANT` | `clawd_contracts::CLAWD_VARIANT` | 02 |
| `bundle_dir`, `StylePack`, `STYLE_MAGIC`, `STYLE_MAX`, `STYLE_ID`, `load_styles`, `Bundle`, `load_bundle`, `is_clawd`, `clock_seq`, `update_state`, `clock_style_api`, `clock_style`, `style_state` | `clawd_firmware::firmware::{same}` (`STYLE_ID` -> `is_valid_style_id`) | 09 |

## `feed`
| Python | Rust | Phase |
|---|---|---|
| `FeedError` | `clawd_contracts::FeedError` | 02 |
| `FeedFile`, `Manifest` | `clawd_contracts::{FeedFile, FeedManifest}` (re-export `clawd_firmware::feed::Manifest`) | 02 |
| `KINDS PACK_KINDS STYLE_KINDS RENDERER_PLATFORMS TIMEOUT DOWNLOAD_TIMEOUT CHECK_EVERY MAX_FILE RENDERER_MAX` | `clawd_firmware::feed::{same}` | 09 |
| `CACHE_DIR` | `AppPaths::feed_cache_dir` | 02 |
| `renderer_platform`, `parse_manifest`, `decrypt`, `Release`, `make_release`, `Feed`, `from_settings`, `pack_state`, `plan`, `needed`, `worth_it`, `offer`, `_styles`, `HEAP_KEYS` | `clawd_firmware::feed::{same}` (`_styles` -> `styles_of`) | 09 |

## `ota`
| Python | Rust | Phase |
|---|---|---|
| `SETTLE LEASE UPLOAD_LEASE CRC_WAIT PACK_TRIES` | `clawd_firmware::ota::{same}` | 09 |
| `install`, `install_packs`, `install_release`, `install_then_save`, `_wait_crcs` | `clawd_firmware::ota::{same}` (`OtaOptions`) | 09 |

## `preview`
| Python | Rust | Phase |
|---|---|---|
| `W H FRAME_BYTES RUN_TIMEOUT CACHE_MAX CLIP_MAGIC RENDER_API CLASSIC PAK_MAX` | `clawd_firmware::preview::{same}` | 09 |
| `preview_dir`, `Renderer`, `Frame`, `parse_clip`, `apng`, `apng_data_url`, `load_renderer`, `same_commit`, `unpack_renderer`, `load_unpacked`, `bmp_data_url`, `live_payload`, `pak_count`, `PakCache`, `FrameCache`, `frame_key` | `clawd_firmware::preview::{same}` | 09 |
| `RENDERER_FILE UNPACK_MAX UNPACK_FILES MARK` | internal to `preview` | 09 |

## `appupdate`
| Python | Rust | Phase |
|---|---|---|
| `UpdateError`, `AppRelease` | `clawd_contracts::{UpdateError, AppRelease}` | 02 |
| `REPO`, `BUNDLE_ID`, `SUMS_NAME` | `clawd_contracts::{UPDATE_REPO, BUNDLE_ID, SUMS_NAME}` (+ `SUMS_SIG_NAME`, `MINISIGN_PUBKEY`) | 02 |
| `STAGE_DIR` | `AppPaths::app_update_stage_dir` | 02 |
| `parse_version`, `is_newer`, `this_platform`, `asset_name`, `parse_sums`, `team_id`, `sha256_file` | `clawd_appupdate::version::{same}` | 10 |
| `Source`, `from_settings`, `auto_enabled`, `API CHECK_EVERY FIRST_CHECK RETRY RECHECK TIMEOUT DOWNLOAD_TIMEOUT MAX_DOWNLOAD MAX_JSON` | `clawd_appupdate::source::{same}` | 10 |
| (new) minisign | `clawd_appupdate::minisig::verify_sums` | 10 |
| `NO_TEAM`, `MAC_SWAP`, `bundle_team`, `check_bundle`, `Runner` | `clawd_appupdate::codesign::{NO_TEAM, mac_swap_script, bundle_team, check_bundle}`, `clawd_config::CommandRunner` | 10 |
| `Staged`, `running_bundle`, `can_apply`, `Updater` (`state staged clean check check_async apply`), `updater` | `clawd_appupdate::updater::{same}` (+ `UpdaterState`, `UpdaterOptions`) | 10 |

## `engine`, `tray`, `popover`, `icons`, `cli`, `settings_app`, `settings_api` (app crate)
| Python | Rust | Phase |
|---|---|---|
| `engine.Engine` | `clawd_app::engine::EngineHandle` (+ `Status`) | 12 |
| `engine.daemon_running` | `clawd_events::daemon_running` | 04 |
| `engine.TrackingSinks`, `has_claude_token` | `clawd_usage::HttpSinks`, `clawd_config::has_claude_token` | 07/06 |
| `tray.status`, `lost` | `clawd_app::tray` (phase 12) | 12 |
| `tray.open_path` | `clawd_store::open::open_path` | 03 |
| `tray.ask_text`, `alert`, `Tray`, `first_run`, `main` | `clawd_app::{dialogs, tray, lifecycle}` | 12 |
| `popover.view`, `load_pak`, `MacPopover`, `supported` | `clawd_app::popover` (`handle` stubbed in 02) | 12 |
| `icons.status_icon`, `base_icon` | PNG files in `app/icons/` | 12 |
| `cli.main`, `remembered_clock` | `clawd_app::cli` | 12 |
| `settings_app.command`, `main`, `Channel`, `TABS` | `ShellCmd` + `windows.rs` (`state.rs` frozen) | 02/12 |
| `settings_app` stdin protocol (`refresh app_update_apply set_clock use_clock usage_source setup`) | `clawd_app::state::ShellCmd` | 02 |
| `SettingsApi` (32 JS-called methods) | `clawd_app::commands::<group>::<name>` behind `py_api` | 13 |
| `SettingsApi.preview/busy/set_hooks/close_mdns` | not exposed | 13 |
| `settings_api.SCHEMA`, `clean`, `_err`, `_fail` | `commands` helpers (phase 13) | 13 |

### `py_api` allowlist -> group file
`state`: state. `clock`: login scan set_clock reboot factory_reset save. `agents`: agents set_agent
set_agents disconnect_all. `setup`: setup_state finish_setup setup_clocks setup_use_clock. `usage`:
usage_preview usage_sources set_usage_source clawd_pak. `update`: app_update app_update_check
app_update_apply app_update_page. `firmware`: fw check_update update. `backup`: export_backup
import_backup. `misc`: set_autostart open_log open_web. `jobs`: job.
