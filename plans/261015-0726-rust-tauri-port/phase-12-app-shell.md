---
phase: 12
title: "App shell: engine, tray, popover, CLI"
status: pending
priority: P1
effort: "3d"
dependencies: [1, 2]
---

# Phase 12: App shell — engine wiring, tray, popover, CLI, lifecycle

## Overview
Vỏ Tauri v2 thay `tray.py` (551), `engine.py` (241), `popover.py` (320), `cli.py` (130), `icons.py` (48), và vai trò "tray" của `settings_app.py`: 1 process, tray-only, engine tokio, xử lý `ShellCmd` từ settings, cửa sổ tạo lười + destroy khi đóng (ngưỡng paint trong `spike-decision.md`), CLI giữ nguyên subcommand. Phần lớn làm song song với 03–10 trên API stub; bước cuối "golden thật" chạy sau khi 03–10 merge (B'').

## Context Links
- `clawd_desktop/{tray,engine,popover,cli,icons,settings_app}.py`, `ui/popover.html`
- scout §2 (popover messages), §5 (threads → tokio tasks)
- [researcher-01 §1–4](./research/researcher-01-tauri-packaging.md), `reports/spike-decision.md`
- Red team #1, #5, #7, #9, #10, #12, #14
- Tests Python: `test_engine` (9), `test_tray` (17), `test_popover` (11), `test_firstrun` (phần cli/tray)

