# Fork releases for the migration tests (phase 14)

How the **owner** makes the test releases that 0.1.4 and the Rust app update from. Everything
happens on **a fork** of the repository with its own `release` environment and its own **test**
minisign key. Never on the main repo: a normal release there is offered to every installed 0.1.x.

Why a hand-made release: `rust-release.yml` only ever creates a **draft + prerelease** `rust-v*`
(so it can never become `/releases/latest` of the main repo). 0.1.4 and the Rust updater only
offer `GET /repos/<repo>/releases/latest`, i.e. a **published, non-prerelease** release whose tag
is plain `vX.Y.Z`. So: build with the workflow on the fork (tag `rust-vX.Y.Z`), then publish the
same files by hand as `vX.Y.Z` on the fork.

## 1. Fork and environment (once)

1. Fork the repo; in the fork enable Actions. Commit `desktop-rs/Cargo.lock` (a tag build uses `--locked`).
2. Test minisign key, on your own machine (minisign 0.11+; **never** the production key):

   ```sh
   minisign -G -p fork-test.pub -s fork-test.key        # strong password; keep both files
   sed -n 2p fork-test.pub                               # the public key line: starts with RW, 56 chars
   ```

3. Fork Settings > Environments > `release`: required reviewer = you; deployment tags `rust-v*` only.
   Secrets (environment `release`): `MINISIGN_KEY` (whole content of `fork-test.key`),
   `MINISIGN_PASSWORD`, `CLAWD_FEED_URL`, `CLAWD_FEED_TOKEN` (any value for a test: the firmware
   feed is not exercised; never a real token), and for macOS `MACOS_CERT_P12`,
   `MACOS_CERT_PASSWORD`, `MACOS_SIGN_IDENTITY`, `APPLE_ID`, `APPLE_TEAM_ID`, `APPLE_APP_PASSWORD`
   (a **Developer ID Application certificate of the same Team** as 0.1.4; see `mac.md`).
   Repository **variables**: `MINISIGN_PUBKEY` = the public key line, `EXPECTED_TEAM_ID` = the
   10-character Team ID of 0.1.4 (`codesign -dv --verbose=2 /Applications/Clawd.app 2>&1 | grep TeamIdentifier`).
4. The test binary differs from production in exactly one constant: `CLAWD_MINISIGN_PUBKEY` (baked
   in by the workflow from the variable above; `clawd_contracts::MINISIGN_PUBKEY` honours it at
   build time). For a local Windows build: `$env:CLAWD_MINISIGN_PUBKEY = "<the RW... line>"`
   before `packaging-rs\build_windows.ps1`.
5. The Windows test needs no signing secrets (no Authenticode). Mac needs the Developer ID ones.

## 2. Stamp a version (the tag must equal the build)

The workflow requires `GITHUB_REF_NAME` without `rust-` to equal `v<version>` of
`desktop-rs/app/Cargo.toml` (inherited from `[workspace.package]`) **and** of
`desktop-rs/app/tauri.conf.json`. Per test version, on a branch of the fork:

```sh
git switch -c test-0.2.1 port/<the-branch-under-test>
sed -i 's/^version = "0.2.0"/version = "0.2.1"/' desktop-rs/Cargo.toml        # [workspace.package]
sed -i 's/"version": "0.2.0"/"version": "0.2.1"/' desktop-rs/app/tauri.conf.json
(cd desktop-rs && cargo update --workspace)       # Cargo.lock lists every workspace crate's version
git diff --stat                                   # exactly those three files
git commit -am "test: stamp 0.2.1" && git tag rust-v0.2.1 && git push fork test-0.2.1 rust-v0.2.1
```

Versions used by the procedures: `0.2.1` (migration), `0.2.2` (Rust to Rust), bad variants
`0.2.3`..`0.2.8`, rollback `0.2.9` (a **Python** build, section 6). Use plain digits only:
`0.1.4` rejects anything else.

## 3. Build

Push the tag. In the fork's Actions the run `rust-v0.2.1` does `verify`, `test`, then (after you
approve the `release` environment) `build-windows`, `build-mac` (arm64 on `macos-14`, x86_64 on
`macos-15-intel`), `sign` (writes `SHA256SUMS` + `SHA256SUMS.minisig`, verifies it with the public
key) and `publish`, which creates a **draft prerelease** `rust-v0.2.1` with exactly seven assets:

