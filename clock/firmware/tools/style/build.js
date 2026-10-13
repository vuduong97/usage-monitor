#!/usr/bin/env node
// build.js — compile the style pack sources (packs/*.js) into firmware/styles/<id>.pak
// plus styles/index.json, the list the desktop app bundles and offers.
//
//   node firmware/tools/style/build.js <reference-firmware>/src [out_dir]
//
// The fonts come from the reference Clawd firmware's LVGL sources, like
// tools/clawd_assets.js. Packs are deterministic: the same sources give the same bytes.
'use strict';

const fs = require('fs');
const path = require('path');
const { Style, mix } = require('./style_pack');

const ORDER = [
  'gameboy', 'synth', 'paper', 'blueprint', 'pastel', 'onebit',
  // Batch A: retro computers & devices
  'ambercrt', 'vt100tui', 'mac84', 'win95', 'biossetup', 'c64', 'zxspec', 'casiolcd', 'calc', 'hifivfd',
  // Batch C: games & toys
  'tetris', 'pinballdmd', 'pacmaze', 'jrpgstatus', 'hiscore', 'invaders', 'eggpet', 'platformer', 'chessclock', 'racinghud',
  // Batch D: instruments & gauges
  'vumeter', 'scope', 'pfd', 'fuel', 'monitor', 'rack', 'weather', 'powerbank', 'lab', 'sonar',
  // Batch E: interface styles
  'hairline', 'widget', 'tonal', 'brutal', 'soft', 'oled', 'ink', 'tufte', 'frost', 'bigtype',
];
const MAX_PACK = 20 * 1024;

function main() {
  const fontDir = process.argv[2];
  if (!fontDir || !fs.existsSync(path.join(fontDir, 'font_styrene_14.c'))) {
    console.error('usage: node tools/style/build.js <reference-firmware>/src [out_dir]');
    process.exit(1);
  }
  const outDir = process.argv[3] || path.join(__dirname, '..', '..', 'styles');
  fs.mkdirSync(outDir, { recursive: true });
  const index = [];
  let failed = false;
  for (const id of ORDER) {
    const src = require(path.join(__dirname, 'packs', `${id}.js`));
    if (src.id !== id) throw new Error(`packs/${id}.js declares id ${src.id}`);
    const S = new Style({ id, name: src.name, bg: src.bg, light: !!src.light, fontDir });
    S.rect(0, 0, 240, 240, src.bg);
    src.draw(S, { mix });
    const r = S.build();
    const file = `${id}.pak`;
    fs.writeFileSync(path.join(outDir, file), r.buf);
    const crc = r.buf.readUInt32LE(12).toString(16).padStart(8, '0');
    index.push({ id, name: src.name, file, size: r.buf.length, crc, light: !!src.light });
    const fonts = r.fonts.map((f) => (f.builtin !== undefined ? `${f.key}(fw)` : `${f.key}:${f.glyphs}g/${f.bytes}B`)).join(' ');
    console.log(`${file}: ${r.buf.length} B, ${r.ops} ops, ${r.tracks} tracks, ${r.palette} colours, fonts ${fonts}`);
    if (r.buf.length > MAX_PACK) { console.error(`${file}: over the ${MAX_PACK} B budget`); failed = true; }
  }
  fs.writeFileSync(path.join(outDir, 'index.json'), JSON.stringify({ api: 1, styles: index }, null, 2) + '\n');
  if (failed) process.exit(1);
}

main();
