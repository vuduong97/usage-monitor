---
phase: 8
title: "Clock API, discovery, bootwatch"
status: completed
priority: P1
effort: "1.5d"
dependencies: [2]
---

# Phase 08: Clock API, mDNS discovery, bootwatch (crate `clawd-clock`)

## Overview
Port `clockapi.py` (256), `discovery.py` (409), `bootwatch.py` (106): client HTTP tới API cài đặt của đồng hồ (Digest auth), tìm đồng hồ qua mDNS `_clawd._tcp.local.` + IP nhập tay + ClockStore (`clock.json`), phát hiện đồng hồ reboot để đẩy lại dữ liệu.

## Context Links
- `clawd_desktop/clockapi.py`, `discovery.py`, `bootwatch.py`, `PROTOCOL.md`
- Git log: `c6f18fe` (địa chỉ gõ sai không làm crash resolver — `UnicodeError` IDNA → `ClockError`)
- [researcher-02 §mDNS, traps 6–7](./research/researcher-02-crates-compat.md)
- Tests Python: `test_clockapi` (13), `test_discovery` (15), `test_bootwatch` (11)

## Requirements
- Functional:
  - `ClockApi` async (reqwest): `heap, config, save(changes), status, scan, reboot, factory_reset, export, restore, get_file(path,max_bytes), upload_firmware, upload_pack(kind,data), style, install_style, clear_style, wait_back(booted_before, deadline=150)`; retries + `RETRY_GAP`; 401 → `AuthNeeded`; ≥400 → `ClockError("HTTP n: <200 chars>")`; connect/timeout lỗi → retry rồi `ClockError("no answer from …")`; host sai → `ClockError` không panic. Digest auth qua `diqwest`; serialize request mỗi client (Mutex như `_lock`).
  - `config_diff(old,new)` khớp Python.
  - Discovery: `Clock{base,usage_url,label}`, `manual_clock(raw)`, `valid_host`, `ClockStore` (`clock.json` load/save/unpin, cùng schema), `stored_clock`, `MdnsBrowser` (`mdns-sd` 0.21, service `_clawd._tcp.local.`, resolve timeout như `RESOLVE_TIMEOUT_MS`), `ClockFinder` (`targets, describe, report(url,ok), set_manual, use, retry`, logic `_choose/_adopt/_remember` y hệt).
  - BootWatch: theo dõi `uptime` qua `/api/heap`, gọi `on_restart(url, uptime)`.
- Non-functional: mDNS daemon thread duy nhất, dừng được (`close_mdns`); không bind cổng LAN nào ngoài 5353 multicast.

## Architecture
`crates/clawd-clock/src/{api.rs, diff.rs, clock.rs, store.rs, mdns.rs, finder.rs, bootwatch.rs}`. Finder nhận `MdnsSource` trait (prod = mdns-sd, test = fake) để test xác định.

## Related Code Files
- Own: `desktop-rs/crates/clawd-clock/**`, `desktop-rs/crates/clawd-testkit/src/clock.rs` (fake clock Digest), `tools/golden/export_clock.py`, `desktop-rs/fixtures/clock/**`
- Read-only: `clawd_desktop/{clockapi,discovery,bootwatch}.py`

## Implementation Steps
1. Exporter: `config_diff` cases, `manual_clock`/`valid_host` inputs (gồm Unicode/IDNA hỏng), `clock.json` mẫu, chuỗi sự kiện finder (mdns thấy/mất, report ok/fail, manual) với clock giả → targets/describe.
2. Port api + diff với fake clock server (testkit), gồm Digest challenge.
3. Port clock/store/finder với `MdnsSource` fake; rồi `mdns.rs` thật.
4. Port bootwatch.
5. Bảng mapping test (39).

## Todo
- [ ] Exporter + fixtures clock
- [ ] ClockApi + Digest + config_diff
- [ ] ClockStore + ClockFinder + mdns-sd
- [ ] BootWatch
- [ ] Bảng mapping test

## Success Criteria
- [ ] `cargo test -p clawd-clock` xanh; golden khớp
- [ ] Thủ công trên LAN thật: Rust thấy đồng hồ qua mDNS trên Win11 và mac (ghi kết quả vào report); nếu Windows không nhận mDNS, IP nhập tay vẫn chạy

## Risk Assessment
- `mdns-sd` trên Windows có báo cáo không nhận response (issue #33). Tín hiệu: test LAN thật thất bại trên Windows. Phản ứng: thử bind theo interface; nếu vẫn lỗi → báo integrator, cân nhắc gọi `DnsServiceBrowse` (Win32 DNS-SD API) — không chặn wave vì có manual IP + ClockStore nhớ địa chỉ.
- Đồng hồ ESP32 chậm sau boot → giữ timeout/`RESEND_TIMEOUT` như Python.

## Security Considerations
- Mật khẩu web đồng hồ lấy từ `clawd_store::creds`, chỉ gửi qua Digest; không log.

## Next Steps
- 09 (OTA dùng ClockApi), 12 (engine finder/bootwatch), 13 (`scan`, `setup_clocks`, `set_clock`, `login`, `reboot`, `factory_reset`, backup).
