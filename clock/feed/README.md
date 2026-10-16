# Firmware feed

Clawd desktop installs new clock firmware and packs from here with one click (Settings ›
Cập nhật, or the tray's "● Có firmware mới"). There is no new app release. The files are
never public. They sit in a private R2 bucket, encrypted, and are served only through
this Worker.

```
tools/release.sh ──(wrangler)──▶ R2 clawd-fw (private)       KV clawd-fw-keys
   build · test · encrypt          rel/<seq>/<name>.enc         key:<seq> -> AES key
                                   rel/<seq>/manifest.json
                                   channels/{beta,stable}.json
Clawd.app ──Bearer app token──▶ Worker clawd-fw-feed
   GET  /v1/latest?channel=stable     -> manifest
   POST /v1/grant {"seq": N}          -> {key, urls (HMAC, 5 min)}
   GET  /v1/file/<seq>/<name>?exp&sig -> the .enc file
   decrypt in memory -> /update, /api/clawdpak, /api/deskpak, /api/style on the clock
```

## What protects what

- **Not public.** The bucket has no public access and no custom domain. A file is
  served only for a signed URL. The Worker signs with `URL_SECRET`, and the URL
  expires after `URL_TTL_S` (300 s); the Worker refuses expiries more than 600 s away.
  `/v1/latest` and `/v1/grant` need an app token from `APP_TOKENS`. It is
  comma-separated, so a token can be rotated: add the new one, ship an app, then
  drop the old one.
- **Encrypted.** Every file is encrypted with AES-256-GCM under a fresh 32-byte key per
  release. The format is `nonce(12) | ciphertext | tag(16)` with AAD
  `clawd-fw/<seq>/<name>`. The key lives only in KV, never in the bucket, the
  manifest or the app. The app decrypts in memory and checks size, SHA-256 and CRC-32
  against the manifest. Nothing decrypted is written to disk, with one exception: the
  clock-screen renderer (below). An operating system runs a program only from a file, so the
  app unpacks it into `~/.config/claude-usage-monitor/renderer/` and checks every file's
  hash each time a settings window loads it. It is the compiled, stripped drawing code. It is not source, and it
  is no more than `firmware.bin` already carries.
- **Limits, honestly.**
  - The app token ships inside a public app, so someone who digs it out can call the
    feed like the app does.
  - The ESP8266 keeps the image and packs unencrypted in flash.
  - The upload to the clock goes over plain HTTP on the LAN.

  So this keeps the files out of public reach and out of casual hands. It does not
  stop a determined person who owns a clock. The rate limit is 120 requests/min per IP.

## Manifest

`rel/<seq>/manifest.json` is copied to `channels/<channel>.json` when published:

```json
{"v": 1, "seq": 6, "date": "2026-10-13T…Z", "notes": "…", "variant": "esp8266-clawd",
 "layouts": 1, "styleApi": 1, "commit": "abc1234",
 "files": [{"name": "firmware.bin", "kind": "fw", "size": 501520, "enc": 501548,
            "sha256": "…", "crc": "…"},
           {"name": "clawd.pak", "kind": "clawdpak", …}, {"name": "desk.pak", "kind": "deskpak", …},
           {"name": "styles/index.json", "kind": "styles", …}, {"name": "styles/synth.pak", "kind": "style", …}]}
```

A release can also list the settings window's clock-screen renderer, one zip per platform,
in its own list. Apps from before this list refuse any `files` row of a kind they do not
know, but they ignore a key they do not know. That is why the renderers are not in `files`:

```json
"renderers": [{"name": "renderer/win-x64.zip", "kind": "renderer", "platform": "win-x64", "size": "\u2026",
               "enc": "\u2026", "sha256": "\u2026", "crc": "\u2026"},
              {"name": "renderer/mac.zip", "kind": "renderer", "platform": "mac", "\u2026": "\u2026"}]
```

Each zip is clawd-desktop's `tools/build_preview.py --zip` output: `clawd_render[.exe]`,
`clawd.pak`, `styles/` and `preview.json` (which records the commit and fwSeq). The
renderer is built from this release's commit, so its pixels and style packs are exactly
what this firmware draws. `/v1/grant` signs the renderers too. With an older Worker the app
gets no URL for them and falls back to its approximate preview.

The app installs the firmware when `seq` is above the clock's `fwSeq` (it never
downgrades). Then it sends each pack whose CRC-32 differs from `pakCrc` / `deskCrc`
in the clock's `/api/heap` (firmware 6+), and re-sends the clock's active style pack
when the release has a different build of it.

## Releasing

