#!/usr/bin/env node
// clawd_assets.js — port the Clawd UI assets to the ESP8266 clock.
//
// Reads the reference Clawd firmware (ESP32, see NOTICE) sources (the official Clawd splash animations
// and the LVGL bitmap fonts) and writes:
//
//   data/clawd.pak  17 animations, frames packed 4 bits per cell (palettes are
//                   <= 16 colours). Uploaded to the clock's filesystem, not
//                   linked: the image must fit the stock 4m3m OTA budget.
//   src/features/clawd/clawd_fonts.h
//                   the fonts the 240x240 Clawd layout uses (PROGMEM), cut
//                   down to the glyphs that layout can draw, kerning kept.
//
// The pixels are the same ones the reference firmware renders: glyphs stay 4 bpp
// anti-aliased exactly as lv_font_conv produced them, and frames are not
// re-quantised.
//
// Usage:
//   node tools/clawd_assets.js <path-to-reference-firmware>/src
'use strict';

const fs = require('fs');
const path = require('path');

const src = process.argv[2];
if (!src || !fs.existsSync(path.join(src, 'splash_animations.h'))) {
  console.error('usage: node tools/clawd_assets.js <reference-firmware>/src');
  process.exit(1);
}
const outDir = path.join(__dirname, '..', 'src', 'features', 'clawd');
fs.mkdirSync(outDir, { recursive: true });

const ASCII = range(0x20, 0x7e);

// Fonts used by the reference firmware's small (240x240) layout, see compute_layout() in
// the reference firmware's ui.cpp. Charsets cover every string that layout draws; the
// status line and the reset/pace lines carry daemon text, so they keep ASCII.
const FONTS = [
  { name: 'tiempos_34', file: 'font_tiempos_34.c', chars: cps('Usage0123456789: APM') },
  { name: 'styrene_24', file: 'font_styrene_24.c', chars: cps('0123456789%-') },
  // Hero and pixel-retro usage layouts (usage.layout 2 and 5): digits only, to stay small.
  { name: 'styrene_48', file: 'font_styrene_48.c', chars: cps('0123456789%-') },
  { name: 'mono_32',    file: 'font_mono_32.c',    chars: cps('0123456789:-') },
  { name: 'styrene_14', file: 'font_styrene_14.c', chars: ASCII },
  { name: 'mono_18',    file: 'font_mono_18.c',    chars: null },   // all of it
];

function range(a, b) { const r = []; for (let i = a; i <= b; i++) r.push(i); return r; }
function cps(s) { return [...new Set([...s].map((c) => c.codePointAt(0)))]; }

