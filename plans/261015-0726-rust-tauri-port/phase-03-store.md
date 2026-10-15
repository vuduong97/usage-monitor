---
phase: 3
title: "Store: paths, prefs, creds, autostart, pause"
status: pending
priority: P1
effort: "1.5d"
dependencies: [2]
---

# Phase 03: Store — paths, prefs, creds, autostart, firstrun, pause, log

## Overview
Port các module lớp đáy về lưu trữ: `paths.py`, `creds.py`, `autostart.py`, `firstrun.py`, `pause.py`, `RotatingLog`, `open_path`. Hub nhiều import nhất → tương thích byte-level với file/OS store do Python ghi là yêu cầu cứng.

## Context Links
- `clawd_desktop/paths.py` (260), `creds.py` (51), `autostart.py` (91), `firstrun.py` (83), `pause.py` (72), `tray.py` (`open_path`)
- [researcher-02 §Keyring, traps 1–4, 8–10](./research/researcher-02-crates-compat.md)
- Red team #1 (autostart), #9 (pause ≠ ota.busy), #11 (prefs), #14 (OS store trong test)
- Tests Python: `test_prefs` (5), `test_firstrun` (6, phần paths/firstrun), `test_tray` (autostart: `:90`, `:96`), `test_engine` (pause: `:109`)

