// desk_pak.js — the /desk.pak container (CLDK), shared by desk_to_pak.js and
// desk_repack.js. The frame deltas inside are desk_codec.js ops; this file only
// stores them.
//
// Little-endian.
//   header  "CLDK", u16 version, u16 count, u16 size (240), u16 reserved,
//           u32 reserved                                                   16 B
//   desc    name[16], u8 x, y, w, h (crop on the panel), u16 frameCount,
//           u16 loopStart, u16 maxFrameBytes (largest raw delta), u8 flags
//           (bit 0: once), u8 reserved, u16 palette[16] (RGB565),
//           u32 indexOff                                                   64 B
//
// v2 (read only here; the clock still reads it):
//   index   u32 offsets[frameCount + 2]: delta i = [off[i], off[i+1]) for
//           i < frameCount, then the wrap delta (last frame -> loopStart)
//   holds   u16 ms[frameCount]
//   deltas  raw ops, back to back
//
// v3 (written): every delta is compressed on its own, and identical deltas
// are stored once.
//   index   8 B per delta, frameCount + 1 of them (the last is the wrap):
//           u32 off, u16 stored bytes, u16 raw bytes | 0x8000 when stored
//           uncompressed (raw-deflate did not help); 0/0 = empty delta
//   holds   u16 ms[frameCount]
//   deltas  raw DEFLATE streams (RFC 1951, no zlib/gzip wrapper) with a
//           512 B window, so the clock inflates through a 512 B ring
//           (DeskPack.cpp, uzlib). Pixels are identical to v2: the ops inside
//           are the same bytes.
//
// Inflating costs the clock ~6 us per raw byte. The animations in RAW_ANIMS
// (big deltas, measured 2026-10-12: debugger draw p95 17.6 -> 24.2 ms, typing
// boss 19.0 -> 27.6 ms when compressed) keep every delta stored, so they draw
// as fast as v2; the rest is compressed. A stored delta is still deduped.
'use strict';

const zlib = require('zlib');

const HEADER = 16, DESC = 64, SIZE = 240;
const STORED = 0x8000;
const WINDOW_BITS = 9;     // 512 B; zlib keeps matches within w_size - 262, well inside the ring
const RAW_ANIMS = new Set(['building', 'debugger', 'typing boss', 'wizard', 'happy', 'juggling']);

function deflate(raw) {
  return zlib.deflateRawSync(raw, { level: 9, windowBits: WINDOW_BITS, memLevel: 9 });
}

function inflate(buf) {
  return zlib.inflateRawSync(buf, { windowBits: 15 });
}

// anims: [{ name, x, y, w, h, loopStart, once, palette[16 RGB565], holds[n],
//           frames: [n + 1 raw delta Buffers] }]
// raw: names whose deltas are stored uncompressed (default RAW_ANIMS).
function writePak(anims, raw = RAW_ANIMS) {
  const head = Buffer.alloc(HEADER);
  head.write('CLDK', 0, 'latin1');
  head.writeUInt16LE(3, 4);
  head.writeUInt16LE(anims.length, 6);
  head.writeUInt16LE(SIZE, 8);
  const descs = [], bodies = [];
  let off = HEADER + DESC * anims.length;
  for (const a of anims) {
    const n = a.frames.length - 1;             // + the wrap delta
    const keepRaw = raw.has(a.name);
    const index = Buffer.alloc(8 * (n + 1));
    const holds = Buffer.alloc(2 * n);
    a.holds.forEach((h, i) => holds.writeUInt16LE(h, 2 * i));
    let fo = off + index.length + holds.length;
    const seen = new Map();                    // raw delta (hex) -> [off, stored, rawField]
    const data = [];
    a.frames.forEach((f, i) => {
      if (f.length >= STORED) throw new Error(`${a.name}: delta ${i} is ${f.length} B (max ${STORED - 1})`);
      let e = f.length ? seen.get(f.toString('latin1')) : [0, 0, 0];
      if (!e) {
        const z = keepRaw ? f : deflate(f);
        const body = z.length < f.length ? z : f;
        e = [fo, body.length, f.length | (body === f ? STORED : 0)];
        seen.set(f.toString('latin1'), e);
        data.push(body);
        fo += body.length;
      }
      index.writeUInt32LE(e[0], 8 * i);
      index.writeUInt16LE(e[1], 8 * i + 4);
      index.writeUInt16LE(e[2], 8 * i + 6);
    });
    const d = Buffer.alloc(DESC);
    d.write(a.name.slice(0, 15), 0, 'latin1');
    d.writeUInt8(a.x, 16); d.writeUInt8(a.y, 17); d.writeUInt8(a.w, 18); d.writeUInt8(a.h, 19);
    d.writeUInt16LE(n, 20); d.writeUInt16LE(a.loopStart, 22);
    d.writeUInt16LE(Math.max(...a.frames.map((f) => f.length)), 24);
    d.writeUInt8(a.once ? 1 : 0, 26);
    for (let k = 0; k < 16; k++) d.writeUInt16LE(a.palette[k] || 0, 28 + 2 * k);
    d.writeUInt32LE(off, 60);
    descs.push(d);
    const body = Buffer.concat([index, holds, ...data]);
    bodies.push(body);
    off += body.length;
  }
  return Buffer.concat([head, ...descs, ...bodies]);
}

// v2 or v3 -> the anims shape writePak takes (frames = raw deltas).
function readPak(buf) {
  if (buf.toString('latin1', 0, 4) !== 'CLDK') throw new Error('not a CLDK pack');
  const version = buf.readUInt16LE(4), count = buf.readUInt16LE(6);
  if (version !== 2 && version !== 3) throw new Error(`CLDK v${version} not supported`);
  if (buf.readUInt16LE(8) !== SIZE) throw new Error('not a 240 px pack');
  const anims = [];
  for (let i = 0; i < count; i++) {
    const o = HEADER + DESC * i;
    const n = buf.readUInt16LE(o + 20);
    const io = buf.readUInt32LE(o + 60);
    const palette = [];
    for (let k = 0; k < 16; k++) palette.push(buf.readUInt16LE(o + 28 + 2 * k));
    const frames = [];
    let holdsOff;
    if (version === 2) {
      for (let k = 0; k <= n; k++) frames.push(buf.subarray(buf.readUInt32LE(io + 4 * k), buf.readUInt32LE(io + 4 * (k + 1))));
      holdsOff = io + 4 * (n + 2);
    } else {
      for (let k = 0; k <= n; k++) {
        const at = buf.readUInt32LE(io + 8 * k), len = buf.readUInt16LE(io + 8 * k + 4), raw = buf.readUInt16LE(io + 8 * k + 6);
        const body = buf.subarray(at, at + len);
        const f = raw & STORED ? body : len ? inflate(body) : Buffer.alloc(0);
        if (f.length !== (raw & ~STORED)) throw new Error(`#${i} delta ${k}: ${f.length} B, index says ${raw & ~STORED}`);
        frames.push(f);
      }
      holdsOff = io + 8 * (n + 1);
    }
    const holds = [];
    for (let k = 0; k < n; k++) holds.push(buf.readUInt16LE(holdsOff + 2 * k));
    anims.push({
      name: buf.toString('latin1', o, o + 16).replace(/\0.*$/s, ''),
      x: buf[o + 16], y: buf[o + 17], w: buf[o + 18], h: buf[o + 19],
      loopStart: buf.readUInt16LE(o + 22), once: !!(buf[o + 26] & 1), palette, holds, frames,
    });
  }
  return { version, anims };
}

module.exports = { writePak, readPak, STORED, WINDOW_BITS, RAW_ANIMS };
