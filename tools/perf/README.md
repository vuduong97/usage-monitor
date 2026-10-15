# tools/perf: repeatable performance measurement

Measures the numbers behind the Rust + Tauri port goals (`plans/261015-0726-rust-tauri-port/plan.md`,
Goals 1-4) for any tray app, so the Python 0.1.x baseline, the phase 01 spike and the final app (phase 14)
are measured the same way.

| Script | Platforms | Measures |
|---|---|---|
| `measure.ps1` | Windows 10/11, Windows PowerShell 5.1 (no extra modules) | Python app (`-Kind python`) and the Tauri spike or app (`-Kind tauri`) |
| `measure.sh` | macOS, Linux (bash, perl, curl) | the Tauri spike or app via its own log stamps. **Not run on macOS yet: deferred to phase 14.** |

Both print one JSON document and can write it with `-Out` / `--out`.

## Output

```json
{
  "summary": {
    "installer_bytes": 22352760,
    "idle_private_mb_10s": 39.0,
    "idle_private_mb_10m": 39.1,
    "tray_ms": 1620.1,
    "paint_cold_ms": 1556.6,
    "paint_warm_ms": 2099.3,
    "rss_after_50_cycles_mb": 39.3
  },
  "spread": { "paint_cold_ms": { "median": 0, "min": 0, "max": 0, "n": 3, "values": [0, 0, 0] } },
  "runs": [ { "run": 1, "load": {}, "idle_10s": {}, "window": {} } ]
}
```

`summary` holds medians over the runs; `spread` has min/max/values of each; `runs` has everything else
(private bytes, working set, WebView2 process counts, the spike's own timeline, CPU load at the start of the run).

| Field | Definition |
|---|---|
| `installer_bytes` | size of the file given with `-Installer`; `null` without it |
| `idle_private_mb_10s` / `_10m` | **private working set** of the main process (Task Manager's "Memory") at launch + 10 s and launch + 10 min, with no window ever opened. `runs[].idle_10s.private_bytes_mb` (commit) and `ws_mb` (incl. shared DLL pages) are recorded next to it |
| `tray_ms` | just before `CreateProcess` -> the process owns its tray window (class `*SystemTrayIcon` for pystray, `tray_icon_app` for tray-icon). It is the moment before `Shell_NotifyIcon`, a few ms early. Tauri runs also record `launch_to_main_ms` (loader, before `main`) and `main_to_tray_ms` (the spike's own stamps) |
| `paint_cold_ms` | trigger -> the first frame with page content, first window since launch |
| `paint_warm_ms` | median of the same over every later open in the open/close cycles |
| `rss_after_50_cycles_mb` | private working set 5 s after the last of 50 open/close cycles (`-Cycles`) |

How paint is seen: a probe thread calls `PrintWindow(PW_RENDERFULLCONTENT)` on the app's window every
~1 ms and calls the window painted when more than 0.4% of the client-area pixels differ from the dominant
colour (an empty window is one flat colour). That sees what the user sees, whatever the app does internally,
so Python and Tauri are comparable. `-DumpFramesDir <dir>` saves the first painted frame of each window as PNG
so the detector can be checked by eye. Trigger:

* Tauri spike: `GET /open` on the spike's control port, which calls the same `open_settings()` as the tray click
  (the click itself cannot be injected from outside); the timestamp is taken right before the request is written.
* Python: starts `Clawd.exe settings --piped` like the tray's "Cai dat..." item (the settings window is a
  separate process there, so every Python open is a cold process start; "warm" only means a warm file cache).
* Close: Tauri `window.close()` (CloseRequested, then destroy); Python `CloseMainWindow()`. After each close the
  script waits until no `msedgewebview2.exe` of the run is left (matched by the run's isolated user-data dir, so other
  apps' WebView2 processes are never counted) and records how long that took.

Not measured on this path: a real mouse click, a reboot-cold disk cache, Windows 10, mac. Tauri runs also do the
`SetProcessWorkingSetSize(-1,-1)` experiment at the end (`window.trim_after`, `trim_after_10s`);
`-TrimAfterDestroy` runs the app with trimming after every destroyed window.

## Safety: the app never sees your data

Every run uses its own directory under `%TEMP%\clawd-perf`: `USERPROFILE`, `HOME`, `APPDATA`, `LOCALAPPDATA`,
`TEMP`, WebView2 data all point into it, and environment variables that look like credentials
(`*TOKEN*`, `*SECRET*`, `*KEY*`, `ANTHROPIC*`, `CLAUDE*`, `OPENAI*`, `CODEX*`, `GITHUB*`, `GH_*`, `AWS_*` ...) are
removed. For the Python app the profile is preseeded with `desktop.json` saying the first run is done, because a
fresh profile would write the **real** `HKCU\...\Run\Clawd` autostart value. Not isolated: the Windows
Credential Manager (the Python app reads it only after it knows a clock; a fresh profile knows none) and the
network (the Python app polls GitHub unauthenticated for updates). The Python app listens on `CLAWD_EVENT_PORT`
(a per-run port), the spike on `CLAWD_SPIKE_PORT`, so neither collides with a real Clawd on 47913.

## Usage (Windows)

```powershell
# Python baseline: prebuilt PyInstaller app; -Installer only reads the file size
.\tools\perf\measure.ps1 -Kind python -Exe D:\vu\usage-monitor\dist\Clawd\Clawd.exe -Runs 3 -Out py.json

# Tauri spike (build first: cd desktop-rs\spike; cargo build --release)
.\tools\perf\measure.ps1 -Kind tauri -Exe desktop-rs\spike\target\release\clawd-spike.exe -Installer .\spike-setup.exe -Runs 3 -Out spike.json

# Quick smoke test (10 s idle, 3 cycles, 1 run, ~30 s)
.\tools\perf\measure.ps1 -Kind tauri -Exe ...\clawd-spike.exe -Runs 1 -Cycles 3 -IdleSeconds 10

# 10-minute idle of 3 instances side by side (they do not influence each other), then the window part
# one run after another, then one merged file
.\tools\perf\measure.ps1 -Kind tauri -Exe ... -Phase idle -Parallel -Runs 3 -Out idle.json
.\tools\perf\measure.ps1 -Kind tauri -Exe ... -Phase window -Runs 3 -Out window.json
.\tools\perf\measure.ps1 -Merge idle.json,window.json -Out all.json
```

Parameters: `-Runs` (default 3, each a fresh launch), `-Cycles` (50), `-IdleSeconds` (600; use 10 for smoke tests),
`-GapMs` (300 between cycles), `-Phase all|idle|window`, `-Parallel` (idle phase only; `tray_ms` is then not taken),
`-ExtraEnv @{ NAME = 'value' }` (extra environment for the app, e.g. `CLAWD_SPIKE_ARGS` experiments),
`-WorkDir`, `-BasePort`, `-TrayClassRegex`.

Default scripts-execution policy blocks unsigned files: run `powershell -NoProfile -ExecutionPolicy Bypass -File ...`.

## Reading the numbers: CPU contention

Timings move with machine load; `runs[].load` records CPU percent and the number of compiler/linker processes
at the start of each run (and `machine.load_start`/`load_end`). Repeat at least 3 times and quote the median with
the min-max spread. A reference figure should be taken with `build_like_procs` at 0 and CPU load under ~15%.
Idle memory is hardly load sensitive; tray and paint times are.

## Usage (macOS / Linux)

```sh
tools/perf/measure.sh --exe desktop-rs/spike/target/release/clawd-spike --runs 3 --out spike-mac.json
```

See the header of `measure.sh` for what differs from the Windows script (internal stamps instead of pixels,
`footprint` for memory).