## Requirements
- Functional:
  - `AppPaths`: config dir `~/.config/claude-usage-monitor/` mọi OS; root inject cho test; mọi path ở scout §3.
  - **Prefs** (red team #11): lưu dạng `serde_json::Map` (preserve_order) — không struct typed; accessor typed (`clock_screen() -> Option<i64>`, `setup_done() -> Vec<String>` bỏ phần tử sai kiểu, …) trả `None`/mặc định khi sai kiểu, như các reader khoan dung của Python (`firstrun.py:22-28`, `paths.py:185-194`). Chỉ JSON không hợp lệ / không phải object → `{}`; tồn tại nhưng không đọc được → lỗi (không coi rỗng khi sắp ghi). `load_prefs`, `update_prefs(FnOnce(&mut Map))`, `save_prefs`.
  - **Lock** cùng file `desktop.lock` như Python (`paths.py:84-129`): Windows `LockFileEx` byte 0 non-blocking (≙ `msvcrt.locking(LK_NBLCK,1)`), macOS `flock(LOCK_EX|LOCK_NB)`; poll 20 ms tới `PREFS_IO_WAIT` rồi timeout. Ghi atomic: temp riêng `desktop.<pid>.<thread>.tmp` + replace có retry; `json.dumps(indent=2)` ⇒ pretty 2 spaces.
  - **Pause** (red team #9): `pause::{hold(seconds, reason), release(), PauseReader::reason()}` trên `pause.json` (`until` wall clock, không tin `until` xa hơn `MAX_LEASE` 300 s, ghi atomic) — engine dùng. **Riêng** `set_ota_busy`/`ota_busy` (`ota.busy`, stale 900 s) — appupdate dùng. Không trộn.
  - `clock_screen`, `set_clock_screen` (nhận mọi int không phải bool), `usage_payload`, `set_usage_payload`.
  - `RotatingLog`: `clawd-desktop.log`, xoay `.log.1` ở 1 MB; layer `tracing`.
  - **Creds** (clock password): service `vn.clawd.desktop`, user = clock key, secret `"user\npassword"`, default `admin`.
    - Windows: `wincred` dùng `CredReadW/CredWriteW` trực tiếp, tái hiện `keyring/backends/Windows.py`: TargetName=`service`, fallback `user@service`, swap khi khác user, blob ghi UTF-16LE, đọc UTF-16 rồi fallback UTF-8, `CRED_PERSIST_ENTERPRISE`, Comment `Stored using python-keyring`.
    - macOS: generic password (service, account) qua `security-framework`.
  - **Autostart** (red team #1): Windows `HKCU\...\Run` value `Clawd` = `"<exe>"` **không tham số** (`autostart.py:65-71`); macOS `~/Library/LaunchAgents/vn.clawd.desktop.plist` = **byte-copy** dict `launch_agent()` (`autostart.py:33-45`: Label, ProgramArguments=[exe], RunAtLoad, KeepAlive{SuccessfulExit:false}, ThrottleInterval 30, ProcessType Interactive). `is_enabled`, `set_enabled`, `disable`. Viết tay (winreg qua `windows` crate, plist qua `plist` crate) — không `auto-launch`. Không cần `migrate_from_python`: đường dẫn exe không đổi (`{localappdata}\Programs\Clawd\Clawd.exe`, `/Applications/Clawd.app/Contents/MacOS/Clawd`).
  - **OsStoreNames** inject (red team #14): service keyring / Run value / LaunchAgent label lấy từ `OsStoreNames`; test dùng `vn.clawd.desktop.test-<uuid>`, `ClawdTest-<uuid>`; test **từ chối chạy** nếu tên = tên production.
  - firstrun: state trong desktop.json, cùng key. `open_path(path)`: mở file/thư mục bằng shell OS.
- Non-functional: không panic khi file hỏng.

## Architecture
`crates/clawd-store/src/{paths.rs, prefs.rs, lock.rs, log.rs, creds/{mod.rs, wincred.rs, mac.rs}, autostart.rs, firstrun.rs, pause.rs, open.rs}`; fakes trong `testkit/src/store.rs`.

## Related Code Files
- Own: `desktop-rs/crates/clawd-store/**`, `desktop-rs/crates/clawd-testkit/src/store.rs`, `tools/golden/export_store.py`, `desktop-rs/fixtures/store/**`
- Read-only: `clawd_desktop/{paths,creds,autostart,firstrun,pause}.py`

## Implementation Steps
1. Exporter (qua `_harness.py`): prefs round-trip (key lạ, float, unicode, **giá trị sai kiểu** như `usage.screen=300`, `setup_done` không phải list) → sau `update_prefs` một key, file byte-khớp trừ key đổi; firstrun states; pause lease (hết hạn, quá 300 s); ota_busy stale; `launch_agent([exe])` dict + chuỗi Run value.
2. Port paths/prefs/lock; test 2 process đồng thời không mất update; test `#[ignore]` chạy tay: Python `paths._prefs_lock` song song Rust.
3. Port creds Win + mac; script `tools/golden/creds_roundtrip.py` (Python ghi → Rust đọc → Rust ghi → Python đọc, gồm collision) **chỉ chạy trên CI VM / Windows user tạm**, với service test.
4. Port autostart (golden plist/Run), firstrun, pause, log, open_path.
5. Bảng mapping test Python của phase (ported/dropped/python-only).

## Todo
- [ ] Exporter + fixtures store (gồm sai kiểu)
- [ ] paths/prefs(Map)/lock + test đồng thời
- [ ] creds Win (CredReadW) + mac + roundtrip (tên test)
- [ ] autostart byte-copy, firstrun, pause lease, ota_busy, log, open_path
- [ ] Bảng mapping test

## Success Criteria
- [ ] `cargo test -p clawd-store` xanh Win + mac, 0 `todo!()`
- [ ] Prefs sai kiểu không làm mất key nào (golden)
- [ ] Plist + Run value byte-khớp `autostart.py`
- [ ] Credential roundtrip Python↔Rust pass trên Windows VM (log trong report)

## Risk Assessment
- Lock khác cơ chế → ghi đè giữa 2 process. Tín hiệu: test đồng thời fail. Phản ứng: test `#[ignore]` Python+Rust song song là cổng.
- macOS Keychain ACL: Developer ID cùng Team → item app tự tạo không hỏi lại (giả định; phase 14 kiểm thật).

## Security Considerations
- Không log secret; zeroize buffer credential; test không bao giờ dùng tên OS store production.

## Next Steps
- 12/13 dùng qua stub; 09 dùng `pause::hold`; 10 dùng `ota_busy`.
