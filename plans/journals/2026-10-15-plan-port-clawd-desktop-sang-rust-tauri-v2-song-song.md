---
title: Plan port Clawd desktop sang Rust + Tauri v2 (song song)
date: 2026-10-15
summary: "15 phase, cap 6 agent, red-team 15 finding, validate 4 quyết định"
---

# Plan port Clawd desktop sang Rust + Tauri v2 (song song)

# Plan port Clawd desktop → Rust + Tauri v2

- Plan: plans/261015-0726-rust-tauri-port (15 phase, interface-first; wave A 01+02, wave B/B' 03–13, wave C 14–15).
- Research: Tauri 2.12.4 tray-only + lazy window khả thi; 300 ms paint là rủi ro lớn nhất (WebView2 cold 0.4–0.7 s).
- Advisory (oracle thay kongming): giữ Inno Setup vì updater 0.1.4 chạy cờ Inno `/SILENT`; chia nhỏ fixtures theo phase; merge cuốn chiếu.
- Red team 4 lens: 15 finding accept. Quan trọng: autostart Python không có `--autostart` và có KeepAlive; check_public cấm thư mục `firmware`; release.yml kiểm tag = `__version__` Python; thiếu kênh settings→tray (ShellCmd); pause.json ≠ ota.busy; Prefs phải là Map; panic unwind.
- Phát hiện: bản mac 0.1.4 đã ký Developer ID + notarize (README lỗi thời) → bản Rust phải cùng Team ID.
- Quyết định owner: minisign ký SHA256SUMS; bỏ BLE; paint fallback 600 ms (RAM ưu tiên); cap 6 agent + reviewer agent; merge thẳng main; thiết bị: Windows VM, mac arm64, đồng hồ thật (mac x64 ngoại lệ).
- Bài học: lỗi khi edit có ký tự Việt dạng escape `\uXXXX` trong tool edit → ghi literal; dùng script unescape để sửa.

> Historical work record — not durable authority. Prefer docs/specs/ADRs for current decisions.
