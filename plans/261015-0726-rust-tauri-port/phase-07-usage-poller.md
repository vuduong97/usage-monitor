---
phase: 7
title: "Usage poller + publisher + codex"
status: pending
priority: P1
effort: "2.5d"
dependencies: [2]
---

# Phase 07: Usage poller, publisher, Codex usage (crate `clawd-usage`)

## Overview
Port nửa "mạng + xuất bản" của `usage.py` và `codex_usage.py` (165): poll OAuth usage endpoint (fallback Haiku probe), tính % cửa sổ 5h/tuần, PlanSelector/CliProxySource, attention flags, encode payload, `Session`/`HttpSinks`/`Publisher` đẩy `/api/usage` lên đồng hồ, Codex usage từ session logs. **Bỏ BLE** (non-goal: `discover_target`, `retrieve_connected_macos`, `connect_and_run`, `ble_loop`, `unpair_macos`, `_blueutil`, `main`).

## Context Links
- `clawd_desktop/usage.py` (từ `poll_api` tới `Publisher`, `start_events`), `codex_usage.py`
- `engine.py:28-40` (`TrackingSinks`), scout §5
- Tests Python: `test_usage_sources` (33), `test_http_push` (14; `:52`, `:172` BLE → dropped), `test_attention` (3), `test_codex_usage` (9), `test_macos_multidir` (8 test PlanSelector/`poll_active_payload`, `usage.py:915`, `:1044`), fixture `oauth_usage_max.json`

## Requirements
- Functional:
  - `poll_usage_endpoint(token)` GET `https://api.anthropic.com/api/oauth/usage` (headers y hệt `USAGE_HEADERS_TEMPLATE`); `poll_api` Haiku probe fallback (`API_URL`, `API_BODY`, header ratelimit); `usage_from_oauth_usage`, `_billing_period_info`, `_minutes_until`, `_window_pct` khớp số học Python (làm tròn!).
  - Lỗi: 401 → `TokenExpired` (UI "login expired"), không áp dụng → `UsageNotApplicable`.
  - `PlanSelector`, `poll_claude`, `poll_active`, `poll_active_payload`, `_poll_dir`, `CliProxySource` (list accounts TTL 300 s, api-call qua proxy).
  - `to_ascii`, `encode_payload(limit=DEFAULT_MAX_WRITE 180)`, `add_chime_field`, `add_clock_fields`, `take_attention_flag` (`attention` file, max age 60 s), `take_clears`.
  - `HttpSinks`/`Publisher`: POLL_INTERVAL 60, TICK 5, push `HTTP_PUSH_PATH=/api/usage` timeout 5 s, resend 15 s sau reboot; hook `on_result` cho engine (TrackingSinks); `refresh_requested`/`wake` → `Notify`.
  - Codex: đọc session logs `~/.codex/sessions/**` như `codex_usage.py`.
  - Ghi `usage_payload.json` qua `clawd_store::set_usage_payload`.
- Mỗi vòng poll/publish bọc bắt lỗi + panic (log, tiếp tục) như `engine.py:185` `return_exceptions=True` (red team #12).
- Non-functional: một `reqwest::Client` dùng chung (truyền vào), không tạo client mỗi lần poll; idle giữa các poll không giữ buffer lớn.

## Architecture
`crates/clawd-usage/src/{oauth.rs, probe.rs, math.rs, selector.rs, cliproxy.rs, payload.rs, attention.rs, publisher.rs, sinks.rs, codex.rs}`. HTTP qua trait `HttpClient` (prod = reqwest, test = testkit fake) để golden test không cần mạng.

## Related Code Files
- Own: `desktop-rs/crates/clawd-usage/**`, `desktop-rs/crates/clawd-testkit/src/usage.rs`, `tools/golden/export_usage.py`, `desktop-rs/fixtures/usage/**`
- Read-only: `clawd_desktop/usage.py`, `codex_usage.py`, `engine.py`

## Implementation Steps
1. Exporter: response mẫu (oauth max/pro/enterprise/rỗng/lỗi 401/429) + `now` cố định → payload dict; encode_payload với text dài/Unicode; codex log mẫu → usage; attention file các tuổi.
2. Port math/oauth/probe/payload/attention → golden.
3. Port selector/cliproxy/codex.
4. Port Publisher/HttpSinks với fake clock server (testkit) — test resend, offline clock không chặn vòng.
5. Bảng mapping test (~65 port; BLE dropped).

## Todo
- [ ] Exporter + fixtures usage
- [ ] math/oauth/probe/payload/attention
- [ ] selector/cliproxy/codex
- [ ] Publisher/HttpSinks
- [ ] Bảng mapping test

## Success Criteria
- [ ] `cargo test -p clawd-usage` xanh; golden 100% (gồm làm tròn %, phút còn lại)
- [ ] Không còn symbol BLE; PORT-MAP ghi "dropped: BLE (dev-only)"
- [ ] Clock offline: vòng poll tiếp tục, không treo > 5 s (test)

## Risk Assessment
- Anthropic đổi API/headers giữa chừng → giữ hằng số trong 1 file, golden dựa fixture không mạng.
- Khác biệt float/round Python (`round` banker's) vs Rust → test biên .5 trong golden.

## Security Considerations
- Token chỉ gửi tới `api.anthropic.com` và proxy cấu hình; không log header Authorization.

## Next Steps
- 12 wiring engine; 13 `usage_preview`, `usage_sources`, `set_usage_source`, `login`.
