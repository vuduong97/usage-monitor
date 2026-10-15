# Spike decision (phase 01)

> ## WARNING: paint_cold is above 600 ms
> Settings open (tray click to first painted frame) on this machine: **cold 692 ms median (590-890 ms, n=4), warm 632 ms median (p75 734 ms, n=100)**,
> first-ever open with a fresh WebView2 profile **2.3-3.3 s**. The 300 ms target is out of reach with Design A and the 600 ms accepted ceiling is **not reliably met**
> (27 of 100 warm opens <= 600 ms). Per the plan this does **not** block phases 03-10; it blocks only the final design of 12/13. **Owner must be told.**
> About 70-80% of the time is WebView2 creation (`WebviewWindowBuilder::build` returns after 404-591 ms), before the page starts loading. See "Paint" below.

Numbers and reproduction commands: [perf-baseline.md](./perf-baseline.md). Everything below was measured on one Windows 11 machine (i5-10400, 16 GB, WebView2 155.0.4283.45) with other agents compiling in the background.

**Limits of this spike, stated up front**
- **There is no Python window baseline.** Owner decision: no Python window measurements. The 300/600 ms thresholds are therefore judged against absolute numbers only, not against the Python app.
  (Smoke-only, unreliable: Python settings paint 1.5-2.5 s.)
- **macOS: not measured, deferred to phase 14** (`tools/perf/measure.sh` written and syntax-checked, never run on a Mac). All conclusions below are Windows-only.
- Installer size is an **estimate** (no `iscc`).

## Decisions

