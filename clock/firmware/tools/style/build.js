#!/usr/bin/env node
// build.js — compile the style pack sources (packs/*.js) into firmware/styles/<id>.pak
// plus styles/index.json, the list the desktop app bundles and offers.
//
//   node firmware/tools/style/build.js <reference-firmware>/src [out_dir]
//   node firmware/tools/style/build.js <reference-firmware>/src --fit [id ...]
//
// The fonts come from the reference Clawd firmware's LVGL sources, like
// tools/clawd_assets.js. Packs are deterministic: the same sources give the same bytes.
// --fit writes nothing: per pack, which Clawd animations fit at its cell and at cell 2
// (clear of text and data, on screen), to choose its set in anims.js.
'use strict';

const fs = require('fs');
const path = require('path');
const { Style, mix, API, clawdPakNames } = require('./style_pack');
const ANIMS = require('./anims');

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
const MAX_ANIMATED = 64;   // ops with an animation track the clock accepts (STYLE_MAX_ANIMATED)
const MIN_ANIMS = 5;   // Clawd animations per screen (anims.js), at most 8 (the firmware's list)

// The pack's source drawn, its Clawd turned into animations with `cfg`.
function compose(id, fontDir, cfg) {
  const src = require(path.join(__dirname, 'packs', `${id}.js`));
  if (src.id !== id) throw new Error(`packs/${id}.js declares id ${src.id}`);
  const S = new Style({ id, name: src.name, bg: src.bg, light: !!src.light, fontDir });
  S.rect(0, 0, 240, 240, src.bg);
  src.draw(S, { mix });
  const animOps = S.animateClawd(cfg);
  return { S, src, animOps };
}

// --fit: every animation against every anim op of the pack, at its cell and at cell 2.
function fitReport(fontDir, ids) {
  const all = [...clawdPakNames().keys()];
  for (const id of ids.length ? ids : ORDER) {
    const cfg = ANIMS[id] || { set: ['waving'] };
    const cells = new Set();
    const lines = [];
    for (const cell of [cfg.cell, 2]) {
      const { S } = compose(id, fontDir, { ...cfg, set: ['waving'], cell });
      const anims = S.ops.filter((q) => q.kind === 6);
      const c = anims[0].a;
      if (cells.has(c)) continue;
      cells.add(c);
      const boxes = S.protectedBoxes();
      const fit = [], miss = [];
      for (const n of all) {
        const hits = [...new Set(anims.flatMap((a) => S.animFit(a, n, boxes)))];
        if (hits.length) miss.push(`${n} (${hits.slice(0, 2).join('; ')})`); else fit.push(n);
      }
      lines.push(`  cell ${c}: fits ${fit.length}: ${fit.join(', ')}`);
      lines.push(`          no: ${miss.join(' | ')}`);
    }
    console.log(`${id} (now ${(cfg.set || []).join(', ')})\n${lines.join('\n')}`);
  }
}

function main() {
  const fontDir = process.argv[2];
  if (!fontDir || !fs.existsSync(path.join(fontDir, 'font_styrene_14.c'))) {
    console.error('usage: node tools/style/build.js <reference-firmware>/src [out_dir | --fit [id ...]]');
    process.exit(1);
  }
  if (process.argv[3] === '--fit') return fitReport(fontDir, process.argv.slice(4));
  const outDir = process.argv[3] || path.join(__dirname, '..', '..', 'styles');
  fs.mkdirSync(outDir, { recursive: true });
  const index = [];
  let failed = false;
  for (const id of ORDER) {
    const cfg = ANIMS[id];
    if (!cfg) throw new Error(`anims.js: no Clawd animations for ${id}`);
    const min = cfg.min ?? MIN_ANIMS;
    if (min > MIN_ANIMS) throw new Error(`anims.js: ${id} min ${min} is above ${MIN_ANIMS}`);
    if (cfg.set.length < min) throw new Error(`anims.js: ${id} has ${cfg.set.length} animations (${min} at least)`);
    if (cfg.set.length > 8) throw new Error(`anims.js: ${id} has ${cfg.set.length} animations (8 at most)`);
    if (cfg.cell !== undefined && cfg.cell < 2) throw new Error(`anims.js: ${id} cell ${cfg.cell} (2 at least)`);
    const { S, src, animOps } = compose(id, fontDir, cfg);
    S.checkAnimFit();
    const r = S.build();
    const file = `${id}.pak`;
    fs.writeFileSync(path.join(outDir, file), r.buf);
    const crc = r.buf.readUInt32LE(12).toString(16).padStart(8, '0');
    index.push({ id, name: src.name, file, size: r.buf.length, crc, light: !!src.light });
    const fonts = r.fonts.map((f) => (f.builtin !== undefined ? `${f.key}(fw)` : `${f.key}:${f.glyphs}g/${f.bytes}B`)).join(' ');
    const cell = S.ops.find((q) => q.kind === 6).a;
    console.log(`${file}: ${r.buf.length} B, ${r.ops} ops, ${r.tracks} tracks, ${r.palette} colours, Clawd ${cfg.set.join('/')} x${animOps} cell ${cell}, fonts ${fonts}`);
    if (r.buf.length > MAX_PACK) { console.error(`${file}: over the ${MAX_PACK} B budget`); failed = true; }
    const tracked = S.ops.filter((o) => o.track !== 255).length;
    if (tracked > MAX_ANIMATED) { console.error(`${file}: ${tracked} animated ops (the clock takes ${MAX_ANIMATED})`); failed = true; }
  }
  fs.writeFileSync(path.join(outDir, 'index.json'), JSON.stringify({ api: API, styles: index }, null, 2) + '\n');
  if (failed) process.exit(1);
}

main();
