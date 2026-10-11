#!/usr/bin/env node
// desk_to_pak.js — render clawd-on-desk's animated Clawd SVGs into /desk.pak,
// the clock's full-screen "agent" animations (format CLDK v2).
//
// PERSONAL USE ONLY. The artwork belongs to its authors (clawd-on-desk
// assets/LICENSE: All Rights Reserved; the Clawd character is Anthropic's).
// This script reads a local clone and writes build/desk/ — both gitignored.
// Never commit the output or ship it to anyone.
//
// Pipeline, per animation:
//   1. open the SVG in headless Chrome, pause every CSS animation, read the
//      timing (intro = one-shot parts, loop = the repeating cycle), then step
//      currentTime at the animation's fps and screenshot the 30x30-unit window
//      at 2x; each 2x2 block becomes one panel pixel (dominant non-black
//      colour, or background). Renders are cached in build/desk/cache/.
//   2. merge the colours down to a 15-colour palette (+ black), collapse
//      identical consecutive frames into longer holds, crop to the union box;
//   3. encode every frame as a delta from the previous one (frame 0 from a
//      black screen) plus one "wrap" delta from the last frame back to the
//      loop start, so the clock only redraws what changed;
//   4. decode the result again and compare it with the render (--verify
//      fails the run when a frame is off by more than 2% of its pixels).
//
// Usage:
//   CLAWD_ON_DESK_DIR=/tmp/clawd-on-desk node tools/desk/desk_to_pak.js [--only a,b] [--probe] [--sheet]
// Output: build/desk/desk.pak, build/desk/desk.crc.json (+ build/desk/sheets/*.png with --sheet)
'use strict';

const fs = require('fs');
const path = require('path');
const zlib = require('zlib');
const crypto = require('crypto');
const { execFileSync } = require('child_process');
const { PNG } = require('pngjs');
const { ANIMS, SIZE, PX_PER_UNIT, WINDOW, FPS, MAX_MS } = require('./desk_anims');
const codec = require('./desk_codec');

const SS = 2;                                  // screenshot px per panel pixel
const VB = { x: -15, y: -25, w: 45, h: 45 };   // every clawd-on-desk Clawd SVG uses this viewBox
const PAGE_PX = VB.w * PX_PER_UNIT * SS;
const CLIP = { x: (WINDOW.x - VB.x) * PX_PER_UNIT * SS, y: (WINDOW.y - VB.y) * PX_PER_UNIT * SS, width: SIZE * SS, height: SIZE * SS };
const RENDER_VERSION = 2;                      // bump to invalidate build/desk/cache
const MAX_BAD = 0.02;                          // --verify: share of pixels a frame may get wrong

const root = path.resolve(__dirname, '..', '..');
const outDir = path.join(root, 'build', 'desk');
const cacheDir = path.join(outDir, 'cache');
const args = process.argv.slice(2);
const flag = (n) => args.includes(n);
const opt = (n) => { const i = args.indexOf(n); return i >= 0 ? args[i + 1] : null; };

function chromePath() {
  const c = [process.env.CHROME_PATH,
    '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
    '/usr/bin/google-chrome', '/usr/bin/chromium'].filter(Boolean);
  const p = c.find((x) => fs.existsSync(x));
  if (!p) throw new Error('Chrome not found; set CHROME_PATH');
  return p;
}

function svgDir() {
  const d = process.env.CLAWD_ON_DESK_DIR;
  if (!d) throw new Error('set CLAWD_ON_DESK_DIR to a local clawd-on-desk clone');
  const s = path.join(d, 'assets', 'svg');
  if (!fs.existsSync(s)) throw new Error(`no assets/svg in ${d}`);
  return s;
}