## Requirements
- Functional:
  - **Lifecycle:** `app.windows: []`; `RunEvent::ExitRequested` → `prevent_exit()` chỉ khi `code.is_none()`; Thoát → `engine.stop()` idempotent (commit `03928c8`) → `app.exit(0)`.
  - **Single instance** (y hệt Python, không plugin): `daemon_running()` true → log + alert "Clawd đang chạy rồi (xem biểu tượng trên thanh menu / khay hệ thống)." → exit 0 (`tray.py:539-542`); `run` headless → in thông báo, exit 1 (`cli.py:24-26`).
  - **Engine** (`engine.rs`): 1 tokio runtime; `run()` gom ClockFinder, usage Publisher (TrackingSinks → status), EventServer (normalize = `clawd_agents::normalize`) + Brain + StatePusher, BootWatch (`on_restart` → resend), `_watch_token`, `_sync_screen`; **targets rỗng khi `PauseReader::reason()` có** (`engine.py:71-82`, `:132`); `poll_now`, `snapshot`, `targets`, `read_clock_screen`, `call` (qua `mpsc`).
  - **Chịu lỗi** (red team #12): mỗi task/loop bọc `catch_unwind` + log, tự khởi động lại sau backoff (≙ catch-all `tray.py:248,297,436`, `gather(return_exceptions=True)` `engine.py:185`); panic hook ghi `clawd-desktop.log`.
  - **ShellCmd handler** (red team #5): nhận từ `AppState.shell_tx` (`state.rs` của 02): `Refresh` (autostart/hooks đổi → cập nhật menu), `AppUpdateApply` (apply qua `clawd_appupdate` → `engine.stop()` → exit), `SetClock{host}` (`finder.set_manual`/`retry` khi rỗng), `UseClock{…}` (`finder.use`), `UsageSource` (`engine.poll_now`), `Setup` (rescan `setup_left`/`new_agents`) — như `tray.py:476-505`.
  - **Background loops:** refresh 3 s (chỉ đổi icon khi khác), feed check (60 s đầu, 1800 s lặp), app update check (lúc mở + 6 h, chỉ stage), agents rescan 600 s.
  - **Tray** (red team #10): icon xanh/vàng/đỏ = PNG vẽ sẵn (sinh một lần từ `icons.py`, commit vào `app/icons/`); menu native **chép nguyên `tray.py:192-208`**: dòng head (disabled), dòng detail (disabled, ẩn khi rỗng), separator, "● Khởi động lại để cập nhật Clawd {v}", "● Có firmware mới (#{n})", "● Hoàn tất thiết lập…", "● Tìm thấy {tên}: nối với Clawd…", "Cài đặt…" (default), "Nhập địa chỉ đồng hồ…" (chỉ khi mất đồng hồ), separator, "Thoát Clawd". Head/detail theo `status()` (`tray.py:72,76`). Không thêm mục nào.
  - **Dialog native** (`dialogs.rs`): port `ask_text` (macOS `osascript`, Windows PowerShell InputBox, `tray.py:101-121`) và `alert` (`:123-135`), `manual()` (`:304-310`) — chạy được khi không có WebView2.
  - **Popover** (macOS, parity): `WebviewWindow` frameless label `popover` tạo khi click trái, positioner `TrayBottomCenter`, mất focus = destroy (không giữ ẩn — RAM ưu tiên, Validation 1). JS→Rust: `invoke('popover_msg', {msg})` → `crate::popover::handle` (registry đóng băng ở 02 gọi vào) cho `ready`, `size{h 40..400}`, `settings`, …; Rust→JS: `clawd://popover-update|pak|visible`. Sửa `app/ui/popover.html`.
  - **Settings window** (`windows.rs`): `open_settings(tab)` tạo `WebviewWindow` label `settings` (`visible(false)`, `backgroundColor`, show khi nhận `clawd://ready`) với **`SettingsSession { base, key, tab }` mới mỗi lần mở** (base/key = đồng hồ đã nhớ, như `tray.py:445-466`); đã mở → focus + `clawd://show-tab`; đóng khi `jobs.busy` → `clawd://busy-close` + chặn; đóng → destroy; Windows: `SetProcessWorkingSetSize(-1,-1)` nếu spike chứng minh có lợi.
  - **CLI** (`cli.rs`, parse tay): `tray` (mặc định), `run`, `find --seconds`, `hooks on|off`, `autostart on|off`, `cleanup` (autostart off + `uninstall_all`, log y hệt), `settings --clock --key --tab` = **chế độ chỉ cửa sổ** (không tray/engine/server, như process settings riêng của Python; `ShellCmd::SetClock` khi không có tray → ghi `clock.json`, `settings_app.py:46-75`), `--version`; bỏ `-psn_*`. **Không** có `--autostart`. Windows: `AttachConsole(ATTACH_PARENT_PROCESS)` cho subcommand.
  - **First-run:** chưa xong setup → mở settings tab `setup`; bật autostart lần đầu như `tray.py:153-166`.
  - Logging: `tracing` → `RotatingLog`.
- Non-functional: tray ≤ 1 s (engine khởi động sau khi tray hiện); RAM idle ≤ 20 MB; không busy-loop.

## Architecture
`app/src/{main.rs, lib.rs, lifecycle.rs, engine.rs, tray.rs, dialogs.rs, popover.rs, windows.rs, cli.rs}`, `app/icons/**`, `app/ui/popover.html`, `app/tauri.conf.json`, `app/tests/shell_*.rs`. Không sửa `state.rs`, `commands/mod.rs`, `build.rs` (đóng băng ở 02), `app/Cargo.toml` (CR qua integrator).

## Related Code Files
- Own: các file trên
- Read-only: `app/src/commands/*.rs` (13), `app/ui/**` khác popover.html, mọi crate

## Implementation Steps
1. Lifecycle + tray + icons + menu literal + dialogs; đo tray ≤ 1 s.
2. Engine wiring trên API stub (chỗ cần impl → test `#[ignore = "needs 03-10"]`).
3. ShellCmd handler + background loops + status → icon/menu.
4. Settings window manager (SettingsSession, ready, busy-close, show-tab).
5. Popover macOS + popover.html. <!-- Updated: Validation Session 1 - Design A only -->
6. CLI + console attach + cleanup + chế độ settings-only.
7. **B'' (sau 03–10 merge):** bỏ `#[ignore]`, chạy test thật; bảng mapping test (engine/tray/popover/cli; `test_tray.py:64,150,157` dialog → port).

## Todo
- [ ] Lifecycle + tray + icons + menu literal + dialogs
- [ ] Engine wiring + pause + catch_unwind
- [ ] ShellCmd handler + loops + status
- [ ] Settings window manager + SettingsSession
- [ ] Popover macOS
- [ ] CLI (không --autostart) + cleanup + settings-only
- [ ] B'': test thật + bảng mapping

## Success Criteria
- [ ] `cargo test -p clawd-app --test 'shell_*'` xanh; `Clawd.exe cleanup` gỡ hook + autostart (OsStoreNames test, home tạm)
- [ ] Tray ≤ 1 s, RAM idle ≤ 20 MB (`tools/perf`) Win11
- [ ] Mở/đóng settings 50 lần: RAM về baseline ±3 MB
- [ ] Panic giả trong 1 loop: app sống, loop tự chạy lại, log có stack

## Risk Assessment
- Paint popover/settings > 300 ms → chấp nhận tới 600 ms (Validation 1); > 600 ms → báo owner.
- Tray macOS cần main thread → cập nhật qua `run_on_main_thread`.
- Phụ thuộc impl 03–10 cho test thật → B'' (+1d critical path, đã tính).

## Security Considerations
- `popover_msg` chỉ chấp nhận từ label `popover`; dialog native không thực thi chuỗi người dùng nhập (truyền làm argument, không ghép shell).

## Next Steps
- 14 e2e + perf gate.

## Contract gate notes (phase 02 review, 2026-10-15)
- `tauri.conf.json` CSP: `default-src 'self'; connect-src ipc: http://ipc.localhost; img-src 'self' data: blob:` (phase 13 removes the HTML meta CSPs and inline styles).
- `SessionSlot` is folded into `AppState` (one `manage`); build `AppState.services: Arc<Services>` at startup. `UsageEnv`, `SettingsSession` use `::new(..)` constructors (non_exhaustive).
- `pause::hold(&AppPaths, secs, reason)` takes paths first (injectable HOME). Autostart returns `io::Result<bool>`. Env-var reads go through `Env`.
- Real `popover.view`, `load_pak`, `tray.status` have no stub: internal to this phase.

- **Goal 3 (owner decision 2026-10-15):** spike paint is 632-692 ms. Try, in order, without spending RAM: show the dark (`#141413`) window as soon as the webview is built; fixed `data_directory` under app local data (never beside the exe); WebView2 browser args via `CLAWD_SPIKE_ARGS`-style experiments. Accept ~0.7 s if still above 600 ms; record numbers in `reports/perf-final.md`. Do not keep a hidden webview.