function arrayBody(text, decl) {
  const i = text.indexOf(decl);
  if (i < 0) throw new Error(`missing ${decl}`);
  const open = text.indexOf('{', i);
  const close = text.indexOf('};', open);
  return text.slice(open + 1, close);
}
function numbers(body) {
  return body.replace(/\/\*[\s\S]*?\*\//g, '').replace(/\/\/.*$/gm, '')
    .split(/[\s,]+/).filter(Boolean).map((t) => Number(t));
}

function hexList(bytes, perLine = 24) {
  const lines = [];
  for (let i = 0; i < bytes.length; i += perLine) {
    lines.push('  ' + bytes.slice(i, i + perLine).map((b) => '0x' + b.toString(16).padStart(2, '0')).join(','));
  }
  return lines.join(',\n');
}

// ---- fonts ------------------------------------------------------------------
function parseFont(file) {
  const t = fs.readFileSync(path.join(src, file), 'utf8');
  const bitmap = numbers(arrayBody(t, 'glyph_bitmap[] ='));
  const glyphs = [];
  const re = /\{\.bitmap_index = (\d+), \.adv_w = (\d+), \.box_w = (\d+), \.box_h = (\d+), \.ofs_x = (-?\d+), \.ofs_y = (-?\d+)\}/g;
  for (let m; (m = re.exec(arrayBody(t, 'glyph_dsc[] =')));) {
    glyphs.push({ off: +m[1], adv: +m[2], w: +m[3], h: +m[4], ox: +m[5], oy: +m[6] });
  }
  // codepoint -> glyph id, from the cmap table
  const map = new Map();
  const cmapRe = /\.range_start = (\d+), \.range_length = (\d+), \.glyph_id_start = (\d+),\s*\.unicode_list = (\w+), \.glyph_id_ofs_list = (\w+), \.list_length = (\d+), \.type = (\w+)/g;
  for (let m; (m = cmapRe.exec(t));) {
    const [start, len, gid] = [+m[1], +m[2], +m[3]];
    const type = m[7];
    if (type === 'LV_FONT_FMT_TXT_CMAP_FORMAT0_TINY') {
      for (let k = 0; k < len; k++) map.set(start + k, gid + k);
    } else if (type === 'LV_FONT_FMT_TXT_CMAP_SPARSE_TINY') {
      const list = numbers(arrayBody(t, `${m[4]}[] =`));
      list.forEach((d, k) => map.set(start + d, gid + k));
    } else {
      throw new Error(`${file}: unsupported cmap ${type}`);
    }
  }
  const lineHeight = +/\.line_height = (\d+)/.exec(t)[1];
  const baseLine = +/\.base_line = (\d+)/.exec(t)[1];
  if (!/\.bpp = 4/.test(t) || !/\.bitmap_format = 0/.test(t)) throw new Error(`${file}: need 4bpp uncompressed`);

  // kerning pairs (glyph-id pairs, 4.4 fixed values scaled by kern_scale)
  const kerns = [];
  if (/\.kern_dsc = &kern_pairs/.test(t)) {
    if (!/\.glyph_ids_size = 0/.test(t)) throw new Error(`${file}: 16-bit kern ids unsupported`);
    const ids = numbers(arrayBody(t, 'kern_pair_glyph_ids[] ='));
    const vals = numbers(arrayBody(t, 'kern_pair_values[] ='));
    const scale = +/\.kern_scale = (\d+)/.exec(t)[1];
    for (let k = 0; k < vals.length; k++) kerns.push({ l: ids[2 * k], r: ids[2 * k + 1], v: (vals[k] * scale) >> 4 });
  } else if (!/\.kern_dsc = NULL/.test(t)) {
    throw new Error(`${file}: unsupported kerning (class-based)`);
  }
  return { bitmap, glyphs, map, lineHeight, baseLine, kerns };
}

function glyphBytes(g, bitmap) {
  return bitmap.slice(g.off, g.off + Math.ceil((g.w * g.h) / 2));
}

function emitFont(spec) {
  const f = parseFont(spec.file);
  const want = (spec.chars || [...f.map.keys()]).filter((cp) => f.map.has(cp)).sort((a, b) => a - b);
  const missing = (spec.chars || []).filter((cp) => !f.map.has(cp));
  if (missing.length) throw new Error(`${spec.file}: missing ${missing.map((c) => String.fromCodePoint(c)).join('')}`);
  const bmp = [];
  const rows = [];
  const idToIdx = new Map();
  want.forEach((cp, idx) => {
    const g = f.glyphs[f.map.get(cp)];
    idToIdx.set(f.map.get(cp), idx);
    const off = bmp.length;
    bmp.push(...glyphBytes(g, f.bitmap));
    rows.push(`  {${cp}, ${off}, ${g.adv}, ${g.w}, ${g.h}, ${g.ox}, ${g.oy}}`);
  });
  if (bmp.length > 0xffff) throw new Error(`${spec.file}: bitmap too large for 16-bit offsets`);
  const kerns = f.kerns.filter((k) => idToIdx.has(k.l) && idToIdx.has(k.r) && k.v !== 0)
    .map((k) => ({ l: idToIdx.get(k.l), r: idToIdx.get(k.r), v: k.v }))
    .sort((a, b) => (a.l - b.l) || (a.r - b.r));
  if (want.length > 255) throw new Error(`${spec.file}: >255 glyphs`);
  const n = spec.name;
  const text = [
    `// ${spec.file}: ${want.length} glyphs, line ${f.lineHeight}, base ${f.baseLine}`,
    `static const uint8_t cf_${n}_bmp[] PROGMEM = {\n${hexList(bmp)}\n};`,
    `static const ClawdGlyph cf_${n}_glyphs[] PROGMEM = {\n${rows.join(',\n')}\n};`,
    kerns.length
      ? `static const ClawdKern cf_${n}_kerns[] PROGMEM = {\n${kerns.map((k) => `  {${k.l}, ${k.r}, ${k.v}}`).join(',\n')}\n};`
      : `#define cf_${n}_kerns nullptr`,
    `static const ClawdFont CF_${n.toUpperCase()} = {cf_${n}_bmp, cf_${n}_glyphs, cf_${n}_kerns, ${want.length}, ${kerns.length}, ${f.lineHeight}, ${f.baseLine}};`,
  ].join('\n');
  return { text, bytes: bmp.length + rows.length * 10 + kerns.length * 3, glyphs: want.length };
}

// ---- animations ----------------------------------------------------------------
function parseAnims() {
  const t = fs.readFileSync(path.join(src, 'splash_animations.h'), 'utf8');
  const table = arrayBody(t, 'splash_anims[SPLASH_ANIM_COUNT] =');
  const anims = [];
  const re = /\{"([^"]+)", "(\w+)", (\d+), (\d+), (\d+), (\d+), (\d+), (\d+), (\d+), (\d+), (\w+), (\w+), (\w+)\}/g;
  for (let m; (m = re.exec(table));) {
    const a = {
      name: m[1], w: +m[3], h: +m[4], ox: +m[5], oy: +m[6], frameCount: +m[7],
      loopStart: +m[8], loopEnd: +m[9], paletteCount: +m[10],
    };
    a.palette = numbers(arrayBody(t, `${m[11]}[16] =`));
    a.frames = numbers(arrayBody(t, `${m[12]}[`));
    a.holds = numbers(arrayBody(t, `${m[13]}[`));
    if (a.frames.length !== a.w * a.h * a.frameCount) throw new Error(`${a.name}: frame size mismatch`);
    if (a.holds.length !== a.frameCount) throw new Error(`${a.name}: holds mismatch`);
    if (a.frames.some((c) => c > 15)) throw new Error(`${a.name}: palette index > 15`);
    anims.push(a);
  }
  if (!anims.length) throw new Error('no animations parsed');
  return anims;
}

