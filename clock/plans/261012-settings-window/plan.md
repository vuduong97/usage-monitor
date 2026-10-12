# Plan: Clawd settings window (B · Studio + Usage page F1 + OTA in the app)

Source: the ak:advise session of 2026-10-12. Preview: `plans/reports/261012-design-settings-window-preview/`.
Repos: `clawd-desktop` (app), `clawd-home/smalltv` (firmware).

## Contract
- **Done means**
  - `Clawd --settings` opens a pywebview window in its own process. The process exits when the window closes.
  - Tabs: Hiển thị, Usage, Wi‑Fi, Cập nhật, App. Each save is 1 `POST /api/config` that carries only the diff.
  - The digest password goes into the OS keyring.
  - Choosing layout D on a clock with old firmware makes the app OTA the bundled `.bin`. Afterwards `/api/heap` reports `fw` and `layouts`, and the clock shows D.
  - Firmware ≤ 524,288 B.
  - RAM and CPU with the window closed are unchanged. The app grows ≤ 10 MB.
  - The existing tests pass, plus new tests for clockapi and the OTA flow.
- **Out of scope:** WireGuard, auth UI, the AP first-setup wizard, removing the web UI, GitHub download, layouts A/B/C/E, the tray popover, themes (stock clock faces).
- **Constraints:** keep the Python engine and pystray; plain HTML/CSS/JS; the engine pauses pushes during save/OTA; the clock's web UI stays as the fallback.

## Phases
1. **Firmware: layout D + capability report**
   - `usage.layout` (0 = classic, 1 = pace). An unknown value draws classic.
   - `/api/heap`: `fw` (build id) and `layouts` (bitmask).
   - D renderer: cards, a pace pill, a time marker and a forecast line. The status line shows only during alerts.
   - Verify: the `tools/clawd_preview` PNGs, a size check, and a flash to the real clock.
2. **clockapi.py:** a sync httpx client with DigestAuth, a cached challenge, retry/backoff, `config_diff`, OTA upload plus a reboot wait, and keyring creds. Unit tests use MockTransport.
3. **Engine pause:** `pause.json` (`until`, `reason`) in CONFIG_DIR. `finder.targets` returns [] while it is valid, so every pusher idles. Afterwards BootWatch resends after the OTA reboot. Tests included.
4. **Settings window:** `cli settings` with a single-instance lock (a pid file). The tray item "Cài đặt…" spawns `sys.executable` (frozen) or `-m clawd_desktop settings`. pywebview loads `clawd_desktop/ui/index.html`, and the js_api bridge wraps clockapi plus the app prefs.
5. **UI (B · Studio)**
   - Tabs: Hiển thị (brightness, auto, rotation, night, tz, main screen), Usage (layout gallery F1 gated by `layouts`, with the preview canvas), Wi‑Fi (list/add/remove/reorder, scan, hostname, AP), Cập nhật (firmware status/OTA, export/import, reboot, factory), App (autostart, hooks, clock address, log).
   - A Save bar that diffs.
6. **Packaging:** pywebview/keyring in the spec, the firmware `.bin` and `firmware.json` (fw id, layouts) bundled as data, `tools/bundle_firmware.sh`. Then build the mac app and measure size and RAM.
7. **Verify end to end** on the clock 10.14.237.36: open the window, save brightness (1 POST), switch to layout D through OTA, check that the clock shows D. Then review, docs, commit.

## Failure Protocol
On any failed verification: stop, consult the advisor with the command, the output and what was tried, then fix.
