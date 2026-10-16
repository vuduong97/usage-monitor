---
title: "Port Clawd desktop từ Python sang Rust + Tauri v2 (song song nhiều agent)"
description: "Viết lại toàn bộ app tray Clawd bằng Rust/Tauri v2, giữ UI HTML/JS và mọi hợp đồng on-disk, chia 15 phase sở hữu file độc quyền để nhiều agent chạy song song."
status: in-progress
priority: P1
effort: "~27 agent-days tổng; wall-clock ~10.5 agent-days (cap 6 agent)"
branch: main
tags: [refactor, infra, frontend, backend, critical]
blockedBy: []
blocks: []
created: 2026-10-15
---

# Port Clawd desktop → Rust + Tauri v2

## Overview

Big-bang port `clawd_desktop/` (Python ~9.5k LOC, pywebview, PyInstaller 57 MB) sang Rust + Tauri v2, tái dùng UI `clawd_desktop/ui/*`. Bản Python đóng băng (chỉ sửa bug, nhánh `python-0.1.x` sau cutover). Plan tối ưu **wall-clock**: interface-first (stub API + contracts đóng băng ở phase 02) → 11 phase chạy song song, mỗi phase sở hữu file độc quyền, merge cuốn chiếu.

Nguồn: advice `ak:advise` 2026-10-15 (hội thoại; Tauri v2, big-bang, mục tiêu tham vọng), [scout](./reports/scout-report.md), [Tauri/packaging](./research/researcher-01-tauri-packaging.md), [crates/compat](./research/researcher-02-crates-compat.md).

## Goals (đo được)

| # | Mục tiêu | Ngưỡng |
|---|---|---|
| 1 | Installer Windows / mỗi file mac | ≤ 8 MB |
| 2 | RAM idle process chính, 0 cửa sổ, T+10 phút | ≤ 20 MB private |
| 3 | Mở Cài đặt: click → first paint | ≤ 300 ms (median 10 lần); nếu spike cold > 300 ms → chấp nhận ≤ 600 ms, không đánh đổi RAM (Validation 1). **Spike (phase 01): 632–692 ms trên Win11 i5-10400 → vượt 600 ms; owner (2026-10-15) cho phép thử giảm không tốn RAM (hiện cửa sổ tối ngay khi build, `data_directory` cố định, tham số WebView2) và chấp nhận ~0.7 s nếu không đạt.** |
| 4 | Tray hiện sau launch | ≤ 1 s |
| 5 | Parity | 100% golden fixtures khớp Python; bảng mapping test (port / bỏ có lý do / chỉ Python) đủ 413 test; mọi mục README chạy được trên Win + mac arm64; mac x64: **ngoại lệ đã duyệt** (không có máy) — chỉ CI build + `codesign --verify` + `spctl` |
| 6 | Migration | 0.1.4 (Win + mac Developer ID) **tải + stage** bản Rust ≤ 6 h; **cài khi user bấm** "● Khởi động lại để cập nhật"; giữ prefs/hook/keyring/autostart |

## Non-goals

- Tính năng mới (ngoại lệ đã duyệt: chữ ký minisign cho SHA256SUMS, Host-check + giới hạn kết nối cho event server); BLE path trong `usage.py` (owner xác nhận bỏ); sửa firmware/renderer C++; Linux; `tauri-plugin-updater`; cài WebView2 trong installer (Python cũng không làm); Authenticode (backlog).

## Frozen contracts (không được đổi — nguồn: scout §3)