// ---- 1. render -------------------------------------------------------------------
// Clip timing from the CSS animations: one-shot parts (finite iterations) and
// start delays form the intro; the infinite ones repeat with the LCM of their
// periods (or the longest period when the LCM is too long to render).
function clipTiming(list, fps, maxMs) {
  let intro = 0;
  const periods = [];
  for (const a of list) {
    if (Number.isFinite(a.iterations)) intro = Math.max(intro, a.delay + a.duration * a.iterations);
    else if (a.duration > 0) { periods.push(a.period); intro = Math.max(intro, a.delay); }
  }
  const step = 1000 / fps;
  if (!periods.length) {                       // one-shot: play, then hold the last frame
    const total = Math.min(Math.max(intro, step), maxMs);
    return { introMs: total, loopMs: 0, once: true };
  }
  const gcd = (a, b) => (b ? gcd(b, a % b) : a);
  const q = periods.map((p) => Math.max(10, Math.round(p / 10) * 10));
  let lcm = q[0];
  for (const p of q.slice(1)) { lcm = (lcm / gcd(lcm, p)) * p; if (lcm > maxMs) break; }
  let loop = lcm <= maxMs ? lcm : Math.max(...q);
  intro = Math.round(intro / step) * step;
  loop = Math.min(loop, maxMs - Math.min(intro, maxMs / 2));
  if (intro + loop > maxMs) intro = Math.max(0, maxMs - loop);
  return { introMs: intro, loopMs: Math.max(loop, step), once: false };
}

async function renderFrames(page, svgText, fps, maxMs) {
  const svg = svgText
    .replace(/<\?xml[^>]*>/, '')
    .replace(/<svg\b([^>]*?)\swidth="[^"]*"/, '<svg$1')
    .replace(/<svg\b([^>]*?)\sheight="[^"]*"/, '<svg$1')
    .replace(/<svg\b/, `<svg width="${PAGE_PX}" height="${PAGE_PX}"`);
  await page.setContent(
    `<!doctype html><html><body style="margin:0;background:#000;overflow:hidden">${svg}</body></html>`,
    { waitUntil: 'load' });
  const list = await page.evaluate(() => document.getAnimations().map((a) => {
    a.pause();
    const t = a.effect.getTiming(), c = a.effect.getComputedTiming();
    const duration = Number(c.duration) || 0;
    const alt = /alternate/.test(t.direction || '');
    return { delay: t.delay || 0, duration, iterations: c.iterations,
      period: alt ? 2 * duration : duration };
  }).map((a) => ({ ...a, iterations: a.iterations === Infinity ? null : a.iterations })));
  for (const a of list) if (a.iterations === null) a.iterations = Infinity;
  const timing = clipTiming(list, fps, maxMs);
  const step = 1000 / fps;
  const total = timing.introMs + timing.loopMs;
  const n = Math.max(1, Math.round(total / step));
  const frames = [];
  for (let i = 0; i < n; i++) {
    const t = i * step;
    await page.evaluate((tt) => { for (const a of document.getAnimations()) a.currentTime = tt; }, t);
    const buf = await page.screenshot({ type: 'png', clip: CLIP });
    frames.push(toPixels(PNG.sync.read(buf)));
  }
  return { frames, stepMs: step, ...timing, loopStart: timing.once ? n - 1 : Math.round(timing.introMs / step) };
}

// One 0xRRGGBB (0 = background) per panel pixel: the dominant non-black colour
// of its SSxSS block when at least half the block is ink.
function toPixels(png) {
  const out = new Int32Array(SIZE * SIZE);
  for (let y = 0; y < SIZE; y++) {
    for (let x = 0; x < SIZE; x++) {
      const cols = [];
      for (let dy = 0; dy < SS; dy++) for (let dx = 0; dx < SS; dx++) {
        const o = ((y * SS + dy) * png.width + x * SS + dx) * 4;
        const r = png.data[o], g = png.data[o + 1], b = png.data[o + 2];
        if (r + g + b >= 24) cols.push((r << 16) | (g << 8) | b);
      }
      if (cols.length * 2 < SS * SS) continue;
      let best = cols[0], bestN = 0;
      for (const c of cols) { let k = 0; for (const d of cols) if (d === c) k++; if (k > bestN) { best = c; bestN = k; } }
      out[y * SIZE + x] = best || 1;
    }
  }
  return out;
}

// A cache file that does not parse is treated as a miss and re-rendered.
function readCache(file) {
  try {
    const raw = zlib.inflateSync(fs.readFileSync(file));
    const metaLen = raw.readUInt32LE(0);
    const meta = JSON.parse(raw.subarray(4, 4 + metaLen).toString());
    const body = raw.subarray(4 + metaLen);
    if (body.length !== meta.count * SIZE * SIZE * 4) return null;
    return { meta, body };
  } catch {
    return null;
  }
}

function cacheKey(def, svgText, fps, maxMs) {
  return crypto.createHash('sha1')
    .update(JSON.stringify({ v: RENDER_VERSION, SIZE, PX_PER_UNIT, WINDOW, SS, fps, maxMs }))
    .update(svgText).digest('hex').slice(0, 16) + '-' + def.name.replace(/\s+/g, '-');
}

