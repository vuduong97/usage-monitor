# Perf final (phase 14): Rust app on Windows, 2026-10-16

Measured with `tools/perf/measure.ps1 -Kind python` against `Clawd.exe` built with
`cargo build --release -p clawd-app --features tauri/custom-protocol` (main `3e6aba6`-era tree,
before the tray fix below), isolated profile per run (own USERPROFILE/LOCALAPPDATA/port, `first_run_done`
preseeded), Windows 11, i5-10400. Raw JSON: `perf-window.json`, `perf-idle.json` (not committed).

## How the windows were opened (read before comparing with the spike)

The script opens Settings with `Clawd.exe settings --piped`: **a new process per open** (the settings-only
mode), like the Python app. It is not the in-process tray click the shipped app uses; a real click cannot be
injected from outside. So `paint_cold` here is a cold *process* start and `paint_warm` is the same repeated.
The in-process open (tray click) is not measured yet.

## Results

| run | CPU at start | build procs | paint cold (ms) | paint warm median (ms) | RSS after 50 cycles (MB) | WebView2 not exited |
|---|---|---|---|---|---|---|
| 1 | 100% | 15 | 2081 | 642 | 1.3 | 0 |
| 2 | 14% | 0 | 591 | 663 | 3.13 | 0 |
| 3 | 13% | 0 | 655 | 680 | 1.57 | 0 |

Run 1 started with 100% CPU and 15 build processes (workers compiling): its cold paint (2081 ms) is noise;
runs 2 and 3 were on a quiet machine (13-14% CPU).

| Goal | Threshold | Result | Verdict |
|---|---|---|---|
| 2. RAM idle, main process, 0 windows | <= 20 MB private | 3.0 MB at +10 s, 3.1 MB at +10 min (private bytes 6.2 -> 7.7 MB) | pass |
| 3. Settings click -> first paint | 300 ms, relaxed to <= 600-700 ms (owner, 2026-10-15) | cold 591 / 655 ms, warm median 663 ms (642-681 ms) | pass at the relaxed 0.7 s; not at 300 ms |
| 4. Tray after launch | <= 1 s | 1054-1089 ms (idle run 1089 ms) | **just over**; cause found and fixed, see below |
| 50 open/close cycles | no growth | 1.3 / 3.1 / 1.6 MB after 50 cycles, 0 WebView2 processes left | pass |
| 1. Installer | <= 8 MB | not measured: `iscc` is not installed here; `Clawd.exe` is 14.5 MB uncompressed | open |

## Tray: why it was ~1.07 s and what changed

The spike had the tray at 72 ms. The real app waited a fixed ~1 s before creating it: the single-instance
check `daemon_running` does an HTTP probe with a 1 s timeout, and on this machine **a connection to a closed
loopback port takes about 2 s to fail** (measured: 2121 ms for 127.0.0.1, 2021 ms for localhost) instead of
being refused at once, so on a normal start (no other daemon) it always waited out the timeout.
Fix (commit `3e6aba6`, `crates/clawd-events/src/probe.rs`): a 250 ms TCP connect first; the HTTP probe only runs
when something listens. A live daemon accepts a loopback connection immediately, so the single-instance
behaviour is unchanged. **Not re-measured yet** (needs a window launch; owner asked for no more disturbance).

## Not measured / open

* Tray after the fix; the in-process tray-click open; Windows 10; macOS (deferred to the CI/mac checks);
  Python baseline (owner decision 2026-10-15: no Python baseline, absolute numbers only).
* The first-ever WebView2 profile creation: the first window of a fresh profile took 3477 ms in the earlier
  run under build load (`windows.md` item); only users' very first open after install sees it.
