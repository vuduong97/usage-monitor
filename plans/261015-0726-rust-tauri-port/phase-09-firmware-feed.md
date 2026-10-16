---
phase: 9
title: "Firmware, feed, OTA, preview"
status: completed
priority: P1
effort: "2.5d"
dependencies: [2]
---

# Phase 09: Firmware bundle, feed, OTA, preview renderer (crate `clawd-firmware`)

## Overview
Port `firmware.py` (163), `feed.py` (467), `ota.py` (145), `preview.py` (382): firmware/style packs đi kèm, feed release (manifest v1 + `renderers[]`, grant AES-256-GCM, cache mã hoá), kế hoạch cài gói khác đồng hồ, OTA lên đồng hồ (giữ `pause.json` lease suốt quá trình), renderer C++ (subprocess) → APNG data URL. Giữ nguyên hành vi plan `261014-renderer-via-feed`.

## Context Links
- `clawd_desktop/{firmware,feed,ota,preview}.py`, `pause.py`, `plans/261014-renderer-via-feed/plan.md`
- scout §3 (feed endpoints, manifest, renderer platform keys `win-x64`/`mac`/`linux-x64`)
- [researcher-02 §AES-GCM, §subprocess](./research/researcher-02-crates-compat.md)
- Red team #3 (check_public), #6 (dep config), #9 (pause), #15 (redirect/https)
- Tests Python: `test_feed` (11), `test_firmware_ota` (8), `test_renderer_feed` (22), `test_preview` (32, phần preview/firmware), helper `tests/feedkit.py`

## Requirements
- Functional:
  - Feed: `GET /v1/latest?channel=<channel>` (Bearer), `POST /v1/grant {"seq":N}`; URL/token/channel từ env `CLAWD_FEED_URL`/`CLAWD_FEED_TOKEN`/`CLAWD_FEED_CHANNEL` → `clawd_config::read_config_value` (`feed_url`, `feed_token`, `feed_channel`) → build-time `option_env!`; không có → `from_settings()` = None. **Từ chối feed URL không phải `https://` hoặc loopback** (`feed.py:409`). Client `redirect::Policy::none()` như `follow_redirects=False` (`feed.py:266`).
  - `parse_manifest`: validate y hệt (`v==1`, giới hạn size, kinds, `enc = size+28`, `renderers` mảng riêng) → `FeedError`.
  - `decrypt(key, seq, name, blob)`: AES-256-GCM, nonce/AAD layout như `feed.py:170-178`, `aes-gcm` 0.11.
  - Cache mã hoá theo `seq` + verify sha256/crc; drop khi hỏng.
  - Renderer: tải zip theo platform, giải nén vào `~/.config/claude-usage-monitor/renderer/<sha16>/` + marker, verify hash mọi file trước mỗi lần load; bundle renderer (private build) thắng khi bundle là nguồn.
  - `pack_state`, `plan`, `needed`, `worth_it`, `offer`, `update_state`, `style_state`, `is_clawd`, `clock_seq`, `clock_style_api`, `clock_style`, `load_styles`, `load_bundle` (bundle trong resources app, chỉ build private).
  - OTA (`install`, `install_packs`, `install_release`, `install_then_save`, `_wait_crcs`) dùng `clawd_clock::ClockApi`; **`clawd_store::pause::hold(LEASE, reason)` và gia hạn mỗi tick `wait_back`, `release()` khi xong — kể cả lỗi/panic (guard `Drop`)** như `ota.py:28-37`, `:84-100`. `ota.busy` KHÔNG do crate này đặt (settings job ở phase 13 đặt, như `settings_api.py:612-644`).
  - Preview: `Renderer.draw/clip` chạy exe renderer với args như `_args`, timeout; `parse_clip`; `apng` (chunk PNG tự ghi, deflate `miniz_oxide`) → `apng_data_url`.
- Non-functional: tải streaming; giải nén chống path traversal.

## Architecture
`crates/clawd-firmware/src/{bundle.rs, styles.rs, state.rs, feed/{mod.rs, manifest.rs, crypto.rs, cache.rs, grant.rs}, renderer_store.rs, ota.rs, preview/{mod.rs, renderer.rs, clip.rs, apng.rs}}`; fakes (feed server) `testkit/src/fw.rs`.

## Related Code Files
- Own: `desktop-rs/crates/clawd-firmware/**`, `desktop-rs/crates/clawd-testkit/src/fw.rs`, `tools/golden/export_fw.py`, `desktop-rs/fixtures/fw/**` (**không** đặt tên thư mục `firmware` — `tools/check_public.py:13`)
- Read-only: `clawd_desktop/{firmware,feed,ota,preview}.py`, `tests/feedkit.py`

## Implementation Steps
1. Exporter (`_harness.py`, dùng `tests/feedkit.py`): manifest hợp lệ/không → parse/lỗi; blob mã hoá với key/seq/name biết trước (dữ liệu giả, nhỏ) → plaintext; heap mẫu → plan/needed/offer/state; clip bytes giả → frames + **sha256** của APNG (không lưu APNG/zip/pak — `check_public.py:9,14,16`). Zip renderer giả sinh trong test lúc chạy.
2. Port manifest/crypto/cache/state → golden.
3. Port feed client (https rule, no redirect) + renderer_store với fake feed server.
4. Port OTA với fake clock (`testkit::clock`), kiểm pause lease bật/gia hạn/tắt kể cả khi lỗi.
5. Port preview (renderer giả = script in clip).
6. Bảng mapping test (~60).

## Todo
- [ ] Exporter + fixtures `fw` (không nhị phân)
- [ ] manifest/crypto/cache/state
- [ ] feed client (channel, https, no-redirect) + renderer store
- [ ] OTA + pause lease guard
- [ ] preview + APNG
- [ ] Bảng mapping test

## Success Criteria
- [ ] `cargo test -p clawd-firmware` xanh; decrypt Rust giải blob Python (golden); APNG sha256 khớp
- [ ] `check_public.py` xanh với fixtures của phase
- [ ] Feed `http://` không phải loopback bị từ chối (test)
- [ ] OTA thật trên 1 đồng hồ (phase 14 xác nhận)

## Risk Assessment
- Sai nonce/AAD → mọi tải feed fail; golden decrypt là cổng.
- OTA brick đồng hồ nếu lệch trình tự → giữ y hệt Python, không "cải tiến".

## Security Considerations
- Token feed nhúng lúc build (secret), chỉ gửi qua https/loopback; verify sha256 trước khi gửi firmware/chạy renderer; giải nén an toàn.

## Next Steps
- 12 (feed check 60 s / 1800 s), 13 (`check_update`, `update`, `fw`, `job`, `clawd_pak`, `usage_preview`).
