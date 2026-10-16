---
phase: 13
title: "Settings bridge + UI shim"
status: completed
priority: P1
effort: "3d"
dependencies: [1, 2]
---

# Phase 13: Settings bridge — 32 commands sau dispatcher `py_api` + pywebview shim + UI

## Overview
Port `settings_api.py` (932) + phần cửa sổ của `settings_app.py` (173) thành các hàm command sau dispatcher `py_api` (đóng băng ở 02), giữ UI `index.html`/`app.js`/`setup.js`/`preview.js` gần như nguyên vẹn bằng shim `window.pywebview.api` → `invoke('py_api')`. Thay giao thức stdout tray bằng `ShellCmd`. Tối ưu first paint (≤ 300 ms). Phần lớn chạy song song trên API stub; golden thật sau khi 03–10 merge (B'').

## Context Links
- `clawd_desktop/settings_api.py:133+` (`SettingsApi`; `:163-178` `_connect(base,key)`; `:551-573` file dialog backup; `:612-644` OTA job + `ota.busy`; notify `:695,715,720,744,845,890`), `settings_app.py:7-14,46-75,95-152`, `ui/app.js:685-686`, `ui/setup.js:310`
- scout §2; red team #5, #7, #8, #9, #14
- Tests Python: `test_settings_api` (28; `:303` `..._tells_the_tray` → port với `ShellCmd`), `test_setup_api` (6), `test_preview` (phần settings_api)

