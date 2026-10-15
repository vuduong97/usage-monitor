# Perf baseline: Python Clawd vs Tauri 2.12 spike (phase 01)

Date: 2026-10-15. Branch `port/01-spike-baseline` (scripts `tools/perf/`, spike `desktop-rs/spike/`).
Machine: Windows 11 Pro 10.0.26200, Intel i5-10400 (12 logical), 16 GB, WebView2 runtime 155.0.4283.45.
Other agents compile concurrently on this machine; CPU load is quoted per run. Reference quiet figure = CPU 10-30%.

**Scope decisions made during the run (owner/supervisor):**
- Python window measurements (paint, 50 cycles) were **not measured by owner decision**; only the one smoke run, the
  idle numbers and the file sizes were collected before that decision.
- The spike window phase ran as one batch (+ one re-run after a measurement bug, see "Invalid or discarded data").
- macOS: **not measured, deferred to phase 14.** `tools/perf/measure.sh` is written and syntax-checked only.
- The prebuilt `dist/Clawd/Clawd.exe` is **0.1.3** (its log says "Clawd 0.1.3 starting"; source is 0.1.4). The baseline
  below is therefore 0.1.3; the installer size is the published 0.1.4 asset.

All spike numbers below are from `desktop-rs/spike` built with `cargo build --release` (opt-level s, lto, cgo 1,
panic unwind, strip, rustls ring), exe 8,399,360 B. Window/idle runs used `tools/perf/measure.ps1` (README there
defines every field). Raw JSON lived in `D:\vu\perf-work\` (scratch, not committed): key values are reproduced here.

## Headline table (Goals 1-4)

| Goal | Python 0.1.3 baseline | Rust spike | Threshold | Verdict |
|---|---|---|---|---|
| 1 installer | 22,352,760 B (published 0.1.4 asset) | **not built**, `iscc` absent: **estimate 3.0-4.8 MB, best guess ~3.5 MB** (see sizes) | <= 8 MB | pass (estimate) |
| 2 idle private, T+10 s | 38.67 MB (38.45-39.30, n=3) | **2.32 MB** (2.30-2.34, n=6) | <= 20 MB | pass |
| 2 idle private, T+10 min | not obtained (see below) | **0.87 MB** (0.82-1.53, n=3); private bytes 5.19 MB flat | <= 20 MB | pass |
| 3 paint cold (existing WebView2 profile) | not measured by owner decision (smoke: 1,557 ms, n=1, lower bound) | **692 ms median** (590, 676, 709, 890; n=4) | <= 300 ms; <= 600 ms accepted | **FAIL both** |
| 3 paint warm (2nd..51st open) | not measured by owner decision (smoke: 2,099 ms, n=2) | **632 ms pooled median** (p25 594, p75 734, n=100; batch medians 605 and 690) | same | **FAIL 300; at the edge of 600** |
| 3 paint first-ever open (fresh WebView2 profile) | n/a | **2,295 and 3,283 ms** (n=2) | same | FAIL |
| 4 tray | 1,620 ms (smoke, n=1, CPU 45%, builds running) | **72 ms median** (57-100 ms, n=6; main() starts 41-81 ms after launch) | <= 1 s | pass |
| 50 cycles | not measured by owner decision | main private WS 3.86 -> 4.75 MB, 0 WebView2 processes left, 0 cycles with leftovers | back to baseline | pass |

## Every number and how to reproduce it

Setup used below: `S=desktop-rs/spike/target/release/clawd-spike.exe` (copied to `D:\vu\perf-work\spike-main.exe`);
all `measure.ps1` calls via `powershell -NoProfile -ExecutionPolicy Bypass -File tools\perf\measure.ps1 ...`.

### Sizes

| Item | Bytes | Command |
|---|---|---|
| Python 0.1.4 installer (published) | 22,352,760 | `curl -s https://api.github.com/repos/vuduong97/usage-monitor/releases/tags/v0.1.4 \| python -c "import json,sys;[print(a['name'],a['size']) for a in json.load(sys.stdin)['assets']]"` |
| Python `dist/Clawd/Clawd.exe` / dist tree | 8,317,218 / 58,993,424 | `ls -l dist/Clawd/Clawd.exe`; `python D:\vu\perf-work\lzma_est.py dist/Clawd` (column 2) |
| Spike exe: s, unwind, ring (reference) | 8,399,360 | `cd desktop-rs/spike && cargo build --release` |
| s, unwind, **aws-lc** | 8,760,320 (+360,960, +4.3%) | `cargo build --release --no-default-features --features tls-aws-lc --target-dir target/aws-lc` |
| **z**, unwind, ring | 7,154,176 (-1,245,184, -14.8%) | `cargo build --profile release-z` |
| s, **abort**, ring | 4,870,656 (-3,528,704, -42.0%) | `cargo build --profile release-abort` |
| Mac | not measured, deferred to phase 14 | |

