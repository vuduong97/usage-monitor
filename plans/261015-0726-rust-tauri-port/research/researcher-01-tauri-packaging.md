# Research: Porting Clawd (pystray + pywebview + PyInstaller) to Tauri v2: tray-only runtime, packaging and size

Research date: 2026-10-15. Current stable `tauri` crate: **2.12.4** (crates.io API, `max_stable_version`, updated 2026-10-14; `3.0.0-alpha.5` also exists). The config reference already mentions behaviour for "Tauri 2.12.1+". Web search was limited to the Exa provider; the other providers were not configured. I fetched and read every primary source listed below directly. I did not use `source_check`.

## Summary
Tauri v2 can do this. Most of the targets look reachable:
- **Installer ≤ 8 MB:** likely. Official hello-world exe is about 2.8 MB, and NSIS with `downloadBootstrapper` adds 0 MB of WebView2.
- **Tray visible ≤ 1 s:** likely.
- **Idle RAM ≤ 20 MB, no window:** plausible, but only if no webview exists at idle.
- **Settings first paint ≤ 300 ms:** the riskiest target. WebView2 cold creation is a known bottleneck, and Tauri's own Windows benchmark measures about 0.4–0.7 s from launch to DOMContentLoaded.

Two packaging gaps need workarounds:
- Tauri cannot set a custom installer file name. Rename the file after the build.
- Tauri's NSIS per-user default path is `%LOCALAPPDATA%\<productName>`, not `%LOCALAPPDATA%\Programs\<productName>`. You need a custom NSIS template or a hook, or keep Inno Setup. Keeping Inno Setup means losing the turnkey Windows updater.

## Findings

