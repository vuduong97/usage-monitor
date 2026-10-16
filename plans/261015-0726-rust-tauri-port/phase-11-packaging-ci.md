---
phase: 11
title: "Packaging + CI release"
status: in-review
priority: P1
effort: "2d"
dependencies: [1, 2]
---

# Phase 11: Packaging + CI release (Rust build)

## Overview
Đóng gói bản Rust giữ **nguyên hợp đồng phát hành** mà updater 0.1.4 đã ship: Windows = **Inno Setup** cùng AppId/thư mục/flags; macOS = `.app` theo chip, **ký Developer ID cùng Team ID + notarize** (bắt buộc với tag release), `.zip` (update) + `.dmg`. Thêm ký minisign cho `SHA256SUMS`. Workflow release Rust bảo mật secret, chạy song song workflow Python tới cutover (15).

## Context Links
- `packaging/windows/clawd.iss`, `packaging/build_mac.sh`, `packaging/build_windows.ps1`, `packaging/clawd.spec:98-115` (Info.plist), `.github/workflows/release.yml` (`:15-23` check version)
- [researcher-01 §5–7](./research/researcher-01-tauri-packaging.md); `reports/spike-decision.md`
- Red team #2 (version guard), #14 (CI secret), #15 (binary name, Info.plist, WebView2, size gate); decision Q1, Q2

