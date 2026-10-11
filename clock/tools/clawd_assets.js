#!/usr/bin/env node
// clawd_assets.js — port Clawdmeter's UI assets to the ESP8266 SmallTV.
//
// Reads the Clawdmeter firmware sources (the official Clawd splash animations
// and the LVGL bitmap fonts) and writes two PROGMEM headers for
// src/features/clawd/:
//
//   clawd_anims.h  17 animations, frames packed 4 bits per cell (palettes are
//                  <= 16 colours), so the whole set is ~half its source size.
//   clawd_fonts.h  the five fonts the 240x240 Clawdmeter layout uses, cut down
//                  to the glyphs that layout can draw, with kerning kept.
//
// The pixels are the same ones Clawdmeter renders: glyphs stay 4 bpp
// anti-aliased exactly as lv_font_conv produced them, and frames are not
// re-quantised.
//
// Usage:
//   node tools/clawd_assets.js <path-to-Clawdmeter>/firmware/src
'use strict';

const fs = require('fs');
const path = require('path');

const src = process.argv[2];
if (!src || !fs.existsSync(path.join(src, 'splash_animations.h'))) {
  console.error('usage: node tools/clawd_assets.js <Clawdmeter>/firmware/src');
  process.exit(1);
}
const outDir = path.join(__dirname, '..', 'src', 'features', 'clawd');
fs.mkdirSync(outDir, { recursive: true });

const ASCII = range(0x20, 0x7e);

// Fonts used by Clawdmeter's small (240x240) layout, see compute_layout() in
// Clawdmeter's ui.cpp. Charsets cover every string that layout draws; the
// status line and the reset/pace lines carry daemon text, so they keep ASCII.
const FONTS = [
  { name: 'tiempos_34', file: 'font_tiempos_34.c', chars: cps('Usage0123456789: APM') },
  { name: 'styrene_24', file: 'font_styrene_24.c', chars: cps('0123456789%-') },
  { name: 'styrene_14', file: 'font_styrene_14.c', chars: ASCII },
  { name: 'styrene_12', file: 'font_styrene_12.c', chars: ASCII },
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

function emitAnims() {
  const anims = parseAnims();
  let bytes = 0;
  const parts = [];
  const rows = [];
  anims.forEach((a, idx) => {
    const id = a.name.replace(/\W+/g, '_');
    const fb = Math.ceil((a.w * a.h) / 2);
    const packed = [];
    for (let f = 0; f < a.frameCount; f++) packed.push(...pack4(a.frames.slice(f * a.w * a.h, (f + 1) * a.w * a.h)));
    bytes += packed.length + a.holds.length * 2;
    parts.push(`static const uint8_t ca_${id}_frames[${packed.length}] PROGMEM = {\n${hexList(packed)}\n};`);
    parts.push(`static const uint16_t ca_${id}_holds[${a.frameCount}] PROGMEM = {${a.holds.join(',')}};`);
    const pal = a.palette.map((c) => '0x' + c.toString(16).toUpperCase().padStart(4, '0')).join(',');
    if (a.name.length > 15) throw new Error(`${a.name}: name too long`);
    rows.push(`  {"${a.name}", ${a.w}, ${a.h}, ${a.ox}, ${a.oy}, ${a.frameCount}, ${a.loopStart}, ${a.loopEnd}, ${fb}, {${pal}}, ca_${id}_frames, ca_${id}_holds}`);
  });
  const text = [
    `#define CLAWD_ANIM_COUNT ${anims.length}`,
    ...parts,
    `static const ClawdAnim CLAWD_ANIMS[CLAWD_ANIM_COUNT] PROGMEM = {\n${rows.join(',\n')}\n};`,
  ].join('\n');
  return { text, bytes, count: anims.length };
}

const banner = (what) => `// ${what} — generated by tools/clawd_assets.js from the Clawdmeter firmware
// sources (official Anthropic Clawd animations / Styrene, Tiempos and DejaVu
// Sans Mono bitmap fonts). Do not edit by hand; re-run the tool instead.
#pragma once
#include "ClawdAssets.h"
`;

const anims = emitAnims();
fs.writeFileSync(path.join(outDir, 'clawd_anims.h'), banner('Clawd animations') + '\n' + anims.text + '\n');
const fonts = FONTS.map(emitFont);
fs.writeFileSync(path.join(outDir, 'clawd_fonts.h'), banner('Clawdmeter fonts') + '\n' + fonts.map((f) => f.text).join('\n\n') + '\n');

console.log(`clawd_anims.h: ${anims.count} animations, ${anims.bytes} bytes of flash`);
FONTS.forEach((s, i) => console.log(`clawd_fonts.h: ${s.name} ${fonts[i].glyphs} glyphs, ${fonts[i].bytes} bytes`));
