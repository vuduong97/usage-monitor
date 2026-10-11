#!/usr/bin/env node
// desk_to_pak.js — render clawd-on-desk's animated Clawd SVGs into /desk.pak,
// the clock's full-screen "agent" animations.
//
// PERSONAL USE ONLY. The artwork belongs to its authors (clawd-on-desk
// assets/LICENSE: All Rights Reserved; the Clawd character is Anthropic's).
// This script reads a local clone and writes build/desk/ — both gitignored.
// Never commit the output or ship it to anyone.
//
// Pipeline, per animation:
//   1. open the SVG in headless Chrome on a black page (the clock's background),
//      pause every CSS animation and step currentTime at FPS;
//   2. screenshot each step at SS px per output cell and reduce every SSxSS
//      block to one colour (dominant non-black colour if it covers enough of
//      the block, otherwise background);
//   3. merge the colours down to a 15-colour palette (+ black), collapse
//      identical consecutive frames into longer holds, crop to the union box;
//   4. run-length encode each frame (nibble colour, nibble run-1).
//
// Usage:
//   CLAWD_ON_DESK_DIR=/tmp/clawd-on-desk node tools/desk/desk_to_pak.js [--only a,b] [--probe] [--sheet]
// Output: build/desk/desk.pak (+ build/desk/sheets/*.png with --sheet)
'use strict';

const fs = require('fs');
const path = require('path');
const { execFileSync } = require('child_process');
const { PNG } = require('pngjs');
const puppeteer = require('puppeteer-core');
const { ANIMS, GRID, CPU, WINDOW, FPS, MAX_MS, MAX_CELLS } = require('./desk_anims');

const SS = 8;                                  // screenshot px per output cell
const VB = { x: -15, y: -25, w: 45, h: 45 };   // every clawd-on-desk Clawd SVG uses this viewBox
const PX_PER_UNIT = CPU * SS;
const PAGE_PX = VB.w * PX_PER_UNIT;
const MIN_COVER = 0.4;                         // non-black share of a block that makes it ink

const root = path.resolve(__dirname, '..', '..');
const outDir = path.join(root, 'build', 'desk');
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
async function renderFrames(page, svgText) {
  const svg = svgText
    .replace(/<\?xml[^>]*>/, '')
    .replace(/<svg\b([^>]*?)\swidth="[^"]*"/, '<svg$1')
    .replace(/<svg\b([^>]*?)\sheight="[^"]*"/, '<svg$1')
    .replace(/<svg\b/, `<svg width="${PAGE_PX}" height="${PAGE_PX}"`);
  await page.setContent(
    `<!doctype html><html><body style="margin:0;background:#000;overflow:hidden">${svg}</body></html>`,
    { waitUntil: 'load' });
  const totalMs = await page.evaluate((maxMs) => {
    const anims = document.getAnimations();
    let ms = 0;
    for (const a of anims) {
      a.pause();
      const t = a.effect.getComputedTiming();
      const span = Number.isFinite(t.endTime) ? t.endTime : (t.delay || 0) + t.duration;
      if (Number.isFinite(span)) ms = Math.max(ms, span);
    }
    return Math.min(Math.max(ms, 100), maxMs);
  }, MAX_MS);
  const step = 1000 / FPS;
  const n = Math.max(1, Math.round(totalMs / step));
  const frames = [];
  for (let i = 0; i < n; i++) {
    const t = i * step;
    await page.evaluate((tt) => { for (const a of document.getAnimations()) a.currentTime = tt; }, t);
    const buf = await page.screenshot({ type: 'png', clip: { x: 0, y: 0, width: PAGE_PX, height: PAGE_PX } });
    frames.push(PNG.sync.read(buf));
  }
  return { frames, stepMs: step, totalMs };
}

// ---- 2. cells ---------------------------------------------------------------------
// Returns a GRID x GRID array of 0xRRGGBB (0 = background) for the window.
function toCells(png) {
  const cells = new Int32Array(GRID * GRID);
  const ux0 = WINDOW.x - VB.x, uy0 = WINDOW.y - VB.y;   // window origin in page units
  for (let gy = 0; gy < GRID; gy++) {
    for (let gx = 0; gx < GRID; gx++) {
      const px0 = Math.round(ux0 * PX_PER_UNIT) + gx * SS;
      const py0 = Math.round(uy0 * PX_PER_UNIT) + gy * SS;
      const hist = new Map();
      let ink = 0;
      for (let y = py0; y < py0 + SS; y++) {
        for (let x = px0; x < px0 + SS; x++) {
          if (x < 0 || y < 0 || x >= png.width || y >= png.height) continue;
          const o = (y * png.width + x) * 4;
          const r = png.data[o], g = png.data[o + 1], b = png.data[o + 2];
          if (r + g + b < 24) continue;                 // background (or near it)
          ink++;
          const k = (r << 16) | (g << 8) | b;
          hist.set(k, (hist.get(k) || 0) + 1);
        }
      }
      if (ink < MIN_COVER * SS * SS) continue;
      let best = 0, bestN = -1;
      for (const [k, v] of hist) if (v > bestN) { best = k; bestN = v; }
      cells[gy * GRID + gx] = best;
    }
  }
  return cells;
}

