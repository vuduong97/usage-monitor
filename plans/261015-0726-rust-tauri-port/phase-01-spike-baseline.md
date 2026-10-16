---
phase: 1
title: "Spike & baseline đo hiệu năng"
status: completed
priority: P1
effort: "1d"
dependencies: []
---

# Phase 01: Spike & baseline đo hiệu năng

## Overview
Đo bản Python 0.1.4 (số gốc) và dựng spike Tauri v2 vứt-đi để kiểm 4 mục tiêu trước khi phase 11/12/13 khoá thiết kế vỏ app. Output = quyết định thiết kế popover/cửa sổ (A hoặc B) ghi vào report.

## Context Links
- [researcher-01](./research/researcher-01-tauri-packaging.md) §Risks (9 điểm cần đo)
- `dist/Clawd/` (bản Python build sẵn), `clawd_desktop/ui/index.html`

## Requirements
- Functional: script đo lặp lại được (Windows PowerShell + macOS sh) cho: kích thước installer, private bytes/working set process chính tại T+10s, T+10min, thời gian launch→tray icon, click→first paint cửa sổ (cold + warm), RAM sau 50 vòng mở/đóng.
- Spike: Tauri 2.12.x, `app.windows: []`, tray + menu, `prevent_exit` khi `code.is_none()`, tạo `WebviewWindow` lazy với `visible(false)` + `backgroundColor`, show khi JS emit `ready`, đóng = destroy. Load **nguyên** `clawd_desktop/ui/index.html` (copy vào spike).
- Release profile: `opt-level="s"` (thử `"z"`), `lto=true`, `codegen-units=1`, `panic="unwind"` (đo thêm size với `abort` để owner biết chênh lệch; mặc định giữ unwind — red team #12), `strip=true`. Kèm tokio + reqwest(rustls) + hyper server (http1) để đo size thật.
- Non-functional: spike không merge vào app; chỉ số liệu + kết luận.

## Architecture
- `tools/perf/measure.ps1`, `tools/perf/measure.sh`: nhận path exe, in JSON `{installer_bytes, idle_private_mb_10s, idle_private_mb_10m, tray_ms, paint_cold_ms, paint_warm_ms, rss_after_50_cycles_mb}`.
- First paint: JS gọi `performance.now()` khi `DOMContentLoaded` + `requestAnimationFrame` → emit `perf` event; Rust ghi timestamp lúc nhận click tray → hiệu.
- Python baseline: đo `dist/Clawd/Clawd.exe` cùng script (settings window là subprocess → đo thời gian tới cửa sổ hiện qua log).

## Related Code Files
- Create: `tools/perf/measure.ps1`, `tools/perf/measure.sh`, `tools/perf/README.md`, `desktop-rs/spike/**` (Cargo project độc lập, KHÔNG là member workspace)
- Create: `plans/261015-0726-rust-tauri-port/reports/perf-baseline.md`, `reports/spike-decision.md`

## Implementation Steps
1. Viết script đo; chạy với Python 0.1.4 trên Win11 (+ mac nếu có máy) → `perf-baseline.md`.
2. Dựng spike Tauri tray-only như trên; build release; đóng gói thử bằng Inno Setup tối giản (LZMA2) để đo size installer thực.
3. Đo 4 mục tiêu + 50 vòng open/close (kiểm `msedgewebview2.exe` thoát, RSS về baseline).
4. Thử `SetProcessWorkingSetSize(-1,-1)` sau destroy; ghi khác biệt.
5. Ghi `spike-decision.md`: luôn **Design A** (webview popover/settings tạo lười, destroy khi đóng; Windows không có popover — parity Python). Ngưỡng first paint: mục tiêu ≤ 300 ms; nếu paint_cold > 300 ms → ngưỡng chấp nhận **≤ 600 ms**, KHÔNG giữ webview ẩn (RAM ≤ 20 MB ưu tiên). <!-- Updated: Validation Session 1 - 300→600 ms fallback, RAM ưu tiên -->
6. Ghi khuyến nghị size: rustls provider (ring vs aws-lc), opt-level s/z, có cần NASM/CMake trên CI Windows.

## Todo
- [ ] Script đo Win + mac
- [ ] Baseline Python
- [ ] Spike tray-only + lazy window
- [ ] Đo size/RAM/paint/tray + 50 vòng
- [ ] spike-decision.md (ngưỡng paint 300/600, rustls provider, opt-level, panic unwind size)

## Success Criteria
- [ ] `perf-baseline.md` và `spike-decision.md` có số đo thật, mỗi số kèm lệnh tái tạo
- [ ] Ngưỡng paint chốt (300 hoặc 600 ms) ghi rõ; phase 12/13 đọc được mà không cần hỏi thêm

## Risk Assessment
- Paint cold > 300 ms → tự áp ngưỡng ≤ 600 ms (owner đã duyệt, Validation 1), không chờ. Paint cold > 600 ms → báo owner (chỉ chặn 12/13, không chặn 03–10).
- Không có máy mac → đo mac dời sang phase 14; ghi rõ "chưa đo".

## Security Considerations
- Spike không đọc credential thật. Không commit artifact build.

## Next Steps
- Mở khoá 11, 12, 13.
