#!/usr/bin/env node
// desk_repack.js — rewrite an existing desk.pak (CLDK v2 or v3) as CLDK v3
// without re-rendering: the delta ops stay byte for byte the same, only the
// container compresses and de-duplicates them (desk_pak.js; RAW_ANIMS stay
// uncompressed for draw speed). Pixels cannot
// change; tools/desk_check/run.sh checks that against desk.crc.json.
//
//   node tools/desk/desk_repack.js [in.pak] [out.pak]
//   (default: build/desk/desk.pak -> build/desk/desk.pak, the v2 kept as desk.v2.pak)
'use strict';

const fs = require('fs');
const path = require('path');
const { readPak, writePak, RAW_ANIMS } = require('./desk_pak');

const root = path.resolve(__dirname, '..', '..');
const inFile = process.argv[2] || path.join(root, 'build', 'desk', 'desk.pak');
const outFile = process.argv[3] || inFile;

const src = fs.readFileSync(inFile);
const { version, anims } = readPak(src);
const out = writePak(anims);

// Round trip: every raw delta must come back identical.
const back = readPak(out).anims;
anims.forEach((a, i) => a.frames.forEach((f, k) => {
  if (Buffer.compare(f, back[i].frames[k]) !== 0) throw new Error(`${a.name}: delta ${k} changed in the round trip`);
}));

if (outFile === inFile && version === 2) {
  const keep = inFile.replace(/\.pak$/, '.v2.pak');
  fs.copyFileSync(inFile, keep);
  process.stdout.write(`kept the v2 pack as ${path.relative(root, keep)}\n`);
}
fs.writeFileSync(outFile, out);
const pct = ((1 - out.length / src.length) * 100).toFixed(1);
process.stdout.write(`${path.relative(root, outFile)}: CLDK v${version} ${src.length} B -> v3 ${out.length} B ` +
  `(-${pct}%), ${anims.length} animations, round trip ok\n` +
  `stored uncompressed: ${[...RAW_ANIMS].join(', ')}\n`);