async function renderCached(getPage, def, svgText) {
  const fps = def.fps || FPS, maxMs = def.maxMs || MAX_MS;
  const file = path.join(cacheDir, cacheKey(def, svgText, fps, maxMs) + '.bin');
  const cached = fs.existsSync(file) && !flag('--no-cache') ? readCache(file) : null;
  if (cached) {
    const { meta, body } = cached;
    const frames = [];
    for (let i = 0; i < meta.count; i++) {
      const b = body.subarray(i * SIZE * SIZE * 4, (i + 1) * SIZE * SIZE * 4);
      frames.push(new Int32Array(b.buffer.slice(b.byteOffset, b.byteOffset + b.length)));
    }
    return { ...meta, frames };
  }
  const r = await renderFrames(await getPage(), svgText, fps, maxMs);
  const meta = { count: r.frames.length, stepMs: r.stepMs, introMs: r.introMs, loopMs: r.loopMs,
    once: r.once, loopStart: r.loopStart };
  const m = Buffer.from(JSON.stringify(meta));
  const head = Buffer.alloc(4); head.writeUInt32LE(m.length, 0);
  fs.mkdirSync(cacheDir, { recursive: true });
  fs.writeFileSync(file, zlib.deflateSync(Buffer.concat([head, m, ...r.frames.map((f) => Buffer.from(f.buffer))]), { level: 1 }));
  return r;
}

// ---- 2. palette -----------------------------------------------------------------
const rgb565 = (c) => (((c >> 16) & 0xf8) << 8) | (((c >> 8) & 0xfc) << 3) | ((c & 0xff) >> 3);
function dist(a, b) {
  const dr = (a >> 16) - (b >> 16), dg = ((a >> 8) & 255) - ((b >> 8) & 255), db = (a & 255) - (b & 255);
  return 2 * dr * dr + 4 * dg * dg + 3 * db * db;
}
// Reduce to 15 colours + black. Colours that look the same on the RGB565 panel
// are folded first; then, repeatedly, the colour whose merge into its nearest
// neighbour (black included) costs the least error — pixel count x distance —
// is merged. So near-duplicate edge shades go first while a rare but distinct
// colour (a grey fade frame) survives. Survivors keep their exact value (never
// averaged), so flat art stays flat.
function buildPalette(frames) {
  const count = new Map();
  for (const f of frames) for (const c of f) if (c) count.set(c, (count.get(c) || 0) + 1);
  const bucket = new Map();                     // rgb565 -> [representative, count]
  const toBucket = new Map();
  for (const [c, n] of count) {
    const k = rgb565(c);
    const b = bucket.get(k);
    if (!b) bucket.set(k, [c, n]);
    else { if (n > count.get(b[0])) b[0] = c; b[1] += n; }
    toBucket.set(c, k);
  }
  const keys = [...bucket.values()].map((b) => b[0]);
  const n = keys.length;
  const cnt = Float64Array.from([...bucket.values()].map((b) => b[1]));
  const alive = new Uint8Array(n).fill(1);
  const nn = new Int32Array(n), nd = new Float64Array(n);
  const nearest = (i) => {
    let best = -1, bd = dist(0, keys[i]);
    for (let j = 0; j < n; j++) if (j !== i && alive[j]) { const d = dist(keys[i], keys[j]); if (d < bd) { bd = d; best = j; } }
    nn[i] = best; nd[i] = bd;
  };
  for (let i = 0; i < n; i++) nearest(i);
  let live = n;
  while (live > 15) {
    let r = -1, rc = Infinity;
    for (let i = 0; i < n; i++) if (alive[i] && cnt[i] * nd[i] < rc) { rc = cnt[i] * nd[i]; r = i; }
    const t = nn[r];
    alive[r] = 0; live--;
    if (t >= 0) cnt[t] += cnt[r];
    for (let i = 0; i < n; i++) if (alive[i] && nn[i] === r) nearest(i);
  }
  const survivors = [];
  for (let i = 0; i < n; i++) if (alive[i]) survivors.push(i);
  survivors.sort((a, b) => cnt[b] - cnt[a]);
  const palette = [0, ...survivors.map((i) => keys[i])];
  // Every source colour goes to its nearest palette entry (black included), not
  // to wherever its merge chain ended: chains drift.
  const lut = new Map();
  for (const c of count.keys()) {
    let best = 0, bd = dist(0, c);
    for (let k = 1; k < palette.length; k++) { const d = dist(palette[k], c); if (d < bd) { bd = d; best = k; } }
    lut.set(c, best);
  }
  return { palette, lut };
}