### 1. Tray-only lifecycle (no windows, lazy create, destroy on close)
1. **Claim:** Use `.build(ctx)` and then `app.run(|h, e| …)`. On `RunEvent::ExitRequested { code, api }`, call `api.prevent_exit()`. `code` is `None` when the exit comes from the user (last window closed) and `Some(n)` when it comes from `app.exit(n)`. So prevent only when `code.is_none()`, and quitting from the tray still works. Leave `app.windows: []` in config so nothing is created at startup. **Sources:** [RunEvent docs](https://docs.rs/tauri/latest/tauri/enum.RunEvent.html), [App::run example](https://docs.rs/tauri/latest/tauri/struct.App.html), [ExitRequestApi](https://docs.rs/tauri/latest/tauri/struct.ExitRequestApi.html). **Support:** direct for the API and the `code: None` semantics; the `code.is_none()` pattern is my interpretation. **Confidence:** high.
2. **Claim:** Build windows lazily with `WebviewWindowBuilder` (or `from_config`). On Windows it **deadlocks if called from a synchronous command or event handler**, so use `async` commands or another thread. Labels must be unique, so check `get_webview_window(label)` before creating. **Sources:** [WebviewWindowBuilder docs](https://docs.rs/tauri/latest/tauri/webview/struct.WebviewWindowBuilder.html). **Support:** direct. **Confidence:** high.
3. **Claim:** Destroying the window instead of hiding it is the right way to reach low idle RAM. A Tauri 2 project merged exactly this change ("destroy the main WebView when closing to tray instead of hide"). It then calls `EmptyWorkingSet`/`SetProcessWorkingSetSize` on Windows and `malloc_zone_pressure_relief` on macOS to trim the working set. **Sources:** [cockpit-tools PR #686](https://github.com/jlcodes99/cockpit-tools/pull/686) (adopted in v1.3.1). **Support:** direct, but no numbers published. **Confidence:** medium.
4. **Claim:** Known close/destroy caveats, mostly historical:
   - Webview processes stayed alive after close in Tauri 1.x on macOS and Win10. Fixed in wry 0.22.1 ([#5397](https://github.com/tauri-apps/tauri/issues/5397)).
   - The WKProcessPool singleton kept the WebContent process alive ([wry #556](https://github.com/tauri-apps/wry/pull/556), [wry #587](https://github.com/tauri-apps/wry/pull/587)).
   - A window could stay listed after `close()`/`destroy()` in 2.0 betas ([#9353](https://github.com/tauri-apps/tauri/issues/9353)).
   - Closing the same window twice caused a panic. Fixed in 2.0.0-rc.16 ([#11159](https://github.com/tauri-apps/tauri/issues/11159), [release notes](https://v2.tauri.app/release/tauri/v2.0.0-rc.16/)).
   - Still open: repeated `evaluate_script` leaks memory in the webview process ([wry #1489](https://github.com/tauri-apps/wry/issues/1489)), and WebView2 grows with heavy IPC ([#4026](https://github.com/tauri-apps/tauri/issues/4026)).

   **Support:** direct. **Confidence:** medium. I found no current (2.12) evidence either way about leaks over many create/destroy cycles. This needs measuring.
5. **Claim:** Microsoft's guidance: each WebView2 control starts its own browser, renderer and GPU processes. Cold creation plus the first navigation is "a common performance bottleneck". Reusing one WebView is faster than destroying and recreating it, and you should share one environment. This works against "destroy on close" when it comes to first-paint latency. **Sources:** [WebView2 performance best practices](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/performance). **Support:** direct. **Confidence:** high.
6. **Claim:** macOS: hide the Dock icon with `app.set_activation_policy(ActivationPolicy::Accessory)` at runtime, or add `LSUIElement` to `src-tauri/Info.plist`. Tauri merges that file into the generated plist. **Sources:** [ActivationPolicy](https://docs.rs/tauri/latest/x86_64-apple-darwin/tauri/enum.ActivationPolicy.html), [macOS bundle docs](https://v2.tauri.app/distribute/macos-application-bundle/), [issue #6086](https://github.com/tauri-apps/tauri/issues/6086). **Support:** direct. **Confidence:** high. My inference: `LSUIElement` avoids a brief Dock-icon flash at launch, so use both.

### 2. Reported numbers
1. **Binary size:** the official `tauri_hello_world` release binary is **2,831,872 B on Windows** and **2,884,480 B on macOS**, from the tauri-apps/benchmark_results CI runs on 2026-10-12 to 2026-10-14. **Sources:** [benchmark_results](https://github.com/tauri-apps/benchmark_results/) (`tauri-recent-windows.json`, `tauri-recent-macos.json`). **Support:** direct. **Confidence:** high.
2. **Launch to DOMContentLoaded (hello world, includes process start, hyperfine with 3 warm-ups):**
   - Windows runner: mean about **0.42–0.67 s**, with outliers up to 6.5 s.
   - macOS runner: mean about 0.5–1.8 s.
   - `max_memory` is empty in these runs, so there are no official RAM numbers.

   **Sources:** same as above. **Support:** direct, but these are CI runners and not representative of desktops. **Confidence:** medium.
3. **Idle RAM, community reports:**
   - "Under 35 MB idle" for a Tauri 2 tray utility that keeps the webview unloaded until needed ([automatalabs](https://automatalabs.ca/blog/tauri-2-rust-windows-utility-footprint-taskbar-sentinel/); vendor marketing).
   - 30–50 MB idle with a ~6 MB installer for a clipboard manager ([dev.to](https://dev.to/max_nardit/i-ditched-electron-for-tauri-v2-building-a-6mb-native-feeling-clipboard-manager-in-react-rust-jco); webview likely loaded).

   **Support:** direct, but anecdotal and with no stated method. **Confidence:** low. I found no reliable number for the main process alone with no window.
4. **Slow WebView2 cases:** 2–3 s first load on some machines ([WebView2Feedback #1298](https://github.com/MicrosoftEdge/WebView2Feedback/issues/1298), [tauri discussion #10822](https://github.com/tauri-apps/tauri/discussions/10822)). Over 20 s when running elevated on specific Win10 machines ([#13727](https://github.com/tauri-apps/tauri/issues/13727), [WV2 #5300](https://github.com/MicrosoftEdge/WebView2Feedback/issues/5300)). **Support:** direct. **Confidence:** medium.
5. **WebView2 install mode, extra installer size** (official table):

   | Mode | Extra size | Notes |
   |---|---|---|
   | `downloadBootstrapper` | 0 MB | Default; needs internet |
   | `embedBootstrapper` | ~1.8 MB | |
   | `offlineInstaller` | ~127 MB | |
   | `fixedVersion`/`fixedRuntime` | ~180 MB | |
   | `skip` | 0 MB | |

   `bundle.windows.minimumWebview2Version` can force an update check. **Sources:** [Windows Installer docs](https://v2.tauri.app/distribute/windows-installer/). **Support:** direct. **Confidence:** high.
6. **NSIS vs MSI:**
   - NSIS defaults to LZMA compression, which is configurable as `bundle.windows.nsis.compression`.
   - MSI needs WiX v3, builds only on Windows, and is always per-machine style.
   - The docs publish no size comparison.
   - Interpretation: with a ~3–5 MB exe, an NSIS installer should land around 2–4 MB, well under 8 MB.

   **Sources:** [config ref](https://v2.tauri.app/reference/config/), [Windows Installer docs](https://v2.tauri.app/distribute/windows-installer/). **Support:** the size estimate is my inference. **Confidence:** medium.

### 3. Popover anchored to the tray
1. **Claim:** `tauri-plugin-positioner` with feature `tray-icon` provides `Position::TrayCenter`, `TrayBottomCenter` and similar. You must forward tray events with `tauri_plugin_positioner::on_tray_event(app, &event)` in `on_tray_icon_event`; it requires Rust ≥ 1.90. Tray events (`Click`, `DoubleClick`, `Enter`, `Move`, `Leave`) carry the icon `rect`, so you can also position the window manually. **Sources:** [Positioner](https://v2.tauri.app/plugin/positioner/), [System Tray](https://v2.tauri.app/learn/system-tray/). **Support:** direct. **Confidence:** high.
2. **Claim:** Popover window recipe: `decorations(false)`, `skip_taskbar(true)`, `always_on_top(true)`, `resizable(false)`, `visible(false)`. Show it after it is moved and the page is ready, and close it on `WindowEvent::Focused(false)`. A native tray menu (Tauri `Menu`, backed by muda) costs no webview and opens instantly. **Support:** interpretation of the docs plus common practice. **Confidence:** medium. Inference: keep the right-click menu native and use the webview popover only for rich content.

### 4. JS bridge
1. **Claim:** With `app.withGlobalTauri: true`, vanilla JS gets `window.__TAURI__.core.invoke(cmd, args)` and `window.__TAURI__.event.listen/emit`. No bundler is needed. **Sources:** [core API](https://v2.tauri.app/reference/javascript/api/namespacecore/), [Calling Rust](https://v2.tauri.app/develop/calling-rust/), [Calling the frontend](https://v2.tauri.app/develop/calling-frontend/). **Support:** direct. **Confidence:** high.
2. **Shim (my inference, not from a source):** pywebview passes positional arguments and Tauri passes a named-args object. Use one dispatcher command:
   ```js
   window.pywebview = { api: new Proxy({}, { get: (_, name) =>
     (...args) => window.__TAURI__.core.invoke('py_api', { name, args }) }) };
   window.dispatchEvent(new Event('pywebviewready'));
   ```
   On the Rust side, `#[tauri::command] async fn py_api(name: String, args: Vec<serde_json::Value>)` matches on `name`. This also keeps the capability/ACL surface down to one command.
3. **Claim:** CSP is only enforced if you set it. Tauri injects hashes and nonces at build time. The official example allows images with `img-src 'self' asset: http://asset.localhost blob: data:`. The asset protocol is off by default (`app.security.assetProtocol.enable` plus `scope`). **Sources:** [CSP](https://v2.tauri.app/security/csp/), [config ref](https://v2.tauri.app/reference/config/). **Support:** direct. **Confidence:** high. Inference: the simplest way to serve PNG bytes is to return base64 `data:` URLs from commands, or `tauri::ipc::Response` bytes turned into a `blob:` URL. You don't need the asset protocol unless you serve files from disk.
4. **Claim:** Origin differs by platform:
   - Windows uses `http://tauri.localhost`. Setting `useHttpsScheme` changes this, and changing it between releases changes IndexedDB/localStorage.
   - macOS uses `tauri://localhost`.

   The `identifier` sets the webview data directory on Windows. **Sources:** [config ref](https://v2.tauri.app/reference/config/). **Support:** direct. **Confidence:** high.

### 5. Packaging
1. **Claim: installer file name.** No config option exists. The name is always derived from `productName` (e.g. `Clawd_1.2.3_x64-setup.exe`). The feature request [#13999](https://github.com/tauri-apps/tauri/issues/13999) is **OPEN**, and the maintainers' answer is "rename it yourself" ([discussion #6417](https://github.com/orgs/tauri-apps/discussions/6417)). **Support:** direct. **Confidence:** high. Renaming after the build is fine, but the updater's `latest.json` must point to the renamed URL, and the `.sig` file signs the bytes, not the name.
2. **Claim: per-user install path.** The NSIS `installMode` default is `currentUser` (`RequestExecutionLevel user`, no admin). The template sets `$INSTDIR = "$LOCALAPPDATA\${PRODUCTNAME}"`, **not `…\Programs\…`**. It then restores the previous location from `HKCU\Software\<manufacturer>\<productName>` (default value). Uninstall writes to `HKCU\…\Uninstall\<productName>`. The uninstaller also deletes the `HKCU\…\Run` value named `${PRODUCTNAME}` (autostart cleanup) and offers to delete `%APPDATA%\<identifier>` and `%LOCALAPPDATA%\<identifier>`. **Sources:** [installer.nsi template](https://github.com/tauri-apps/tauri/blob/dev/crates/tauri-bundler/src/bundle/windows/nsis/installer.nsi), [Windows Installer docs](https://v2.tauri.app/distribute/windows-installer/). **Support:** direct. **Confidence:** high.
3. **Claim: ways to match `%LOCALAPPDATA%\Programs\Clawd`.**
   - (a) A custom template (`bundle.windows.nsis.template`, a copy of `installer.nsi` with the default path changed). This is the cleanest option.
   - (b) Hooks (`NSIS_HOOK_PREINSTALL`, `POSTINSTALL`, `PREUNINSTALL`, `POSTUNINSTALL`). PREINSTALL runs *after* `SetOutPath $INSTDIR`, so it is awkward for redirecting the path. It is useful for silently running the old Inno uninstaller (`unins000.exe /VERYSILENT`) to avoid duplicate Add/Remove Programs entries.
   - (c) Keep Inno Setup and ship the single Tauri exe, which has frontend assets embedded. Then `tauri-plugin-updater` on Windows can't be used as-is, because it expects NSIS/MSI artifacts and passes NSIS flags such as `/UPDATE`, `/P`, `/R`. You would need a custom updater.

   **Sources:** NSIS docs and template (above), [Updater](https://v2.tauri.app/plugin/updater/). **Support:** direct, plus my interpretation for (c). **Confidence:** medium.
4. **Claim: macOS.**
   - Build per arch with `tauri build --target aarch64-apple-darwin` and `--target x86_64-apple-darwin`. The DMG is named `<productName>_<ver>_<arch>.dmg` ([#13999](https://github.com/tauri-apps/tauri/issues/13999)).
   - For an ad-hoc signature, set `bundle.macOS.signingIdentity: "-"`. The docs note that Apple Silicon requires signing.
   - Default `minimumSystemVersion` is 10.13.
   - `identifier` must be unique; it drives the bundle ID and the webview data dir. Keep the existing Python app's CFBundleIdentifier so prefs, login items and TCC entries carry over.

   **Sources:** [macOS signing](https://v2.tauri.app/distribute/sign/macos/), [macOS bundle](https://v2.tauri.app/distribute/macos-application-bundle/), [config ref](https://v2.tauri.app/reference/config/). **Support:** direct. **Confidence:** high. Inference: ad-hoc signed apps get TCC grants tied to the code hash, so expect permission prompts again after each update. Downloaded DMGs still trigger Gatekeeper's "unidentified developer" warning.

### 6. Plugins
1. **Autostart** (`tauri-plugin-autostart`, wraps the `auto_launch` crate):
   - Windows: registers the exe path under the HKCU Run key, named after `app_name`, which defaults to `package_info().name`.
   - macOS: `MacosLauncher::LaunchAgent` is the default and writes a plist to `~/Library/LaunchAgents`. `AppleScript` adds a Login Item through System Events.
   - Supports `.args(["--autostart"])` and `.app_name("Clawd")`.

   Use the same name as the Python app's existing Run value so the setting carries over. Tauri's NSIS uninstaller already removes `Run\<productName>`. **Sources:** [autostart lib.rs](https://github.com/tauri-apps/plugins-workspace/blob/v2/plugins/autostart/src/lib.rs), [docs](https://v2.tauri.app/plugin/autostart/). **Support:** direct. **Confidence:** high.
2. **Single-instance:** the callback `init(|app, args, cwd| …)` fires in the first instance and the second instance exits. It has no JS API, so no capability is needed. For tray apps the callback should create or show the settings window, not `get_webview_window("main").unwrap()`, because that window may not exist. **Sources:** [Single Instance](https://v2.tauri.app/plugin/single-instance/). **Support:** direct, plus my inference. **Confidence:** high. Unverified: whether it must be registered as the first plugin. The plugin README says so, but I didn't re-check it.
3. **Updater:**
   - Needs `bundle.createUpdaterArtifacts: true` and a minisign keypair (`TAURI_SIGNING_PRIVATE_KEY`; `pubkey` is inline in the config).
   - Endpoints use HTTPS (enforced), with a static `latest.json` keyed `windows-x86_64` / `darwin-aarch64` / `darwin-x86_64`.
   - Windows `installMode`: `passive` (default), `basicUi`, or `quiet`. It **quits the app before installing** (`on_before_exit` hook).
   - macOS uses `.app.tar.gz` plus `.sig`.

   **Sources:** [Updater](https://v2.tauri.app/plugin/updater/). **Support:** direct. **Confidence:** high.

### 7. Size optimizations
1. **Claim:** The official recommended profile is `codegen-units = 1`, `lto = true`, `opt-level = "s"` (`"z"` is smaller and slower), `panic = "abort"`, `strip = true`. Add `build.removeUnusedCommands: true` to drop plugin commands that aren't allowed by the ACL. `build.windows.staticVCRuntime` defaults to true, so no VC++ redistributable is needed. **Sources:** [App Size](https://v2.tauri.app/concept/size/), [config ref](https://v2.tauri.app/reference/config/). **Support:** direct. **Confidence:** high.
2. **Claim (inference):** Expected exe size for Tauri + tokio + reqwest(rustls) + 3–4 plugins is about **4–7 MB** stripped, starting from the ~2.8 MB hello world. Tauri already depends on tokio, so its marginal cost is small. rustls plus a crypto provider and webpki roots plausibly adds about 1–2 MB. Using reqwest `native-tls` (SChannel/Security.framework) is probably smaller. I could not confirm whether the current reqwest/rustls versions default to `aws-lc-rs` (bigger, needs cmake/NASM on Windows) or `ring`. **Support:** inference only. **Confidence:** low. Measure it.

## Contradictions
- Idle-RAM claims range from "8 MB extreme" to "30–60 MB" to "89 MB" across community and SEO sources, with no stated method or with the webview loaded. I treated them as unreliable. The official benchmark has no memory data in its recent runs.
- Microsoft recommends reusing WebView2 instances for startup speed, while the low-idle-RAM goal requires destroying them. Those two targets pull against each other.

## Missing evidence
- No measured main-process RSS or private bytes for a Tauri 2.12 tray-only app with zero windows, on Windows or macOS. On macOS, linking AppKit and WebKit may make ≤ 20 MB hard; I don't know.
- No measured in-process (second or later) WebView2 / WKWebView window creation to first paint inside an already-running Tauri app.
- Whether the WebView2 browser process exits after the last Tauri window is destroyed in 2.12, and whether memory returns to baseline after N create/destroy cycles.
- Actual exe and installer size with reqwest/rustls. The current rustls crypto-provider default was not verified.
- Whether single-instance must be the first plugin registered (README, not re-checked).

## Sources
- Kept: Tauri docs: [Windows Installer](https://v2.tauri.app/distribute/windows-installer/), [App Size](https://v2.tauri.app/concept/size/), [System Tray](https://v2.tauri.app/learn/system-tray/), [Positioner](https://v2.tauri.app/plugin/positioner/), [Single Instance](https://v2.tauri.app/plugin/single-instance/), [Autostart](https://v2.tauri.app/plugin/autostart/), [Updater](https://v2.tauri.app/plugin/updater/), [macOS signing](https://v2.tauri.app/distribute/sign/macos/), [macOS bundle](https://v2.tauri.app/distribute/macos-application-bundle/), [CSP](https://v2.tauri.app/security/csp/), [Config reference](https://v2.tauri.app/reference/config/). These are the primary sources.
- Kept: [installer.nsi template](https://github.com/tauri-apps/tauri/blob/dev/crates/tauri-bundler/src/bundle/windows/nsis/installer.nsi), the ground truth for the install path, registry keys and the Run-key cleanup.
- Kept: [benchmark_results](https://github.com/tauri-apps/benchmark_results/), official binary size and timings from October 2026.
- Kept: [docs.rs RunEvent / App / WebviewWindowBuilder](https://docs.rs/tauri/latest/tauri/enum.RunEvent.html), the lifecycle APIs and the deadlock warning.
- Kept: [MS WebView2 performance](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/performance), the authoritative note on cold-start cost.
- Kept: [cockpit-tools PR #686](https://github.com/jlcodes99/cockpit-tools/pull/686), a real-world destroy-instead-of-hide change with memory trimming.
- Kept: GitHub issues [#13999](https://github.com/tauri-apps/tauri/issues/13999), [#5397](https://github.com/tauri-apps/tauri/issues/5397), [#11159](https://github.com/tauri-apps/tauri/issues/11159), [#9353](https://github.com/tauri-apps/tauri/issues/9353), [wry #1489](https://github.com/tauri-apps/wry/issues/1489).
- Kept: [autostart lib.rs](https://github.com/tauri-apps/plugins-workspace/blob/v2/plugins/autostart/src/lib.rs), the actual registration behaviour.
- Deprioritized: johal.in "Tauri 2.0 vs Electron 30" (SEO-style, unverifiable numbers), CrazySwami research.md (secondary aggregate), techxcelerate docs mirror (secondary), automatalabs and dev.to (anecdotal or marketing; cited as low confidence only).

## Recommendations for the plan
1. **Lifecycle:**
   - Use `app.windows: []`, build the tray in `setup`, and call `prevent_exit()` only when `code.is_none()`.
   - Create the settings window and popover lazily from async contexts. Let the default close destroy them.
   - After destroy, optionally trim the working set (`SetProcessWorkingSetSize(-1,-1)` on Windows).
   - Set `ActivationPolicy::Accessory` and `LSUIElement=true` on macOS.
2. **First paint:**
   - Create windows with `visible(false)` and a `backgroundColor` matching the UI. Show them on a `ready` event from JS or `DOMContentLoaded`.
   - Inline critical CSS and JS, with no bundler and no network fetches before first paint.
   - If the spike shows > 300 ms, fall back to keeping a hidden popover webview, or destroy it only after N minutes idle. That trades RAM for latency.
3. **Tray menu:** use a native menu (muda) for the right-click actions. Use the positioner `TrayBottomCenter`/`TrayCenter` webview popover only for left-click rich content, and close it on focus loss.
4. **Bridge:**
   - Enable `withGlobalTauri`. Add a single `py_api(name, args)` command plus the Proxy shim, which dispatches `pywebviewready`. Use events for push updates in place of pywebview `evaluate_js`; avoid frequent `eval`, given the wry leak report.
   - Set an explicit CSP with `img-src 'self' data: blob:` and serve images as data URLs.
   - Keep `useHttpsScheme` fixed forever.
5. **Windows packaging:**
   - Use NSIS with `installMode: currentUser`, `webviewInstallMode: downloadBootstrapper` (or `embedBootstrapper` if offline-first matters, +1.8 MB), and LZMA.
   - Use a **custom NSIS template** that defaults to `$LOCALAPPDATA\Programs\${PRODUCTNAME}`, plus a PREINSTALL hook that silently removes the old Inno install.
   - Rename the artifact in CI to `Clawd-X.Y.Z-windows-setup.exe` and point `latest.json` at it.
   - Keep `productName: "Clawd"`, so the Run value and uninstall key names line up.
   - Prefer this over keeping Inno, so `tauri-plugin-updater` works unchanged.
6. **macOS packaging:**
   - Build two targets (aarch64, x86_64) with `signingIdentity: "-"` and rename the DMGs in CI.
   - **Reuse the existing bundle identifier** as Tauri `identifier`.
   - Document the Gatekeeper and TCC re-prompt behaviour of ad-hoc signing.
7. **Plugins:**
   - single-instance: register it first; its callback opens or creates the settings window.
   - autostart: `.app_name("Clawd")`, `MacosLauncher::LaunchAgent`, `.args(["--autostart"])`, matching the existing Run name.
   - updater: `installMode: passive`, with a minisign key in CI secrets.
8. **Size:** use the official release profile with `opt-level="s"`; try `"z"` and compare. Add `removeUnusedCommands: true`. Prefer reqwest `native-tls` or `rustls` with `ring` after measuring. Track exe size in CI against a 6 MB budget.

## Risks / unknowns to measure in a spike
1. Idle main-process memory (working set and private bytes) with zero windows, at T+10 s and T+5 min, on Win10, Win11, macOS arm64 and macOS x86_64, against the ≤ 20 MB target. Measure before any window opens and after the open/close cycle.
2. Settings and popover time from tray click to first paint, cold (first window since launch) and warm (second or later). Compare WebView2 against WKWebView, and check whether the 300 ms target is met without keeping a hidden webview.
3. Memory and process count after 50 open/close cycles: does `msedgewebview2.exe` / WebContent exit, and does RSS return to baseline?
4. Exe and installer size with tokio + reqwest (rustls-ring vs aws-lc vs native-tls), opt-level s vs z.
5. Time from launch to tray icon visible (target ≤ 1 s), including `autostart` at login on a cold disk.
6. NSIS custom template: install, upgrade over an existing Inno install, uninstall, and `tauri-plugin-updater` passive update. Check that the install dir stays `%LOCALAPPDATA%\Programs\Clawd` and that there are no duplicate Add/Remove Programs entries.
7. pywebview shim parity: argument and return serialization, exceptions mapped to rejected promises, and `pywebviewready` timing in `setup.js` / `preview.js`.
8. macOS: ad-hoc signed per-arch DMG on a clean machine (Gatekeeper flow), LaunchAgent autostart, and whether TCC prompts reappear after an update.
9. Positioner on multi-monitor and mixed-DPI setups, with the Windows taskbar at top or left and the macOS menu bar on a secondary display.
