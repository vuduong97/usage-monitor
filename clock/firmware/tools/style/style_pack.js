// style_pack.js — compiler for Clawd usage-screen style packs (docs/STYLE_PACK.md).
//
// A pack source (packs/<id>.js) describes one usage screen with a small builder
// API: rectangles, text with data tokens, lines, triangles and 1-bit masks, each
// optionally gated on the usage state and moved/recoloured by an animation track.
// Everything that needs maths the clock should not do (gradients, discs, the
// synthwave grid's perspective, glyph widths of fixed labels) is worked out here,
// so the firmware interpreter stays a fixed list of primitives.
'use strict';

const fs = require('fs');
const path = require('path');
const zlib = require('zlib');

const API = 2;                     // 2: the Clawd animation op (anim)
const HEADER = 96, FONT_DESC = 24, TRACK_DESC = 8, MASK_DESC = 8, OP_SIZE = 36;

const KIND = { rect: 1, text: 2, line: 3, tri: 4, mask: 5, anim: 6 };
const SRC = { none: 0, s: 1, w: 2, ts: 3, tw: 4, fixed: 255 };
const COND = { ok: 0, hot: 1, whot: 2, fresh: 3, over: 4, paceover: 5, out: 6, clock: 7, sr: 8, wr: 9, anim: 10 };

// The official Clawd animations on the clock (/clawd.pak, tools/clawd_assets.js):
// their names, checked at compile time. Frames are crops of a 55x37 cell stage;
// idle Clawd stands centred on column 27 with his feet on row 37.
const CLAWD_PAK = path.join(__dirname, '..', '..', 'data', 'clawd.pak');
const STAGE_CX = 27, STAGE_FEET = 37;
let pakNames = null;
// name -> { w, h, ox, oy, cells }: `cells` lists every stage cell [x, y] any frame paints.
function clawdPakNames() {
  if (pakNames) return pakNames;
  const d = fs.readFileSync(CLAWD_PAK);
  if (d.toString('latin1', 0, 4) !== 'CLWD') throw new Error(`${CLAWD_PAK}: not a Clawd pack`);
  pakNames = new Map();
  for (let i = 0; i < d.readUInt16LE(6); i++) {
    const o = 12 + 72 * i;
    const w = d[o + 16], h = d[o + 17], ox = d[o + 18], oy = d[o + 19];
    const frames = d.readUInt16LE(o + 20), bytes = d.readUInt16LE(o + 26), at = d.readUInt32LE(o + 60);
    const on = new Uint8Array(w * h);
    for (let f = 0; f < frames; f++) {
      for (let c = 0; c < w * h; c++) if ((d[at + f * bytes + (c >> 1)] >> (c & 1 ? 0 : 4)) & 15) on[c] = 1;
    }
    const cells = [];
    for (let c = 0; c < w * h; c++) if (on[c]) cells.push([ox + (c % w), oy + Math.floor(c / w)]);
    pakNames.set(d.toString('latin1', o, o + 16).replace(/\0.*$/s, ''), { w, h, ox, oy, cells });
  }
  return pakNames;
}
// The widest text each token can print (12 h clocks, the longest spans), for the fit check.
const TOKEN_WORST = {
  s: '100', w: '100', sleft: '100', end: '100', clock: '00:00 PM', dow: 'Wed', 'sr@': '00:00 PM',
  'wr@': '00:00 PM', 'wr@d': 'Wed 00:00 PM', 'sr~': '00d 00h', 'wr~': '00d 00h', out: '~00:00 PM',
};
const FLAG = { zeroOk: 1, upper: 2, clamp: 4 };
const ALIGN = { left: 0, center: 1, right: 2 };
const TOKENS = { s: 1, w: 2, sleft: 3, clock: 4, dow: 5, 'sr@': 6, 'wr@d': 7, 'sr~': 8, 'wr~': 9, out: 10, end: 11, 'wr@': 12 };
// Characters each token can print (12/24 h clocks, weekday names, spans, "--").
const NUM = '0123456789-', TIME = '0123456789:- APM', DAYS = 'SunMonTueWedThuFriSat';
const TOKEN_CHARS = {
  s: NUM, w: NUM, sleft: NUM, end: NUM, clock: TIME, dow: DAYS, 'sr@': TIME, 'wr@': TIME,
  'wr@d': TIME + DAYS, 'sr~': '0123456789hmd -', 'wr~': '0123456789hmd -', out: TIME + '~',
};

// Fonts the firmware links (clawd_fonts.h). A text op uses one of these when it
// covers every character the op can print; otherwise the pack embeds a subset.
const BUILTIN = [
  { key: 's14', id: 0, file: 'font_styrene_14.c', chars: range(0x20, 0x7e) },
  { key: 's24', id: 1, file: 'font_styrene_24.c', chars: cps('0123456789%-') },
  { key: 'm18', id: 2, file: 'font_mono_18.c', chars: null },
  { key: 't34', id: 3, file: 'font_tiempos_34.c', chars: cps('Usage0123456789: APM') },
];
const FONT_FILES = {
  s12: 'font_styrene_12.c', s14: 'font_styrene_14.c', s16: 'font_styrene_16.c', s20: 'font_styrene_20.c',
  s24: 'font_styrene_24.c', s28: 'font_styrene_28.c', s48: 'font_styrene_48.c',
  t34: 'font_tiempos_34.c', t56: 'font_tiempos_56.c', m18: 'font_mono_18.c', m32: 'font_mono_32.c',
};

function range(a, b) { const r = []; for (let i = a; i <= b; i++) r.push(i); return r; }
function cps(s) { return [...new Set([...s].map((c) => c.codePointAt(0)))]; }