function pack4(cells) {
  // two cells per byte, high nibble first; every frame starts on a byte
  const out = [];
  for (let i = 0; i < cells.length; i += 2) out.push((cells[i] << 4) | (cells[i + 1] ?? 0));
  return out;
}

// Binary pack, little-endian, uploaded to the clock's filesystem as /clawd.pak
// (the image itself has to stay small enough for the stock flash layout):
//   header  "CLWD", u16 version (1), u16 count, u32 reserved          12 B
//   count x descriptor                                               72 B each
//     char name[16]; u8 w, h, ox, oy; u16 frameCount, loopStart, loopEnd,
//     frameBytes; u16 palette[16]; u32 framesOff, holdsOff, reserved
//   data    per animation: frames (frameCount x frameBytes), holds (u16 ms each)
const PAK_HEADER = 12;
const PAK_DESC = 72;

function emitAnims() {
  const anims = parseAnims();
  const descs = [];
  const blobs = [];
  let off = PAK_HEADER + PAK_DESC * anims.length;
  let maxFrame = 0;
  for (const a of anims) {
    if (a.name.length > 15) throw new Error(`${a.name}: name too long`);
    const fb = Math.ceil((a.w * a.h) / 2);
    maxFrame = Math.max(maxFrame, fb);
    const frames = [];
    for (let f = 0; f < a.frameCount; f++) frames.push(...pack4(a.frames.slice(f * a.w * a.h, (f + 1) * a.w * a.h)));
    const holds = Buffer.alloc(a.frameCount * 2);
    a.holds.forEach((h, i) => holds.writeUInt16LE(h, i * 2));
    const d = Buffer.alloc(PAK_DESC);
    d.write(a.name, 0, 'latin1');
    d.writeUInt8(a.w, 16); d.writeUInt8(a.h, 17); d.writeUInt8(a.ox, 18); d.writeUInt8(a.oy, 19);
    d.writeUInt16LE(a.frameCount, 20); d.writeUInt16LE(a.loopStart, 22);
    d.writeUInt16LE(a.loopEnd, 24); d.writeUInt16LE(fb, 26);
    a.palette.forEach((c, i) => d.writeUInt16LE(c, 28 + i * 2));
    d.writeUInt32LE(off, 60);
    d.writeUInt32LE(off + frames.length, 64);
    descs.push(d);
    blobs.push(Buffer.from(frames), holds);
    off += frames.length + holds.length;
  }
  const head = Buffer.alloc(PAK_HEADER);
  head.write('CLWD', 0, 'latin1');
  head.writeUInt16LE(1, 4);
  head.writeUInt16LE(anims.length, 6);
  const buf = Buffer.concat([head, ...descs, ...blobs]);
  return { buf, count: anims.length, maxFrame };
}

const banner = (what) => `// ${what} — generated by tools/clawd_assets.js from the reference Clawd firmware (ESP32, see NOTICE)
// sources (official Anthropic Clawd animations / Styrene, Tiempos and DejaVu
// Sans Mono bitmap fonts). Do not edit by hand; re-run the tool instead.
#pragma once
#include "ClawdAssets.h"
`;

const anims = emitAnims();
const dataDir = path.join(__dirname, '..', 'data');
fs.mkdirSync(dataDir, { recursive: true });
fs.writeFileSync(path.join(dataDir, 'clawd.pak'), anims.buf);
const fonts = FONTS.map(emitFont);
fs.writeFileSync(path.join(outDir, 'clawd_fonts.h'), banner('Clawd fonts') + '\n' + fonts.map((f) => f.text).join('\n\n') + '\n');

console.log(`data/clawd.pak: ${anims.count} animations, ${anims.buf.length} bytes (largest frame ${anims.maxFrame} B)`);
FONTS.forEach((s, i) => console.log(`clawd_fonts.h: ${s.name} ${fonts[i].glyphs} glyphs, ${fonts[i].bytes} bytes`));
