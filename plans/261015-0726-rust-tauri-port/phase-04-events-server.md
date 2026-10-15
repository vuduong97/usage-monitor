---
phase: 4
title: "Events brain + loopback server"
status: pending
priority: P1
effort: "2.5d"
dependencies: [2]
---

# Phase 04: Events brain + loopback event server

## Overview
Port `events.py` (1153 LOC) **giữ đủ vị trí symbol như Python**: Tracker/Sess/View, `EVENTS`, `CLAUDE_HOOK_EVENTS`, helper chuẩn hoá dùng chung (`tool_action`, `is_question_tool`, `is_debug_prompt`, `is_test_command`, `ascii_text`, `_tool_summary`), `claude_event`/`pi_event`, Pet moods + Moments, Pokes/focus, Brain, StatePusher, EventServer `127.0.0.1:47913`, idle detection, `read_event_port`, `read_pet_config`, cộng `daemon_running()` (probe `GET /state`). Crate `clawd-agents` phụ thuộc crate này (DAG phase 02).

## Context Links
- `clawd_desktop/events.py`, `engine.py:41-56` (`daemon_running`), `PROTOCOL.md`
- scout §3 (routes, Origin 403, MAX_BODY 4 MiB), §5 (asyncio)
- Red team #6 (DAG), #13 (hardening)
- Tests Python: `test_clawd_events` (60), `test_hooks` (phần events)

## Requirements
- Functional:
  - Server: tokio TCP + `hyper` 1 `http1` (không axum). Bind chỉ `127.0.0.1`. Port: env `CLAWD_EVENT_PORT` → config `event_port` → 47913; port 0 cho test.
  - Routes y hệt: `GET /state` → `{desired, sessions, turns, away, focus_left}`; `POST /poke`; `POST /hook/<agent>?e=<event>`. Header `Origin` → 403.
  - **Hardening** (red team #13): body bọc `http_body_util::Limited(4 MiB)` (cả chunked) → 413; `http1::Builder::timer(TokioTimer).header_read_timeout(10s)` + timeout 10 s toàn kết nối; semaphore ~64 kết nối; `Host` khác `127.0.0.1:<port>`/`localhost:<port>` → 403 (chống DNS rebinding đọc `/state`; mọi client hợp lệ đã dùng 127.0.0.1: `integrations/cli/clawd:41`, `integrations/pi/clawd-home.ts:114`, `integrations/opencode/clawd.js:18`). Đây là bổ sung so với Python (đã duyệt).
  - Normalize: `EventServer::new(brain, normalize: NormalizeFn)`; `NormalizeFn` type alias định nghĩa ở đây; app truyền `clawd_agents::normalize`. Test dùng normalize giả.
  - Brain: thời gian qua trait `Clock` inject; `wake` = `tokio::sync::Notify`.
  - StatePusher: POST `http://<clock>/api/state`, đọc `/api/heap`; `wire()` byte-khớp Python; dừng khi `PauseReader` có lý do (engine quyết, xem 12).
  - Idle: Windows `GetLastInputInfo`; macOS `ioreg -c IOHIDSystem` HIDIdleTime (parity).
  - `daemon_running(port, timeout)`: GET `/state`, 200 + có `"desired"`.
- Non-functional: tick ngủ khi không có session; không clone payload lớn mỗi tick.

## Architecture
`crates/clawd-events/src/{vocab.rs (EVENTS, CLAUDE_HOOK_EVENTS, helpers), source_events.rs (claude_event, pi_event, tool_summary), server.rs, brain.rs, tracker.rs, pet.rs, pokes.rs, pusher.rs, idle.rs, config.rs, probe.rs}`; fakes `testkit/src/events.rs`. Server → `mpsc` → Brain; Brain publish `View` qua `watch`.

## Related Code Files
- Own: `desktop-rs/crates/clawd-events/**`, `desktop-rs/crates/clawd-testkit/src/events.rs`, `tools/golden/export_events.py`, `desktop-rs/fixtures/events/**`
- Read-only: `clawd_desktop/events.py`, `engine.py`

## Implementation Steps
1. Exporter: kịch bản event (từ `test_clawd_events.py`) qua Brain Python với clock giả → snapshot `View`/`wire()`; payload Claude/pi mẫu → `claude_event`/`pi_event`; helper vocab; response `/state`.
2. Port vocab + source_events + tracker/pet/pokes/brain → golden.
3. Port server + hardening + probe; test: Origin 403, Host 403, Content-Length > 4 MiB 413, **chunked > 4 MiB 413**, **slow-header client bị cắt ≤ 10 s**, port 0, 404, 100 request đồng thời, 65+ kết nối treo không chặn request mới quá timeout.
4. Port pusher (fake clock testkit) + idle.
5. Bảng mapping 60 test `test_clawd_events` + phần `test_hooks`.

## Todo
- [ ] Exporter + fixtures events
- [ ] vocab + source_events + brain/tracker/pet/pokes + golden
- [ ] Server hyper + hardening + probe + test bảo mật
- [ ] StatePusher + idle
- [ ] Bảng mapping test

## Success Criteria
- [ ] `cargo test -p clawd-events` xanh, golden 100%
- [ ] `curl -s --max-time 0.3 --data-binary @payload.json http://127.0.0.1:<port>/hook/claude` < 20 ms p99 dưới tải
- [ ] `integrations/cli/clawd`, pi extension, opencode plugin gọi được server Rust

## Risk Assessment
- Lệch logic thời gian → golden với clock giả là lưới chính; sửa Rust, không sửa fixture.
- Port 47913 bị Python chiếm → `daemon_running` true → app Rust thoát (đúng hành vi; 14 tắt Python trước).

## Security Considerations
- Chỉ loopback; Origin + Host check; giới hạn body/thời gian/kết nối; chỉ log loại event, không log payload.

## Next Steps
- 05 dùng vocab/`claude_event`; 12 wiring `Brain`, `EventServer`, `StatePusher`.