| # | Question | Decision | Evidence |
|---|---|---|---|
| 1 | Design | **Design A confirmed** (lazy WebviewWindow, `visible(false)` + backgroundColor, show on JS ready, destroy on close, no hidden webview, no Windows popover). Main process idle 2.3 MB private WS / 5.2 MB commit; Goal 2 (<= 20 MB) has a >10x margin, so there is no RAM reason to hold a webview, and the owner ruled Design B out anyway. | idle table |
| 2 | Paint threshold | **Keep 600 ms as the ceiling, 300 ms is not attainable** with Design A (WebView2 creation alone is 400-590 ms). Measured median is 632-692 ms: **above the ceiling, flagged to the owner**. Phases 12/13 build to 600 ms and must not trade RAM for it. Cheap mitigations that do not cost RAM, **untested**: show the (dark, `#141413`) window at `window_built` instead of waiting for the page; keep the `ready` gate only for content; explicit `data_directory`; test WebView2 browser arguments (e.g. in-process GPU) with `CLAWD_SPIKE_ARGS`. Owner call: accept ~0.7 s on mid-range hardware, or allow one of those. | paint table |
| 3 | rustls provider | **ring.** reqwest 0.13 with `rustls-no-provider` + `rustls` feature `ring` + `rustls::crypto::ring::default_provider().install_default()`; certificate roots come from the OS (rustls-platform-verifier), no webpki-roots bundle. aws-lc is +360,960 B (+4.3%) for no feature the app needs. | sizes |
| 4 | NASM / CMake on CI Windows | **Neither is needed** with these versions. Both ring 0.17 and aws-lc-sys 0.46.0 built here on MSVC x86_64 with no `nasm` and no `cmake` on PATH (aws-lc-sys compiled its C sources with the `cc` crate: hashed `.o` files, and used its prebuilt NASM objects). Not verified: Windows aarch64, gnu targets, macOS (phase 14). `windows-latest` has both tools anyway. | aws-lc build output dir |
| 5 | opt-level | **`s`.** z saves 1,245,184 B of exe (-14.8%), about 0.2 MB of installer (LZMA2 estimate 2.05 -> 1.85 MB), irrelevant against an 8 MB budget; z's speed cost was not measured and paint is dominated by WebView2. Revisit only if the real app's installer is within ~1 MB of 8 MB. | sizes |
| 6 | panic | **`unwind`** (red team #12; `catch_unwind` in the engine). abort would save 3,528,704 B of exe (-42%), LZMA2 estimate 2.05 -> 1.59 MB (-0.46 MB installer), again irrelevant to the budget. The unwind exe is 8.4 MB raw, the installer is not. | sizes |
| 7 | WebView2 handling | See below. | cycles table |

### Size and the 8 MB goal
Spike exe (s, unwind, ring): 8,399,360 B; LZMA2 estimate 2.05 MB; installer **estimate 3.0-4.8 MB, best guess ~3.5 MB** (calibrated against the published Python installer, 22,352,760 B vs 19.65 MB estimated). **Estimate, not measured.**
The real app adds code (usage, agents, firmware, clock) and assets, so the margin of >3 MB is a spike margin; re-measure in phase 11 with `desktop-rs/spike/spike.iss` as the template.

### Paint (tray click to first frame with page content)
- cold, existing profile: 590, 676, 709, 890 ms (median 692); one more run at CPU 100% (4,961 ms) discarded.
- warm: batch medians 605 ms and 690 ms; pooled median 632 ms, p25 594, p75 734, p90 1,159 (the tail is background builds).
- first-ever open (fresh WebView2 profile): 2,295 and 3,283 ms. This happens once per install (or after the user deletes the profile). Mitigation without RAM cost: create the profile
  early in a window-less way is not possible; show a native "opening" state on the tray menu or accept it once. Not tested.
- Note: spike used the real, unmodified `index.html`; a Tauri shim and `py_api` calls in the real UI add JS work after first paint, not before.

### WebView2 handling (recommendations for phases 11-13)
- Runtime assumed present (Win11 ships it; Python does the same; installer does not bundle it, per plan non-goals).
- **Always set `data_directory`** on the window builder. Without it the spike's profile went to `<exe>.WebView2\EBWebView` next to the exe, which for `%LOCALAPPDATA%\Programs\Clawd` pollutes the install dir and leaves data after uninstall.
  Use a directory under the app's local data (identifier `vn.clawd.desktop`) and keep it forever (a fresh profile costs +1.6-2.6 s on first open).
- While Settings is open: 6 `msedgewebview2.exe` processes, 82-86 MB private WS in total, on top of the main process. They are not part of Goal 2 as written ("process chinh") but are what the user sees in Task Manager; say so in perf-final.
- After close (destroy): all of them exit, 0 left after every one of 50 cycles, 137-392 ms median (one cold close 1.1 s). Main process private WS 4.0 MB after the first close, 4.1-4.8 MB after 50 cycles (baseline 2.3 MB), private bytes 7.0 -> 8.2 MB, flat after cycle 10: no leak seen.
- `SetProcessWorkingSetSize(-1,-1)` after destroy: private WS 4.75 -> 0.48 MB (0.66 MB 10 s later); commit unchanged (8.21 MB); also takes an idle instance from 2.32 to ~0.38 MB. It improves the Task Manager figure only, at the price of page faults afterwards. Cheap and harmless: **adopt it after window destroy and once ~30 s after start-up**, but do not count it toward the Goal 2 proof; quote commit (5.2 MB idle) as well.
- Windows trims idle processes by itself: the 3 idle spike instances dropped from 2.32 to 0.82-1.53 MB private WS at about T+9 min. Goal 2 at T+10 min is therefore met with an even larger margin, but quote the T+10 s / commit values as the conservative ones.

## What the spike proves and what it does not
- Proves: Tauri 2.12 tray + `prevent_exit` on `code.is_none()` + lazy window + destroy-on-close + loopback hyper http1 + reqwest/rustls all coexist; tray in 57-100 ms (Goal 4: <= 1 s); idle 2.3 MB (Goal 2).
- Does not prove: Goal 3 (fails here), installer size (estimated), anything on macOS, anything about the tray click path with a real mouse, Python-relative improvement.
- Spike limits: the click is simulated by a loopback `GET /open` calling the same `open_settings()`; no real tray menu interaction was automated.

## Deviations and caveats (full list in perf-baseline.md)
- The prebuilt `dist/Clawd/Clawd.exe` is 0.1.3, not 0.1.4.
- Python measurements were cut by owner decision; Python T+10 min idle was never obtained (instances killed from outside).
- Two spike window batches; the first used a vacuous WebView2 process check (fixed in the second); one run in the second batch discarded for CPU saturation.
- CPU contention from other agents' builds is visible in the tails; nothing was re-run to "improve" numbers.
