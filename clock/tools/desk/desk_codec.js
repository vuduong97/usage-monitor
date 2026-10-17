// desk_codec.js — CLDK v2 frame deltas (the clock's DeskPack.cpp decodes the same).
//
// A frame is a stream of ops over the animation's w x h crop, row-major,
// starting at the top-left pixel. Pixels are 4-bit palette indexes (0 = black).
//   00nnnnnn            skip n+1 pixels (1..63) — they keep what is on screen
//   00111111 lo hi      skip a u16 count of pixels
//   01nnnnnn data...    n+1 literal pixels (1..64), packed two per byte, high nibble first
//   1ccccrrr            rrr+1 pixels (1..7) of colour c
//   1cccc111 e          8+e pixels (8..263) of colour c
// Frame 0 is a delta from a black crop; each later frame from the previous
// one; the wrap frame from the last frame back to the loop start.
'use strict';

const zlib = require('zlib');

const GAP = 3;   // unchanged gaps this short inside a row are redrawn: one SPI window instead of two

function encodeDelta(prev, cur, w) {
  const n = cur.length;
  const draw = new Uint8Array(n);
  for (let i = 0; i < n; i++) draw[i] = prev[i] !== cur[i] ? 1 : 0;
  for (let y = 0; y < n / w; y++) {          // fill short gaps between changes on a row
    let last = -1;
    for (let x = 0; x < w; x++) {
      const i = y * w + x;
      if (!draw[i]) continue;
      if (last >= 0 && x - last - 1 <= GAP) for (let k = last + 1; k < x; k++) draw[y * w + k] = 1;
      last = x;
    }
  }
  const out = [];
  let i = 0;
  while (i < n) {
    if (!draw[i]) {
      let r = 0;
      while (i < n && !draw[i]) { r++; i++; }
      if (r === n && !out.length) return Buffer.alloc(0);   // nothing changed
      while (r > 0) {
        if (r >= 64) { const k = Math.min(r, 65535); out.push(0x3f, k & 255, k >> 8); r -= k; } else { out.push(r - 1); r = 0; }
      }
      continue;
    }
    // a drawn segment: colour runs of 3+ as run ops, the rest as literals
    let lit = [];
    const flushLit = () => {
      for (let s = 0; s < lit.length; s += 64) {
        const part = lit.slice(s, s + 64);
        out.push(0x40 | (part.length - 1));
        for (let k = 0; k < part.length; k += 2) out.push((part[k] << 4) | (k + 1 < part.length ? part[k + 1] : 0));
      }
      lit = [];
    };
    while (i < n && draw[i]) {
      const c = cur[i];
      let r = 0;
      while (i + r < n && draw[i + r] && cur[i + r] === c) r++;
      if (r < 3) { for (let k = 0; k < r; k++) lit.push(c); i += r; continue; }
      flushLit();
      i += r;
      while (r > 0) {
        if (r >= 8) { const k = Math.min(r, 263); out.push(0x80 | (c << 3) | 7, k - 8); r -= k; } else { out.push(0x80 | (c << 3) | (r - 1)); r = 0; }
      }
    }
    flushLit();
  }
  return Buffer.from(out);
}

// frames: cropped index frames; returns frames.length + 1 buffers (the last is the wrap).
function encodeAnim(frames, loopStart, w) {
  const out = [];
  let prev = new Uint8Array(frames[0].length);
  for (const f of frames) { out.push(encodeDelta(prev, f, w)); prev = f; }
  out.push(encodeDelta(prev, frames[loopStart], w));
  return out;
}

// Apply one delta to `state` in place. Throws on malformed input.
function applyDelta(state, buf) {
  let p = 0, i = 0;
  const n = state.length;
  while (p < buf.length) {
    const op = buf[p++];
    if (op < 0x40) {
      if (op === 0x3f) { if (p + 2 > buf.length) throw new Error('truncated skip'); i += buf[p] | (buf[p + 1] << 8); p += 2; } else i += op + 1;
    } else if (op < 0x80) {
      const k = (op & 0x3f) + 1;
      for (let j = 0; j < k; j++) {
        const b = buf[p + (j >> 1)];
        if (b === undefined || i >= n) throw new Error('literal past the end');
        state[i++] = j & 1 ? b & 15 : b >> 4;
      }
      p += (k + 1) >> 1;
    } else {
      const c = (op >> 3) & 15;
      let r = (op & 7) + 1;
      if (r === 8) { if (p >= buf.length) throw new Error('truncated run'); r = 8 + buf[p++]; }
      if (i + r > n) throw new Error('run past the end');
      state.fill(c, i, i + r);
      i += r;
    }
    if (i > n) throw new Error('skip past the end');
  }
}

// Returns the crop state after every delta (frames, then the wrap).
function decodeAnim(deltas, cells) {
  const state = new Uint8Array(cells);
  return deltas.map((d) => { applyDelta(state, d); return Uint8Array.from(state); });
}

const crc32 = (u8) => zlib.crc32(Buffer.from(u8.buffer, u8.byteOffset, u8.length)) >>> 0;

module.exports = { encodeDelta, encodeAnim, applyDelta, decodeAnim, crc32, GAP };