// ---- 3. pack one animation ------------------------------------------------------
function packAnim(def, r) {
  const { palette, lut } = buildPalette(r.frames);
  const full = r.frames.map((f) => Uint8Array.from(f, (c) => (c ? lut.get(c) : 0)));
  // collapse identical consecutive frames (never across the loop start)
  const frames = [], holds = [], src = [];
  let loopStart = 0;
  full.forEach((f, i) => {
    const last = frames[frames.length - 1];
    if (i !== r.loopStart && last && Buffer.compare(Buffer.from(last), Buffer.from(f)) === 0) {
      holds[holds.length - 1] += r.stepMs;
    } else {
      if (i === r.loopStart) loopStart = frames.length;
      frames.push(f); holds.push(r.stepMs); src.push(r.frames[i]);
    }
  });
  // union crop
  let x0 = SIZE, y0 = SIZE, x1 = -1, y1 = -1;
  for (const f of frames) for (let i = 0; i < f.length; i++) if (f[i]) {
    const x = i % SIZE, y = (i / SIZE) | 0;
    if (x < x0) x0 = x; if (x > x1) x1 = x; if (y < y0) y0 = y; if (y > y1) y1 = y;
  }
  if (x1 < 0) throw new Error(`${def.name}: empty`);
  const w = x1 - x0 + 1, h = y1 - y0 + 1;
  const crop = (f) => { const o = new Uint8Array(w * h); for (let y = 0; y < h; y++) o.set(f.subarray((y0 + y) * SIZE + x0, (y0 + y) * SIZE + x0 + w), y * w); return o; };
  const cropped = frames.map(crop);
  const enc = codec.encodeAnim(cropped, loopStart, w);
  return {
    name: def.name, x: x0, y: y0, w, h, frames: enc, cropFrames: cropped, src, loopStart, once: r.once,
    holds: holds.map((x) => Math.min(65535, Math.round(x))),
    palette: palette.map(rgb565), paletteRgb: palette,
    srcColors: new Set(r.frames.flatMap((f) => [...new Set(f)])).size - 1,
    timing: `${Math.round(r.introMs)}+${Math.round(r.loopMs)} ms${r.once ? ' once' : ''}`,
  };
}

// ---- 4. verify --------------------------------------------------------------------
// Decode the packed frames (the same algorithm as the clock) and compare each
// with the Chrome render (pixelBad).
// Visibly wrong: off by more than ~40 levels per channel. Background is black,
// so a near-black fade drawn as background is fine.
function pixelBad(want, got) {
  return dist(want, got) > 9 * 40 * 40;
}

function verifyAnim(a) {
  const states = codec.decodeAnim(a.frames, a.w * a.h, a.w);
  let worst = 0, worstAt = 0, overFrames = 0;
  states.slice(0, a.cropFrames.length).forEach((s, i) => {
    if (Buffer.compare(Buffer.from(s), Buffer.from(a.cropFrames[i])) !== 0) throw new Error(`${a.name}: codec mismatch at frame ${i}`);
    let bad = 0, ink = 0;
    for (let y = 0; y < a.h; y++) for (let x = 0; x < a.w; x++) {
      const want = a.src[i][(a.y + y) * SIZE + a.x + x], got = a.paletteRgb[s[y * a.w + x]];
      if (want) ink++;
      if (pixelBad(want, got)) bad++;
    }
    const share = bad / Math.max(ink, 1);
    if (share > worst) { worst = share; worstAt = i; }
    if (share > MAX_BAD) overFrames++;
  });
  const wrapped = states[states.length - 1];
  if (Buffer.compare(Buffer.from(wrapped), Buffer.from(a.cropFrames[a.loopStart])) !== 0) throw new Error(`${a.name}: wrap does not land on the loop start`);
  return { worst, worstAt, overFrames, crc: states.map((s) => codec.crc32(s)) };
}

