#!/usr/bin/env node
// Build one encrypted firmware release for the feed (see README.md). No dependencies.
//
//   node feed/release.mjs pack --src firmware --fw firmware/.pio/build/clawd/firmware.bin \
//        --clawd firmware/data/clawd.pak --desk build/desk/desk.pak --out /tmp/rel \
//        [--notes "..."] [--commit abc1234] [--renderer win-x64=build/renderer/win-x64.zip ...]
//
// --renderer (repeatable): the settings window's clock-screen renderer for one platform, the
// zip clawd-desktop's tools/build_preview.py --zip makes. It goes in the manifest's own
// `renderers` list, not `files`: apps from before it refuse a file of a kind they do not know.
//
// Writes <out>/rel/<seq>/<name>.enc (AES-256-GCM: 12-byte nonce | ciphertext | 16-byte tag,
// AAD "clawd-fw/<seq>/<name>"), <out>/rel/<seq>/manifest.json and <out>/key.b64 (the
// release key, for the Worker's KV only), then prints the file list for tools/release.sh.
import { createCipheriv, createHash, randomBytes } from "node:crypto";
import { mkdirSync, readFileSync, writeFileSync, chmodSync } from "node:fs";
import { dirname, join } from "node:path";
import { parseArgs } from "node:util";

export const OTA_MAX = 524_288;
export const VARIANT = "esp8266-clawd";
export const RENDERER_MAX = 32 * 1024 * 1024;        // the app's feed.RENDERER_MAX
export const RENDERER_PLATFORMS = ["win-x64", "mac", "linux-x64"];   // the app's feed.RENDERER_PLATFORMS

// zlib CRC-32 (what the clock reports for its packs and the style index uses).
const TABLE = (() => {
  const t = new Uint32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    t[n] = c >>> 0;
  }
  return t;
})();
export function crc32(buf) {
  let c = 0xffffffff;
  for (const b of buf) c = TABLE[(c ^ b) & 0xff] ^ (c >>> 8);
  return ((c ^ 0xffffffff) >>> 0).toString(16).padStart(8, "0");
}

export function aad(seq, name) {
  return Buffer.from(`clawd-fw/${seq}/${name}`);
}

export function encrypt(key, seq, name, data) {
  const iv = randomBytes(12);
  const c = createCipheriv("aes-256-gcm", key, iv);
  c.setAAD(aad(seq, name));
  const body = Buffer.concat([c.update(data), c.final()]);
  return Buffer.concat([iv, body, c.getAuthTag()]);
}

// fwSeq, layouts and styleApi from the firmware source, like clawd-desktop's bundle_firmware.py.
export function fromSource(src) {
  const cfg = readFileSync(join(src, "src/config.h"), "utf8");
  const seq = /#define\s+CLAWD_FW_SEQ\s+(\d+)/.exec(cfg);
  if (!seq) throw new Error("no CLAWD_FW_SEQ in src/config.h");
  const ui = readFileSync(join(src, "src/features/clawd/ClawdUi.h"), "utf8").replace(/\\\r?\n/g, " ");
  const en = Object.fromEntries([...ui.matchAll(/(CLAWD_LAYOUT_[A-Z]+)\s*=\s*(\d+)/g)].map((m) => [m[1], +m[2]]));
  const d = /#define\s+CLAWD_LAYOUTS\s+(.+)/.exec(ui);
  if (!d) throw new Error("no CLAWD_LAYOUTS in ClawdUi.h");
  const names = [...d[1].matchAll(/1u?\s*<<\s*(CLAWD_LAYOUT_[A-Z]+)/g)].map((m) => m[1]);
  if (!names.length || names.some((n) => !(n in en))) throw new Error(`cannot read CLAWD_LAYOUTS: ${d[1].trim()}`);
  const layouts = [...new Set(names)].reduce((a, n) => a | (1 << en[n]), 0);
  const sp = /#define\s+STYLE_API\s+(\d+)/.exec(readFileSync(join(src, "src/features/clawd/StylePack.h"), "utf8"));
  return { seq: +seq[1], layouts, styleApi: sp ? +sp[1] : 0 };
}

function check(cond, msg) {
  if (!cond) throw new Error(msg);
}

