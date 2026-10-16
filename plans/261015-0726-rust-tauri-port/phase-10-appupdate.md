---
phase: 10
title: "App self-update + minisign"
status: in-review
priority: P1
effort: "2d"
dependencies: [2]
---

# Phase 10: App self-update + minisign (crate `clawd-appupdate`)

## Overview
Port `appupdate.py` (647): tìm bản mới trên GitHub Releases, tải + verify `SHA256SUMS`, stage, áp dụng khi user bấm (Windows: Inno installer im lặng, detached; macOS: `swap.sh`, verify chữ ký + Team ID). Thêm (Q2): verify chữ ký **minisign** của `SHA256SUMS` bằng public key nhúng.

## Context Links
- `clawd_desktop/appupdate.py` (`:14-17` chỉ user áp dụng, `:57-60` hosts, `:84-91` version, `:156` no-redirect, `:162-168` `trusted`, `:190-201` hop loop, `:212` bỏ draft, `:261-263` `auto_update`, `:296-325` `check_bundle`, `:365-385` `can_apply`, `:450`/`:523`/`:614` ready.json sha, `:600-640` apply)
- Red team #4, #15; decision Q1 (mac Developer ID), Q2 (minisign)
- Tests Python: `test_appupdate` (32; `:98-110` kiểm file đóng gói Python → python-only)

## Requirements
- Functional:
  - Source: repo `vuduong97/usage-monitor` (env `CLAWD_UPDATE_REPO` / config `update_repo`), API override `CLAWD_UPDATE_API`; `trusted(url)` **đúng như Python**: `api.github.com`, `github.com`, mọi `*.githubusercontent.com`, và `http` loopback khi dùng API override; client `redirect::Policy::none()` + vòng hop thủ công gọi `trusted()` mỗi hop; `_get_small` giới hạn size; bỏ release draft/prerelease; `parse_version`, `is_newer`, `this_platform`, `asset_name`, `parse_sums`, `sha256_file`.
  - **minisign** (mới, Q2): tải `SHA256SUMS.minisig`; `minisign-verify` với public key `MINISIGN_PUBKEY` (contracts; mặc định production, override build-time `CLAWD_MINISIGN_PUBKEY` chỉ cho build test trên fork); thiếu/sai chữ ký → `UpdateError` (thông điệp tiếng Việt mới, ghi vào fixture). Áp dụng cho mọi bản Rust→Rust.
  - Hash: giữ sha kỳ vọng **trong bộ nhớ** từ lúc tải; trước khi chạy installer so với file (vẫn ghi `ready.json` cho tương thích stage dir, nhưng không tin nó khi đã có giá trị RAM; lần khởi động lại → tin `ready.json` chỉ khi `.minisig` verify lại pass).
  - `Updater`: state machine (`state`, `_set`, `_failed`, `staged`, `clean`), stage `app-update/` (`failed`, `apply.log`), lockfile 2 process; `auto_enabled` (`auto_update` off → không check). **Chỉ stage tự động; áp dụng chỉ khi user bấm** (`:14-17`).
  - Windows apply: verify → spawn installer **detached** (`DETACHED_PROCESS|CREATE_NEW_PROCESS_GROUP`) `/SILENT /SUPPRESSMSGBOXES /NORESTART` → caller gửi `ShellCmd` quit (12).
  - macOS apply: unzip → `check_bundle` (bundle id, version, `codesign --verify --deep --strict`, Team ID khớp app đang chạy; không team → `NO_TEAM`) → `swap.sh` (`MAC_SWAP` y hệt) → thoát.
  - `can_apply`: chỉ app đóng gói (không chạy từ `target/`), AppTranslocation, quyền ghi; không áp dụng khi `ota_busy()`.
  - Dọn stage dir cũ do Python để lại.
- Non-functional: chuỗi lỗi tiếng Việt giữ nguyên Python.

## Architecture
`crates/clawd-appupdate/src/{source.rs, http.rs (trusted + hop loop), version.rs, sums.rs, minisig.rs, updater.rs, apply_windows.rs, apply_macos.rs, codesign.rs}`; `CommandRunner` trait; fakes `testkit/src/appupdate.rs` (fake GitHub API + redirect).

## Related Code Files
- Own: `desktop-rs/crates/clawd-appupdate/**`, `desktop-rs/crates/clawd-testkit/src/appupdate.rs`, `tools/golden/export_appupdate.py`, `desktop-rs/fixtures/appupdate/**`
- Read-only: `clawd_desktop/appupdate.py`

## Implementation Steps
1. Exporter: release JSON → `latest()`; version compare; SUMS parse; `codesign -dv` output → team id; `can_apply` matrix; `trusted()` matrix (gồm loopback http, gist/raw githubusercontent).
2. Port version/sums/source/http (hop loop) → golden; test redirect sang host lạ → lỗi.
3. minisig verify (khoá test sinh trong test; khoá thật chỉ public key).
4. Updater + stage/lock + hash trong RAM.
5. Apply Win/mac với `CommandRunner` giả (kiểm cờ detached + đối số).
6. Bảng mapping test (32).

## Todo
- [ ] Exporter + fixtures appupdate
- [ ] version/sums/source/trusted + hop loop
- [ ] minisign verify
- [ ] Updater + stage/lock + hash RAM
- [ ] apply Win/mac
- [ ] Bảng mapping test

## Success Criteria
- [ ] `cargo test -p clawd-appupdate` xanh; golden khớp
- [ ] SUMS không chữ ký / sai chữ ký / redirect host lạ → từ chối (test)
- [ ] Update Rust→Rust thật (phase 14 gate)

## Risk Assessment
- Mất secret key minisign → không phát hành được bản updater chấp nhận. Phản ứng: owner giữ bản sao offline; ghi quy trình xoay khoá (bản phát hành cuối với key cũ nhúng key mới) trong `packaging-rs/README.md` (11).
- Updater lỗi ở 0.2.0 → kẹt fleet; giảm bằng gate Rust→Rust ở 14.

## Security Considerations
- Nguồn gốc: minisign (Windows + mac) + Team ID (mac). Authenticode → backlog.

## Next Steps
- 12 (check lúc mở + 6 h, menu "● Khởi động lại để cập nhật"), 13 (`app_update*`), 11 (ký SUMS).