```text
Clawd-0.2.1-windows-setup.exe
Clawd-0.2.1-mac-arm64.zip   Clawd-0.2.1-mac-arm64.dmg
Clawd-0.2.1-mac-x86_64.zip  Clawd-0.2.1-mac-x86_64.dmg
SHA256SUMS                  SHA256SUMS.minisig
```

Gates to read in the log: installer, zip and dmg <= 8,000,000 bytes; mac TeamIdentifier ==
`EXPECTED_TEAM_ID`; notarization and staple succeeded.

## 4. Publish the test release by hand (`vX.Y.Z`, not draft, not prerelease)

```sh
FORK=<you>/usage-monitor-fork
mkdir rel-0.2.1 && cd rel-0.2.1
gh release download rust-v0.2.1 -R $FORK                       # the seven files (works on a draft you own)
# the signature must verify before you publish anything:
minisign -Vm SHA256SUMS -p ../fork-test.pub                    # "Signature and comment signature verified"
sha256sum -c SHA256SUMS                                        # every listed file OK
gh release create v0.2.1 -R $FORK --title "v0.2.1 (test)" --notes "migration test" \
   --latest --target test-0.2.1 *.exe *.zip *.dmg SHA256SUMS SHA256SUMS.minisig
# confirm what the updaters will see:
gh api repos/$FORK/releases/latest --jq '{tag:.tag_name,draft:.draft,prerelease:.prerelease,assets:[.assets[].name]}'
```

`draft` and `prerelease` must both be `false`, the tag `v0.2.1`, and the list must hold the seven assets. Then
delete the draft `rust-v0.2.1` release (`gh release delete rust-v0.2.1 -R $FORK -y`) so there is one source of truth.

Point the app at the fork (file `~/.config/claude-usage-monitor/config`): `update_repo = <you>/usage-monitor-fork`
(or the env var `CLAWD_UPDATE_REPO`; `CLAWD_UPDATE_API` can point at a local fake API for dry runs).

## 5. Making the failure cases

Each is its own release made from a valid one (same `.exe`/`.zip`); the tag must be newer than the
installed version. Delete it right after its case (`gh release delete vX -R $FORK --cleanup-tag -y`).

| Case | How the assets differ |
|---|---|
| bad signature | `minisign -S -s other.key -m SHA256SUMS` with a **second** throw-away key (`minisign -G -p other.pub -s other.key`), upload that `.minisig` |
| tampered SHA256SUMS | keep the valid `.minisig`, edit one hex digit in `SHA256SUMS`, upload both |
| missing .minisig | publish without `SHA256SUMS.minisig` |
| wrong installer | valid signature of a `SHA256SUMS` computed for the real file, then replace the `.exe` with a file of the same size but different bytes |
| draft / prerelease | `gh release create ... --draft` or `--prerelease` (never offered) |
| foreign host | not reproducible with a real release (the asset URL is GitHub's); covered by `clawd-appupdate` tests |

## 6. The rollback release (Python build, signed)

The Rust updater requires the signature even for a Python installer.

```sh
git switch python-0.1.x                               # the frozen Python line; set version 0.2.9 (pyproject, __version__, ...)
# build on Windows:  packaging\build_windows.ps1  ->  dist\Clawd-0.2.9-windows-setup.exe
# macOS:             packaging/build_mac.sh       ->  dist/Clawd-0.2.9-mac-<arch>.zip  (Developer ID, same Team)
sha256sum Clawd-0.2.9-* > SHA256SUMS
minisign -S -s fork-test.key -m SHA256SUMS           # writes SHA256SUMS.minisig
minisign -Vm SHA256SUMS -p fork-test.pub
gh release create v0.2.9 -R $FORK --latest Clawd-0.2.9-* SHA256SUMS SHA256SUMS.minisig
```

The version of the Python build must be above the highest Rust test version installed
(`0.2.2`, or whatever is on the machine).

## 7. Clean-up

Delete every test release and tag from the fork, remove `MINISIGN_KEY`/`MINISIGN_PASSWORD` and the
Apple secrets from the fork's environment, delete the fork if it is not reused. Keep
`fork-test.key` offline only if you will repeat the run. Nothing of this touches the main repo,
its `release` environment or the production minisign key (generated separately, see
`packaging-rs/README.md`).
