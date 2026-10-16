---
phase: 2
title: "Workspace, contracts, API stubs"
status: completed
priority: P1
effort: "1.5d"
dependencies: []
---

# Phase 02: Workspace, contracts, API stubs

## Overview
Phase "mở cổng" cho song song: Cargo workspace, crate `clawd-contracts` (kiểu + hằng số chung), **stub API công khai cho mọi crate** (chữ ký thật, thân `todo!()`), Tauri app skeleton với 3 file đóng băng (`state.rs`, `commands/mod.rs`, `build.rs`), testkit theo area, harness golden cách ly HOME, CI Rust. Kết thúc bằng **contract review gate**. Sai chữ ký ở đây = chặn nhiều phase → làm kỹ.

## Context Links
- [scout §1 module graph, §2 JS seam, §3 contracts](./reports/scout-report.md)
- [researcher-02 crate table](./research/researcher-02-crates-compat.md)
- Red team #1, #3, #5–9, #11, #12, #14, #15 (plan.md)

## Requirements
- Functional:
  - **Workspace** `desktop-rs/Cargo.toml`, members: `crates/clawd-{contracts,testkit,store,events,agents,config,usage,clock,firmware,appupdate}`, `app` (crate `clawd-app`, `[[bin]] name = "Clawd"`; `tauri.conf.json` `mainBinaryName: "Clawd"`), `tests-e2e` (crate rỗng, chủ = 14). `desktop-rs/spike` KHÔNG là member.
  - **`[workspace.dependencies]`** pin sẵn: tokio, serde, serde_json (`preserve_order`), thiserror, tracing, tracing-subscriber, reqwest 0.13 (rustls; provider theo spike-decision nếu có), diqwest, hyper 1 (`server`,`http1`) + hyper-util + http-body-util (event server; không axum), mdns-sd 0.21, aes-gcm 0.11, sha2, hex, base64, fs4, tempfile, zip, miniz_oxide, minisign-verify, windows (Win32_Security_Credentials, Win32_System_Registry, Win32_UI_Input_KeyboardAndMouse, Win32_System_Console, Win32_Storage_FileSystem), security-framework, plist, tauri 2.12, tauri-build, tauri-plugin-positioner, tauri-plugin-dialog (file dialog backup), insta (dev). **Không** `auto-launch`, không `tauri-plugin-single-instance`.
  - **`[profile.release]`**: `opt-level` theo spike (mặc định "s"), `lto=true`, `codegen-units=1`, **`panic="unwind"`** (spike đo chênh size; chỉ đổi sang abort nếu owner duyệt), `strip=true`.
  - **Stub mechanical** từ Python public symbols (scout §1 + `grep ^def|^class`): module Python → module Rust cùng tên; hàm public → `pub fn`/`pub async fn` + doc trích docstring + `todo!()`. Bảng `desktop-rs/PORT-MAP.md`: symbol Python → Rust path → phase chủ; BLE symbols ghi "dropped (Q3)".
  - **Phân chia events/agents (DAG mới):** `clawd-events` giữ `EVENTS`, `CLAUDE_HOOK_EVENTS`, `tool_action`, `is_question_tool`, `is_debug_prompt`, `is_test_command`, `ascii_text`, `_tool_summary`, `claude_event`, `pi_event`, `read_event_port`, `read_pet_config` (đúng vị trí Python). `clawd-agents` **phụ thuộc** `clawd-events`. EventServer nhận `normalize: Arc<dyn Fn(&str, &Value, &str) -> Option<Vec<Value>> + Send + Sync>` (type alias `NormalizeFn` trong events); app truyền `clawd_agents::normalize`. `tray.open_path` → `clawd_store::open_path`.
  - **Contracts gồm:** `Prefs` = newtype quanh `serde_json::Map` + accessor typed trả `None` khi sai kiểu (không struct typed — red team #11); `UsagePayload`, `View`/wire types, `ClockTarget`, `PetConfig`, `FeedManifest` (+`renderers`), `AppRelease`, error enums (`ClockError`, `AuthNeeded`, `FeedError`, `UpdateError`, `TokenExpired`, `UsageNotApplicable`, `AgentError`), `AppPaths` (root inject được), hằng số frozen (port 47913, service `vn.clawd.desktop`, bundle id, Run value `Clawd`, LaunchAgent label, AppId, asset name templates, `MAX_LEASE` 300, `OTA_BUSY_STALE` 900, `MINISIGN_PUBKEY` = `option_env!("CLAWD_MINISIGN_PUBKEY")` mặc định khoá production (placeholder tới khi phase 11 tạo khoá, vào qua CR); override chỉ dùng cho build test trên fork (phase 14). `OsStoreNames { keyring_service, run_value, launch_label }` inject được để test (red team #14).
  - **Pause vs ota.busy** (red team #9): API riêng trong stub `clawd-store`: `pause::{hold(secs, reason), release(), PauseReader::reason()}` (pause.json lease ≤ 300 s, engine đọc) và `paths::{set_ota_busy, ota_busy}` (900 s, chỉ appupdate đọc).
  - **`app/src/state.rs` (đóng băng):** `AppState { engine: EngineHandle, shell_tx: mpsc::Sender<ShellCmd>, jobs: Arc<JobState>, status: watch::Receiver<Status> }`; `enum ShellCmd { Refresh, AppUpdateApply, SetClock { host: String }, UseClock { name, host, port: u16, version }, UsageSource, Setup }` (y hệt giao thức `settings_app.py:7-14`); `JobState { busy: AtomicBool, … }`; `SettingsSession { base, key, tab }` tạo mới mỗi lần mở cửa sổ settings.
  - **`app/src/commands/mod.rs` (đóng băng):** `pub fn register(b) -> b` với **một** `generate_handler![py_api, popover_msg]`; `py_api(window, name, args)`: kiểm `window.label() == "settings"` rồi dispatch theo allowlist 32 tên (scout §2, đã grep: `agents … usage_sources`) sang hàm stub trong `commands/<group>.rs` (chủ = 13); `popover_msg(window, msg)`: kiểm label `popover` rồi gọi `crate::popover::handle` (chủ = 12).
  - **`app/build.rs` (đóng băng):** `tauri_build::try_build(Attributes::new().app_manifest(AppManifest::new().commands(&["py_api","popover_msg"])))` để capabilities có hiệu lực (red team #7).
  - **Event contract:** labels `settings`, `popover`; events `clawd://ready` (JS→Rust, show window), `clawd://show-tab`, `clawd://busy-close`, `clawd://popover-update`, `clawd://popover-pak`, `clawd://popover-visible`.
  - **App skeleton:** `main.rs` gọi `commands::register`; `tauri.conf.json` (`app.windows: []`, `withGlobalTauri: true`, CSP `default-src 'self'; img-src 'self' data: blob:`, identifier `vn.clawd.desktop`, productName `Clawd`, mainBinaryName `Clawd`); copy `clawd_desktop/ui/*` → `app/ui/`, `integrations/{pi/clawd-home.ts,opencode/clawd.js}` → `crates/clawd-agents/assets/` (chủ = 05, nhúng `include_bytes!`; crate lib không phụ thuộc cây thư mục app).
  - **Testkit:** `testkit/src/core.rs` (02): loader fixtures, temp config dir, fake HTTP server tổng quát (route → response, ghi request), clock giả; `testkit/src/<area>.rs` rỗng cho từng phase B.
  - **`tools/golden/_harness.py`** (red team #14): trước khi import `clawd_desktop`, đặt `HOME`/`USERPROFILE`/`CODEX_HOME`/`XDG_CONFIG_HOME` vào temp dir, monkeypatch mọi hằng `paths.*`, `usage.CONFIG_FILE`, `Path.home`, keyring backend = in-memory; audit hook (`sys.addaudithook` `open`) **fail** nếu đọc/ghi dưới home thật. Ghi `{input, output}` JSON (sort keys, float repr ổn định), mỗi file < 512 KB, nhị phân lưu dạng sha256 (không base64 nội dung art).
  - **`tools/check_public.py`** mở rộng: quét `desktop-rs/fixtures/**` tìm magic `CLWD`, base64 PNG/APNG (`iVBORw0KGgo`), `sk-ant-`, đường dẫn home tuyệt đối (`/Users/`, `C:\\Users\\`) → fail.
  - **`.github/workflows/rust-ci.yml`**: trigger `push`/`pull_request`; `permissions: contents: read`; **không secret**; actions pin theo commit SHA; matrix `windows-latest`, `macos-14`: fmt, clippy, test workspace, build app; job `ubuntu-latest` chỉ `cargo check -p clawd-contracts -p clawd-config -p clawd-events` (Linux là non-goal); job `check_public`.
- Non-functional: `cargo build --workspace` xanh Win + mac; stub không có logic.

## Architecture
```text
desktop-rs/
  Cargo.toml  PORT-MAP.md  fixtures/<area>/   (area: store events agents config usage clock fw appupdate settings)
  crates/clawd-contracts  clawd-testkit  clawd-store  clawd-events  clawd-agents
         clawd-config  clawd-usage  clawd-clock  clawd-firmware  clawd-appupdate
  app/   (clawd-app → Clawd.exe: build.rs, src/state.rs, src/commands/mod.rs, ui/, capabilities/)
  tests-e2e/
```

| Crate | Phụ thuộc (chỉ API stub) |
|---|---|
| contracts | – |
| testkit | contracts |
| store | contracts |
| config | contracts, store |
| events | contracts, config |
| agents | contracts, store, config, events |
| usage | contracts, store, config |
| clock | contracts, store |
| firmware | contracts, store, config, clock |
| appupdate | contracts, store, config |
| app | tất cả |

## Related Code Files
- Create: cây trên (stub), `tools/golden/_harness.py`, `.github/workflows/rust-ci.yml`, `desktop-rs/PORT-MAP.md`, `.gitignore` (`desktop-rs/target`)
- Modify: `tools/check_public.py` (thêm quét fixtures)

## Implementation Steps
1. Workspace + profile + deps pin.
2. `clawd-contracts` từ scout §3 + `paths.py`, `pause.py`, `events.py` (`wire`, `View`), `usage.py` (payload keys), `feed.py:120-164`, `autostart.py:33-45`.
3. Stub crate theo PORT-MAP; compile Win + mac.
4. App skeleton + `state.rs` + `commands/mod.rs` + `build.rs` + copy UI/integrations.
5. Testkit core + module rỗng; `_harness.py` + audit hook; 1 fixture mẫu `fixtures/store/prefs_roundtrip.json` chứng minh pipeline.
6. Mở rộng `check_public.py`; `rust-ci.yml`.
7. **Contract review gate**: 1 reviewer agent đối chiếu PORT-MAP với Python (mọi symbol dùng chéo module có stub, đúng crate theo DAG; `ShellCmd` khớp `settings_app.py:7-14`; allowlist khớp grep UI) + owner duyệt. Sửa xong mới mở wave B.

## Todo
- [ ] Workspace + profile (unwind) + deps pin
- [ ] clawd-contracts + hằng số frozen + OsStoreNames
- [ ] Stub 9 crate + PORT-MAP.md (DAG mới events→agents)
- [ ] App skeleton + state.rs + commands/mod.rs + build.rs
- [ ] Testkit core + module per area
- [ ] `_harness.py` cách ly HOME + audit hook
- [ ] check_public mở rộng + rust-ci.yml
- [ ] Contract review gate pass

## Success Criteria
- [ ] `cargo build --workspace` + `cargo test --workspace` (test todo `#[ignore]`) xanh CI Win + mac; ubuntu check xanh
- [ ] PORT-MAP.md phủ 100% public symbol Python dùng chéo module; không cạnh vi phạm DAG
- [ ] Harness chạy exporter mẫu với audit hook bật, không chạm home thật
- [ ] Reviewer + owner ký gate; CR = rỗng

## Risk Assessment
- Chữ ký sai phát hiện muộn → tín hiệu: > 2 CR/ngày. Phản ứng: integrator dừng nhận phase mới 0.5d, sửa contracts một lượt, rebase.
- `windows`/`security-framework` chỉ build trên OS tương ứng → `cfg(target_os)` ngay trong stub.

## Security Considerations
- Feed URL/token: `option_env!` lúc build (thay `_feed_build.py`), không trong repo. CI không có secret.

## Next Steps
- Mở wave B (03–10); 11–13 khi có thêm phase 01.