```sh
tools/fetch_renderer.sh                      # renderer zips of HEAD (Actions: Windows + macOS) -> build/renderer/
tools/release.sh --notes "Màn chờ hiện IP"   # build + tests + encrypt + upload -> beta
tools/release.sh --promote                   # beta's release -> stable (what users get)
tools/release.sh --stable --notes "…"        # straight to both
```

- The working tree must be clean (`--dirty` overrides this, and the commit is then marked `-dirty`).
- `CLAWD_FW_SEQ` must be above every published release, or the script stops.
- It runs the host test, the Worker test and the preview checks (`--skip-tests` skips them).
- It needs `build/desk/desk.pak` (or `DESK_PAK=`).
- It adds every `build/renderer/<platform>.zip` (or `RENDERER_DIR=`). Each zip must be built from HEAD with this fwSeq, or the script stops. A missing platform only prints a warning; that platform's app then shows its approximate preview. `tools/fetch_renderer.sh` builds the zips with `.github/workflows/renderer.yml`, so the commit must be pushed first; `tools/fetch_renderer.sh` builds with clawd-desktop `main` (or `DESKTOP_REF=`). That workflow checks out the public clawd-desktop repo for `build_preview.py`. The Mac program is signed ad hoc. To sign it with the app's Developer ID, add a keychain-import step to the workflow and set `CLAWD_SIGN_IDENTITY`.
- With `--dirty`, the firmware comes from the working tree but the renderer comes from HEAD as GitHub built it. Keep `--dirty` releases to the beta channel.
- A release with renderers needs the Worker that signs them (`npx wrangler deploy` in `feed/worker` once, after this change). An older Worker still serves the release, but without previews.
- Publishing to Cloudflare needs a `Feed licence: <owner, scope, date>` line in `feed/LICENCE`, because the art and fonts reach every app user. `--private` skips that check. Use it only for the owner's own test, while no public app holds the feed token.
- The manifest is uploaded last, so a client never sees a release with missing files.
- To put your own clock on beta, add `feed_channel = beta` in `~/.config/claude-usage-monitor/config`.

## Deployed

- Account `72100c7c…` (thien.vu97ht123@gmail.com).
- Worker `https://clawd-fw-feed.thien-vu97ht123.workers.dev`.
- R2 bucket `clawd-fw`, with its r2.dev URL disabled.
- KV namespace `clawd-fw-keys`.
- The app token is kept on the owner's Mac in `~/.clawd-feed/app_token` (mode 600). It is not in any repo.
- Release 6 was published to beta and stable with `--private` on 2026-10-13.
- Release 7 (`why: login` shows "Login expired"; commit `94a837e`, renderers win-x64 + mac) was published to beta with `--private` on 2026-10-15, from Windows. Stable is still on 6.
- Build an app that uses this feed with:
  `CLAWD_FEED_URL=https://clawd-fw-feed.thien-vu97ht123.workers.dev CLAWD_FEED_TOKEN="$(cat ~/.clawd-feed/app_token)" sh packaging/build_mac.sh`

## First deploy

```sh
cd feed/worker
npx wrangler login
npx wrangler r2 bucket create clawd-fw                # private by default: do not enable public access
npx wrangler kv namespace create clawd-fw-keys        # put the id in wrangler.toml
openssl rand -base64 32 | npx wrangler secret put URL_SECRET
openssl rand -hex 24    | tee /dev/tty | npx wrangler secret put APP_TOKENS   # this is the app token
npx wrangler deploy                                   # -> https://clawd-fw-feed.<account>.workers.dev
```

The Clawd desktop build takes the URL and the token from `CLAWD_FEED_URL` /
`CLAWD_FEED_TOKEN`. In CI these are the repository secrets of the same names, and
`packaging/clawd.spec` writes them into the app. To test against a local feed instead, set
`feed_url` / `feed_token` in the config file or the environment.

## Tests

```sh
node --test feed/worker/test/*.test.mjs               # Worker logic, fake R2/KV
sh tools/release.sh --dirty --local /tmp/fwfeed --stable   # a release into a local store
(cd feed/worker && npx wrangler dev --local --persist-to /tmp/fwfeed --port 8799 \
   --var APP_TOKENS:test-token --var URL_SECRET:dev-secret-123)
# in clawd-desktop: the update job against a fake clock, over the real Worker
uv run python tools/feed_e2e.py --feed http://127.0.0.1:8799 --token test-token \
   --fw ../clawd-home/firmware/.pio/build/clawd/firmware.bin \
   --clawd ../clawd-home/firmware/data/clawd.pak --desk ../clawd-home/build/desk/desk.pak
```