## Requirements
- Functional:
  - `packaging-rs/windows/clawd.iss`: AppId `{6C9B4E2A-1F7D-4C3B-9A51-C1A7D0E5B9F3}`, `DefaultDirName={localappdata}\Programs\Clawd`, `PrivilegesRequired=lowest`, `OutputBaseFilename=Clawd-{#AppVersion}-windows-setup`, `CloseApplications=force`, LZMA2 solid; `[Files]` chỉ `Clawd.exe` + resources; `[InstallDelete]` xoá `{app}\_internal` + file PyInstaller thừa; `[UninstallRun]` giữ `taskkill /IM Clawd.exe` + `Clawd.exe cleanup`; `[Run]` postinstall y hệt bản cũ (không `skipifsilent` → app mở lại sau update). **Không** thêm `[Code]`/bootstrapper WebView2 (Python không có; parity).
  - `app/tauri.macos.conf.json` + `app/Info.plist` (merge): `LSUIElement`, `LSMinimumSystemVersion 11.0`, `NSHighResolutionCapable`, **`NSLocalNetworkUsageDescription`** (chuỗi y hệt `clawd.spec`), **`NSBonjourServices: ["_clawd._tcp"]`**; entitlements tái dùng `packaging/entitlements.plist` (copy → `packaging-rs/entitlements.plist`). `app/tauri.windows.conf.json`: icon, không bundle NSIS/MSI.
  - `packaging-rs/build_windows.ps1`: `cargo build --release -p clawd-app` → assert `target/release/Clawd.exe` tồn tại → ISCC.
  - `packaging-rs/build_mac.sh`: build per-arch, tạo `.app` (Tauri bundler), assert `Contents/MacOS/Clawd`; ký Developer ID (hardened runtime) + notarize + staple như `build_mac.sh` cũ; **tag release mà thiếu cert → fail** (Q1: phải cùng Team ID để 0.1.4 tự update); build thử không tag cho phép ad-hoc. `ditto` → `Clawd-X.Y.Z-mac-<arch>.zip` + `.dmg`.
  - Resources private: firmware bundle + renderer chỉ khi `CLAWD_PRIVATE_FIRMWARE=1`, đọc từ `clawd_desktop/firmware`, `clawd_desktop/renderer` (đầu ra của `tools/bundle_firmware.py`, `tools/build_preview.py`, giữ nguyên các tool này); feed URL/token qua env lúc `cargo build`.
  - **minisign** (Q2): sau khi tạo `SHA256SUMS`, `minisign -S -s <key> -m SHA256SUMS` → `SHA256SUMS.minisig`; `packaging-rs/README.md`: tạo khoá (`minisign -G`), cất bản sao offline, quy trình xoay khoá; public key → `MINISIGN_PUBKEY` (contracts, qua CR integrator).
  - **`.github/workflows/rust-release.yml`** (red team #14): trigger **chỉ** `push: tags: ['rust-v*']` + `workflow_dispatch`; `permissions: contents: read` mặc định, `contents: write` chỉ job `publish`; secrets (Developer ID, notarize, feed, minisign) chỉ trong job build/sign, thuộc **GitHub environment `release` có owner duyệt**; actions pin commit SHA; matrix `windows-latest`, `macos-14` (arm64), `macos-15-intel` (x86_64); bước **tag ↔ version**: `GITHUB_REF_NAME` (bỏ tiền tố `rust-`) == version trong `app/Cargo.toml` == `app/tauri.conf.json` (red team #2); size gate: installer/zip/dmg ≤ 8 MB (fail), exe size chỉ báo cáo; `SHA256SUMS` + `.minisig`; publish = draft **và** `prerelease: true`; bước gate `gh release view <tag> --json isDraft,isPrerelease` phải thấy draft/prerelease — **không bao giờ** để release `rust-v*` thành "latest" trên repo chính (0.1.4 đọc `/releases/latest`; `parse_version("rust-v…")` = None ⇒ cả fleet ngừng nhận update). Tag `rust-v*` dùng cho thử nghiệm nội bộ; đường thật đi qua `release.yml` sau cutover (15).
  - `tools/check_public.py` xanh trong job.
- Non-functional: cache cargo; build tái lập.

## Architecture
`packaging-rs/{windows/clawd.iss, build_windows.ps1, build_mac.sh, entitlements.plist, README.md}`, `app/tauri.{windows,macos}.conf.json`, `app/Info.plist`, `.github/workflows/rust-release.yml`.

## Related Code Files
- Own (create): các file trên
- Read-only: `packaging/**`, `.github/workflows/release.yml`, `tools/{bundle_firmware,build_preview}.py`

## Implementation Steps
1. Inno script + InstallDelete; build thử với exe skeleton của 02.
2. Script mac: build, assert binary, sign Developer ID/ad-hoc theo tag, notarize, zip+dmg, `codesign --verify --deep --strict`, `spctl --assess`.
3. Overlay tauri conf + Info.plist mạng cục bộ.
4. minisign: tài liệu tạo khoá; bước ký trong workflow.
5. `rust-release.yml` + environment `release` + tag↔version + size gate.
6. Thử local: cài installer Rust đè Python 0.1.4 (Windows VM) — thư mục, shortcut, 1 entry Add/Remove Programs, `_internal\` biến mất; `/SILENT /SUPPRESSMSGBOXES /NORESTART` không UI; gỡ chạy `cleanup`.

## Todo
- [ ] Inno script Rust + InstallDelete
- [ ] build scripts Win/mac + Developer ID bắt buộc khi tag
- [ ] tauri platform conf + Info.plist (local network, Bonjour)
- [ ] minisign ký SUMS + README khoá
- [ ] rust-release.yml (trigger, permissions, environment, SHA pin, tag↔version, size gate)
- [ ] Thử cài đè 0.1.4 + gỡ

## Success Criteria
- [ ] Tag `rust-v0.0.1-test` tạo đủ artifact đúng tên + `SHA256SUMS.minisig`; installer ≤ 8 MB
- [ ] `codesign -dv` bản mac cho cùng TeamIdentifier với 0.1.4
- [ ] Cài đè 0.1.4 Windows: 1 entry Add/Remove Programs, app khởi động, `Clawd.exe cleanup` chạy khi gỡ
- [ ] Workflow không chạy trên push nhánh; secret chỉ trong environment có duyệt

## Risk Assessment
- macOS 15 chặn mạng cục bộ nếu thiếu key Info.plist → đã thêm; phase 14 kiểm mDNS trên Sequoia.
- Máy không có WebView2: tray vẫn chạy, nhập IP qua menu native (12) — như Python.

## Security Considerations
- Secret chỉ trong environment `release`; không echo; artifact build public không chứa firmware; actions pin SHA.

## Next Steps
- 14 dùng artifact (tag test); 15 gộp vào `release.yml`.