// Every file of a release, checked: [{name, kind, data}].
export function collect({ src, fw, clawd, desk, styleApi }) {
  const out = [];
  const image = readFileSync(fw);
  check(image.length > 0 && image[0] === 0xe9, `${fw}: not an ESP8266 app image`);
  check(image.length <= OTA_MAX, `${fw}: ${image.length} B is over the ${OTA_MAX} B OTA budget`);
  out.push({ name: "firmware.bin", kind: "fw", data: image });
  const cp = readFileSync(clawd);
  check(cp.subarray(0, 4).toString() === "CLWD" && cp.readUInt16LE(4) === 1, `${clawd}: not a clawd.pak v1`);
  out.push({ name: "clawd.pak", kind: "clawdpak", data: cp });
  const dp = readFileSync(desk);
  check(dp.subarray(0, 4).toString() === "CLDK" && [2, 3].includes(dp.readUInt16LE(4)), `${desk}: not a desk.pak v2/v3`);
  out.push({ name: "desk.pak", kind: "deskpak", data: dp });
  if (styleApi) {
    const dir = join(src, "styles");
    const raw = readFileSync(join(dir, "index.json"));
    const index = JSON.parse(raw);
    check(index.api === styleApi, `styles/index.json is for style API ${index.api}, the image draws ${styleApi}`);
    out.push({ name: "styles/index.json", kind: "styles", data: raw });
    for (const e of index.styles) {
      check(/^[a-z0-9_-]{1,15}\.pak$/.test(e.file), `styles: bad file name ${e.file}`);
      const p = readFileSync(join(dir, e.file));
      check(p.length === e.size && p.subarray(0, 4).toString() === "CSTY" && crc32(p.subarray(16)) === e.crc,
            `styles/${e.file} does not match index.json: rebuild the packs`);
      out.push({ name: `styles/${e.file}`, kind: "style", data: p });
    }
  }
  return out;
}

// The renderer zips, checked: [{name, kind, platform, data}]. `specs`: "platform=path" strings.
export function collectRenderers(specs = []) {
  const out = [];
  for (const s of specs) {
    const i = s.indexOf("=");
    check(i > 0, `--renderer ${s}: give platform=path`);
    const platform = s.slice(0, i), path = s.slice(i + 1);
    check(RENDERER_PLATFORMS.includes(platform), `--renderer: unknown platform ${platform} (${RENDERER_PLATFORMS.join(", ")})`);
    check(!out.some((r) => r.platform === platform), `--renderer: ${platform} given twice`);
    const data = readFileSync(path);
    check(data.length > 0 && data.length <= RENDERER_MAX, `${path}: ${data.length} B (max ${RENDERER_MAX})`);
    check(data.readUInt32LE(0) === 0x04034b50, `${path}: not a zip`);
    out.push({ name: `renderer/${platform}.zip`, kind: "renderer", platform, data });
  }
  return out;
}

export function pack({ src, fw, clawd, desk, out, notes = "", commit = "", date = new Date().toISOString(), key,
                       renderer = [] }) {
  const meta = fromSource(src);
  const files = collect({ src, fw, clawd, desk, styleApi: meta.styleApi });
  const renderers = collectRenderers(renderer);
  key = key || randomBytes(32);
  const dir = join(out, "rel", String(meta.seq));
  const manifest = {
    v: 1, seq: meta.seq, date, notes, variant: VARIANT, layouts: meta.layouts, styleApi: meta.styleApi, commit,
    files: [],
  };
  if (renderers.length) manifest.renderers = [];
  for (const f of [...files, ...renderers]) {
    const blob = encrypt(key, meta.seq, f.name, f.data);
    const path = join(dir, f.name + ".enc");
    mkdirSync(dirname(path), { recursive: true });
    writeFileSync(path, blob);
    const row = {
      name: f.name, kind: f.kind, size: f.data.length, enc: blob.length,
      sha256: createHash("sha256").update(f.data).digest("hex"), crc: crc32(f.data),
    };
    if (f.platform) manifest.renderers.push({ ...row, platform: f.platform });
    else manifest.files.push(row);
  }
  writeFileSync(join(dir, "manifest.json"), JSON.stringify(manifest, null, 2) + "\n");
  writeFileSync(join(out, "key.b64"), key.toString("base64"));
  chmodSync(join(out, "key.b64"), 0o600);
  return manifest;
}

if (import.meta.url === `file://${process.argv[1]}`) {
  const [cmd, ...rest] = process.argv.slice(2);
  const { values: a } = parseArgs({
    args: rest,
    options: { src: { type: "string" }, fw: { type: "string" }, clawd: { type: "string" }, desk: { type: "string" },
               out: { type: "string" }, notes: { type: "string", default: "" }, commit: { type: "string", default: "" },
               renderer: { type: "string", multiple: true, default: [] } },
  });
  try {
    if (cmd === "seq") {
      console.log(fromSource(a.src).seq);
    } else if (cmd === "pack") {
      for (const k of ["src", "fw", "clawd", "desk", "out"]) check(a[k], `--${k} is required`);
      const m = pack(a);
      for (const f of [...m.files, ...(m.renderers || [])]) console.log(`rel/${m.seq}/${f.name}.enc`);
    } else {
      throw new Error("usage: release.mjs seq --src DIR | pack --src DIR --fw BIN --clawd PAK --desk PAK --out DIR");
    }
  } catch (e) {
    console.error(`release: ${e.message}`);
    process.exit(1);
  }
}