z, abort and aws-lc were built from the source before two later spike-only knobs (`CLAWD_SPIKE_QUIET`, `CLAWD_SPIKE_DATA_DIR`) were added;
those add a few env reads, not a measurable size change (the reference exe was rebuilt with them and is byte-identical in size).

### Installer size: ESTIMATE (no Inno Setup on this machine)

`where iscc` finds nothing, and neither `C:\Program Files (x86)\Inno Setup 6\ISCC.exe` nor `C:\Program Files\Inno Setup 6` exists.
Nothing was installed. `desktop-rs/spike/spike.iss` (LZMA2, solid, per-user) is committed so phase 11 can produce the real number.
Estimate = LZMA2 (python `lzma`, preset 9 extreme, x86 BCJ filter for exe/dll/pyd, 64 MB dictionary) of the payload plus Inno overhead:

| Payload | Raw | LZMA2 estimate | Command |
|---|---|---|---|
| Python `dist/Clawd` | 58,993,424 | 19,647,676 | `python D:\vu\perf-work\lzma_est.py D:/vu/usage-monitor/dist/Clawd` |
| Spike s/unwind/ring | 8,399,360 | 2,050,849 | `python D:\vu\perf-work\lzma_est.py spike-s-unwind-ring.exe` |
| Spike z/unwind | 7,154,176 | 1,848,837 | same, `spike-z.exe` |
| Spike s/abort | 4,870,656 | 1,594,499 | same, `spike-abort.exe` |

Calibration: the published Python installer is 22,352,760 B vs 19,647,676 B estimated, i.e. Inno stub + a less aggressive default
compressor cost +2,705,084 B (x1.138). Applied to the spike: lower bound 2.05 MB + ~1.0 MB stub = ~3.0 MB; upper bound if the whole
+2.7 MB were fixed overhead = 4.75 MB; best guess ~3.5 MB. **This is an estimate, not a measurement**; with an 8 MB budget the margin is >3 MB either way.
(NSIS/Tauri bundler was not tried; WebView2 bootstrapper adds 0 MB in `downloadBootstrapper` mode.)

### Idle memory (private working set of the main process, 0 windows ever opened)

| Metric | Python 0.1.3 | Spike |
|---|---|---|
| private WS @ T+10 s | 39.30, 38.67, 38.45 -> **38.67** (private bytes 42.4, WS incl. shared 63.8) | 2.32 (n=6: 2.30, 2.34, 2.36, 2.34, 2.32, 2.30) (private bytes 5.19, WS 14.8-15.8) |
| private WS @ T+10 min | not obtained | 0.87, 0.82, 1.53 -> **0.87** (private bytes 5.18/5.19/5.21, constant; WS 3.5-5.7) |
| after `SetProcessWorkingSetSize(-1,-1)` | n/a | 0.37-0.39 MB private WS, 1.1 MB WS, private bytes unchanged (+0.07) |

- Spike idle series (3 instances side by side, one sample per 30 s for 10 min): flat 2.32-2.38 MB until T+510 s, then Windows trimmed
  the idle processes by itself to 0.82-1.53 MB between T+510 s and T+540 s. Private bytes (commit) never moved from 5.18-5.21 MB.
  So "private WS" at T+10 min is lower than at T+10 s only because the OS trims; the conservative number is the 2.3 MB / 5.2 MB commit.
- Python: 3 instances started together; in two attempts all instances were terminated from outside at about T+7.5 min (16:46:56-16:47:11,
  and again 16:55:07-16:55:19, then ~17:03), so no Python T+10 min value exists. Python T+10 s is solid (n=3 plus the smoke run: 39.0).
- Reproduce spike idle: `measure.ps1 -Kind tauri -Exe D:\vu\perf-work\spike-main.exe -Phase idle -Parallel -Runs 3 -Out idle.json`
  (per-sample CSV: `%TEMP%\clawd-perf\idle-samples-tauri.csv`). Python: `measure.ps1 -Kind python -Exe D:\vu\usage-monitor\dist\Clawd\Clawd.exe -Phase idle -Parallel -Runs 3 -BasePort 47950 -Out py-idle.json`.