// ---- colours -------------------------------------------------------------------
function rgb(hex) { return [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16)); }
function hex(c) { return '#' + c.map((v) => Math.round(v).toString(16).padStart(2, '0')).join(''); }
function mix(a, b, k) { const A = rgb(a), B = rgb(b); return hex(A.map((v, i) => v * (1 - k) + B[i] * k)); }
function rgb565(h) { const [r, g, b] = rgb(h); return ((r >> 3) << 11) | ((g >> 2) << 5) | (b >> 3); }
// Two ops never show together: one needs a condition the other excludes, or their gates
// on the same source don't overlap. hot/whot/over imply ok (the clock sets them with data).
function exclusive(a, b) {
  const DATA = (1 << COND.hot) | (1 << COND.whot) | (1 << COND.over), OK = 1 << COND.ok;
  const need = (o) => o.need | (o.need & DATA ? OK : 0);
  if ((need(a) & b.not) || (need(b) & a.not)) return true;
  return !!(a.gateSrc && a.gateSrc === b.gateSrc && (a.gateHi <= b.gateLo || b.gateHi <= a.gateLo));
}
function hexOf(v) { return hex([(v >> 11) << 3, ((v >> 5) & 63) << 2, (v & 31) << 3]); }   // RGB565 -> '#rrggbb'

