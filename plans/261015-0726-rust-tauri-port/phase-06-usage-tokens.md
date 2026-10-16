---
phase: 6
title: "Config + token sources"
status: completed
priority: P1
effort: "1.5d"
dependencies: [2]
---

# Phase 06: Config file + Claude token sources (crate `clawd-config`)

## Overview
Tách nửa "đọc nguồn" của `usage.py` (1615 LOC) thành crate riêng để chạy song song với poller (phase 07): file `config` key=value, `config_dirs`, http targets, chime/clock settings, và mọi nguồn OAuth token Claude (Anthropic store v5, Keychain, `.credentials.json`, token store, CLIProxy key/url). Không có mạng ở crate này.

## Context Links
- `clawd_desktop/usage.py:44-330` (hằng số, `_extract_access_token`, `_decode_keychain_blob`, `_read_token_keychain`, `read_config_dirs`, `read_token_for`, `_read_token_store`, `read_config_value`, `read_onoff_setting`, `normalize_http_target`, `read_http_targets`, `read_chime_setting`, `read_clock_setting`, `detect_hour_format`, `read_cliproxy_key`, `read_cliproxy_url`, `read_usage_source`)
- scout §3 (Claude token: macOS Keychain qua `security` CLI; Win/Linux `.credentials.json`)
- [researcher-02 trap 5](./research/researcher-02-crates-compat.md) (Keychain ACL)
- Tests Python: `test_token_store` (9), `test_freeride` (4, `_decode_keychain_blob`), `test_macos_multidir` (9 test token/keychain theo dir; 8 test PlanSelector/`poll_active_payload` → phase 07; 4 test BLE `:190-213` → dropped Q3), fixtures `credentials_direct.json`, `credentials_nested.json`

## Requirements
- Functional:
  - `read_config_value(name)`: parse y hệt (comment `#`, `key=value`, key case-insensitive, strip); `read_onoff_setting`; `read_config_dirs` (mặc định `[~/.claude]`, comma list, `~` expand).
  - Token theo thứ tự Python: Anthropic store v5 (`~/.config/anthropic/active_config` + `credentials/<profile>.json`, `expires_at` ms) → Keychain (macOS) → `<dir>/.credentials.json`; xử lý blob lồng/thẳng, raw token `sk-ant-…`, blob Keychain hex/không in được (`_decode_keychain_blob`).
  - macOS Keychain: giữ **cơ chế gọi `security find-generic-password`** như Python (cùng hành vi prompt/ACL như bản hiện tại; trust ACL của `/usr/bin/security` đã được user "Luôn cho phép" sẽ còn hiệu lực với exe mới). Không chuyển sang Security API trong plan này.
  - Multi-config-dir macOS (`test_macos_multidir`): Keychain service theo dir (đọc logic Python).
  - CLIProxy: key Keychain `clawd-cliproxy` (fallback `clawdmeter-cliproxy`), url mặc định `http://localhost:8317`.
  - `read_usage_source`, `read_http_targets` + `normalize_http_target` (bare host → `http://host/api/usage`).
  - `detect_hour_format` (12/24h theo locale OS).
  - `TokenExpired`, `UsageNotApplicable` error types (từ contracts).
- Non-functional: không log token; đọc lỗi → `None`, không panic.

## Architecture
`crates/clawd-config/src/{config_file.rs, dirs.rs, tokens/{mod.rs, anthropic_store.rs, keychain.rs, credentials_file.rs, decode.rs}, cliproxy.rs, http_targets.rs, locale.rs}`.

## Related Code Files
- Own: `desktop-rs/crates/clawd-config/**`, `desktop-rs/crates/clawd-testkit/src/config.rs`, `tools/golden/export_config.py`, `desktop-rs/fixtures/config/**`
- Read-only: `clawd_desktop/usage.py`, `tests/fixtures/*`

## Implementation Steps
1. Exporter: nhiều file `config` mẫu → giá trị đọc; blob credential (direct, nested, store v5, hết hạn, hỏng) → token/None; Keychain blob mẫu → decode; http targets.
2. Port config_file/dirs/http_targets/locale.
3. Port tokens (mock `security` CLI qua trait `CommandRunner` để test trên mọi OS).
4. Port cliproxy readers.
5. Bảng mapping test (22 port; BLE dropped).

## Todo
- [ ] Exporter + fixtures config
- [ ] config_file/dirs/http_targets/locale
- [ ] tokens (store v5, keychain, file, decode)
- [ ] cliproxy readers
- [ ] Bảng mapping test

## Success Criteria
- [ ] `cargo test -p clawd-config` xanh; golden 100% khớp
- [ ] Trên máy owner (mac nếu có): đọc được token thật, khớp Python (`uv run python -c "from clawd_desktop import usage; print(bool(usage.read_token_for(...)))"`)

## Risk Assessment
- Claude Code đổi storage format (đã xảy ra v5) → test fixtures phủ cả 3 định dạng; lỗi đọc trả None → UI báo "login expired" như commit `2c259b9`.

## Security Considerations
- Token chỉ trong bộ nhớ; không ghi ra fixture thật (fixture dùng token giả).

## Next Steps
- 07 poller, 05 (`read_config_dirs`), 04 (`event_port`, pet config).
