# packaging-rs: packaging and release of the Rust build

The Rust + Tauri port (`desktop-rs/`) keeps the release contract that Clawd 0.1.x already
ships, so the 0.1.x self-update installs it: the same Inno `AppId`, install folder and silent
flags on Windows, the same bundle id and **Developer ID Team ID** on macOS, the same asset names.
The Python packaging in `packaging/` is untouched and keeps releasing through `release.yml`
until the cutover.

| File | What it is |
|---|---|
| `windows/clawd.iss` | Inno Setup script. `AppId {6C9B4E2A-1F7D-4C3B-9A51-C1A7D0E5B9F3}`, `{localappdata}\Programs\Clawd`, per-user, `[InstallDelete]` removes the PyInstaller `_internal\`, `[UninstallRun]` stops the app and runs `Clawd.exe cleanup`. No `[Code]`, no WebView2 bootstrapper (parity with Python). |
| `build_windows.ps1` | `cargo build --release -p clawd-app`, asserts `Clawd.exe`, runs `iscc`: `dist\Clawd-<version>-windows-setup.exe`. |
| `build_mac.sh` | Tauri bundler builds `Clawd.app` for this Mac's arch, then Developer ID signing (hardened runtime), notarize, staple, `dist/Clawd-<version>-mac-<arch>.zip` + `.dmg`. |
| `entitlements.plist` | Byte copy of `packaging/entitlements.plist`. |
| `check_version.py` | Prints the build version; with a tag, requires `rust-v<version>`. |
| `../desktop-rs/app/tauri.{windows,macos}.conf.json`, `../desktop-rs/app/Info.plist` | Tauri per-platform overlays and the macOS Info.plist keys (`LSUIElement`, `NSLocalNetworkUsageDescription`, `NSBonjourServices`). |
| `../.github/workflows/rust-release.yml` | The release workflow. |

## Local builds

Windows (needs Rust 1.91, Inno Setup 6, Python 3.11+ for `check_version.py`):

```powershell
./packaging-rs/build_windows.ps1
```

macOS (needs Rust 1.91 and `cargo install tauri-cli --version "^2" --locked`):

```sh
sh packaging-rs/build_mac.sh
```

Without `CLAWD_SIGN_IDENTITY` the app is signed ad-hoc: it runs on this Mac, but it is **not a
release** (0.1.x will not accept it). Set `CLAWD_REQUIRE_SIGNING=1` to make a missing identity or
missing notarization an error (the script does that by itself when `GITHUB_REF_TYPE=tag`).

Environment read at build time: `CLAWD_FEED_URL`, `CLAWD_FEED_TOKEN` (firmware feed, baked into
the binary by cargo, never in the repo), `CLAWD_MINISIGN_PUBKEY`, `CARGO_TARGET_DIR`.
`CLAWD_PRIVATE_FIRMWARE=1` also ships `clawd_desktop/firmware` and `clawd_desktop/renderer`
(output of `tools/bundle_firmware.py` and `tools/build_preview.py`) next to the exe /
in `Contents/Resources`. Clock owners only: both scripts refuse it in CI, public artifacts carry
no firmware.

Installer size: not measured here (no `iscc` in the build sandbox). The spike estimate was about
3.5 MB (`reports/spike-decision.md`); the workflow's size gate is the real number.

## Release workflow (`rust-release.yml`)

* **Triggers**: pushing a tag `rust-v*`, or a manual run. Nothing runs on a branch push. Only a
  `rust-v*` tag run builds, signs and publishes; a manual run on a branch runs just the secret-free
  gates (`verify`, `test`) and never binds the environment `release`.
* **Secrets** live only in the GitHub environment `release` (Settings, Environments; add the owner
  as required reviewer and set Deployment branches and tags to the tag pattern `rust-v*` only).
  Jobs `build-windows`,
  `build-mac` and `sign` are bound to it; `verify` and `publish` are not.
  Secrets: `MACOS_CERT_P12`, `MACOS_CERT_PASSWORD`, `MACOS_SIGN_IDENTITY`, `APPLE_ID`,
  `APPLE_TEAM_ID`, `APPLE_APP_PASSWORD` (same names as `release.yml`), `CLAWD_FEED_URL`,
  `CLAWD_FEED_TOKEN`, `MINISIGN_KEY`, `MINISIGN_PASSWORD`.
  Repository **variable** (not a secret): `MINISIGN_PUBKEY`.
* **Permissions**: `contents: read` everywhere; `contents: write` only in `publish`.
* **Tag and version**: `GITHUB_REF_NAME` without `rust-` must equal `v<version>` of
  `desktop-rs/app/Cargo.toml` (inherited from `[workspace.package]`) and of
  `desktop-rs/app/tauri.conf.json`. A test tag therefore needs a version bump in the same commit,
  e.g. `0.2.0-test.1` and `rust-v0.2.0-test.1`.
* **Signing**: a tag run without the Developer ID certificate, or without notarization, fails.
  There are no ad-hoc releases; only a local `build_mac.sh` run without a tag may be ad-hoc.
* **Size gate**: installer, zip and dmg must be at most 8,000,000 bytes (fails the run). The exe
  size is only reported.
* **Assets**: `Clawd-<v>-windows-setup.exe`, `Clawd-<v>-mac-{arm64,x86_64}.{zip,dmg}`,
  `SHA256SUMS`, `SHA256SUMS.minisig`. The workflow checks that these are exactly the files present.
* **Publish**: a draft **and** prerelease, `--latest=false`, then a gate
  (`gh release view <tag> --json isDraft,isPrerelease`) that must see both true. A `rust-v*`
  release must never become "latest" on the main repo: Clawd 0.1.4 reads `/releases/latest`, and
  `parse_version("rust-v...")` is `None`, so the whole fleet would stop getting updates. Never
  publish the draft by hand. `rust-v*` tags are for internal tests; the real path is `release.yml`
  after the cutover (phase 15).
* **Before the first tag**: commit `desktop-rs/Cargo.lock` (a tag build uses `--locked`), create
  the environment, secrets and variable above.

## minisign key (Q2: SHA256SUMS is signed)

Create the key once, on an offline machine (`minisign` 0.11+):

```sh
minisign -G -p clawd-minisign.pub -s clawd-minisign.key     # asks for a password: use a strong one
cat clawd-minisign.pub                                       # 2nd line = the public key (base64, starts RW)
```

1. Store `clawd-minisign.key` and the password in the password manager **and** keep an offline copy
   (encrypted USB). Losing the key means shipping a new public key in a new build, and clients
   that cannot verify can no longer update.
2. GitHub: secret `MINISIGN_KEY` = the whole content of the `.key` file, secret `MINISIGN_PASSWORD`
   = its password (both in environment `release`), repository variable `MINISIGN_PUBKEY` = the
   public key line.
3. The same public key must be in the app: `MINISIGN_PUBKEY` in
   `desktop-rs/crates/clawd-contracts/src/consts.rs` is still the placeholder
   (`MINISIGN_PUBKEY_PLACEHOLDER`, verification fails closed). Replacing it is a Contract CR to the
   integrator. Until then CI builds embed the variable through `CLAWD_MINISIGN_PUBKEY`, which the
   constant honours at build time; a local build needs the same variable.
4. The workflow signs with `minisign -S -s <key> -m SHA256SUMS` (writes `SHA256SUMS.minisig`) and
   verifies it with the public key before anything is uploaded.
5. **Rotation**: generate a new key pair; ship a build whose `MINISIGN_PUBKEY` is the new public
   key, signed with the *old* key (so existing clients accept it); once that build is the minimum
   version in the fleet, switch the secrets and the variable to the new key. A lost or leaked key
   cannot be rotated this way: that needs a manual re-install (Developer ID / installer download).

## What no one has verified yet (needs a Windows VM, a Mac, or the real runners)

`iscc` on the script, the installer size, notarization, `minisign` in CI, the runner labels
(`macos-14` = arm64, `macos-15-intel` = x86_64; `build_mac.sh` fails if `uname -m` disagrees),
`cargo install tauri-cli` and `cargo tauri build --bundles app` on macOS. Check list for phase 14:

* Install over 0.1.4 in a clean Windows VM: same folder, one Add/Remove Programs entry,
  `_internal\` gone, `/SILENT /SUPPRESSMSGBOXES /NORESTART` shows no UI, app starts, uninstall runs
  `Clawd.exe cleanup`.
* `codesign -dv --verbose=2 Clawd.app` gives the same `TeamIdentifier` as the 0.1.4 app;
  `spctl --assess --type execute -vv` says `source=Notarized Developer ID`.
* Dry run in a fork with a test tag (never on the main repo): the draft prerelease has all five
  files plus `SHA256SUMS` and `SHA256SUMS.minisig`.
* The entitlements are a copy of the PyInstaller ones (unsigned executable memory, no library
  validation). A Rust app probably needs neither: if it runs and notarizes without them, empty the
  file.
* On a machine without the VC++ runtime: does `Clawd.exe` start? A string scan of a local release
  build found imports of the Universal CRT (`api-ms-win-crt-*`, part of Windows 10/11) and no
  `VCRUNTIME140.dll`, so it should; confirm on the clean VM.