// ---- 3. palette -----------------------------------------------------------------
function dist(a, b) {
  const dr = (a >> 16) - (b >> 16), dg = ((a >> 8) & 255) - ((b >> 8) & 255), db = (a & 255) - (b & 255);
  return 2 * dr * dr + 4 * dg * dg + 3 * db * db;
}
function buildPalette(frames) {
  const count = new Map();
  for (const f of frames) for (const c of f) if (c) count.set(c, (count.get(c) || 0) + 1);
  // Merge the rarest colour into its nearest neighbour until 15 remain. Colours
  // are kept exact (never averaged) so flat pixel art stays flat.
  const map = new Map([...count.keys()].map((k) => [k, k]));
  const live = new Map(count);
  while (live.size > 15) {
    let rare = null, rareN = Infinity;
    for (const [k, v] of live) if (v < rareN) { rare = k; rareN = v; }
    let near = null, nearD = Infinity;
    for (const k of live.keys()) if (k !== rare) { const d = dist(k, rare); if (d < nearD) { near = k; nearD = d; } }
    live.set(near, live.get(near) + rareN);
    live.delete(rare);
    for (const [k, v] of map) if (v === rare) map.set(k, near);
  }
  const palette = [0, ...[...live.keys()].sort((a, b) => live.get(b) - live.get(a))];
  const index = new Map(palette.map((c, i) => [c, i]));
  const lut = (c) => (c ? index.get(map.get(c)) : 0);
  return { palette, lut };
}
const rgb565 = (c) => (((c >> 16) & 0xf8) << 8) | (((c >> 8) & 0xfc) << 3) | ((c & 0xff) >> 3);

// ---- 4. pack one animation ------------------------------------------------------
function packAnim(def, rendered) {
  const cellFrames = rendered.frames.map(toCells);
  const { palette, lut } = buildPalette(cellFrames);
  let idx = cellFrames.map((f) => Uint8Array.from(f, lut));
  // collapse identical consecutive frames
  const frames = [], holds = [];
  for (const f of idx) {
    const last = frames[frames.length - 1];
    if (last && Buffer.compare(Buffer.from(last), Buffer.from(f)) === 0) holds[holds.length - 1] += rendered.stepMs;
    else { frames.push(f); holds.push(rendered.stepMs); }
  }
  // union crop
  let x0 = GRID, y0 = GRID, x1 = -1, y1 = -1;
  for (const f of frames) for (let i = 0; i < f.length; i++) if (f[i]) {
    const x = i % GRID, y = (i / GRID) | 0;
    if (x < x0) x0 = x; if (x > x1) x1 = x; if (y < y0) y0 = y; if (y > y1) y1 = y;
  }
  if (x1 < 0) throw new Error(`${def.name}: empty`);
  const w = x1 - x0 + 1, h = y1 - y0 + 1;
  if (w * h > MAX_CELLS) throw new Error(`${def.name}: crop ${w}x${h} exceeds ${MAX_CELLS} cells`);
  const enc = frames.map((f) => {
    const out = [];
    let cur = -1, run = 0;
    const flush = () => { while (run > 0) { const r = Math.min(run, 16); out.push((cur << 4) | (r - 1)); run -= r; } };
    for (let y = y0; y <= y1; y++) for (let x = x0; x <= x1; x++) {
      const v = f[y * GRID + x];
      if (v === cur) run++;
      else { flush(); cur = v; run = 1; }
    }
    flush();
    return Buffer.from(out);
  });
  return {
    name: def.name, w, h, ox: x0, oy: y0, frames: enc, cropFrames: frames,
    holds: holds.map((x) => Math.min(65535, Math.round(x))),
    palette: palette.map(rgb565), paletteRgb: palette,
    srcColors: new Set(cellFrames.flatMap((f) => [...f])).size - 1,
  };
}

