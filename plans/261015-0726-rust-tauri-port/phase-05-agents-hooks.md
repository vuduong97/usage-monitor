---
phase: 5
title: "Agents + hooks"
status: completed
priority: P1
effort: "2d"
dependencies: [2]
---

# Phase 05: Agents + hooks

## Overview
Port `agents/*` (7 adapter + base + plugin + registry, ~870 LOC) và `hooks.py` (170): detect agent, cài/gỡ hook vào config của agent (merge, backup, ghi atomic, không đụng file không parse được), chuẩn hoá payload hook theo từng agent (`normalize(agent_id, payload, hint)`). Phụ thuộc `clawd-events` cho vocab dùng chung (`EVENTS`, `CLAUDE_HOOK_EVENTS`, `is_question_tool`, `is_debug_prompt`, `ascii_text`, `claude_event`, `pi_event`, `read_event_port`) — giữ vị trí như Python, không nhân bản (red team #6).

## Context Links
- `clawd_desktop/agents/{base,plugin,claude,codex,copilot,cursor,gemini,opencode,pi,__init__}.py`, `hooks.py`, `events.py:487-553` (`_tool_summary`, `claude_event`, `pi_event`)
- scout §3: hook commands, marks, agent config paths, bundled integrations
- Git log: các fix gần đây phải giữ — gỡ chỉ handler của mình trong group dùng chung (`8f0b32d`), Gemini group không matcher (`b8a9edb`), Windows paths (`e5fc45c`)
- Tests Python: `test_agents` (24), `test_hooks` (10), `test_setup_api` (phần agents)

## Requirements
- Functional:
  - `Agent` trait: `id, found, where_, beta, config_dir, is_installed, install, uninstall, normalize, info`; registry 7 agent; `found_ids`, `uninstall_all`, `refresh_plugins`.
  - Command strings **byte-khớp** Python: posix `'<CONFIG_DIR>/clawd-event.sh' <id> <event>`; Windows `cmd /c "curl.exe -s -o NUL --max-time 0.3 --connect-timeout 0.2 --data-binary @- http://127.0.0.1:<port>/hook/<id>?e=<event> || exit 0"`; Claude Code: posix `{"type":"command","command":"'<CONFIG_DIR>/clawd-event.sh'","timeout":2}`, Windows exec-form `curl.exe` args, `async:true`, `timeout:5`.
  - Nhận diện "của mình": `MARKS=("clawd-event","/hook/claude")`, `SCRIPT_MARK`, `HTTP_MARK` — hook do bản Python cài phải được Rust nhận ra và gỡ đúng.
  - `merge_groups`, `merge_flat`, `has_ours`, `edit_json`: file không parse → `AgentError`, không ghi; backup trước khi ghi; atomic replace; giữ nguyên format JSON khác (key order — dùng `serde_json` `preserve_order`).
  - `write_script` (`HOOK_SH`, chmod 755) vào config dir; plugin agents copy `clawd-home.ts`/`clawd.js` từ `crates/clawd-agents/assets/` (02 copy sẵn, phase này sở hữu) nhúng bằng `include_bytes!`.
  - `settings_files()` đọc `clawd_config::read_config_dirs()` (stub phase 06); port hook lấy từ `clawd_events::read_event_port`; danh sách event Claude từ `clawd_events::CLAUDE_HOOK_EVENTS`.
  - `pub fn normalize(agent_id, payload, hint) -> Option<Vec<Value>>` khớp signature `clawd_events::NormalizeFn`; app truyền vào EventServer. Khớp Python cho mọi agent.
- Non-functional: không bao giờ ghi file config của user khi đang ở chế độ test (mọi path qua `home` inject).

## Architecture
`crates/clawd-agents/src/{lib.rs, base.rs, json_edit.rs, plugin.rs, claude.rs, codex.rs, copilot.rs, cursor.rs, gemini.rs, opencode.rs, pi.rs, hooks.rs, normalize.rs, registry.rs}`.

## Related Code Files
- Own: `desktop-rs/crates/clawd-agents/**`, `desktop-rs/crates/clawd-testkit/src/agents.rs`, `tools/golden/export_agents.py`, `desktop-rs/fixtures/agents/**`
- Read-only: `clawd_desktop/agents/**`, `hooks.py`, `events.py`

## Implementation Steps
1. Exporter: với home tạm, chạy Python `install`/`uninstall` từng agent trên các config ban đầu (rỗng, có handler user, group dùng chung, file hỏng, Gemini không matcher, đường dẫn Windows) × platform `win32`/`darwin` → snapshot file trước/sau; payload mẫu từng agent → normalized events.
2. Port base/json_edit/hooks → golden install/uninstall.
3. Port 7 adapter + plugin + registry.
4. Port normalize từng adapter (dùng `clawd_events::claude_event`/`pi_event`); golden normalized.
5. Bảng mapping test (`test_agents` 24, `test_hooks` 10 — `test_hooks.py:170-190` test CLI Python `integrations/cli/clawd` → python-only).

## Todo
- [ ] Exporter + fixtures agents (2 platform)
- [ ] base/json_edit/hooks
- [ ] 7 adapter + plugin + registry
- [ ] normalize (NormalizeFn)
- [ ] Bảng mapping test

## Success Criteria
- [ ] `cargo test -p clawd-agents` xanh; golden trước/sau byte-khớp cho cả `win32` và `darwin`
- [ ] Rust gỡ sạch hook do Python 0.1.4 cài, giữ nguyên handler user (fixture riêng)
- [ ] File config không parse được → không bị ghi (test)

## Risk Assessment
- Thứ tự key / format JSON khác Python → diff không cần thiết trong config user. Tín hiệu: golden lệch whitespace. Phản ứng: so sánh golden theo giá trị JSON + 1 test byte-level cho Claude `settings.json` (indent của Python `_write`).
- Đường dẫn có dấu cách/Unicode trên Windows → test riêng.

## Security Considerations
- Chỉ ghi vào đúng file config đã biết; không theo symlink ra ngoài home; không thực thi gì từ config user.

## Next Steps
- 13 (`agents`, `set_agent`, `set_agents`, `disconnect_all`), 12 (rescan 600 s, cleanup CLI).