- Python profile was isolated and preseeded (`desktop.json` first_run_done) so the real `HKCU\...\Run\Clawd` was not written; the Python app
  downloaded update 0.1.4 from GitHub (unauthenticated) into the isolated profile at about T+95 s, which is part of its real idle behaviour.

### Tray time

| Run | tray_ms (launch -> tray window exists) | launch -> main() | main() -> tray build returned | CPU load at start |
|---|---|---|---|---|
| batch A r1 | 60 | 46.8 | 16 | 27% |
| batch A r2 | 59 | 41.1 | 25 | 21% |
| batch A r3 | 57 | 42.7 | 18 | 23% |
| batch B r1 | 100 | 80.9 | 25 | 32% |
| batch B r2 | 88 | 63.0 | 18 | 27% |
| batch B r3 | 83 | 62.9 | 22 | 23% |
| batch B r4 (**invalid**: CPU 99-100%, 12 compiler processes) | 445 | 345.4 | 46 | 99% |

Median of the six valid runs: **72 ms** (57-100). Python smoke: 1,620 ms (one run, CPU 45%, 27 cargo/rust processes elsewhere), so unreliable but the order of magnitude is clear.
Definition caveat: the moment is when the process owns its tray window (class `tray_icon_app` / pystray `*SystemTrayIcon`), a few ms before `Shell_NotifyIcon` completes.
Reproduce: `measure.ps1 -Kind tauri -Exe <spike> -Phase window -Runs 3 -Cycles 0 ...` (the `tray_ms` field).

### Paint (tray click -> first frame with page content)

Method: trigger `GET /open` on the spike control port (calls the same `open_settings()` as the tray click; a real mouse click cannot be injected),
timestamp taken right before the request is written, then `PrintWindow(PW_RENDERFULLCONTENT)` of the window ~every 1 ms; painted = >0.4% of client pixels differ from the
flat colour (checked by eye on saved frames: the real settings UI is on them). The window is created `visible(false)` + background `#141413`, shown when the page reports
`DOMContentLoaded` + 2 animation frames. The spike runs with `CLAWD_SPIKE_QUIET=1` (no focus stealing). UI = unmodified `clawd_desktop/ui/index.html`.

| Case | Values (ms) | Median | Load |
|---|---|---|---|
| cold, existing WebView2 profile (batch A r2, r3; batch B r2, r3) | 590, 890, 709 (708.6), 676 | **692** | CPU 21-28% at start |
| cold, existing profile, batch B r4 (**invalid**, excluded) | 4,961 | n/a | CPU 99-100%, 12-14 compilers |
| first-ever open, fresh profile (batch A r1, batch B r1) | 2,295, 3,283 | 2,789 | 27-32% |
| warm, batch A (50 opens, run 1) | median 605, p25 589, p75 648, min 550, max 2,420 | 605 | CPU 12-27% |
| warm, batch B (50 opens, run 1) | median 690, p25 602, p75 987, min 549, max 9,869 | 690 | CPU 18% at start; cycles 28-40 overlapped heavy builds elsewhere |
| warm pooled (100 opens) | p25 594, **median 632**, p75 734, p90 1,159 | 632 | 27 of 100 <= 600 ms; 71 of 100 <= 700 ms |

Where the time goes (spike's own stamps, ms after `open_requested`, warm opens): `window_built` +404 to +591 (the synchronous `WebviewWindowBuilder::build`: HWND + WebView2
environment/controller; browser process start), `ready` +539 to +810 (DOMContentLoaded 134-224 ms after navigation start, i.e. ~130-220 ms after `window_built`), `shown` +20-45 ms after `ready`.
So roughly 70-80% of the 600 ms is WebView2 creation before the page even starts loading. Cold (first window of the process, profile exists) adds 0-300 ms on top.
WebView2 profile location: without an explicit `data_directory` the profile was created next to the exe as `<exe>.WebView2\EBWebView`; the spike has `CLAWD_SPIKE_DATA_DIR`
and the script sets it. Reproduce (the one batch, quiet, 1 shared profile, run 1 = first-ever + 50 cycles):
`measure.ps1 -Kind tauri -Exe D:\vu\perf-work\spike-main.exe -Phase window -Runs 4 -Cycles 50 -CyclesLaterRuns 0 -Quiet -BasePort 47970 -SharedWebViewDir D:\vu\perf-work\wv2-shared -Out window.json`.
Python: smoke only (1 run, 2 cycles, CPU 45-80%): visible 1,520 ms / paint 1,557 ms cold, 1,702 and 2,496 ms warm (each Python open is a new process). **Not measured by owner decision; do not use as a baseline.**