// ---- pack file ---------------------------------------------------------------------
// Little-endian.
//   header  "CLDK", u16 version (2), u16 count, u16 size (240), u16 reserved,
//           u32 reserved                                                   16 B
//   desc    name[16], u8 x, y, w, h (crop on the panel), u16 frameCount,
//           u16 loopStart, u16 maxFrameBytes, u8 flags (bit 0: once),
//           u8 reserved, u16 palette[16] (RGB565), u32 indexOff            64 B
//   index   u32 offsets[frameCount + 2]: frame i = [off[i], off[i+1]) for
//           i < frameCount, then the wrap delta (last frame -> loopStart)
//   holds   u16 ms[frameCount]
//   frames  delta ops over the w x h crop, row-major (see desk_codec.js)
function writePak(anims) {
  const head = Buffer.alloc(16);
  head.write('CLDK', 0, 'latin1');
  head.writeUInt16LE(2, 4);
  head.writeUInt16LE(anims.length, 6);
  head.writeUInt16LE(SIZE, 8);
  const descs = [], bodies = [];
  let off = 16 + 64 * anims.length;
  for (const a of anims) {
    const n = a.frames.length - 1;             // + the wrap delta
    const index = Buffer.alloc(4 * (n + 2));
    const holds = Buffer.alloc(2 * n);
    let fo = off + index.length + holds.length;
    a.frames.forEach((f, i) => { index.writeUInt32LE(fo, 4 * i); fo += f.length; });
    index.writeUInt32LE(fo, 4 * (n + 1));
    a.holds.forEach((h, i) => holds.writeUInt16LE(h, 2 * i));
    const d = Buffer.alloc(64);
    d.write(a.name.slice(0, 15), 0, 'latin1');
    d.writeUInt8(a.x, 16); d.writeUInt8(a.y, 17); d.writeUInt8(a.w, 18); d.writeUInt8(a.h, 19);
    d.writeUInt16LE(n, 20); d.writeUInt16LE(a.loopStart, 22);
    d.writeUInt16LE(Math.max(...a.frames.map((f) => f.length)), 24);
    d.writeUInt8(a.once ? 1 : 0, 26);
    for (let k = 0; k < 16; k++) d.writeUInt16LE(a.palette[k] || 0, 28 + 2 * k);
    d.writeUInt32LE(off, 60);
    descs.push(d);
    const body = Buffer.concat([index, holds, ...a.frames]);
    bodies.push(body);
    off += body.length;
  }
  return Buffer.concat([head, ...descs, ...bodies]);
}

// ---- contact sheets (QA) -----------------------------------------------------------
function writeSheet(a, file) {
  const pick = [...new Set([0, 1, 2, 3, 4, 5].map((k) => Math.round(k * (a.cropFrames.length - 1) / 5)))];
  const W = SIZE, png = new PNG({ width: pick.length * (W + 4), height: W });
  png.data.fill(40);
  pick.forEach((fi, j) => {
    const f = a.cropFrames[fi], bx = j * (W + 4);
    for (let y = 0; y < SIZE; y++) for (let x = 0; x < SIZE; x++) {
      const cx = x - a.x, cy = y - a.y;
      const c = cx >= 0 && cy >= 0 && cx < a.w && cy < a.h ? a.paletteRgb[f[cy * a.w + cx]] : 0;
      const o = (y * png.width + bx + x) * 4;
      png.data[o] = c >> 16; png.data[o + 1] = (c >> 8) & 255; png.data[o + 2] = c & 255; png.data[o + 3] = 255;
    }
  });
  fs.writeFileSync(file, PNG.sync.write(png));
}

// Render | packed | diff (red = wrong pixel) of one frame at 2x, for QA.
function writeWorst(a, fi, file) {
  const S = 2, W = SIZE * S, png = new PNG({ width: 3 * W + 8, height: W });
  png.data.fill(40);
  const put = (bx, x, y, c) => {
    for (let dy = 0; dy < S; dy++) for (let dx = 0; dx < S; dx++) {
      const o = ((y * S + dy) * png.width + bx + x * S + dx) * 4;
      png.data[o] = c >> 16; png.data[o + 1] = (c >> 8) & 255; png.data[o + 2] = c & 255; png.data[o + 3] = 255;
    }
  };
  const f = a.cropFrames[fi];
  for (let y = 0; y < SIZE; y++) for (let x = 0; x < SIZE; x++) {
    const want = a.src[fi][y * SIZE + x];
    const cx = x - a.x, cy = y - a.y;
    const got = cx >= 0 && cy >= 0 && cx < a.w && cy < a.h ? a.paletteRgb[f[cy * a.w + cx]] : 0;
    put(0, x, y, want);
    put(W + 4, x, y, got);
    put(2 * W + 8, x, y, pixelBad(want, got) ? 0xff0000 : (want ? 0x404040 : 0));
  }
  fs.writeFileSync(file, PNG.sync.write(png));
}

