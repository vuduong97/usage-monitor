# Test releases without a fork: the local fake update API

Preferred over [`fork-release.md`](fork-release.md) since the cutover. Both updaters (Python 0.1.x
and Rust 0.2.x) honour `CLAWD_UPDATE_API`; when it is a loopback `http://` address they also accept
downloads from `http://127.0.0.1`. [`fake_api.py`](fake_api.py) serves a folder of release files as
`/repos/<any>/<any>/releases/latest`, so an installed app can be updated from files on disk:

* **The release candidate itself**: the files of the draft `vX.Y.Z` made by `release.yml` on the main
  repo (a draft is invisible to users). This tests exactly what will be published, Developer ID and
  notarization included, with no copy of the signing secrets anywhere else.
* **Rust -> Rust and the signature cases**: two local builds that differ only in version, with a
  **test** minisign key baked in. Never the production key.

The case numbers (A, B, C) are those of [`windows.md`](windows.md) and [`mac.md`](mac.md); only the
way the release is made changes. Record each result in `reports/migration-report.md`.

## 1. Case A: 0.1.4 -> the release candidate

```sh
gh release download vX.Y.Z -R vuduong97/usage-monitor -D rel-X.Y.Z     # the draft's 7 files
minisign -Vm rel-X.Y.Z/SHA256SUMS -P "$(gh variable get MINISIGN_PUBKEY -R vuduong97/usage-monitor)"
(cd rel-X.Y.Z && shasum -a 256 -c SHA256SUMS)
python3 tools/migration/fake_api.py --dir rel-X.Y.Z                   # leave it running
```

Install the signed 0.1.4 (`gh release download v0.1.4 -p '*mac-arm64.dmg'` or `*windows-setup.exe`),
set it up completely (`mac.md` / `windows.md` section 2.1), quit it from the tray, then start it with
the variable so its update check asks the fake API:

```sh
# mac
CLAWD_UPDATE_API=http://127.0.0.1:8765 /Applications/Clawd.app/Contents/MacOS/Clawd
```

```powershell
# Windows
$env:CLAWD_UPDATE_API = "http://127.0.0.1:8765"; & "$env:LOCALAPPDATA\Programs\Clawd\Clawd.exe"
```

The first check is 90 s after start (or Settings > Cập nhật > Kiểm tra bản mới). The fake API log
shows `releases/latest`, `SHA256SUMS` and the installer/zip being fetched; the tray then shows
"● Khởi động lại để cập nhật". Click it and verify as in section 2.3 of the procedure. The app that
comes up after the update was started by the updater, not by your shell, so it reads the real API
again: nothing else needs undoing.

## 2. Case B: Rust -> Rust, and the refused cases

On the test machine (mac: needs Rust 1.91 and `cargo install tauri-cli --version "^2" --locked`):

```sh
minisign -G -W -p /tmp/clawd-test.pub -s /tmp/clawd-test.key          # throw-away key, no password
export CLAWD_MINISIGN_PUBKEY="$(sed -n 2p /tmp/clawd-test.pub)"
```

Build `0.2.90` (installed) and `0.2.91` (the update) from the same commit; versions above any real
release so nothing real is offered instead. The two version fields must agree (`check_version.py`):

```sh
stamp() {  # stamp <version>: workspace version, tauri.conf.json, Cargo.lock
  sed -i.bak -E "s/^version = \"[^\"]+\"/version = \"$1\"/" desktop-rs/Cargo.toml
  sed -i.bak -E "s/\"version\": \"[^\"]+\"/\"version\": \"$1\"/" desktop-rs/app/tauri.conf.json
  rm -f desktop-rs/Cargo.toml.bak desktop-rs/app/tauri.conf.json.bak
  (cd desktop-rs && cargo update --workspace -q)
}
stamp 0.2.90 && CLAWD_SIGN_IDENTITY="<identity>" sh packaging-rs/build_mac.sh   # mac
stamp 0.2.91 && CLAWD_SIGN_IDENTITY="<identity>" sh packaging-rs/build_mac.sh
git checkout desktop-rs/Cargo.toml desktop-rs/Cargo.lock desktop-rs/app/tauri.conf.json
mkdir -p rel-0.2.91 && cp dist/Clawd-0.2.91-* rel-0.2.91/
(cd rel-0.2.91 && shasum -a 256 Clawd-* > SHA256SUMS && minisign -S -s /tmp/clawd-test.key -m SHA256SUMS)
```

On Windows use `.\packaging-rs\build_windows.ps1` with `$env:CLAWD_MINISIGN_PUBKEY` set, and
`Get-FileHash` / `minisign.exe` for the sums. On mac both builds must carry the **same Team ID**
(`codesign -dv --verbose=2`): any identity works (an *Apple Development* one too), an ad-hoc build
never updates. Install 0.2.90 (mac: copy `Clawd.app` to `/Applications`; Windows: run its
installer), then for each case start the fake API and the app as in section 1:

| Case | Fake API | Expected |
|---|---|---|
| B1 valid | `--dir rel-0.2.91` | staged, click, 0.2.91 runs (`Clawd --version`), config unchanged |
| B2 bad signature | `--dir rel-0.2.91 --case bad-sig` | refused, the error names the signature; 0.2.90 keeps running |
| B3 other key | `--dir rel-0.2.91 --minisig other.minisig` (`minisign -S -s other.key -m SHA256SUMS -x other.minisig`) | refused |
| B4 tampered sums | `--case tampered-sums` | refused |
| B5 no signature | `--case no-minisig` | not offered (no update item) |
| B6 wrong installer | `--case wrong-installer` | refused after download (SHA-256) |
| B7 prerelease / draft | `--case prerelease`, `--case draft` | not offered |

Run B2-B7 before B1 (B1 replaces the installed version). Afterwards reinstall the real release and
delete `/tmp/clawd-test.*`, `rel-0.2.91/` and the test builds in `dist/`.

## 3. Case C: rollback to Python

As in `fork-release.md` section 6, but serve the signed Python build with
`fake_api.py --dir rel-0.2.99` instead of publishing it.

## 4. Publish gate (the real release)

Publish `vX.Y.Z` only when all of these hold, each recorded in `reports/migration-report.md`:

1. Case A passed on mac **and** Windows with the draft's own files.
2. Case B (B1-B7) passed on mac **and** Windows.
3. No unexplained exit: after each install, the app is still running 2 minutes later, and the log
   (`~/.config/claude-usage-monitor/clawd-desktop.log`) has no `Quit:` / `Exit requested` /
   `Event loop ended` line that nobody caused. A clean exit is not restarted by the mac LaunchAgent
   (`KeepAlive {SuccessfulExit=false}`) and not at all on Windows until the next login.
4. The production minisign key has an offline backup.

Then: `gh release edit vX.Y.Z -R vuduong97/usage-monitor --draft=false --latest` and check that
`gh api repos/vuduong97/usage-monitor/releases/latest --jq .tag_name` prints `vX.Y.Z`.