### 50 open/close cycles (spike, batch B run 1; batch A run 1 identical in shape)

| Item | Batch A | Batch B | Command |
|---|---|---|---|
| main private WS after first close (3 s later) | 4.01 MB | 3.86 MB | `measure.ps1 ... -Phase window` (`window.after_first_close`) |
| private WS after cycles 10/20/30/40 | 4.95/4.30/4.26/4.50 | 4.51/4.96/4.88/4.96 | `window.trace_every_10` |
| **rss_after_50_cycles** (5 s after the last close) | **4.09 MB** (private bytes 8.07) | **4.75 MB** (private bytes 8.21) | `window.rss_after_50_cycles_mb` |
| baseline (idle before any window) | 2.30 MB (5.17) | 2.30 MB (5.17) | |
| msedgewebview2.exe of the run while one window is open | 6 processes, 82-86 MB private WS in total (batch B; batch A's check was vacuous, see below) | | `window.webview2_procs_open`, `webview2_private_ws_mb_open` |
| WebView2 processes left after a close | 0 after every one of 50 cycles | 0 after every one of 50 cycles | `window.wv2_cycles_not_exited = 0`, `wv2_procs_after_cycles = 0` |
| time until they have exited | 137 ms median (batch A: check vacuous) | **380 ms median** (cold 392; one cold close in batch B r2 took 1,112 ms) | `window.wv2_exit_ms_median` |

Conclusion: no leak visible over 50 cycles (private bytes 7.0 -> 8.2 MB, trend flat after cycle 10; the ~0.9-1.4 MB over the first-close value is
allocator/WebView2-host residue and does not grow). The main process returns to within ~2.5 MB of the idle baseline, not exactly to it.
The 82-86 MB of WebView2 child processes are outside "main process" Goal 2 but are RAM the user sees while Settings is open.

### SetProcessWorkingSetSize(-1,-1) experiment (spike `/trim`, on the settled process after 50 cycles, no window)

| | private WS | WS (incl. shared) | private bytes (commit) |
|---|---|---|---|
| before | 4.75 MB | 28.47 MB | 8.21 MB |
| immediately after | 0.48 MB | 1.21 MB | 8.21 MB |
| 10 s later | 0.66 MB | 1.75 MB | 8.21 MB |

Also applied to an idle instance (no window ever opened): private WS 2.32 -> 0.37-0.39 MB. It changes what Task Manager's "Memory" column shows, not the commit charge;
pages are faulted back in on use. The spike supports `CLAWD_SPIKE_TRIM=1` (trim after every destroyed window); that mode was not separately measured (no extra window runs).
Reproduce: `curl http://127.0.0.1:<CLAWD_SPIKE_PORT>/trim` on a spike started with `CLAWD_SPIKE_CONTROL=1`, or read the `window.trim_after*` fields of the window phase.

## Invalid or discarded data (and why)

- Batch A (first window run) used the old WebView2 detector: `webview2_procs_open = 0` was a **vacuous** check because Tauri put its profile next to the exe, not
  under the isolated LOCALAPPDATA. Its paint, tray, idle and RSS numbers are valid and are quoted above; its WebView2-exit numbers are not. Batch B (the one re-run) fixed this
  (profile directory forced to a directory of ours) and is the source for process counts and exit times.
- Batch B run 4 (cold, existing profile): invalid, other agents' builds had the CPU at 99-100% (paint 4,961 ms, tray 445 ms). Excluded from all medians.
- Batch B cycles ~28-40 (values 1.2-9.9 s) coincided with heavy builds elsewhere; they stay in the pooled warm figures (median 632 ms is robust to them; p90 1,159 ms is not).
- `summary.paint_cold_ms` in the script's own JSON (890 / 1,996 ms) is the median of all runs including the first-ever open and the invalid run; the table above is the corrected view.
- Three earlier attempts at the 10-minute idle (Python and spike together) were terminated from outside at ~T+7.5 min (the first two) and by a script bug (the 10-min data were not written);
  the third spike attempt completed and is the source above. Cause of the external terminations unknown (all instances at once, ~8 min apart; Python's log shows a clean "Stopped").

## Not measured

- macOS (all of it): **not measured, deferred to phase 14**.
- Python: paint cold/warm, 50 cycles, T+10 min idle, repeated tray time (owner decision / external termination).
- Real mouse click on the tray icon, Windows 10, a reboot-cold disk cache, Authenticode/AV scan effects, `CLAWD_SPIKE_TRIM=1` as a mode, WebView2 browser-argument experiments.
- Installer: real Inno/NSIS output (no `iscc`).