// ---- pack file ---------------------------------------------------------------------
// Little-endian.
//   header  "CLDK", u16 version (1), u16 count, u8 grid, u8 cells/unit,
//           u16 reserved, u32 reserved                                    16 B
//   desc    name[16], u8 w, h, ox, oy, u16 frameCount, loopStart, loopEnd,
//           u16 maxFrameBytes (largest RLE frame), u16 palette[16] (RGB565),
//           u32 indexOff                                                    64 B
//   index   u32 offsets[frameCount + 1] (RLE frame i = [off[i], off[i+1]))
//   holds   u16 ms[frameCount]
//   frames  RLE: byte = colour << 4 | (run - 1), row-major over the w x h crop
function writePak(anims) {
  const head = Buffer.alloc(16);
  head.write('CLDK', 0, 'latin1');
  head.writeUInt16LE(1, 4);
  head.writeUInt16LE(anims.length, 6);
  head.writeUInt8(GRID, 8);
  head.writeUInt8(CPU, 9);
  const descs = [], bodies = [];
  let off = 16 + 64 * anims.length;
  for (const a of anims) {
    const n = a.frames.length;
    const index = Buffer.alloc(4 * (n + 1));
    const holds = Buffer.alloc(2 * n);
    let fo = off + index.length + holds.length;
    a.frames.forEach((f, i) => { index.writeUInt32LE(fo, 4 * i); fo += f.length; });
    index.writeUInt32LE(fo, 4 * n);
    a.holds.forEach((h, i) => holds.writeUInt16LE(h, 2 * i));
    const d = Buffer.alloc(64);
    d.write(a.name.slice(0, 15), 0, 'latin1');
    d.writeUInt8(a.w, 16); d.writeUInt8(a.h, 17); d.writeUInt8(a.ox, 18); d.writeUInt8(a.oy, 19);
    d.writeUInt16LE(n, 20); d.writeUInt16LE(0, 22); d.writeUInt16LE(n - 1, 24);
    d.writeUInt16LE(Math.max(...a.frames.map((f) => f.length)), 26);
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
  const cell = 4, cols = Math.min(8, a.cropFrames.length);
  const rows = Math.ceil(a.cropFrames.length / cols);
  const W = GRID * cell, png = new PNG({ width: cols * (W + 4), height: rows * (W + 4) });
  png.data.fill(40);
  a.cropFrames.forEach((f, i) => {
    const bx = (i % cols) * (W + 4), by = ((i / cols) | 0) * (W + 4);
    for (let gy = 0; gy < GRID; gy++) for (let gx = 0; gx < GRID; gx++) {
      const c = a.paletteRgb[f[gy * GRID + gx]];
      for (let y = 0; y < cell; y++) for (let x = 0; x < cell; x++) {
        const o = ((by + gy * cell + y) * png.width + bx + gx * cell + x) * 4;
        png.data[o] = c >> 16; png.data[o + 1] = (c >> 8) & 255; png.data[o + 2] = c & 255; png.data[o + 3] = 255;
      }
    }
  });
  fs.writeFileSync(file, PNG.sync.write(png));
}

async function main() {
  const dir = svgDir();
  const only = opt('--only');
  const defs = ANIMS.filter((d) => !only || only.split(',').includes(d.name));
  if (!defs.length) throw new Error('no animations selected');
  fs.mkdirSync(path.join(outDir, 'sheets'), { recursive: true });
  const browser = await puppeteer.launch({ executablePath: chromePath(), headless: true,
    args: ['--force-device-scale-factor=1', '--hide-scrollbars'] });
  const anims = [];
  try {
    const page = await browser.newPage();
    await page.setViewport({ width: PAGE_PX, height: PAGE_PX, deviceScaleFactor: 1 });
    for (const def of defs) {
      const text = fs.readFileSync(path.join(dir, def.svg), 'utf8');
      const r = await renderFrames(page, text);
      if (flag('--probe')) {
        // union ink box in SVG units over the whole viewBox
        let x0 = 1e9, y0 = 1e9, x1 = -1, y1 = -1;
        for (const p of r.frames) for (let y = 0; y < p.height; y += 2) for (let x = 0; x < p.width; x += 2) {
          const o = (y * p.width + x) * 4;
          if (p.data[o] + p.data[o + 1] + p.data[o + 2] < 24) continue;
          if (x < x0) x0 = x; if (x > x1) x1 = x; if (y < y0) y0 = y; if (y > y1) y1 = y;
        }
        const u = (v, o) => (v / PX_PER_UNIT + o).toFixed(1);
        console.log(`${def.name.padEnd(14)} ${r.frames.length} frames ${Math.round(r.totalMs)} ms  x ${u(x0, VB.x)}..${u(x1, VB.x)}  y ${u(y0, VB.y)}..${u(y1, VB.y)}`);
        continue;
      }
      const a = packAnim(def, r);
      anims.push(a);
      const bytes = a.frames.reduce((s, f) => s + f.length, 0);
      console.log(`${a.name.padEnd(14)} ${String(a.frames.length).padStart(3)} frames  ${a.w}x${a.h}@${a.ox},${a.oy}  ` +
        `${a.srcColors} -> ${a.paletteRgb.length - 1} colours  ${bytes} B (max frame ${Math.max(...a.frames.map((f) => f.length))} B)`);
      if (flag('--sheet')) writeSheet(a, path.join(outDir, 'sheets', `${a.name.replace(/\s+/g, '-')}.png`));
    }
  } finally {
    await browser.close();
  }
  if (flag('--probe')) return;
  const pak = writePak(anims);
  const file = path.join(outDir, 'desk.pak');
  fs.writeFileSync(file, pak);
  let rev = '';
  try { rev = execFileSync('git', ['-C', process.env.CLAWD_ON_DESK_DIR, 'rev-parse', '--short', 'HEAD']).toString().trim(); } catch {}
  console.log(`${path.relative(root, file)}: ${anims.length} animations, ${pak.length} bytes (clawd-on-desk ${rev || '?'})`);
}

main().catch((e) => { console.error(e.message || e); process.exit(1); });