async function main() {
  const dir = svgDir();
  const only = opt('--only');
  const defs = ANIMS.filter((d) => !only || only.split(',').includes(d.name));
  if (!defs.length) throw new Error('no animations selected');
  fs.mkdirSync(path.join(outDir, 'sheets'), { recursive: true });
  let browser = null, page = null;
  const getPage = async () => {
    if (page) return page;
    const puppeteer = require('puppeteer-core');
    browser = await puppeteer.launch({ executablePath: chromePath(), headless: true,
      args: ['--force-device-scale-factor=1', '--hide-scrollbars'] });
    page = await browser.newPage();
    await page.setViewport({ width: PAGE_PX, height: PAGE_PX, deviceScaleFactor: 1 });
    return page;
  };
  const anims = [], crcs = {};
  let failed = 0;
  try {
    for (const def of defs) {
      const text = fs.readFileSync(path.join(dir, def.svg), 'utf8');
      if (flag('--probe')) {
        const r = await renderFrames(await getPage(), text, def.fps || FPS, def.maxMs || MAX_MS);
        let x0 = 1e9, y0 = 1e9, x1 = -1, y1 = -1, edge = 0;
        for (const f of r.frames) for (let i = 0; i < f.length; i++) if (f[i]) {
          const x = i % SIZE, y = (i / SIZE) | 0;
          if (x < x0) x0 = x; if (x > x1) x1 = x; if (y < y0) y0 = y; if (y > y1) y1 = y;
          if (x === 0 || y === 0 || x === SIZE - 1 || y === SIZE - 1) edge++;
        }
        console.log(`${def.name.padEnd(14)} ${r.frames.length} frames intro ${r.introMs} loop ${r.loopMs}${r.once ? ' once' : ''}  ` +
          `ink x ${x0}..${x1} y ${y0}..${y1}${edge ? `  CLIPPED (${edge} edge px)` : ''}`);
        continue;
      }
      const r = await renderCached(getPage, def, text);
      const a = packAnim(def, r);
      const v = verifyAnim(a);
      crcs[a.name] = v.crc;
      anims.push(a);
      const bytes = a.frames.reduce((s, f) => s + f.length, 0);
      const bad = v.worst > MAX_BAD;
      if (bad) failed++;
      console.log(`${a.name.padEnd(14)} ${String(a.frames.length - 1).padStart(3)} frames ${a.timing.padEnd(16)} ` +
        `${a.w}x${a.h}@${a.x},${a.y}  ${a.srcColors} -> ${a.paletteRgb.length - 1} colours  ` +
        `${String(bytes).padStart(7)} B (max frame ${Math.max(...a.frames.map((f) => f.length))} B)  ` +
        `worst ${(v.worst * 100).toFixed(2)}% @${v.worstAt}${bad ? `  FAIL (${v.overFrames} frames)` : ''}`);
      if (flag('--sheet')) {
        const base = path.join(outDir, 'sheets', a.name.replace(/\s+/g, '-'));
        writeSheet(a, `${base}.png`);
        writeWorst(a, v.worstAt, `${base}-worst.png`);
      }
    }
  } finally {
    if (browser) await browser.close();
  }
  if (flag('--probe')) return;
  const pak = writePak(anims);
  const file = path.join(outDir, 'desk.pak');
  fs.writeFileSync(file, pak);
  fs.writeFileSync(path.join(outDir, 'desk.crc.json'), JSON.stringify(crcs));
  let rev = '';
  try { rev = execFileSync('git', ['-C', process.env.CLAWD_ON_DESK_DIR, 'rev-parse', '--short', 'HEAD']).toString().trim(); } catch {}
  console.log(`${path.relative(root, file)}: ${anims.length} animations, ${pak.length} bytes (clawd-on-desk ${rev || '?'})`);
  if (failed && flag('--verify')) { console.error(`${failed} animation(s) over ${MAX_BAD * 100}% bad pixels`); process.exit(1); }
}

main().catch((e) => { console.error(e.message || e); process.exit(1); });
