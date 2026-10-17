#!/usr/bin/env node
// plan_anims.js <reference-firmware>/src > plan.json — per pack, the cell, nudge and Clawd animation set
// that fit (clear of text, data and the edge). Keeps anims.js picks first, then fills to 8.
'use strict';
const path = require('path');
const { Style, mix, clawdPakNames } = require('./style_pack');
const ANIMS = require('./anims');
const fontDir = process.argv[2];
const PRIORITY = ['crab walking', 'walking', 'waving', 'dancing', 'pointing', 'jumping happy', 'jumping',
  'basketball', 'skateboard', 'laptop', 'soccer', 'magnifier', 'cloud', 'sailing scene', 'trumpet', 'racing car'];
const ids = Object.keys(ANIMS);
const MIN = 5, MAX = 8;

function fitAt(id, cfg) {
  const src = require(path.join(__dirname, 'packs', `${id}.js`));
  const S = new Style({ id, name: src.name, bg: src.bg, light: !!src.light, fontDir });
  S.rect(0, 0, 240, 240, src.bg); src.draw(S, { mix });
  S.animateClawd({ ...cfg, set: ['waving'] });
  const ops = S.ops.filter((q) => q.kind === 6), boxes = S.protectedBoxes();
  const fit = PRIORITY.filter((n) => clawdPakNames().has(n) && ops.every((o) => !S.animFit(o, n, boxes).length));
  return { cell: ops[0].a, fit };
}
const out = {};
for (const id of ids) {
  const base = ANIMS[id], at0 = base.at || [0, 0];
  let best = null;
  const tries = [];
  tries.push({ cell: base.cell, at: at0 });
  const r0 = fitAt(id, { ...base, at: at0 });
  if (r0.fit.length >= MIN) best = { cell: r0.cell, at: at0, fit: r0.fit };
  if (!best) {
    for (const cell of [r0.cell === 2 ? undefined : 2, ...(r0.cell === 2 ? [2] : [])].filter((c) => c !== undefined)) {
      for (let dy = -2; dy <= 2; dy += 2) for (let dx = -8; dx <= 8; dx += 2) {
        const at = [at0[0] + dx, at0[1] + dy];
        const r = fitAt(id, { ...base, cell, at });
        const score = r.fit.length * 100 - Math.abs(dx) - Math.abs(dy);
        if (!best || score > best.score) best = { score, cell: r.cell, at, fit: r.fit };
      }
    }
  }
  const keep = base.set.filter((n) => best.fit.includes(n));
  const set = [...keep, ...best.fit.filter((n) => !keep.includes(n))].slice(0, MAX);
  out[id] = { cell: best.cell, at: best.at, set, fits: best.fit.length };
}
console.log(JSON.stringify(out, null, 1));