Config dir `~/.config/claude-usage-monitor/` + file trong đó; `desktop.json` + lock `desktop.lock`; `pause.json` (lease ≤ 300 s, engine) **khác** `ota.busy` (900 s, chặn app-update); keyring service `vn.clawd.desktop` (Win: TargetName=`service` / fallback `user@service`, blob UTF-16LE); event server `127.0.0.1:47913` routes `/state`, `/poke`, `/hook/<agent>?e=`; hook command strings + marks; autostart: Run value `Clawd` = `"<exe>"` (không tham số), LaunchAgent `vn.clawd.desktop` = byte-copy `autostart.launch_agent()` (KeepAlive SuccessfulExit=false, ThrottleInterval 30, ProcessType Interactive); binary `Clawd.exe` / `Clawd.app/Contents/MacOS/Clawd`; bundle id `vn.clawd.desktop`; mac ký **Developer ID cùng Team ID** với 0.1.4; asset names `Clawd-X.Y.Z-windows-setup.exe`, `Clawd-X.Y.Z-mac-{arm64,x86_64}.{zip,dmg}`, `SHA256SUMS` (+ mới `SHA256SUMS.minisig`); Inno AppId `{6C9B4E2A-1F7D-4C3B-9A51-C1A7D0E5B9F3}`, dir `{localappdata}\Programs\Clawd`, flags `/SILENT /SUPPRESSMSGBOXES /NORESTART`, uninstall gọi `Clawd.exe cleanup`; feed manifest v1 + `renderers[]`, AES-256-GCM; tag release = version trong build.

## Execution strategy (DAG)

```text
Wave A (song song, ngày 1-1.5): 01 spike-baseline        02 workspace-contracts ──(contract review gate)──┐
Wave B (song song sau 02):     03 store  04 events-server  05 agents-hooks  06 usage-tokens  07 usage-poller
                               08 clock  09 firmware-feed  10 appupdate
Wave B' (sau 02 + 01):         11 packaging-ci   12 app-shell   13 settings-bridge
                               (12/13 làm mọi phần không cần impl; bước "golden thật" chạy khi 03–10 đã merge = B'')
Wave C (tuần tự):              14 e2e-parity-migration → 15 release-cutover
```