## Requirements
- Functional:
  - **32 hàm** tên y hệt (grep `ui/*.js|*.html` 2026-10-15): `agents, app_update, app_update_apply, app_update_check, app_update_page, check_update, clawd_pak, disconnect_all, export_backup, factory_reset, finish_setup, fw, import_backup, job, login, open_log, open_web, reboot, save, scan, set_agent, set_agents, set_autostart, set_clock, set_usage_source, setup_clocks, setup_state, setup_use_clock, state, update, usage_preview, usage_sources` trong `commands/<group>.rs`; dispatcher `py_api` + allowlist ở `commands/mod.rs` (02, đóng băng) gọi vào. JSON vào/ra giống Python; lỗi → promise reject với message Python.
  - Không expose `preview`, `busy`, `set_hooks`, `close_mdns`.
  - **Session per window:** dùng `SettingsSession { base, key, tab }` do 12 tạo mỗi lần mở (≙ `_connect(base,key)`); `ClockApi` + creds theo session.
  - **ShellCmd** (red team #5): chỗ Python `notify(...)` → gửi `AppState.shell_tx`: `set_autostart`/agents → `Refresh`; `app_update_apply` → `AppUpdateApply` (không tự exit); `set_clock` → `SetClock`; `setup_use_clock` → `UseClock`; `set_usage_source` → `UsageSource`; wizard/agent → `Setup`.
  - **Job dài** (OTA, feed, update): tokio task; `job` trả tiến độ như Python; **đặt `ota_busy` trong suốt job OTA** (≙ `settings_api.py:612-644`), clear bằng guard `Drop`; `jobs.busy` (AtomicBool trong `JobState` của 02) cho 12 chặn đóng cửa sổ. Pause lease do `clawd_firmware::ota` lo (09).
  - Backup `export_backup`/`import_backup`: file dialog qua `tauri-plugin-dialog` (02 đã pin).
  - **Shim** `app/ui/shim.js` (load đầu tiên): `window.pywebview = { api: new Proxy({}, { get: (_, n) => (...a) => window.__TAURI__.core.invoke('py_api', { name: n, args: a }) }) }`; dispatch `pywebviewready` sau khi gắn; listen `clawd://show-tab` → `window.clawdShowTab(tab)`, `clawd://busy-close` → `window.clawdBusyClose()`; emit `clawd://ready` sau `DOMContentLoaded` + 1 rAF.
  - **`capabilities/default.json`**: window `settings`: `core:event:default` + `allow-py-api`; window `popover`: `core:event:default` + `allow-popover-msg`. Có hiệu lực nhờ AppManifest ở `build.rs` (02). Test: popover gọi `py_api` → bị từ chối.
  - First paint: CSS critical inline, không fetch mạng trước paint; `state`, `usage_preview` tải sau paint.
  - `open_log`, `open_web` → `clawd_store::open_path`; `open_web` chỉ URL đồng hồ/GitHub đã biết.
  - Preview: data URL APNG từ 09; CSP `img-src 'self' data: blob:`.
- Non-functional: diff UI tối thiểu (shim + ready + 2 listener).

## Architecture
`app/src/commands/{state.rs, agents.rs, clock.rs, setup.rs, usage.rs, update.rs, firmware.rs, backup.rs, misc.rs, jobs.rs}` (không gồm `mod.rs`), `app/ui/{shim.js, index.html, app.js, setup.js, preview.js}`, `app/capabilities/default.json`, `app/tests/settings_*.rs`. Dependency mới cho app → CR integrator.

## Related Code Files
- Own: các file trên (mọi `app/ui/**` trừ `popover.html`); `tools/golden/export_settings.py`, `desktop-rs/fixtures/settings/**`
- Read-only: `clawd_desktop/{settings_api,settings_app}.py`, `clawd_desktop/ui/**`, `app/src/{state.rs, commands/mod.rs}`, file của 12

## Implementation Steps
1. Kiểm allowlist 02 khớp grep UI.
2. Exporter `export_settings.py` (`_harness.py` cách ly HOME — không đọc `clawd.pak`/renderer thật; dùng pak/renderer giả nhỏ; output không chứa art/base64 PNG) với clock/feed giả → response JSON từng hàm.
3. Shim + ready + listeners + capabilities; UI chạy với stub (hàm trả lỗi "chưa có") để kiểm render.
4. Port hàm theo nhóm trên API stub; test cần impl → `#[ignore = "needs 03-10"]`.
5. First paint; đo bằng `tools/perf`.
6. **B'':** chạy golden thật; bảng mapping test.

## Todo
- [ ] Allowlist khớp UI
- [ ] Exporter + fixtures settings (cách ly, không art)
- [ ] Shim + ready + listeners + capabilities (+ test deny)
- [ ] 32 hàm + ShellCmd + jobs/ota_busy + dialog backup
- [ ] First paint (300/600 ms)
- [ ] B'': golden thật + bảng mapping

## Success Criteria
- [ ] Mọi tên JS gọi có hàm; golden response khớp
- [ ] `test_settings_api.py:303` tương đương: chọn đồng hồ trong wizard → engine đổi target (qua ShellCmd)
- [ ] UI đủ tab + wizard 4 bước trên Win11 + mac
- [ ] First paint cold ≤ 300 ms (chấp nhận ≤ 600 ms nếu spike chốt vậy — Validation 1)

## Risk Assessment
- Kiểu trả về lệch (int/float, null/thiếu key) → JS lỗi ngầm; golden là cổng.
- 300 ms không đạt → dùng ngưỡng ≤ 600 ms; > 600 ms → báo owner.

## Security Considerations
- Allowlist 32 tên + kiểm label `settings`; AppManifest + capabilities theo window; không command chạy shell tuỳ ý.

## Next Steps
- 14 e2e UI.

## Contract gate notes (phase 02 review, 2026-10-15)
- **CSP:** index.html has 19 inline `style=` attributes (lines 858, 964, 973, 1025, 1028, 1035, 1039, 1052, 1085, 1100, 1110, 1114, 1165, 1176, 1182, 1189, 1209, 1239, 1282) which a strict CSP blocks. Move them into ~8 utility classes in the existing `<style>` block; add a test that `app/ui/*.html` has no `\bstyle=` attribute. CSSOM sets (`el.style.width = …`) are fine. Do NOT relax CSP; `style-src-attr 'unsafe-inline'` is last-resort only.
- Delete both `<meta http-equiv="Content-Security-Policy">` tags (index.html:7 has `connect-src 'none'`, `unsafe-eval`, which would block IPC fetch).
- Per-window state lives in `SettingsSession.data: Mutex<SessionData>` (stub in `commands/session.rs`, owned by this phase); shared services via `AppState.services`.

- **Goal 3 (owner decision 2026-10-15):** spike paint is 632-692 ms. Try, in order, without spending RAM: show the dark (`#141413`) window as soon as the webview is built; fixed `data_directory` under app local data (never beside the exe); WebView2 browser args via `CLAWD_SPIKE_ARGS`-style experiments. Accept ~0.7 s if still above 600 ms; record numbers in `reports/perf-final.md`. Do not keep a hidden webview.
