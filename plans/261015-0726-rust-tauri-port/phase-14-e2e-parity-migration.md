---
phase: 14
title: "E2E, parity, perf gate, migration"
status: pending
priority: P1
effort: "3d"
dependencies: [3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13]
---

# Phase 14: E2E, parity, perf gate, migration + update Rust→Rust

## Overview
Phase duy nhất chạm mọi thứ nhưng chỉ **thêm test + phát hiện lỗi tích hợp** (fix nằm ở nhánh phase chủ file, qua integrator). Chạy end-to-end trên đồng hồ thật, chốt số hiệu năng, kiểm nâng cấp 0.1.4 → Rust (Win + mac), **update Rust→Rust thật**, và bản rollback cài được qua updater Rust.

## Context Links
- Phase 03–13; `README.md` (checklist parity); `reports/perf-baseline.md`, `reports/spike-decision.md`; `tools/perf/**`
- Red team #2, #4, #12, #15; decisions Q1, Q2

## Requirements
- Functional:
  - `cargo build --workspace --release` sạch; `rg 'todo!\(' desktop-rs` = 0; không còn `#[ignore = "needs 03-10"]`.
  - **E2E tự động** (`desktop-rs/tests-e2e/`): app headless (`run`) với config dir tạm + `OsStoreNames` test + fake clock + fake Anthropic usage → hook POST (curl thật) → fake clock nhận `/api/state` + `/api/usage` khớp golden; `cleanup` gỡ sạch hook; **fault injection**: panic trong 1 loop → app sống, loop tự chạy lại.
  - **Parity thủ công** theo README (Windows VM sạch, mac Apple silicon macOS 15, đồng hồ Clawd thật; mac x64 = ngoại lệ đã duyệt: chỉ CI build + verify chữ ký): wizard 4 bước, mọi tab, lưu vào đồng hồ, Digest password, OTA thật 1 lần, preview đủ kiểu, cài/gỡ 7 agent, autostart (Run/plist byte-khớp), tray chấm + mục ●, "Nhập địa chỉ đồng hồ…" khi không có WebView2 (máy Win10 tạm gỡ WebView2), mDNS trên macOS 15 (quyền mạng cục bộ).
  - **Migration 0.1.4 → Rust** (`tools/migration/`), repo fork + config `update_repo`: build Rust **version thật đóng dấu từ tag test** (tag `vX.Y.Z` không draft — `appupdate.py:212` bỏ draft) →
    - Windows VM sạch: cài 0.1.4, thiết lập đủ (đồng hồ, password, 3 agent, autostart, usage source) → 0.1.4 stage bản Rust ≤ 6 h (hoặc bấm "Kiểm tra bản mới") → **user bấm "● Khởi động lại để cập nhật"** → kiểm prefs nguyên vẹn, password không bị hỏi lại, hook chạy, Run value không đổi, 1 entry Add/Remove Programs, `_internal\` đã xoá, không vòng update lại (tag = version).
    - mac (Developer ID cùng Team): như trên qua `swap.sh`; ghi lại prompt Keychain/mạng cục bộ nếu có.
  - **Update Rust→Rust** (red team #4): phát hành `vN` và `vN+1` trên fork (có `.minisig`), cài `vN`, để updater Rust stage + user bấm áp dụng `vN+1` trên Win11 + mac. Thêm case: `.minisig` sai → từ chối.
  - **Rollback rehearsal:** build Python (nhánh `python-0.1.x`) với version > `vN+1`, phát hành trên fork kèm `SHA256SUMS.minisig` → updater Rust cài được, app Python chạy, autostart vẫn khởi động (Run value không tham số).
  - **Perf gate** (`reports/perf-final.md`): 4 mục tiêu, cùng điều kiện baseline, so với Python.
  - Bảng mapping 413 test tổng hợp từ report các phase → `reports/test-mapping.md` (ported / dropped+lý do / python-only), không dòng trống.
  - **Fork release** (`tools/migration/fork-release.md`): tạo release `v*` **không draft, không prerelease** trên fork bằng tay từ artifact build (`rust-release.yml` chạy trên fork với version đóng dấu từ tag test); fork có environment `release` riêng chứa cert Developer ID cùng Team (mac) và **khoá minisign test**; binary test build với `CLAWD_MINISIGN_PUBKEY` = khoá test (khác production chỉ ở hằng số này).
- Non-functional: mọi lỗi → fix có test hồi quy.

## Architecture
`desktop-rs/tests-e2e/{main.rs, fake_anthropic.rs, scenarios/*.rs}`, `tools/migration/{windows.md, run_windows.ps1, mac.md, fork-release.md}`.

## Related Code Files
- Own: `desktop-rs/tests-e2e/**`, `tools/migration/**`, `plans/261015-0726-rust-tauri-port/reports/{perf-final,parity-checklist,migration-report,test-mapping}.md`
- Modify (qua integrator, nhánh phase chủ): fix lỗi tích hợp; job e2e trong `rust-ci.yml`

## Implementation Steps
1. Build release sạch, bỏ ignore, sửa lỗi tích hợp.
2. E2E headless + fault injection + job CI.
3. Parity checklist 3 nền tảng.
4. Migration Win + mac trên fork.
5. Update Rust→Rust + rollback rehearsal.
6. Perf gate + test-mapping.

## Todo
- [ ] Build release sạch, 0 todo/ignore
- [ ] E2E + fault injection + CI
- [ ] Parity checklist
- [ ] Migration Win + mac
- [ ] Rust→Rust + rollback rehearsal
- [ ] Perf gate + test-mapping

## Success Criteria
- [ ] E2E xanh CI Win + mac
- [ ] `parity-checklist.md` 100% pass (ngoại lệ có owner duyệt)
- [ ] `migration-report.md`: Win + mac pass; Rust→Rust pass; rollback pass
- [ ] `perf-final.md`: 4 mục tiêu đạt (hoặc ngưỡng nới ở 01)
- [ ] `test-mapping.md`: 413/413 dòng có trạng thái

## Risk Assessment
- Lỗi tích hợp dồn cuối → giảm bằng merge cuốn chiếu + B''. Tín hiệu: > 10 lỗi ngày đầu → integrator dừng, rà contracts.
- Không có máy mac x64 (Validation 1) → chỉ CI build + `codesign --verify` + `spctl`; rủi ro chạy thực tế x64 chấp nhận.

## Security Considerations
- Fake server chỉ loopback; không token thật trong CI; fork test dùng khoá minisign test riêng, không khoá production.

## Next Steps
- 15 cutover.