// ---- LVGL fonts (same parser as tools/clawd_assets.js) -----------------------------
function arrayBody(text, decl) {
  const i = text.indexOf(decl);
  if (i < 0) throw new Error(`missing ${decl}`);
  const open = text.indexOf('{', i);
  return text.slice(open + 1, text.indexOf('};', open));
}
function numbers(body) {
  return body.replace(/\/\*[\s\S]*?\*\//g, '').replace(/\/\/.*$/gm, '').split(/[\s,]+/).filter(Boolean).map(Number);
}
const fontCache = new Map();
function parseFont(dir, file) {
  const key = path.join(dir, file);
  if (fontCache.has(key)) return fontCache.get(key);
  const t = fs.readFileSync(key, 'utf8');
  const bitmap = numbers(arrayBody(t, 'glyph_bitmap[] ='));
  const glyphs = [];
  const re = /\{\.bitmap_index = (\d+), \.adv_w = (\d+), \.box_w = (\d+), \.box_h = (\d+), \.ofs_x = (-?\d+), \.ofs_y = (-?\d+)\}/g;
  const dsc = arrayBody(t, 'glyph_dsc[] =');
  for (let m; (m = re.exec(dsc));) glyphs.push({ off: +m[1], adv: +m[2], w: +m[3], h: +m[4], ox: +m[5], oy: +m[6] });
  const map = new Map();
  const cmapRe = /\.range_start = (\d+), \.range_length = (\d+), \.glyph_id_start = (\d+),\s*\.unicode_list = (\w+), \.glyph_id_ofs_list = (\w+), \.list_length = (\d+), \.type = (\w+)/g;
  for (let m; (m = cmapRe.exec(t));) {
    const [start, len, gid, type] = [+m[1], +m[2], +m[3], m[7]];
    if (type === 'LV_FONT_FMT_TXT_CMAP_FORMAT0_TINY') for (let k = 0; k < len; k++) map.set(start + k, gid + k);
    else if (type === 'LV_FONT_FMT_TXT_CMAP_SPARSE_TINY') numbers(arrayBody(t, `${m[4]}[] =`)).forEach((d, k) => map.set(start + d, gid + k));
    else throw new Error(`${file}: unsupported cmap ${type}`);
  }
  const kerns = [];
  if (/\.kern_dsc = &kern_pairs/.test(t)) {
    const ids = numbers(arrayBody(t, 'kern_pair_glyph_ids[] ='));
    const vals = numbers(arrayBody(t, 'kern_pair_values[] ='));
    const scale = +/\.kern_scale = (\d+)/.exec(t)[1];
    for (let k = 0; k < vals.length; k++) kerns.push({ l: ids[2 * k], r: ids[2 * k + 1], v: (vals[k] * scale) >> 4 });
  }
  const f = { bitmap, glyphs, map, kerns, lineHeight: +/\.line_height = (\d+)/.exec(t)[1], baseLine: +/\.base_line = (\d+)/.exec(t)[1] };
  // A middle dot (U+00B7) where the font has none: its full stop, raised to mid x-height.
  const dot = f.map.get(0x2e), x = f.map.get(0x78);
  if (!f.map.has(0xb7) && dot !== undefined && x !== undefined) {
    const g = f.glyphs[dot], xg = f.glyphs[x];
    glyphs.push({ ...g, oy: Math.round(xg.oy + xg.h / 2 - g.h / 2) });
    map.set(0xb7, glyphs.length - 1);
  }
  fontCache.set(key, f);
  return f;
}

// A subset of `f` for code points `want` (sorted), laid out like clawd_fonts.h.
function subset(f, want, file) {
  const missing = want.filter((cp) => !f.map.has(cp));
  if (missing.length) throw new Error(`${file}: no glyph for ${missing.map((c) => JSON.stringify(String.fromCodePoint(c))).join(' ')}`);
  const idToIdx = new Map();
  const bmp = [];
  const glyphs = want.map((cp, idx) => {
    const g = f.glyphs[f.map.get(cp)];
    idToIdx.set(f.map.get(cp), idx);
    const off = bmp.length;
    bmp.push(...f.bitmap.slice(g.off, g.off + Math.ceil((g.w * g.h) / 2)));
    return { cp, off, adv: g.adv, w: g.w, h: g.h, ox: g.ox, oy: g.oy };
  });
  const kerns = f.kerns.filter((k) => idToIdx.has(k.l) && idToIdx.has(k.r) && k.v !== 0)
    .map((k) => ({ l: idToIdx.get(k.l), r: idToIdx.get(k.r), v: k.v }))
    .sort((a, b) => (a.l - b.l) || (a.r - b.r));
  if (glyphs.length > 255 || bmp.length > 0xffff) throw new Error(`${file}: subset too large`);
  return { glyphs, kerns, bmp, lineHeight: f.lineHeight, baseLine: f.baseLine };
}

// Pixel width of `s` exactly as ClawdDraw's walkText measures it.
function textWidth(f, s) {
  let pen = 0, prev = -1, prevAdv = 0;
  for (const ch of s) {
    let gid = f.map.get(ch.codePointAt(0));
    if (gid === undefined) gid = f.map.get(63);
    if (gid === undefined) continue;
    if (prev >= 0) {
      const k = f.kerns.find((q) => q.l === prev && q.r === gid);
      pen += (prevAdv + (k ? k.v : 0) + 8) >> 4;
    }
    prev = gid;
    prevAdv = f.glyphs[gid].adv;
  }
  if (prev >= 0) pen += (prevAdv + 8) >> 4;
  return pen;
}

// ---- masks -------------------------------------------------------------------------
// Clawd, the 32x40 sprite the mockups recolour (one colour, transparent eyes).
const CLAWD_ART = [
  '..########............########..', '..########............########..', '..########............########..',
  '..########............########..', '..########............########..', '..########............########..',
  '..########............########..', '..########............########..', '################################',
  '################################', '################################', '################################',
  '######..################..######', '######..################..######', '####....################....####',
  '####....################....####', ...Array(16).fill('################################'),
  ...Array(8).fill('####....####........####....####'),
];
function artMask(rows) { return { w: rows[0].length, h: rows.length, on: (x, y) => rows[y]?.[x] === '#' }; }
function clawdMasks() {
  const body = artMask(CLAWD_ART);
  const eye = (x, y) => !body.on(x, y) && y >= 12 && y <= 15 && ((x >= 4 && x < 8) || (x >= 24 && x < 28));
  const edge = (x, y) => body.on(x, y) && (!body.on(x - 1, y) || !body.on(x + 1, y) || !body.on(x, y - 1) || !body.on(x, y + 1));
  return {
    clawd: body,
    clawd_eyes: { w: 32, h: 40, on: eye },
    clawd_edge: { w: 32, h: 40, on: edge },
    clawd_inner: { w: 32, h: 40, on: (x, y) => body.on(x, y) && !edge(x, y) },
  };
}

// ---- builder -----------------------------------------------------------------------
function parseWhen(when) {
  let need = 0, not = 0;
  for (const w of (when || '').split(/\s+/).filter(Boolean)) {
    const neg = w.startsWith('!');
    const bit = COND[neg ? w.slice(1) : w];
    if (bit === undefined) throw new Error(`unknown condition ${w}`);
    if (neg) not |= 1 << bit; else need |= 1 << bit;
  }
  return { need, not };
}

class Style {
  constructor({ id, name, bg, light = false, fontDir }) {
    if (!/^[a-z0-9_-]{1,15}$/.test(id)) throw new Error(`bad id ${id}`);
    this.id = id; this.name = name; this.bg = bg; this.light = light; this.fontDir = fontDir;
    this.palette = []; this.ops = []; this.tracks = []; this.masks = []; this.maskIds = new Map();
    this.fontUse = new Map();     // font key -> { lit, tok } code point sets
    this.clawd = clawdMasks();
  }
  color(c) {
    if (typeof c !== 'string' || !/^#[0-9a-f]{6}$/i.test(c)) throw new Error(`bad colour ${c}`);
    const v = rgb565(c);
    let i = this.palette.indexOf(v);
    if (i < 0) { i = this.palette.length; this.palette.push(v); }
    if (i > 254) throw new Error('palette > 255 colours');
    return i;
  }
  font(key) { return parseFont(this.fontDir, FONT_FILES[key]); }
  width(key, s) { return textWidth(this.font(key), s); }

  // An animation track: steps of stepMs, each { dx, dy, vis, color }.
  track(stepMs, steps) {
    const t = { stepMs, steps: steps.map((s) => ({ dx: s.dx | 0, dy: s.dy | 0, vis: s.vis === false ? 0 : 1, color: s.color ? this.color(s.color) : 255 })) };
    if (t.steps.length < 1 || t.steps.length > 255 || stepMs < 10) throw new Error('bad track');
    this.tracks.push(t);
    return this.tracks.length - 1;
  }
  blink(ms, onFirst = false) { return this.track(ms, [{ vis: onFirst }, { vis: !onFirst }]); }

  op(kind, color, o = {}) {
    const { need, not } = parseWhen(o.when);
    const op = {
      kind, color: this.color(color), need, not, track: o.track ?? 255, flags: 0,
      x: 0, y: 0, w: 0, h: 0, dxSrc: 0, x1Src: 0, dxSpan: 0, x1Span: 0, phase: o.phase || 0,
      gateSrc: 0, a: 0, gateLo: 0, gateHi: 0, b: 0, c: 0, x3: 0, y3: 0,
    };
    if (o.dx) { op.dxSrc = SRC[o.dx[0]]; op.dxSpan = o.dx[1]; }
    if (o.x1) { op.x1Src = SRC[o.x1[0]]; op.x1Span = o.x1[1]; }
    if (o.gate) { op.gateSrc = SRC[o.gate[0]]; op.gateLo = Math.round(o.gate[1] * 1000); op.gateHi = o.gate[2] === undefined ? 0xffff : Math.round(o.gate[2] * 1000); }
    if (o.zeroOk) op.flags |= FLAG.zeroOk;
    this.ops.push(op);
    return op;
  }
  rect(x, y, w, h, color, o = {}) {
    if (w <= 0 && !o.x1) return null;
    const op = this.op(KIND.rect, color, o);
    Object.assign(op, { x, y, w, h, a: Math.max(0, Math.min(255, Math.floor(o.r || 0))) });
    op.c = o.minW || 0;
    // pattern: ['diag', mod, phase] pixels where (x + y + phase) % mod == 0;
    //          ['cols', mod, phase, on] where (x + phase) % mod < on; ['rows', ...] the same in y
    if (o.pattern) {
      const [kind, mod, ph = 0, on = 1] = o.pattern;
      const k = { diag: 1, cols: 2, rows: 3 }[kind];
      if (!k || mod < 1 || mod > 255) throw new Error(`bad pattern ${o.pattern}`);
      op.b = mod; op.x3 = ((ph % mod) + mod) % mod; op.y3 = (k << 8) | on;
    }
    return op;
  }
  strokeRect(x, y, w, h, color, o = {}) {
    this.rect(x, y, w, 1, color, o); this.rect(x, y + h - 1, w, 1, color, o);
    this.rect(x, y + 1, 1, h - 2, color, o); this.rect(x + w - 1, y + 1, 1, h - 2, color, o);
  }
  text(str, x, y, font, color, o = {}) {
    const op = this.op(KIND.text, color, o);
    const upper = !!o.upper;
    const enc = encodeTemplate(str, upper);
    Object.assign(op, { x, y, str: enc.bytes, a: font, b: ALIGN[o.align || 'left'] });
    if (upper) op.flags |= FLAG.upper;
    if (o.clamp) op.flags |= FLAG.clamp;
    let u = this.fontUse.get(font);
    if (!u) { u = { lit: new Set(), tok: new Set() }; this.fontUse.set(font, u); }
    for (const ch of enc.lit) u.lit.add(ch.codePointAt(0));
    for (const ch of enc.tok) u.tok.add(ch.codePointAt(0));
    return op;
  }
  line(x0, y0, x1, y1, color, o = {}) {
    const op = this.op(KIND.line, color, o);
    Object.assign(op, { x: x0, y: y0, w: x1, h: y1 });
    return op;
  }
  tri(p, color, o = {}) {
    const op = this.op(KIND.tri, color, o);
    Object.assign(op, { x: Math.round(p[0][0]), y: Math.round(p[0][1]), w: Math.round(p[1][0]), h: Math.round(p[1][1]), x3: Math.round(p[2][0]), y3: Math.round(p[2][1]) });
    return op;
  }
  // name: a Clawd mask, or a custom one registered with defMask.
  mask(name, x, y, scale, color, o = {}) {
    const op = this.op(KIND.mask, color, o);
    Object.assign(op, { x, y, a: this.maskId(name), b: scale });
    return op;
  }
  defMask(name, w, h, on) { this.clawd[name] = { w, h, on }; }

  // Official Clawd animations (docs/STYLE_PACK.md, "anim"): `names` play in turn, each
  // after `idle` ms on its first frame. (x, y) is the screen origin of the 55x37 stage,
  // `cell` its pixels per art cell. Colours are the roles the official palette maps to.
  anim(names, x, y, cell, colors, o = {}) {
    const pak = clawdPakNames();
    for (const n of names) if (!pak.has(n)) throw new Error(`${this.id}: no animation "${n}" in ${CLAWD_PAK}`);
    if (!names.length || names.length > 8) throw new Error(`${this.id}: 1..8 animations per op`);
    if (!(cell >= 1 && cell <= 8)) throw new Error(`${this.id}: bad cell ${cell}`);
    if (o.track !== undefined) throw new Error(`${this.id}: an animation takes no track`);
    const idle = Math.round(o.idle ?? 2000);   // 8 s left him still ~80 % of the time on the clock
    if (idle < 0 || idle > 65535) throw new Error(`${this.id}: idle ${idle} ms`);
    const op = this.op(KIND.anim, colors.body, o);
    const c = (k) => this.color(colors[k] ?? colors.body);
    Object.assign(op, {
      x, y, h: idle, a: cell, str: Buffer.from(names.join(','), 'latin1'),
      b: c('shade'), c: c('ink'), x3: c('grey') | (c('light') << 8), y3: c('accent'),
    });
    // x3 is written as int16: keep the light role's index (<= 254) in the low 15 bits' range
    if (op.x3 > 0x7fff) op.x3 -= 0x10000;
    if (op.h > 0x7fff) op.h -= 0x10000;  // idle up to 65535 ms, read back as u16
    return op;
  }

  // Turn the pack's Clawd masks into official animations (`cfg`, tools/style/anims.js).
  // Every group of Clawd layers drawn at one spot in one state becomes one anim op, gated
  // on `anim` (the clock has every animation); the masks stay as the fallback (`!anim`).
  // The no-data Clawd (`!ok`) is left alone: he sleeps. Moving tracks (bob, blink) are
  // dropped: the animation is the motion. Data bindings (dx, gate) carry over.
  animateClawd(cfg) {
    const names = new Map([...this.maskIds].map(([n, id]) => [id, n]));
    const LAYERS = ['clawd', 'clawd_inner', 'clawd_edge', 'clawd_eyes'];
    const OK = 1 << COND.ok, ANIM = 1 << COND.anim;
    const isClawd = (op) => op.kind === KIND.mask && LAYERS.includes(names.get(op.a));
    const state = (op) => [op.b, op.need, op.not, op.gateSrc, op.gateLo, op.gateHi, op.dxSrc, op.dxSpan].join(',');
    // A group: the layers at one spot in one state. Copies 1-2 px apart in the same state
    // (a glow, synth's halo) join the group drawn last, so one animation replaces them all.
    const spots = [];
    for (const op of this.ops) {
      if (!isClawd(op) || (op.not & OK)) continue;
      const s = state(op);
      if (!spots.some((p) => p.s === s && p.x === op.x && p.y === op.y)) spots.push({ s, x: op.x, y: op.y });
    }
    const keyOf = new Map();
    for (const p of spots) {
      const near = spots.filter((q) => q.s === p.s && Math.abs(q.x - p.x) <= 2 && Math.abs(q.y - p.y) <= 2);
      const last = near[near.length - 1];
      keyOf.set(`${p.s}|${p.x},${p.y}`, `${last.s}|${last.x},${last.y}`);
    }
    const key = (op) => keyOf.get(`${state(op)}|${op.x},${op.y}`);
    const groups = new Map();
    for (const op of this.ops) {
      if (!isClawd(op) || (op.not & OK)) continue;
      const k = key(op);
      if (!groups.has(k)) groups.set(k, []);
      groups.get(k).push(op);
    }
    // A group plays an animation when it has a body layer. A fill alone under an outline
    // drawn by other groups at that spot (hifivfd, mac84) only hides: the outline animates.
    const outlined = (q) => [...groups.values()].some((g) => g.some((r) => r.x === q.x && r.y === q.y && names.get(r.a) === 'clawd_edge'));
    const animates = (g) => g.some((q) => names.get(q.a) === 'clawd' || names.get(q.a) === 'clawd_edge') ||
      (g.some((q) => names.get(q.a) === 'clawd_inner') && !outlined(g[0]));
    // the animation goes where the group's last layer at its own spot is drawn (on top);
    // worked out before the loop below changes need/not
    const lastAt = new Map();
    for (const [k, g] of groups) lastAt.set(k, g.filter((q) => `${state(q)}|${q.x},${q.y}` === k).pop());
    // conditions the clock only sets with live data: such a mask needs no !ok copy
    const DATA = (1 << COND.hot) | (1 << COND.whot) | (1 << COND.over);
    const out = [];
    const anims = [];
    for (const op of this.ops) {
      const k = isClawd(op) && !(op.not & OK) ? key(op) : null;
      if (k && op === lastAt.get(k) && animates(groups.get(k))) {
        const a = this.animFor(groups.get(k), cfg, names);
        out.push(a);
        anims.push(a);
      }
      if (k && !(op.need & OK) && !(op.need & DATA)) {
        // one Clawd for every state: without data he stays as he was (a copy for !ok),
        // with data the animation plays, the mask only as the fallback
        out.push({ ...op, not: op.not | OK });
        op.need |= OK;
      }
      if (k) op.not |= ANIM;
      out.push(op);
    }
    if (!anims.length) throw new Error(`${this.id}: no Clawd to animate`);
    // the clock refuses a pack with more than 8, and plays only the first one shown: the
    // masks of any other shown with it are hidden (!anim), so that Clawd would vanish
    if (anims.length > 8) throw new Error(`${this.id}: ${anims.length} Clawd animations (8 at most)`);
    for (let i = 0; i < anims.length; i++) {
      for (let j = i + 1; j < anims.length; j++) {
        if (!exclusive(anims[i], anims[j])) throw new Error(`${this.id}: two Clawd animations can show at once (ops ${i}, ${j})`);
      }
    }
    this.ops = out;
    return anims.length;
  }

  // ---- fit: does an animation stay clear of the screen's text and data? ---------------
  // The boxes the Clawd must not cover: every text op (its line box at the widest the
  // tokens print, plus its track's moves) and every op bound to data (dx, x1, a gate),
  // Clawd's own masks and animations excepted.
  protectedBoxes() {
    const names = new Map([...this.maskIds].map(([n, id]) => [id, n]));
    const DATA = (s) => s > 0 && s < SRC.fixed;
    const out = [];
    for (const op of this.ops) {
      if (op.kind === KIND.anim || (op.kind === KIND.mask && /^clawd/.test(names.get(op.a) || ''))) continue;
      let x0, y0, x1, y1;
      if (op.kind === KIND.text) {
        let s = '';
        for (const b of op.str) s += b < 13 ? TOKEN_WORST[Object.keys(TOKENS).find((k) => TOKENS[k] === b)] : String.fromCharCode(b);
        if (op.flags & FLAG.upper) s = s.toUpperCase();
        const f = this.font(op.a);
        const str = Buffer.from(s, 'latin1').toString('utf8');
        const w = textWidth(f, str);
        x0 = op.x - [0, w >> 1, w][op.b];   // align: left, centre, right
        x1 = x0 + w;
        // the glyphs' ink rows (ClawdDraw: top = baseline - h - oy), not the taller line box
        const base = op.y + f.lineHeight - f.baseLine;
        const gs = [...str].map((ch) => f.glyphs[f.map.get(ch.codePointAt(0)) ?? f.map.get(63)]).filter((g) => g && g.h);
        if (!gs.length) continue;
        y0 = base - Math.max(...gs.map((g) => g.h + g.oy)); y1 = base - Math.min(...gs.map((g) => g.oy));
      } else if (DATA(op.dxSrc) || DATA(op.x1Src) || op.gateSrc) {
        if (op.kind === KIND.rect) {
          x0 = op.x; y0 = op.y; y1 = op.y + op.h;
          x1 = op.x + Math.max(op.w, op.c, op.w + (DATA(op.x1Src) ? op.x1Span : 0));
        } else if (op.kind === KIND.mask) {
          const m = this.masks[op.a];
          x0 = op.x; y0 = op.y; x1 = op.x + m.w * op.b; y1 = op.y + m.h * op.b;
        } else {                         // line / triangle: the hull of its points
          const xs = [op.x, op.w, ...(op.kind === KIND.tri ? [op.x3] : [])], ys = [op.y, op.h, ...(op.kind === KIND.tri ? [op.y3] : [])];
          x0 = Math.min(...xs); x1 = Math.max(...xs) + 1; y0 = Math.min(...ys); y1 = Math.max(...ys) + 1;
        }
      } else continue;
      if (DATA(op.dxSrc)) { x0 += Math.min(0, op.dxSpan); x1 += Math.max(0, op.dxSpan); }
      if (op.track !== 255) {             // a moving label: every step's offset
        const st = this.tracks[op.track].steps;
        x0 += Math.min(0, ...st.map((q) => q.dx)); x1 += Math.max(0, ...st.map((q) => q.dx));
        y0 += Math.min(0, ...st.map((q) => q.dy)); y1 += Math.max(0, ...st.map((q) => q.dy));
      }
      out.push({ op, x0, y0, x1, y1 });
    }
    return out;
  }

  // What animation `name` would hit if `anim` (an anim op) played it: [] when it fits.
  animFit(anim, name, boxes = this.protectedBoxes()) {
    const a = clawdPakNames().get(name);
    const cell = anim.a;
    const sweep = anim.dxSrc > 0 && anim.dxSrc < SRC.fixed ? anim.dxSpan : 0;
    const hits = new Set();
    const home = anim.home;                 // where the old Clawd mask already drew: his own
    for (const [cx, cy] of a.cells) {
      const x0 = anim.x + cx * cell + Math.min(0, sweep), x1 = anim.x + (cx + 1) * cell + Math.max(0, sweep);
      const y0 = anim.y + cy * cell, y1 = y0 + cell;
      if (home && x0 >= home.x0 && x1 <= home.x1 && y0 >= home.y0 && y1 <= home.y1) continue;
      if (x0 < 0 || y0 < 0 || x1 > 240 || y1 > 240) hits.add('off screen');
      for (const b of boxes) {
        if (x0 < b.x1 && b.x0 < x1 && y0 < b.y1 && b.y0 < y1 && !exclusive(anim, b.op)) {
          hits.add(b.op.kind === KIND.text ? `text "${[...b.op.str].map((c) => (c < 13 ? '{}' : String.fromCharCode(c))).join('')}"` : `data op at ${b.x0},${b.y0}`);
        }
      }
    }
    return [...hits];
  }

  // Every anim op's every animation must fit; throws with what each one would cover.
  checkAnimFit() {
    const boxes = this.protectedBoxes();
    const bad = [];
    for (const op of this.ops.filter((q) => q.kind === KIND.anim)) {
      for (const n of op.str.toString('latin1').split(',')) {
        const hits = this.animFit(op, n, boxes);
        if (hits.length) bad.push(`${n} at ${op.x},${op.y} cell ${op.a}: ${hits.join(', ')}`);
      }
    }
    if (bad.length) throw new Error(`${this.id}: Clawd animations over the screen:\n  ${bad.join('\n  ')}`);
  }

  // One anim op for a group of Clawd layers (same spot, scale and state).
  animFor(group, cfg, names) {
    const top = group[group.length - 1];   // the spot drawn last (not a glow copy beside it)
    const layer = (n) => group.find((q) => names.get(q.a) === n && q.x === top.x && q.y === top.y);
    // An outlined Clawd's body is its fill, unless the fill is (nearly) the background
    // (mac84's white, a VFD's unlit segments): then the outline is what shows.
    const dist = (a, b) => [0, 1, 2].reduce((t, i) => t + Math.abs(rgb(a)[i] - rgb(b)[i]), 0);
    const inner = layer('clawd_inner'), edge = layer('clawd_edge');
    const fill = inner && (!edge || dist(hexOf(this.palette[inner.color]), this.bg) > 90) ? inner : null;
    const body = layer('clawd') || fill || edge || inner || group.find((q) => names.get(q.a) !== 'clawd_eyes');
    // the eye colour: this state's eye layer, else any with-data eye layer at the same spot
    const eyes = layer('clawd_eyes') || this.ops.find((q) => q.kind === KIND.mask && names.get(q.a) === 'clawd_eyes' &&
      q.x === body.x && q.y === body.y && q.b === body.b && !(q.not & (1 << COND.ok)));
    const hex = (i) => hexOf(this.palette[i]);
    const s = body.b;
    // per-state colours: cfg.states = { hot: { body, shade, ink } } when the group needs that condition
    let st = {};
    for (const [cond, v] of Object.entries(cfg.states || {})) if (body.need & (1 << COND[cond])) st = v;
    const cell = cfg.cell ?? s + 1;
    const bodyC = st.body ?? cfg.body ?? hex(body.color);
    // Clawd's own orange keeps a shade (the official #b86848 is 15 % darker); others are flat
    const orange = (h) => { const [r, g, b] = [1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16)); return Math.abs(r - 217) + Math.abs(g - 119) + Math.abs(b - 87) < 40; };
    const colors = {
      body: bodyC,
      shade: st.shade ?? cfg.shade ?? (orange(bodyC) ? mix(bodyC, '#000000', 0.15) : bodyC),
      ink: st.ink ?? cfg.ink ?? (eyes ? hex(eyes.color) : this.bg),
      grey: cfg.grey ?? mix(bodyC, this.bg, 0.5),
      light: cfg.light ?? mix(bodyC, '#ffffff', 0.7),
      accent: cfg.accent ?? mix(bodyC, this.bg, 0.3),
    };
    const [ax, ay] = cfg.at || [0, 0];
    // idle Clawd's body (16 cells wide) centred on the mask's 32 px, his feet on its bottom
    const x = body.x + 16 * s - STAGE_CX * cell + ax;
    const y = body.y + 40 * s - STAGE_FEET * cell + ay;
    const op = this.anim(cfg.set, x, y, cell, colors, { idle: cfg.idle });
    this.ops.pop();                      // anim() appended it; the caller places it
    Object.assign(op, {
      // (the group's layers drawn before this one already carry !anim: not for the animation)
      need: body.need | (1 << COND.anim) | (1 << COND.ok), not: body.not & ~(1 << COND.anim), gateSrc: body.gateSrc, gateLo: body.gateLo,
      gateHi: body.gateHi, dxSrc: body.dxSrc, dxSpan: body.dxSpan,
    });
    // Clawd's own room, swept like the animation: the old masks' box and the standing
    // Clawd's (stage cells 15..38 x 20..36, what crab walking and dancing cover). animFit
    // lets an animation cover it; what it adds beyond must clear text, data and the edge.
    const sw = body.dxSrc > 0 && body.dxSrc < SRC.fixed ? body.dxSpan : 0;
    op.home = {
      x0: Math.min(x + 15 * cell, ...group.map((q) => q.x)) + Math.min(0, sw),
      y0: Math.min(y + 20 * cell, ...group.map((q) => q.y)),
      x1: Math.max(x + 39 * cell, ...group.map((q) => q.x + 32 * q.b)) + Math.max(0, sw),
      y1: Math.max(y + 37 * cell, ...group.map((q) => q.y + 40 * q.b)),
    };
    return op;
  }
  maskId(name) {
    if (this.maskIds.has(name)) return this.maskIds.get(name);
    const m = this.clawd[name];
    if (!m) throw new Error(`unknown mask ${name}`);
    this.masks.push(m);
    this.maskIds.set(name, this.masks.length - 1);
    return this.masks.length - 1;
  }
  // A filled disc (centre and radius may be fractional) as a mask, pixel centres inside.
  disc(cx, cy, r, color, o = {}) {
    const x0 = Math.floor(cx - r), y0 = Math.floor(cy - r), n = Math.ceil(2 * r) + 2;
    const name = `disc:${cx % 1},${cy % 1},${r}`;
    if (!this.clawd[name]) this.defMask(name, n, n, (x, y) => ((x0 + x + 0.5 - cx) ** 2 + (y0 + y + 0.5 - cy) ** 2) <= r * r);
    return this.mask(name, x0, y0, 1, color, o);
  }

  // ---- binary --------------------------------------------------------------------
  build() {
    const fonts = this.resolveFonts();
    const parts = [];
    let off = HEADER;
    const place = (buf, align = 4) => { off = (off + align - 1) & ~(align - 1); const at = off; parts.push({ at, buf }); off += buf.length; return at; };

    const pal = Buffer.alloc(this.palette.length * 2);
    this.palette.forEach((v, i) => pal.writeUInt16LE(v, i * 2));
    const paletteOff = place(pal);

    const fontDescs = Buffer.alloc(fonts.length * FONT_DESC);
    const fontsOff = place(fontDescs);
    fonts.forEach((f, i) => {
      const d = i * FONT_DESC;
      fontDescs.writeUInt8(f.builtin ?? 255, d + 23);
      if (f.builtin !== undefined) return;
      const g = Buffer.alloc(f.data.glyphs.length * 10);
      f.data.glyphs.forEach((q, k) => {
        g.writeUInt16LE(q.cp, k * 10); g.writeUInt16LE(q.off, k * 10 + 2); g.writeUInt16LE(q.adv, k * 10 + 4);
        g.writeUInt8(q.w, k * 10 + 6); g.writeUInt8(q.h, k * 10 + 7); g.writeInt8(q.ox, k * 10 + 8); g.writeInt8(q.oy, k * 10 + 9);
      });
      const kb = Buffer.alloc(f.data.kerns.length * 3);
      f.data.kerns.forEach((k, j) => { kb.writeUInt8(k.l, j * 3); kb.writeUInt8(k.r, j * 3 + 1); kb.writeInt8(k.v, j * 3 + 2); });
      fontDescs.writeUInt32LE(place(g), d);
      fontDescs.writeUInt32LE(place(Buffer.from(f.data.bmp)), d + 4);
      fontDescs.writeUInt32LE(f.data.bmp.length, d + 8);
      fontDescs.writeUInt32LE(kb.length ? place(kb) : 0, d + 12);
      fontDescs.writeUInt16LE(f.data.kerns.length, d + 16);
      fontDescs.writeUInt8(f.data.glyphs.length, d + 18);
      fontDescs.writeUInt8(f.data.lineHeight, d + 19);
      fontDescs.writeUInt8(f.data.baseLine, d + 20);
    });

    const trackDescs = Buffer.alloc(this.tracks.length * TRACK_DESC);
    const tracksOff = place(trackDescs);
    this.tracks.forEach((t, i) => {
      const s = Buffer.alloc(t.steps.length * 4);
      t.steps.forEach((q, k) => { s.writeInt8(q.dx, k * 4); s.writeInt8(q.dy, k * 4 + 1); s.writeUInt8(q.vis, k * 4 + 2); s.writeUInt8(q.color, k * 4 + 3); });
      trackDescs.writeUInt16LE(t.stepMs, i * TRACK_DESC);
      trackDescs.writeUInt8(t.steps.length, i * TRACK_DESC + 2);
      trackDescs.writeUInt32LE(place(s), i * TRACK_DESC + 4);
    });

    const maskDescs = Buffer.alloc(this.masks.length * MASK_DESC);
    const masksOff = place(maskDescs);
    this.masks.forEach((m, i) => {
      const stride = Math.ceil(m.w / 8);
      const bits = Buffer.alloc(stride * m.h);
      for (let y = 0; y < m.h; y++) for (let x = 0; x < m.w; x++) if (m.on(x, y)) bits[y * stride + (x >> 3)] |= 0x80 >> (x & 7);
      if (m.w > 255 || m.h > 255) throw new Error('mask too large');
      maskDescs.writeUInt8(m.w, i * MASK_DESC); maskDescs.writeUInt8(m.h, i * MASK_DESC + 1);
      maskDescs.writeUInt32LE(place(bits), i * MASK_DESC + 4);
    });

    // strings: NUL-terminated templates, shared when equal
    const strs = [];
    const strOff = new Map();
    let slen = 0;
    for (const op of this.ops) {
      if (op.kind !== KIND.text && op.kind !== KIND.anim) continue;
      const k = op.str.toString('latin1');
      if (!strOff.has(k)) { strOff.set(k, slen); strs.push(op.str, Buffer.from([0])); slen += op.str.length + 1; }
      op.w = strOff.get(k);
    }
    const strings = Buffer.concat(strs);
    const stringsOff = place(strings);

    const ops = Buffer.alloc(this.ops.length * OP_SIZE);
    this.ops.forEach((op, i) => {
      const d = i * OP_SIZE;
      if (op.kind === KIND.text) op.a = fonts.findIndex((f) => f.key === op.a);
      ops.writeUInt8(op.kind, d); ops.writeUInt8(op.color, d + 1);
      ops.writeUInt16LE(op.need, d + 2); ops.writeUInt16LE(op.not, d + 4);
      ops.writeUInt8(op.track, d + 6); ops.writeUInt8(op.flags, d + 7);
      ops.writeInt16LE(op.x, d + 8); ops.writeInt16LE(op.y, d + 10); ops.writeInt16LE(op.w, d + 12); ops.writeInt16LE(op.h, d + 14);
      ops.writeUInt8(op.dxSrc, d + 16); ops.writeUInt8(op.x1Src, d + 17);
      ops.writeInt16LE(op.dxSpan, d + 18); ops.writeInt16LE(op.x1Span, d + 20);
      ops.writeUInt16LE(op.phase % 65536, d + 22);
      ops.writeUInt8(op.gateSrc, d + 24); ops.writeUInt8(op.a, d + 25);
      ops.writeUInt16LE(op.gateLo, d + 26); ops.writeUInt16LE(op.gateHi, d + 28);
      ops.writeUInt8(op.b, d + 30); ops.writeUInt8(op.c, d + 31);
      ops.writeInt16LE(op.x3, d + 32); ops.writeInt16LE(op.y3, d + 34);
    });
    const opsOff = place(ops);

    const size = (off + 3) & ~3;
    const out = Buffer.alloc(size);
    for (const p of parts) p.buf.copy(out, p.at);
    out.write('CSTY', 0, 'latin1');
    out.writeUInt8(API, 4); out.writeUInt8(this.light ? 1 : 0, 5); out.writeUInt16LE(HEADER, 6);
    out.writeUInt32LE(size, 8);
    out.write(this.id, 16, 15, 'latin1');
    out.write(this.name.slice(0, 23), 32, 23, 'latin1');
    out.writeUInt16LE(rgb565(this.bg), 56);
    out.writeUInt8(fonts.length, 58); out.writeUInt8(this.tracks.length, 59);
    out.writeUInt16LE(this.ops.length, 60); out.writeUInt16LE(this.palette.length, 62);
    out.writeUInt8(this.masks.length, 64);
    out.writeUInt32LE(paletteOff, 68); out.writeUInt32LE(fontsOff, 72); out.writeUInt32LE(tracksOff, 76);
    out.writeUInt32LE(opsOff, 80); out.writeUInt32LE(masksOff, 84);
    out.writeUInt32LE(stringsOff, 88); out.writeUInt32LE(strings.length, 92);
    out.writeUInt32LE(zlib.crc32(out.subarray(16)) >>> 0, 12);
    if (this.ops.length > 400 || fonts.length > 8) throw new Error(`${this.id}: too many ops/fonts`);
    return { buf: out, fonts: fonts.map((f) => ({ key: f.key, builtin: f.builtin, glyphs: f.data?.glyphs.length, bytes: f.data ? f.data.bmp.length + f.data.glyphs.length * 10 + f.data.kerns.length * 3 : 0 })), ops: this.ops.length, tracks: this.tracks.length, palette: this.palette.length };
  }

  // Literal text must exist in the font; token characters it lacks are dropped
  // (the firmware draws '?' for them, e.g. a 12 h suffix in a digits-only font).
  resolveFonts() {
    const out = [];
    for (const [key, u] of this.fontUse) {
      const f = this.font(key);
      const lit = [...u.lit];
      const bad = lit.filter((cp) => !f.map.has(cp));
      if (bad.length) throw new Error(`${this.id}: ${key} has no glyph for ${bad.map((c) => JSON.stringify(String.fromCodePoint(c))).join(' ')}`);
      const want = [...new Set([...lit, ...[...u.tok].filter((cp) => f.map.has(cp))])].sort((a, b) => a - b);
      const b = BUILTIN.find((q) => q.key === key);
      // The linked font is good enough when it has every literal character and the
      // digits; a token letter it lacks (12 h suffix, "--") then draws as nothing.
      const core = [...lit, ...[...u.tok].filter((cp) => cp >= 0x30 && cp <= 0x3a)];
      if (b && (b.chars === null || core.every((cp) => b.chars.includes(cp)))) { out.push({ key, builtin: b.id }); continue; }
      out.push({ key, data: subset(f, want, FONT_FILES[key]) });
    }
    return out;
  }
}

// '{s}%' -> bytes with token codes; also the characters it can print.
function encodeTemplate(str, upper) {
  const bytes = [];
  let lit = '', tok = '';
  const re = /\{([^}]+)\}/g;
  let last = 0;
  const put = (s) => {
    const t = upper ? s.toUpperCase() : s;
    lit += t;
    bytes.push(...Buffer.from(t, 'utf8'));
  };
  for (let m; (m = re.exec(str));) {
    put(str.slice(last, m.index));
    const code = TOKENS[m[1]];
    if (!code) throw new Error(`unknown token {${m[1]}}`);
    bytes.push(code);
    tok += upper ? TOKEN_CHARS[m[1]].toUpperCase() : TOKEN_CHARS[m[1]];
    last = re.lastIndex;
  }
  put(str.slice(last));
  return { bytes: Buffer.from(bytes), lit, tok };
}

module.exports = { Style, mix, rgb565, API, SRC, COND, TOKENS, KIND, clawdPakNames };
