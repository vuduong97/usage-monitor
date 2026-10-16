---
phase: 15
title: "Release cutover"
status: in-progress
priority: P1
effort: "0.5d"
dependencies: [14]
---

# Phase 15: Release cutover

## Overview
Chuyển kênh phát hành sang bản Rust: version 0.2.0, `release.yml` build Rust với tag `v*` (updater 0.1.4 thấy), giữ đường phát hành Python trên nhánh `python-0.1.x`, cập nhật README.

## Context Links
- `.github/workflows/{release.yml, ci.yml, rust-release.yml, rust-ci.yml}`, `README.md`, `pyproject.toml`, `clawd_desktop/__init__.py`
- `reports/migration-report.md`, `reports/perf-final.md`; red team #2, #4, #15; decisions Q1, Q2

## Requirements
- Functional:
  - Tạo nhánh `python-0.1.x` từ commit cuối trước cutover; `release.yml` Python giữ nguyên ở nhánh đó (check version Python vẫn đúng ở đó) → vẫn phát hành được 0.1.x / bản rollback.
  - Trên `main`: version 0.2.0 ở `app/Cargo.toml` + `app/tauri.conf.json` (nguồn duy nhất cho `--version`, Info.plist, Inno `AppVersion`). `release.yml`: thay job mac/windows Python bằng job Rust (từ `rust-release.yml`, giữ environment `release`, permissions, SHA pin, minisign, size gate); **viết lại bước "The tag is the app's version"** (`release.yml:15-23`) để so `GITHUB_REF_NAME` với `app/Cargo.toml` + `tauri.conf.json`; giữ `public-check` (`check_public.py`), `publish` draft. Xoá `rust-release.yml`.
  - `ci.yml` trên `main`: Rust CI; pytest Python chuyển sang `python-0.1.x` (Python trên `main` chỉ còn tham chiếu, không build) — `test_versions_agree` không còn chạy trên `main`.
  - README: cài đặt (tên file không đổi); **mac: ký Developer ID + notarize** (bỏ đoạn "chưa đăng ký với Apple"/Vẫn mở, sửa đúng thực tế); cập nhật: tải tự động, cài khi bấm "● Khởi động lại để cập nhật"; có thể bị hỏi lại quyền Keychain/mạng cục bộ một lần; mục build từ mã nguồn → Rust.
  - Release notes 0.2.0: số đo trước/sau (perf-final).
  - **End-of-support 0.1.x:** sửa lỗi bảo mật cho 0.1.x 3 tháng sau 0.2.0 (hoặc khi số tải 0.1.x/Release insights cho thấy đã chuyển phần lớn) — ghi trong README nhánh `python-0.1.x`.
- Non-functional: release nháp; owner kiểm 3 artifact + `SHA256SUMS` + `.minisig` rồi publish.

## Related Code Files
- Modify: `.github/workflows/release.yml`, `.github/workflows/ci.yml`, `README.md`, `app/tauri.conf.json` (chỉ `version`), `app/Cargo.toml` (version, qua integrator)
- Delete: `.github/workflows/rust-release.yml`
- Create: nhánh `python-0.1.x`

## Implementation Steps
1. Nhánh `python-0.1.x`.
2. Bump version; gộp workflow; viết lại check tag↔version.
3. README + release notes + end-of-support.
4. Tag `v0.2.0` → draft → owner kiểm → publish.
5. Theo dõi 2 tuần: log lỗi updater; tỉ lệ đã áp dụng (download count `.exe`/`.zip` 0.2.0 vs 0.1.4).

## Todo
- [x] Nhánh python-0.1.x (release.yml Python còn chạy; CI chạy trên nhánh, xanh)
- [x] Version + workflow gộp + check tag↔version Rust (`release.yml` tag `v*`, draft; `rust-release.yml` xoá; env `release` cần thêm `v*`)
- [x] README (mac Developer ID, cập nhật khi bấm) + release notes + end-of-support
- [ ] Draft release `v0.2.0`
- [ ] Thử 0.1.4→0.2.0 và Rust→Rust + tamper trên mac và Windows (`tools/migration/local-release.md`)
- [ ] Sao lưu khoá minisign, owner publish

## Success Criteria
- [ ] Release `v0.2.0` có `Clawd-0.2.0-windows-setup.exe`, `Clawd-0.2.0-mac-{arm64,x86_64}.{zip,dmg}`, `SHA256SUMS`, `SHA256SUMS.minisig`
- [ ] Máy 0.1.4 (Win + mac) **stage** 0.2.0 ≤ 6 h; bấm "● Khởi động lại để cập nhật" → chạy 0.2.0
- [ ] Không vòng update (0.2.0 không thấy chính nó là bản mới)

## Risk Assessment
- Lỗi nghiêm trọng sau phát hành → 0.2.1 Rust sửa nhanh, hoặc bản Python version cao hơn từ `python-0.1.x` (đã diễn tập ở 14, có `.minisig`).
- Người dùng không bấm cập nhật → fleet ở lại 0.1.4 lâu; đường phát hành Python vẫn mở tới end-of-support.

## Security Considerations
- `check_public.py` + secret scan trước tag; publish chỉ qua environment `release`.

## Next Steps
- Archive plan; journal; backlog Authenticode.
