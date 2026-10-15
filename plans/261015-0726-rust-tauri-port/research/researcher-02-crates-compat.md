# Research: Rust crate choices & on-disk/OS-store compatibility for the Python→Rust (Tauri/tokio) port

Date: 2026-10-15. Crate versions are from the crates.io API on this date.

## Summary
You can read and write the Python app's existing secrets, prefs and autostart entries from Rust, but two defaults don't match. **Windows:** the `keyring` crate's default Windows target name is `"{user}.{service}"`, while Python writes `"{service}"` (or `"{username}@{service}"` when two users collide). So Rust must pass an explicit target or call `CredReadW`/`CredWriteW` directly. **macOS:** the Keychain naming is the same in both (`service` + `account`). The new risk there is the Keychain access prompt (ACL), not the item name. The other parts are safe defaults: aes-gcm 0.11 (byte-compatible with Python `AESGCM`), reqwest 0.13 (now uses rustls by default) plus diqwest 3.2, mdns-sd 0.21, fs4 or std file locks with tempfile, and auto-launch 0.6 configured with the same names Python used.

## Findings

### 1. Keyring / credential stores
1. **Claim:** Python `WinVaultKeyring` stores a `CRED_TYPE_GENERIC` credential with `TargetName=service`, `UserName=username`, `Comment="Stored using python-keyring"` and `Persist=CRED_PERSIST_ENTERPRISE`. If a credential with a different `UserName` already exists under `service`, Python moves the old one to `TargetName="{old_username}@{service}"` and writes the new one at `service`. To read, it tries `service` first, and falls back to `"{username}@{service}"` when the target is missing or `UserName` doesn't match. It reads the blob as UTF-16 and falls back to UTF-8; it only ever writes UTF-16. **Sources:** [keyring/backends/Windows.py](https://raw.githubusercontent.com/jaraco/keyring/main/keyring/backends/Windows.py). **Support:** direct evidence (source code). **Confidence:** high.
2. **Claim:** In Rust `keyring` 4.2.0 (default `v1` feature = the old v3-style API; it is built on `keyring-core` plus `windows-native-keyring-store` 1.1.0), the default target is `{prefix}{user}{delimiter}{service}{suffix}`, which with defaults is `"user.service"`. An explicit `target` modifier is used exactly as given. The default persistence is Enterprise. Passwords are stored as UTF-16 blobs "for interoperability with native Windows credential APIs". **Sources:** [windows_native_keyring_store docs](https://docs.rs/windows-native-keyring-store/latest/windows_native_keyring_store/), [cred.rs](https://raw.githubusercontent.com/open-source-cooperative/windows-native-keyring-store/main/src/cred.rs), [crates.io keyring](https://crates.io/api/v1/crates/keyring). **Support:** direct. **Confidence:** high. The last 3.x release is 3.6.3 (2025-07-27).
3. **Claim:** **Trap.** When an entry is created with an explicit `target`, `specifiers` is `None`. Writing a *new* credential through it therefore sets `UserName=""`; it only keeps the existing `UserName` if the credential already exists. Python would then see `UserName != username`, fall back to `user@service`, and miss the secret. **Sources:** [cred.rs `build_from_specifiers`/`set_secret`](https://raw.githubusercontent.com/open-source-cooperative/windows-native-keyring-store/main/src/cred.rs). **Support:** interpretation of source. **Confidence:** medium-high. **Fix:** call `update_attributes({"username": user})` after the first write, or (simpler and exact) write with `windows` crate `CredWriteW` yourself, setting the same fields Python does.
4. **Claim:** Python's macOS backend calls `set/find_generic_password(keychain, service, username)` with `username=''` when it is None. That is a generic-password item with `kSecAttrService=service` and `kSecAttrAccount=username`, which is the same mapping as the Rust apple-native store and `security-framework`'s `get_generic_password(service, account)`. **Sources:** [keyring/backends/macOS/__init__.py](https://raw.githubusercontent.com/jaraco/keyring/main/keyring/backends/macOS/__init__.py). **Support:** direct for Python; Rust mapping is from crate docs/knowledge (not re-fetched). **Confidence:** high (Python), medium (Rust detail).
5. **Claim:** Claude Code keeps OAuth credentials on macOS in the Keychain generic-password item `Claude Code-credentials`. Claude Code reads it by shelling out to `/usr/bin/security`, and the item is created **without a trusted-application list**, so other readers get password prompts. "Always Allow" reportedly doesn't stick, and many items with that service prefix can pile up. The file `~/.claude/.credentials.json` can exist on macOS and disagree with the Keychain; it is the store used on Windows/Linux. **Sources:** [claude-code#77697](https://github.com/anthropics/claude-code/issues/77697), [#89985](https://github.com/anthropics/claude-code/issues/89985), [#91180](https://github.com/anthropics/claude-code/issues/91180), [#91158](https://github.com/anthropics/claude-code/issues/91158), [#10039](https://github.com/anthropics/claude-code/issues/10039). **Support:** direct (issue reports, not official docs). **Confidence:** medium.
   - *Inference:* reading this item in-process from Rust (`security-framework` 3.7.0) will also prompt. The Rust binary is a new code identity, so any "Always Allow" the user gave the Python/`security` binary does not carry over. Match whatever the Python app does today (shell `security find-generic-password -s "Claude Code-credentials" -w` vs. the Security API) so users don't see a new kind of prompt. If several items match the service prefix, pick by exact service name or the most recent modification date.
   - The same ACL issue applies to the app's *own* Python-created keyring items: the new Rust binary will be asked for access once. Code-sign it with a stable identity so "Always Allow" survives updates (inference).

### 2. mDNS browse
6. **Claim:** `mdns-sd` 0.21.5 (2026-10-05) is pure Rust and runtime-agnostic. It runs its own daemon thread and delivers events over flume channels, which you can `.recv_async()` from tokio. It supports querier and responder roles on Windows, macOS and Linux, over IPv4 and IPv6. Dependencies are socket2, mio, if-addrs and flume. It describes itself as "beta". **Sources:** [README](https://raw.githubusercontent.com/keepsimple1/mdns-sd/main/README.md), [deps](https://crates.io/api/v1/crates/mdns-sd/0.21.5/dependencies). **Support:** direct. **Confidence:** high.
7. **Claim:** Coexisting with mDNSResponder/Bonjour or the Windows built-in mDNS (Win10 1703+) on UDP 5353 needs shared binding (SO_REUSEADDR/REUSEPORT). On Windows, SO_REUSEADDR sharing is "indeterminate" by design. Windows-specific "can't receive responses" reports exist for mdns-sd, and the maintainer could not reproduce them. **Sources:** [MS SO_REUSEADDR](https://learn.microsoft.com/en-us/windows/win32/winsock/using-so-reuseaddr-and-so-exclusiveaddruse), [MS mDNS in the enterprise](https://techcommunity.microsoft.com/blog/networkingblog/mdns-in-the-enterprise/3275777), [mdns-sd#33](https://github.com/keepsimple1/mdns-sd/issues/33). **Support:** direct + interpretation. **Confidence:** medium.
   - *Inference:* the Windows Defender Firewall "allow access" prompt is tied to the executable path, and the first inbound UDP listener triggers it. The Python app's rule was for `python.exe` or the frozen exe, so the Rust exe will prompt again. Consider adding a per-user-safe rule from the installer (`netsh advfirewall firewall add rule ... program=<exe> protocol=UDP localport=5353`). Keep a fallback path: manual IP entry, or unicast HTTP probe of the last known IP.
   - Alternatives: `zeroconf`, `astro-dnssd` (wrap Bonjour/Avahi; need the Bonjour SDK on Windows, so not recommended), `simple-mdns`, `libmdns` (responder only). mdns-sd is the practical default. Not benchmarked here.

### 3. AES-GCM
8. **Claim:** Python `cryptography` `AESGCM(key).encrypt(nonce, data, aad)` returns `ciphertext || 16-byte tag` with a 12-byte nonce. RustCrypto `aes-gcm` 0.11.1 `Aes{128,256}Gcm::decrypt(nonce, Payload { msg: ct_with_tag, aad })` expects exactly that layout. Pick the cipher type by key length (16 → Aes128Gcm, 32 → Aes256Gcm). **Sources:** [crates.io aes-gcm](https://crates.io/api/v1/crates/aes-gcm). The layout claim comes from well-known API docs and was not re-fetched. **Support:** interpretation. **Confidence:** high.
   - Note: aes-gcm 0.11 moved to `hybrid-array`. `Nonce::from_slice`/`GenericArray` code from 0.10 tutorials won't compile unchanged. Use `Nonce::try_from(&bytes[..12])` or the 0.11 equivalent. Add a golden test using a Python-generated vector.

### 4. Local HTTP server (127.0.0.1 hook endpoint)
9. **Claim:** axum 0.8.9 (tokio/hyper) vs. tiny_http 0.12.0 (last released 2022-10, synchronous, thread-per-request) vs. raw hyper 1.x. **Sources:** [crates.io axum](https://crates.io/api/v1/crates/axum), [crates.io tiny_http](https://crates.io/api/v1/crates/tiny_http). **Support:** direct for versions. **Confidence:** high.
   - *Inference (size):* reqwest already pulls in hyper, http, tower and tokio, so adding axum costs little (roughly a few hundred KB in release; not measured). tiny_http only saves size if you drop reqwest, which you won't. Use axum with `default-features=false, features=["tokio","http1","json"]`, bind to `127.0.0.1:<same port as Python>`, and set a body-size limit. Keep the same route paths, JSON shapes and port so existing Claude Code hook configs (curl commands in settings.json) keep working.

### 5. HTTP client to the ESP32
10. **Claim:** In reqwest 0.13.5, **rustls is the default TLS backend**, the crypto provider defaults to **aws-lc-rs**, and roots come from `rustls-platform-verifier` (0.7.1), i.e. the OS trust store. The `rustls-tls` feature was renamed `rustls`, and `rustls-no-provider` lets you choose another provider (e.g. ring). **Sources:** [reqwest v0.13.0 release](https://github.com/seanmonstar/reqwest/releases/tag/v0.13.0), [blog](https://seanmonstar.com/blog/reqwest-v013-rustls-default/), [docs](https://docs.rs/reqwest/latest/reqwest/). **Support:** direct. **Confidence:** high.
11. **Claim:** aws-lc-rs on `x86_64-pc-windows-msvc` needs a C compiler and *recommends* NASM (it can be avoided). There is an open issue about CMake still being required on Windows; another page says non-FIPS builds never need CMake. **Sources:** [aws-lc-rs Windows reqs](https://aws.github.io/aws-lc-rs/requirements/windows), [platform support](https://aws.github.io/aws-lc-rs/platform_support.html), [aws-lc-rs#828](https://github.com/aws/aws-lc-rs/issues/828). **Support:** direct, but the sources conflict. **Confidence:** medium.
12. **Claim:** Basic auth is built in (`RequestBuilder::basic_auth`). For digest auth, `diqwest` 3.2.0 (2026-02) depends on `reqwest ^0.13` and `digest_auth ^0.3` (digest_auth 0.3.1 dates from 2023 and is stable). **Sources:** [diqwest deps](https://crates.io/api/v1/crates/diqwest/3.2.0/dependencies), [digest_auth](https://crates.io/api/v1/crates/digest_auth). **Support:** direct. **Confidence:** high.
   - The ESP32 is almost certainly plain `http://` on the LAN, so TLS barely matters for it. *Inference:* the ESP32 web server (esp-idf httpd / ESPAsyncWebServer) may use MD5 digest with `qop=auth`, which digest_auth supports. Set short timeouts and `pool_max_idle_per_host(0)`, because ESP32 servers handle keep-alive poorly.

### 6. Cross-process lock + atomic write
13. **Claim:** Current versions: `fs4` 1.1.0 (2026-04), `fd-lock` 4.0.4, `tempfile` 3.27.0. `std::fs::File::lock`/`try_lock` are also stable in recent Rust; I believe since 1.89, but did not re-verify. **Sources:** crates.io API ([fs4](https://crates.io/api/v1/crates/fs4), [fd-lock](https://crates.io/api/v1/crates/fd-lock), [tempfile](https://crates.io/api/v1/crates/tempfile)). **Support:** direct for versions. **Confidence:** high for versions, medium for the std-lock version.
   - *Inference / compatibility:* a Rust lock only interlocks with Python if both use the **same primitive on the same file**. Python `filelock` uses a sidecar `X.lock` file: `msvcrt.locking` on Windows (a byte-range lock via LockFile), `fcntl.flock` on Unix. fs4 and std use `LockFileEx` (byte-range, starting at offset 0, so it overlaps with msvcrt) on Windows and `flock` on Unix. So lock the **same sidecar file** Python locks, never the JSON file itself. Check what the Python code actually does first; if it doesn't lock at all, the user-upgrades-in-place scenario doesn't need interop. This only matters if both versions can run at once, e.g. during the upgrade.
   - Atomic write: `NamedTempFile::new_in(same_dir)` → write → `sync_all` → `persist(target)`. On Windows, persist uses `MoveFileExW(REPLACE_EXISTING)`, the same as Python `os.replace`. It fails with ERROR_ACCESS_DENIED / sharing violation if another process has the target open without FILE_SHARE_DELETE (Python `open()` doesn't use it), or while AV/indexers hold it. Retry persist with backoff (e.g. 5×50 ms). Keep the JSON formatting byte-identical (indent, key order, `ensure_ascii`) if diffs or users care: serde_json `PrettyFormatter::with_indent(b"    ")` or 2 spaces, plus `preserve_order` (indexmap) and an ASCII-escaping formatter to match Python's `ensure_ascii=True` default.

### 7. Golden / differential testing
14. **Claim:** `insta` 1.49.0 (features `json`, `glob`), with `serde_json` and `glob!()` to run every fixture file. Alternatives: `datatest-stable` or `rstest` `#[files]`. **Source:** [crates.io insta](https://crates.io/api/v1/crates/insta). **Support:** direct for version; the approach is researcher recommendation.
   - Approach: a Python script (run in CI or by hand) writes `tests/fixtures/<case>/{input.*, expected.json}` from the *real* Python functions: usage parsing, AES vectors (key, nonce, aad, ct), prefs migration, keyring target resolution and mDNS TXT parsing. Rust tests deserialize `input`, run, and `assert_eq!(serde_json::Value)` against `expected`. Normalize floats/timestamps before comparing. Use insta snapshots only for Rust-only outputs. Check the fixtures into git so Rust CI doesn't need Python. Also do "old-file" tests: real prefs/state files saved from older Python versions must load without loss.

### 8. C++ helper exe emitting PNG frames
15. **Researcher recommendation (not from fetched sources):**
   - Run the helper with `tokio::process::Command`, `stdout(Stdio::piped())` and `kill_on_drop(true)`. On Windows, set `creation_flags(0x0800_0000 /*CREATE_NO_WINDOW*/)` so no console window flashes.
   - Read through a `BufReader` with a ≥256 KB capacity. Make sure the helper puts stdout in binary mode (`_setmode(_fileno(stdout), _O_BINARY)`) so `\n` isn't turned into CRLF and corrupting PNGs.
   - Framing: a length prefix (u32 LE + bytes) is best. If the helper can't change, parse PNG frames by reading the 8-byte signature, then chunks `[len u32 BE][type 4][data][crc 4]` until `IEND`. Use `read_exact` and no scanning.
   - Drain stderr in a separate task to avoid pipe deadlock. Send frames over a `tokio::sync::watch` (latest frame wins) to drop stale frames.

### 9. Autostart migration
16. **Claim:** In `auto-launch` 0.6.0, the Windows code writes `HKCU|HKLM\...\CurrentVersion\Run\<app_name>` = command line. It also writes `Explorer\StartupApproved\Run\<app_name>` = enabled bytes, which can **override a user's Task Manager "disabled" choice**. `is_enabled` reads StartupApproved. `WindowsEnableMode::Dynamic` tries HKLM first, so use `CurrentUser`. On macOS, LaunchAgent mode writes `~/Library/LaunchAgents/<app_name>.plist` with `Label=<app_name>` and `RunAtLoad`. It does not run `launchctl`. AppleScript and SMAppService (13+) modes also exist. **Sources:** [auto-launch windows.rs](https://raw.githubusercontent.com/zzzgydi/auto-launch/main/src/windows.rs), [macos.rs](https://raw.githubusercontent.com/zzzgydi/auto-launch/main/src/macos.rs). **Support:** direct. **Confidence:** high. tauri-plugin-autostart wraps auto-launch 0.6 and has an `app_name()` builder ([changelog](https://v2.tauri.app/release/autostart/all-versions/)).
   - Migration: reuse Python's exact Run value name and plist filename/Label. Then enabling overwrites the old entry in place instead of adding a second launcher. On first run, if the old entry exists, rewrite it to point to the new exe; if the name differs, delete the old one. Respect an existing StartupApproved "disabled" state rather than calling `enable()` blindly. Since the plist label is reused, `launchctl bootout gui/$UID/<label>` the old agent (or just let the next login pick up the new plist).

## Crate mapping table

| Python lib / role | Rust crate (version, 2026-10-15) | Size / notes |
|---|---|---|
| `keyring` (WinVault / macOS) | `keyring` 4.2.0 (`v1` feature) or `keyring-core` + `windows-native-keyring-store` 1.1.0 / `apple-native-keyring-store` 1.0.2; or `windows` (Win32_Security_Credentials) for raw `CredReadW/CredWriteW` | Small. Raw Win32 gives exact Python parity; recommended on Windows. |
| macOS Keychain (Claude creds) | `security-framework` 3.7.0 | Small; ACL prompts (see trap) |
| `zeroconf` / `python-zeroconf` | `mdns-sd` 0.21.5 | Small, pure Rust, own thread |
| `cryptography` AESGCM | `aes-gcm` 0.11.1 | Tiny; 0.11 API changes |
| `aiohttp`/`http.server` (hook server) | `axum` 0.8.9 (min features) | Marginal, since hyper/tokio are shared with reqwest |
| `requests`/`httpx` | `reqwest` 0.13.5 (rustls + aws-lc default; platform verifier 0.7.1) | Largest dep (~1–3 MB est., unmeasured); aws-lc build needs C compiler (+NASM rec.) |
| `requests.auth.HTTPDigestAuth` | `diqwest` 3.2.0 (→ `digest_auth` 0.3.1) | Tiny |
| `filelock` / `portalocker` | `fs4` 1.1.0 or std `File::lock`; `fd-lock` 4.0.4 | Tiny |
| atomic write (`os.replace`) | `tempfile` 3.27.0 `NamedTempFile::persist` | Tiny |
| `subprocess.Popen` | `tokio::process` | — |
| registry / plist autostart | `auto-launch` 0.6.0 / `tauri-plugin-autostart` | Small |
| pytest golden tests | `insta` 1.49.0 + `serde_json` (dev) | Dev only |

## Compatibility traps
1. **Windows target name:** Rust's default is `user.service`; Python uses `service` or `username@service`. You must pass an explicit target and implement Python's fallback order when reading.
2. **Empty UserName:** writing a new credential via the keyring-rs `target` modifier leaves `UserName` empty, which breaks Python's lookup. Set username explicitly, or use CredWriteW.
3. **Credential blob encoding:** UTF-16LE with no NUL terminator. Python reads UTF-8 as a fallback; a Rust read of a UTF-8 blob fails with BadEncoding. Read the raw secret and decode both ways yourself.
4. **Collision swap:** Python moves an existing credential to `old_user@service` on a username change. The Rust write path must do the same, or the old Python version (on downgrade) and the new one will disagree.
5. **macOS Keychain ACL:** a new binary means new prompts, for both the app's own items and `Claude Code-credentials`. Sign the binary with a stable identity.
6. **Firewall:** the Rust exe gets a new Windows Firewall prompt for mDNS (5353) and any LAN listener. Bind the hook server to 127.0.0.1 only so it doesn't prompt.
7. **Port 5353 sharing:** this needs REUSEADDR alongside Bonjour or Windows DNS Client. Behavior can be flaky; ship a manual-IP fallback.
8. **Prefs JSON:** keep key names, unknown keys (round-trip them through `#[serde(flatten)] extra: Map`), number formats (Python writes `1.0` and `NaN`/`Infinity` — serde_json rejects NaN), and indentation. Lock the same sidecar file Python locks.
9. **Windows `persist`** fails while another process has the file open. Retry.
10. **auto-launch** resets the StartupApproved "disabled" flag, and its `Dynamic` mode writes HKLM. Use `CurrentUser` and reuse Python's value name and plist label.
11. **reqwest 0.13** feature renames (`rustls-tls` → `rustls`) and the aws-lc build toolchain on Windows CI.
12. **aes-gcm 0.11** API differs from 0.10 examples.

## Recommendations for the plan
- Add a `compat` module per OS store, with a Windows implementation on raw `CredReadW/CredWriteW`. It should reproduce `_resolve_credential` / `set_password` from Windows.py exactly, with unit tests built from Python-generated fixtures. On macOS, use keyring/apple-native or security-framework with `(service, account)` unchanged.
- Treat the following as **frozen contracts** before porting, and pin them with golden fixtures generated by the current Python app:
  - the prefs JSON schema
  - keyring service/user names
  - hook port and routes
  - the Run value name and LaunchAgent label
  - the AES key/nonce/AAD layout
- Use these defaults: tokio + axum (loopback) + reqwest 0.13 (rustls default) + diqwest; mdns-sd with an IP fallback; fs4 or std lock on Python's lock file; tempfile persist with retry; auto-launch with `CurrentUser` and Python's names.
- Add a first-run migration step: detect the Python autostart entry, credentials and prefs; rewrite autostart to the new exe path; never delete secrets.
- Before committing, measure binary size (`cargo bloat`) with reqwest+aws-lc vs. `rustls-no-provider`+ring. Also validate the Windows build toolchain for aws-lc (NASM/CMake) in CI.

## Contradictions
- aws-lc-rs Windows needs: the platform-support page says CMake is never required for non-FIPS builds, but issue #828 reports a CMake dependency on Windows. Verify in CI.
- mdns-sd on Windows: some users report not receiving responses (#33); the maintainer couldn't reproduce.

## Missing evidence
- The exact service/username strings and lock mechanism used by the current Python app. Read the Python source; not researched here.
- Whether the Python app reads the Claude Keychain item via `security` CLI or the Security API. This affects prompt behavior.
- The `keyring` 4.x `v1` API's exact `Entry::new_with_target` semantics with respect to UserName. Inferred from windows-native-keyring-store source, not tested.
- Measured binary-size numbers (none fetched; all size notes are estimates).
- The stabilization version of std `File::lock` (believed 1.89; not re-verified).
- The pywin32-ctypes blob encoding (assumed UTF-16LE without BOM, consistent with Python's UTF-16 decode).

## Sources
- Kept: jaraco/keyring Windows.py & macOS backend (raw GitHub): exact Python naming and encoding.
- Kept: windows-native-keyring-store docs + cred.rs: Rust target naming, UTF-16, the UserName trap.
- Kept: crates.io API for each crate: current versions.
- Kept: reqwest v0.13.0 release notes and seanmonstar blog: rustls default, aws-lc, platform verifier.
- Kept: aws-lc-rs requirements pages and issue #828: Windows build toolchain.
- Kept: mdns-sd README, its deps, and issue #33.
- Kept: auto-launch windows.rs/macos.rs: registry and plist behavior.
- Kept: anthropics/claude-code issues #77697, #89985, #91180, #91158, #10039: Claude credential storage behavior (community reports; no official docs found).
- Kept: Microsoft Learn SO_REUSEADDR; MS Tech Community mDNS.
- Deprioritized: third-party mDNS libs (beacon, pion, hashicorp) — other ecosystems.

## Next steps
- Grep the Python source for `keyring.set_password(`, the lock implementation, the prefs path and format, the hook port, and the autostart name. Then generate fixtures.
- Build a spike that round-trips a WinVault credential: Python writes → Rust reads → Rust writes → Python reads, covering the collision case.
- CI spike: reqwest 0.13 on windows-msvc (aws-lc) and `cargo bloat` size measurements.

```acceptance-report
{
  "criteriaSatisfied": [
    {"id": "criterion-1", "status": "satisfied", "evidence": "Concise report covering all 9 topics, crate table with crates.io versions, traps, recommendations, contradictions and residual risks"}
  ],
  "changedFiles": ["C:/Users/User/.pi/agent/sessions/--D--vu-usage-monitor--/subagent-artifacts/outputs/242cd9e4-1221-495c-a440-baf9ae7f7b01/task-2.md", "D:/vu/usage-monitor/plans/261015-0726-rust-tauri-port/research/researcher-02-crates-compat.md"],
  "testsAddedOrUpdated": [],
  "commandsRun": [],
  "validationOutput": ["Versions taken from crates.io API on 2026-10-15; Python keyring and keyring-rs Windows store behavior verified against source code"],
  "residualRisks": [
    "keyring-rs target-modifier empty-UserName behavior inferred from source, not runtime-tested",
    "Size estimates unmeasured",
    "Python app's actual service names/lock mechanism not inspected",
    "Claude Code keychain behavior based on GitHub issue reports, not official docs",
    "aws-lc-rs Windows CMake requirement contradictory"
  ],
  "noStagedFiles": true,
  "diffSummary": "Added research markdown report",
  "reviewFindings": ["no blockers"],
  "manualNotes": "Key finding: keyring crate is now 4.2.0 (keyring-core split); Windows default target 'user.service' differs from Python's 'service' / 'user@service' — use explicit target or raw CredWriteW."
}
```
