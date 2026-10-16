---
title: One public repo: clock firmware ships with the app, style packs from a signed release
status: in progress
created: 2026-10-17
branch: feat/monorepo (worktree ../clawd-desktop-monorepo)
source: ak:advise session 2026-10-17 (confirmed reframing)
---

# Monorepo: firmware inside usage-monitor

## Contract
- **Objective:** clawd-home moves into this repo under `clock/` with its history (secrets
  filtered); every `vX.Y.Z` tag ships firmware + clawd.pak + desk.pak + style packs + renderer
  inside Clawd.app / Setup.exe; usage style packs update remotely through a fixed, minisign-signed
  GitHub Release `packs-api{N}`.
- **Done means**
  1. `git log -- clock/firmware` shows clawd-home's history; gitleaks over the result is clean
     or every finding is reviewed and allowed.
  2. A release's `Resources/firmware/firmware.json` has the `fwSeq`/`styleApi` of
     `config.h`/`StylePack.h` at the tagged commit, plus `clawd.pak`, `desk.pak`, `styles/`.
  3. The bundled firmware serves `clawd.pak`/`desk.pak` to the install path (tests).
  4. `tools/publish_packs.sh` (and `packs.yml`) create/refresh `packs-api{N}` with
     `--latest=false`; the app downloads it, checks the signature, `api` and `rev`, and shows
     new packs without a reinstall.
  5. No feed code in the app, no `CLAWD_FEED_*`, no `CLAWD_PRIVATE_FIRMWARE`, no
     `tools/check_public.py`; NOTICE/README say the firmware is here.
  6. CI fails when `clock/firmware/src` changes without a `CLAWD_FW_SEQ` bump, and when the
     latest release is not a `v*` tag.
  7. Clock at fwSeq 8 / styleApi 2 + an app built from the same commit: no "Không hỗ trợ" card.
- **Out of scope:** remote firmware updates; replacing fonts/art; shutting the Cloudflare feed
  (it keeps serving 0.1.x/0.2.0 until they update; `clock/feed/` stays as the legacy tool).
- **Constraints:** repo name `vuduong97/usage-monitor` stays and stays public; nothing is pushed
  from this session (publishing makes the firmware public, irreversible: owner's step).
- **Owner accepted:** copyright risk of `clawd_fonts.h`, `clawd.pak`, `desk.pak` in a public repo.

## Design
- Layout: `clock/` = clawd-home (firmware/, tools/, docs/, feed/, design/ without `.otf`).
  Root `PROTOCOL.md` is the only copy.
- Bundle (`build/firmware/`, from `tools/bundle_firmware.py`): `firmware.json` gains
  `packs: {clawdpak: {file,size,crc}, deskpak: {...}}`; `styles/index.json` gains `rev`
  (commit time of the last change to `clock/firmware/styles`).
- App: `clawd_firmware::feed` is replaced by `clawd_firmware::packs`: pack plan/state on the
  `Bundle`, and the remote style source (`index.json` + `.minisig` + `<id>.pak` under
  `https://github.com/<UPDATE_REPO>/releases/download/packs-api{N}/`). Remote styles replace the
  bundled ones only when signature, `api == bundle.styleApi`, every pack's sha256 and header
  check out, and `rev > bundled rev`.
- Tray offer: bundle only (`update_state` available or a pack differs).
- Release CI: one `firmware` job (ubuntu, PlatformIO) builds the bundle once as an artifact;
  mac/windows jobs build the renderer from `clock/firmware` and put both in the app.

## Phases
1. [ ] Import clawd-home history under `clock/` (filter-repo, drop `.otf`, gitleaks, review)
2. [ ] Layout: single PROTOCOL.md, commit desk.pak, drop clock's renderer.yml, gitignore
3. [ ] Bundle: `bundle_firmware.py` (packs, rev, build/firmware), `tools/build_clock.sh`
4. [ ] App: packs module, Bundle packs, OTA, session/commands/tray/lifecycle, UI, delete feed
5. [ ] Packs channel: `tools/publish_packs.sh`, `.github/workflows/packs.yml`
6. [ ] CI: firmware job, fwSeq guard, latest guard; release.yml bundles firmware + renderer
7. [ ] Docs: NOTICE, README, packaging/README, clock/README
8. [ ] Verify: fmt, clippy, tests, local build_mac.sh, the real clock (no flashing)