- **Cap 6 agent cùng lúc** (Validation 1). Lịch slot sau 01+02: `04, 07, 09, 12, 13, 05` → khi trống: `03, 06, 08` → `10, 11` → đuôi B'' 12/13 khi 03–10 đã merge.
- Wall-clock ≈ 1.5 (A) + 4.5 (B/B') + 1 (B'') + 3 (14) + 0.5 (15) ≈ **10.5 agent-days** với cap 6 (lý thuyết 11 slot ≈ 8.5); tuần tự ~27.
- Review mỗi merge: 1 `reviewer` agent soi diff + golden + mapping test → owner đọc tóm tắt reviewer và duyệt.

## Phases

| # | Phase | Wave | Depends on | Owns (độc quyền) | Effort |
|---|---|---|---|---|---|
| 1 | [Spike & baseline đo hiệu năng](./phase-01-spike-baseline.md) | A | – | `tools/perf/**`, `desktop-rs/spike/**` | 1d |
| 2 | [Workspace, contracts, API stubs](./phase-02-workspace-contracts.md) | A | – | `desktop-rs/Cargo.toml`, `crates/clawd-contracts/**`, `crates/clawd-testkit/src/{lib,core}.rs`, stub ban đầu mọi crate, `app/build.rs`, `app/src/state.rs`, `app/src/commands/mod.rs`, `tools/golden/_harness.py`, `tools/check_public.py`, `.github/workflows/rust-ci.yml` | 1.5d |
| 3 | [Store: paths, prefs, creds, autostart, pause](./phase-03-store.md) | B | 2 | `crates/clawd-store/**`, `testkit/src/store.rs`, golden `store` | 1.5d |
| 4 | [Events brain + loopback server](./phase-04-events-server.md) | B | 2 | `crates/clawd-events/**`, `testkit/src/events.rs`, golden `events` | 2.5d |
| 5 | [Agents + hooks](./phase-05-agents-hooks.md) | B | 2 | `crates/clawd-agents/**`, `testkit/src/agents.rs`, golden `agents` | 2d |
| 6 | [Config + token sources](./phase-06-usage-tokens.md) | B | 2 | `crates/clawd-config/**`, `testkit/src/config.rs`, golden `config` | 1.5d |
| 7 | [Usage poller + publisher + codex](./phase-07-usage-poller.md) | B | 2 | `crates/clawd-usage/**`, `testkit/src/usage.rs`, golden `usage` | 2.5d |
| 8 | [Clock API, discovery, bootwatch](./phase-08-clock.md) | B | 2 | `crates/clawd-clock/**`, `testkit/src/clock.rs`, golden `clock` | 1.5d |
| 9 | [Firmware, feed, OTA, preview](./phase-09-firmware-feed.md) | B | 2 | `crates/clawd-firmware/**`, `testkit/src/fw.rs`, golden `fw` | 2.5d |
| 10 | [App self-update + minisign](./phase-10-appupdate.md) | B | 2 | `crates/clawd-appupdate/**`, `testkit/src/appupdate.rs`, golden `appupdate` | 2d |
| 11 | [Packaging + CI release](./phase-11-packaging-ci.md) | B' | 1, 2 | `packaging-rs/**`, `app/tauri.windows.conf.json`, `app/tauri.macos.conf.json`, `app/Info.plist`, `.github/workflows/rust-release.yml` | 2d |
| 12 | [App shell: engine, tray, popover, CLI](./phase-12-app-shell.md) | B' | 1, 2 (golden: 3–10) | `app/src/{main,lib,lifecycle,engine,tray,dialogs,popover,windows,cli}.rs`, `app/icons/**`, `app/ui/popover.html`, `app/tauri.conf.json`, `app/tests/shell_*.rs` | 3d |
| 13 | [Settings bridge + UI shim](./phase-13-settings-bridge.md) | B' | 1, 2 (golden: 3–10) | `app/src/commands/*.rs` (trừ `mod.rs`), `app/ui/**` (trừ popover.html), `app/capabilities/**`, golden `settings`, `app/tests/settings_*.rs` | 3d |
| 14 | [E2E, parity, perf gate, migration](./phase-14-e2e-parity-migration.md) | C | 3–13 | `desktop-rs/tests-e2e/**`, `tools/migration/**`, reports | 3d |
| 15 | [Release cutover](./phase-15-release-cutover.md) | C | 14 | `README.md`, `.github/workflows/{release,ci}.yml`, version fields | 0.5d |

### Trạng thái thực thi (cập nhật 2026-10-16, tối)

| # | Trạng thái | Ghi chú |
|---|---|---|
| 1 | completed | Merge `bb699bf`. Spike: paint 632–692 ms, RAM idle 2.3 MB, tray 72 ms |
| 2 | completed | Merge `e380854`; contract gate PASS-WITH-FIXES (đã sửa) |
| 3 | completed | Merge `60bcf87`; roundtrip creds Py↔Rust OK; macOS keychain/plist/lock chưa build được (chờ CI mac / 14) |
| 4 | completed | Merge; 21 unit + 8 behaviour + 5 golden + 10 pusher + 16 server |
| 5 | completed | Merge `781999c`; golden agents hết ignore sau 03/04/06 |
| 6 | completed | Merge `844f40f` |
| 7 | completed | Merge `8812c53` |
| 8 | completed | Merge + review fixes (mdns stop, timeout per-op, sleep inject) |
| 9 | completed | Merge `eac5172` |
| 10 | completed | Merge + hardening (staged file check); reviewer PASS; Rust→Rust update/rollback và mac swap.sh phải test thật ở 14 |
| 11 | completed | Merge + review fixes (sign chỉ chạy trên tag rust-v*); iscc/notarize/minisign chưa kiểm trên runner thật; owner phải tạo environment `release`, secrets, vars MINISIGN_PUBKEY + EXPECTED_TEAM_ID, commit Cargo.lock |
| 12 | completed (GUI → 14) | B'' xong, 0 ignore; sửa `EngineHandle::stop()` và thứ tự cài hook `on_payload` trước khi chạy loop (bug do e2e tìm ra); checklist GUI chuyển sang 14 |
| 13 | completed (GUI → 14) | Đã merge + 39 test bridge còn thiếu đã viết (settings_api/setup/preview); còn thiếu runtime test deny popover (cần dòng dev-dependency `tauri` feature `test` trong `app/Cargo.toml`) |
| 14 | in-progress | Phần headless xong và đã merge: 26 test e2e, tài liệu migration Win/mac, test-mapping 413 hàm (388 ported, 6 partial, 9 dropped, 9 Python-only, 1 gap), parity checklist 49 dòng. Lần chạy GUI cô lập đầu tiên đã chạy (tray 1.34 s, cold paint 3.5 s, warm 690 ms, RAM idle 3 MB; đo lúc máy bận). CHƯA chạy: đo lại lúc rảnh, perf-final, parity thủ công trên Win + đồng hồ thật, migration 0.1.4→Rust, Rust→Rust, rollback, Mac qua GitHub Actions |
| 15 | in-progress | Nhánh `python-0.1.x` đã push (CI xanh). Trên `feat/rust-cutover`: `rust-release.yml` → `release.yml` (tag `v*`, chỉ tạo draft, không prerelease cho X.Y.Z), `rust-ci.yml` → `ci.yml` (Python test chỉ còn ở `python-0.1.x`), `check_version.py` nhận `vX.Y.Z`, README (mac Developer ID, cập nhật khi bấm, dev = Rust), `packaging-rs/release-notes/0.2.0.md`, log lý do thoát app. Thử cập nhật bằng fake API cục bộ (`tools/migration/{fake_api.py,local-release.md}`) thay cho fork. Còn: env `release` thêm tag `v*`, tag `v0.2.0` → draft, thử mac + Windows, sao lưu khoá minisign, publish |

Mac Intel (owner, 2026-10-16): bỏ runner `macos-15-intel` từ 0.2.1 (job ~45 phút, `cargo install tauri-cli` 29 phút; file x86_64 của 0.1.x chỉ có 2 lượt tải). 0.2.0 là bản Intel cuối.

Cổng publish (owner, 2026-10-16): fleet công khai → chỉ publish `v0.2.0` khi 0.1.4→0.2.0 (file draft thật) **và** Rust→Rust + các ca tamper đã PASS trên **cả mac lẫn Windows**, và `D:\clawd-minisign\` đã có bản sao lưu.

Tồn đọng: khoá minisign production đã tạo (`MINISIGN_PUBKEY` là biến repo), chưa sao lưu; environment `release` hiện chỉ cho tag `rust-v*` (cutover cần `v*`); `desktop-rs/Cargo.lock` đã commit; `check_public` đã xanh (allow-list file design B); test flaky cần theo dõi `feed_security::a_grant_may_only_name_relative_feed_paths`; mac chỉ kiểm qua GitHub Actions.

Đường dẫn crate tương đối `desktop-rs/`; app = `desktop-rs/app/`. Fixtures: `desktop-rs/fixtures/<area>/` (area `fw`, KHÔNG `firmware` — `tools/check_public.py:13` cấm thư mục tên `firmware`), exporter `tools/golden/export_<area>.py` — phase sở hữu area.

## File ownership rules (chống xung đột)

1. Mỗi file một phase ghi. Integrator (session chính) sở hữu: `desktop-rs/Cargo.lock`, `desktop-rs/app/Cargo.toml` (phase gửi dependency CR), `plans/**`, và `rust-ci.yml` sau khi 02 xong.
2. Phase 02 tạo stub cho mọi crate + module testkit theo area (`testkit/src/<area>.rs` rỗng, `mod` khai báo sẵn); sau merge 02, crate + module testkit chuyển cho phase tương ứng. `app/src/state.rs`, `app/src/commands/mod.rs`, `app/build.rs` **đóng băng** như contracts.
3. `clawd-contracts` + 3 file đóng băng trên chỉ thay đổi *additive* qua Contract CR trong report phase → integrator gom 1 lần/ngày, rebase. > 2 CR/ngày → dừng nhận phase mới 0.5d, sửa contracts một lượt.
4. Dep mới: thêm vào `Cargo.toml` của crate mình (crate lib) hoặc gửi CR (app). `[workspace.dependencies]` đã pin ở 02. Không commit `Cargo.lock` ở nhánh phase.
5. Phase B chỉ dùng API stub của crate khác; test dùng fake trong module testkit của chính mình hoặc `testkit::core`.
6. `clawd_desktop/**` chỉ đọc. Không agent nào sửa `.github/workflows/**` ngoài phase sở hữu (02: rust-ci, 11: rust-release, 15: release/ci).
7. Merge gate chung: `python3 tools/check_public.py` xanh.

## Integrator protocol (session chính, rolling)

- Mỗi phase chạy trong git worktree riêng, nhánh `port/NN-<slug>` từ `main`; merge thẳng vào `main` (Validation 1 — `desktop-rs/` tách biệt, không ảnh hưởng build/release Python).
- Gate merge: `cargo fmt --check`; `cargo clippy -p <crate> -- -D warnings`; `cargo test -p <crate>` (gồm golden); 0 `todo!()` trong file mình sở hữu; `check_public` xanh; report có bảng mapping test Python (ported / dropped+lý do / python-only).
- Merge ngay khi green + reviewer agent pass + owner duyệt tóm tắt. Sau mỗi merge: regenerate `Cargo.lock`, `cargo build --workspace`.
- `--advice`: advisory checkpoint (agent `oracle` — runtime không có `kongming`) sau mỗi wave và trước merge phase 12/13/14.
- File chuyển quyền có thứ tự: `app/tauri.conf.json` (12 → 15 chỉ sửa `version`), `rust-ci.yml` (02 → integrator thêm job e2e của 14).

## Cross-plan dependencies

| Quan hệ | Plan | Ghi chú |
|---|---|---|
| Kế thừa hành vi | `261014-renderer-via-feed` | Đã implemented; phase 09 port `renderers[]` + cache/verify renderer giống hệt. Không blocking. |

## Success Criteria

- [ ] Goals 1–4 đạt trên Win11 + mac arm64 (số đo trong `reports/perf-final.md`)
- [ ] Golden 100% khớp; bảng mapping 413 test đầy đủ, mỗi dòng có trạng thái
- [ ] Migration 0.1.4 → Rust pass (Win + mac); update Rust→Rust pass; bản rollback cài được qua updater Rust
- [ ] Release `v0.2.0` phát hành; Python còn phát hành được từ `python-0.1.x`

## Decisions (owner, 2026-10-15)

- Q1 mac migration: 0.1.4 mac đã ký Developer ID + notarize (log run 38551613930: `source=Notarized Developer ID`) → bản Rust **bắt buộc** ký cùng Team ID; tự cập nhật như Windows.
- Q2 nguồn gốc bản cập nhật: minisign ký `SHA256SUMS` (phase 10 verify, phase 11 ký trong CI environment có duyệt); Authenticode → backlog.
- Q3 BLE: bỏ.

## Red Team Review

### Session — 2026-10-15
**Findings:** 15 (15 accepted, 0 rejected; #15 accept một phần — Authenticode → backlog theo Q2)
**Severity breakdown:** 4 Critical, 9 High, 2 Medium

| # | Finding | Severity | Disposition | Applied To |
|---|---|---|---|---|
| 1 | Autostart sai format (`--autostart`, thiếu KeepAlive/ThrottleInterval/ProcessType, auto-launch) | Critical | Accept | 02, 03, 12, plan |
| 2 | Tag v0.2.0 trượt check version Python; vòng update vô hạn nếu tag ≠ build | Critical | Accept | 11, 14, 15 |
| 3 | `check_public` chặn `fixtures/firmware` + file nhị phân | Critical | Accept | plan, 02, 09 |
| 4 | Update Rust→Rust + rollback chưa từng chạy thật | Critical | Accept | 14, 15 |
| 5 | Thiếu kênh settings→tray (`ShellCmd`), `AppState`/session per window | High | Accept | 02, 12, 13 |
| 6 | Chia events/agents sai DAG; firmware thiếu dep config + `CLAWD_FEED_CHANNEL` | High | Accept | 02, 04, 05, 09 |
| 7 | Registry `py_api` vs 32 command; `popover_msg`; 1 invoke_handler; AppManifest/build.rs | High | Accept | 02, 12, 13 |
| 8 | File không chủ: `app/Cargo.toml`, `build.rs`, testkit, members; fake-core | High | Accept | plan, 02, 12, 13 |
| 9 | `pause.json` (lease) ≠ `ota.busy` | High | Accept | 02, 03, 09, 10, 12, 13 |
| 10 | Menu tray không khớp `tray.py:192-208`; thiếu nhập IP (không WebView2) | High | Accept | 12 |
| 11 | Prefs typed làm mất key khi sai kiểu | High | Accept | 02, 03 |
| 12 | `panic=abort` làm sập cả app | High | Accept | 01, 02, 12, 14 |
| 13 | Event server: chunked/slowloris/DNS-rebinding | High | Accept | 04 |
| 14 | CI release: trigger/secret/permissions; exporter đọc HOME thật; test ghi OS store thật | High | Accept | 02, 03, 11, 12 |
| 15 | Updater: redirect từng hop, feed https, sha trong RAM, nguồn gốc (minisign); binary `Clawd.exe`; CI Ubuntu; WebView2 bootstrap; mapping test; Info.plist mạng cục bộ; 0.1.4 cần user bấm | Medium | Accept (Authenticode → backlog) | 02, 06, 07, 09, 10, 11, 14, 15, plan |

### Whole-Plan Consistency Sweep

- Decision delta: autostart byte-copy không flag; DAG events→agents (bỏ trait); `ShellCmd`/`AppState`/`SettingsSession` đóng băng ở 02; `py_api`+`popover_msg` + AppManifest; pause lease ≠ ota_busy; Prefs = Map; panic unwind + catch_unwind; fixtures area `fw`; bỏ fake-core → B''; bỏ single-instance plugin, auto-launch, WebView2 bootstrapper, exe gate; thêm minisign, Host check, gate Rust→Rust + rollback; mac Developer ID bắt buộc.
- Đã grep mọi file plan cho thuật ngữ cũ (`EventNormalizer`, `fake-core`, `auto-launch`, `--autostart`, `migrate_from_python`, `axum`, `single-instance`, `fixtures/firmware`, `panic="abort"`, `3 OS`, `Mở nhật ký`, `tự cài 0.2.0`): chỉ còn ở dạng phủ định/lịch sử.
- `ak plan validate` OK; `tools/check_public.py` OK.
- Advisory checkpoint 2 (oracle): Go có điều kiện → đã sửa: override `CLAWD_MINISIGN_PUBKEY` cho build fork (02, 10, 14), quy trình fork release `v*` (14), `rust-v*` luôn draft+prerelease + gate (11), integrations vào `crates/clawd-agents/assets/` (02, 05), mac x64 ngoại lệ có duyệt (plan).
- Mâu thuẫn chưa giải quyết: **0**.

## Validation Log

### Session 1 — 2026-10-15
**Trigger:** `/ak:plan validate` sau red-team + 2 advisory checkpoint (owner chọn).
**Questions asked:** 4 (verification pass bỏ qua: Red Team Review đã có dẫn chứng file:line)

#### Questions & Answers

1. **[Risks]** Nếu spike cho thấy mở Cài đặt > 300 ms, agent tự xử lý hướng nào?
   - Options: Nới ≤ 600 ms, giữ RAM ≤ 20 MB (Recommended) | Giữ 300 ms, chấp nhận RAM cao | Dừng hỏi owner
   - **Answer:** Nới ≤ 600 ms, giữ RAM ≤ 20 MB
   - **Rationale:** bỏ Design B (webview ẩn), phase 12/13 không phải chờ owner.
2. **[Tradeoffs]** Bao nhiêu agent song song và review thế nào?
   - Options: 6 agent + reviewer agent + owner xem nhanh (Recommended) | 11 agent + reviewer agent | 4 agent + owner review kỹ
   - **Answer:** 6 agent, reviewer agent + owner xem nhanh
   - **Rationale:** owner chưa vững Rust → reviewer agent là lớp review chính; wall-clock ~10.5 agent-days.
3. **[Assumptions]** Thiết bị thật cho phase 14? (multi-select)
   - Options: Máy/VM Windows sạch | Mac Apple silicon | Mac Intel | Đồng hồ Clawd thật
   - **Answer:** Máy/VM Windows sạch, Mac Apple silicon, Đồng hồ Clawd thật
   - **Rationale:** mac x64 không kiểm tay được → ngoại lệ, chỉ CI build + verify chữ ký.
4. **[Architecture]** Các phase merge vào đâu?
   - Options: Thẳng main (Recommended) | Nhánh rust-port sống lâu
   - **Answer:** Thẳng main
   - **Rationale:** `desktop-rs/` tách biệt; tránh rebase nhánh dài theo bugfix Python.

#### Confirmed Decisions
- Ngưỡng paint: 300 ms mục tiêu, 600 ms chấp nhận, không đánh đổi RAM.
- Concurrency cap 6; review = reviewer agent + owner duyệt tóm tắt.
- Thiết bị: Windows VM, mac arm64, đồng hồ thật; mac x64 ngoại lệ.
- Trunk = `main`.

#### Impact on Phases
- Phase 01: bước 5 + risk (Design A only, 300/600 ms).
- Phase 12: popover destroy khi mất focus, bỏ Design B.
- Phase 13: success/risk theo 300/600 ms.
- Phase 14: thiết bị cụ thể, mac x64 ngoại lệ.
- plan.md: Goals 3/5, lịch slot cap 6, wall-clock, integrator protocol (main, reviewer agent).

### Whole-Plan Consistency Sweep
- Grep `rust-port`, `Design B`, `cap 6–8`, `mac x64 nếu có máy`, critical path cũ: không còn bản cũ (chỉ dòng ghi "lý thuyết 11 slot ≈ 8.5" cố ý).
- Mâu thuẫn chưa giải quyết: **0**. Verification: Failed 0.

<!-- slug: rust-tauri-port -->

## Contract gate log

### Phase 02 gate - 2026-10-15
- Reviewer verdict: PASS-WITH-FIXES (9 blocking: testkit deps/ownership, `Services`+`SettingsSession` state, `UsageEnv` injection, time model (`tokio::time::Instant`), `Autostart` -> `io::Result`, `Env` injection, PORT-MAP bootwatch edge, harness hardening, CI SHA verification). Oracle: conditional GO (merge 02 before 01 is safe; commit `plans/` first; per-phase checkpoint commits; D: drive env vars for every worker; each phase removes its crate's stub `#![allow]`).
- Accepted deviations: `pause::hold(&AppPaths, ..)`; `SessionSlot` folded into `AppState`; `check_public` exempts `sk-ant-test-`/`sk-ant-fake-` only.
- Worker rules for Wave B: commit per green milestone (<=15 min), `CARGO_HOME`/`CARGO_TARGET_DIR`/`TMP` on D:, `-p <crate>` builds only (integrator builds `--workspace`), builds >3 min run in background, delete target dir after merge, remove own crate's stub allow and keep clippy `-D warnings` green.

### Goal 3 decision - 2026-10-15
- Phase 01 spike: paint cold 692 ms (n=4), warm 632 ms (n=100; 27% <= 600 ms), first open on a fresh WebView2 profile 2.3-3.3 s; WebView2 creation is 70-80% of the time. See reports/spike-decision.md.
- Owner: allow RAM-free mitigations in phases 12/13 (show the dark window at `window_built`, explicit `data_directory`, WebView2 browser args) and accept ~0.7 s if 600 ms is not reachable. Do NOT trade RAM (hidden/pre-warmed webview stays ruled out). Phases 03-10 unaffected.
- Owner: no Python window baseline (Python window measurements skipped); Goal 3 judged on absolute numbers. macOS paint deferred to phase 14.
