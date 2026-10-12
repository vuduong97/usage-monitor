// Approximate canvas drawing of the clock's usage screens (240x240), from the real usage
// payload. The settings window shows the clock's own render when the app has it
// (preview.py); this is the fallback when it does not, so it is labelled "gần đúng".
// Mirrors the firmware layouts in clawd-home/firmware/src/features/clawd/ClawdUi.cpp:
// 0 classic (two cards + status line), 1 pace (taller cards, pace pill, time
// marker on the bar, forecast line), 2 hero (one big 5-hour number), 3 rings
// (two concentric rings, clock in the middle), 4 gauges (two 270 degree
// gauges), 5 pixel (20-block bars, Clawd strolling the floor). System fonts
// stand in for the clock's own. Clawd is the real sprite when the app could read
// /clawd.pak off the user's clock (settings_api.clawd_pak, setPak below), played
// like ClawdMascot / ClawdWalker in the firmware's ClawdAnim.cpp; without it, the
// app's clock icon.
'use strict';
const PV = (() => {
  const C = { bg:'#000', panel:'#1f1f1e', text:'#faf9f5', dim:'#b0aea5', accent:'#d97757',
              green:'#788c5d', amber:'#d97757', red:'#c0392b', track:'#2a2a28' };
  const SANS = '-apple-system, BlinkMacSystemFont, "Segoe UI", system-ui, sans-serif';
  const F = { s14:`14px ${SANS}`, s24:`500 24px ${SANS}`, t34:'34px "New York", "Iowan Old Style", Georgia, serif',
              m16:'16px ui-monospace, Menlo, Consolas, monospace',
              s48:`500 48px ${SANS}`, m18:'18px ui-monospace, Menlo, Consolas, monospace',
              m32:'32px ui-monospace, Menlo, Consolas, monospace' };
  const W = 240, H = 240, WIN_S = 300, WIN_W = 10080;
  // The usage payload the app sends the clock ({s, sr, w, wr, t, tf, ok}) as the drawers read it.
  // null when there is nothing to show (no payload yet, or the "no data" beat).
  function fromPayload(p) {
    if (!p || p.ok === false || typeof p.s !== 'number') return null;
    const n = (v, dflt) => (typeof v === 'number' && isFinite(v) ? v : dflt);
    let clock = '';
    if (typeof p.t === 'number') {                // local wall-clock seconds: read the UTC fields
      const d = new Date(p.t * 1000), h = d.getUTCHours(), m = String(d.getUTCMinutes()).padStart(2, '0');
      clock = p.tf === 12 ? `${h % 12 || 12}:${m} ${h < 12 ? 'AM' : 'PM'}` : `${String(h).padStart(2, '0')}:${m}`;
    }
    return { s: n(p.s, 0), sr: Math.round(n(p.sr, -1)), w: n(p.w, 0), wr: Math.round(n(p.wr, -1)), clock };
  }
  const pctColor = p => p >= 80 ? C.red : p >= 50 ? C.amber : C.green;
  function fmtReset(m) {
    if (m < 0) return '---';
    if (m < 60) return `${m}m`;
    if (m < 1440) return `${Math.floor(m / 60)}h ${m % 60}m`;
    return `${Math.floor(m / 1440)}d ${Math.floor((m % 1440) / 60)}h`;
  }
  function pace(pct, resetMin, win) {
    const t = Math.min(1, Math.max(0, (win - resetMin) / win));
    if (t < 0.05) return { label:'Just reset', color:C.green, t };
    const end = pct / t;
    if (end > 100) return { label:'Over pace', color:C.red, t };
    if (end >= 70) return { label:'On pace', color:C.amber, t };
    return { label:'Under pace', color:C.green, t };
  }
  function forecast(pct, resetMin, win, clock) {
    const el = win - resetMin;
    if (pct >= 100) return { text:'Limit reached', color:C.red };
    if (el < win * 0.05 || pct < 1) return { text:'Too early to tell', color:C.dim };
    const left = (100 - pct) / (pct / el);
    if (left >= resetMin) return { text:'Lasts until reset', color:C.green };
    if (win === WIN_S) {
      const [h, m] = clock.split(':').map(Number);
      const t = (h * 60 + m + Math.round(left)) % 1440;
      return { text:`Out at ~${String(Math.floor(t / 60)).padStart(2, '0')}:${String(t % 60).padStart(2, '0')}`, color:C.red };
    }
    return { text:`Out in ~${fmtReset(Math.round(left))}`, color:C.red };
  }
  function rr(c, x, y, w, h, r, col) { c.fillStyle = col; c.beginPath(); c.roundRect(x, y, w, h, Math.min(r, w / 2, h / 2)); c.fill(); }
  function tx(c, s, x, y, font, col, align = 'left') { c.font = font; c.fillStyle = col; c.textAlign = align; c.textBaseline = 'top'; c.fillText(s, x, y); }
  function tw(c, s, font) { c.font = font; return c.measureText(s).width; }
  function bar(c, x, y, w, h, pct, col) {
    rr(c, x, y, w, h, h / 2, C.track);
    const f = Math.round(w * Math.max(0, Math.min(100, pct)) / 100);
    if (f > 0) rr(c, x, y, Math.max(f, h), h, h / 2, col);
  }
  // ---- Clawd from the clock's own clawd.pak ----------------------------------------------
  // Format: firmware/tools/clawd_assets.js (clawd-home); reader: ClawdPack.cpp. The players
  // follow ClawdAnim.cpp (ClawdPlayer, ClawdMascot at usage-rate group 0, ClawdWalker): when
  // the firmware's timings or acts change, change them here too.
  const MAS = { gx0:15, gy0:16, gw:28, gh:21, actLoopMs:2500, stillMs:4000, acts:['pointing', 'waving'] };
  const WLK = { standMs:1200, x0:4, x1:W - 4, feet:234 };
  function parsePak(b64) {
    try {
      const bin = atob(b64), u = new Uint8Array(bin.length);
      for (let i = 0; i < bin.length; i++) u[i] = bin.charCodeAt(i);
      const dv = new DataView(u.buffer);
      if (bin.slice(0, 4) !== 'CLWD' || dv.getUint16(4, true) !== 1) return null;
      const anims = [];
      for (let i = 0, n = dv.getUint16(6, true); i < n; i++) {
        const d = 12 + 72 * i;
        let name = '';
        for (let k = 0; k < 15 && u[d + k]; k++) name += String.fromCharCode(u[d + k]);
        const pal = [];
        for (let k = 0; k < 16; k++) {
          const v = dv.getUint16(d + 28 + 2 * k, true);
          pal.push(`rgb(${((v >> 11) & 31) * 255 / 31 | 0},${((v >> 5) & 63) * 255 / 63 | 0},${(v & 31) * 255 / 31 | 0})`);
        }
        anims.push({ name, w:u[d + 16], h:u[d + 17], ox:u[d + 18], oy:u[d + 19], frameCount:dv.getUint16(d + 20, true),
                     loopStart:dv.getUint16(d + 22, true), loopEnd:dv.getUint16(d + 24, true),
                     frameBytes:dv.getUint16(d + 26, true), pal, framesOff:dv.getUint32(d + 60, true),
                     holdsOff:dv.getUint32(d + 64, true) });
      }
      const end = Math.max(...anims.map(a => Math.max(a.framesOff + a.frameCount * a.frameBytes, a.holdsOff + 2 * a.frameCount)));
      if (!anims.length || end > u.length) return null;
      return { u, dv, anims, find: name => anims.findIndex(a => a.name === name) };
    } catch (e) { return null; }
  }
  function cellOf(pak, a, frame, x, y) {               // clawdFrameCell
    if (x < 0 || y < 0 || x >= a.w || y >= a.h) return 0;
    const i = y * a.w + x, b = pak.u[a.framesOff + frame * a.frameBytes + (i >> 1)];
    return i & 1 ? b & 15 : b >> 4;
  }
  function holdOf(pak, a, frame) { return pak.dv.getUint16(a.holdsOff + 2 * frame, true) || 100; }
  // ClawdPlayer: intro -> loop -> outro. step() advances at most one frame, on its due time.
  function player(pak) {
    const p = { a:null, frame:0, frameStart:0, hold:100, loopEntered:0, inLoop:false, loopRelease:false, holding:false };
    p.start = (i, now) => {
      p.a = pak.anims[Math.max(0, i)]; p.frame = 0; p.frameStart = now; p.loopEntered = now;
      p.inLoop = false; p.loopRelease = false; p.holding = false; p.hold = holdOf(pak, p.a, 0);
    };
    p.step = (now, releaseAfter) => {
      if (p.holding) return null;
      const due = p.frameStart + p.hold;
      if (now < due) return null;
      if (releaseAfter && p.inLoop && !p.loopRelease && due - p.loopEntered >= releaseAfter) p.loopRelease = true;
      const fromLoop = p.inLoop, a = p.a;
      let next = p.frame + 1, completed = false;
      if (p.frame === a.loopEnd && !p.loopRelease) next = a.loopStart;
      if (next >= a.frameCount) { completed = true; next = 0; p.loopRelease = false; }
      p.frame = next; p.frameStart = due; p.hold = holdOf(pak, a, next);
      const nowIn = next >= a.loopStart && next <= a.loopEnd;
      if (nowIn && !fromLoop) p.loopEntered = due;
      p.inLoop = nowIn;
      return { completed, fromLoop, at:due };
    };
    return p;
  }
  function mascotSprite(pak) {                          // ClawdMascot, rate group 0
    const walk = pak.find('walking'), p = player(pak);
    const m = { last:null, acting:false, stillSince:0, actIdx:0 };
    const rest = t => { p.start(walk, t); p.holding = true; m.acting = false; m.stillSince = t; };
    m.update = now => {
      if (m.last === null || now < m.last) rest(now);
      m.last = now;
      for (let guard = 0; guard < 200; guard++) {
        if (!m.acting) {
          if (now - m.stillSince < MAS.stillMs) return;
          const t = m.stillSince + MAS.stillMs, idx = pak.find(MAS.acts[m.actIdx++ % MAS.acts.length]);
          if (idx < 0) { m.stillSince = t; return; }
          p.start(idx, t); m.acting = true;
          continue;
        }
        const r = p.step(now, MAS.actLoopMs);
        if (!r) return;
        if (r.completed) rest(r.at);
      }
    };
    m.draw = (c, x, y) => {
      const a = p.a;
      for (let gy = 0; gy < MAS.gh; gy++)
        for (let gx = 0; gx < MAS.gw; gx++) {
          const v = cellOf(pak, a, p.frame, MAS.gx0 + gx - a.ox, MAS.gy0 + gy - a.oy);
          if (v) { c.fillStyle = a.pal[v]; c.fillRect(x + gx * 2, y + gy * 2, 2, 2); }
        }
    };
    return m;
  }
  const crabCells = (f, fromLoop) => (f === 1 ? (fromLoop ? 1 : 0) : (f === 4 || f === 5 || f === 8) ? 1 : 0);
  function walkerSprite(pak) {                          // ClawdWalker, rate group 0
    const idx = pak.find('crab walking'), p = player(pak);
    const w = { last:null, x:WLK.x0, dir:0, face:1, target:WLK.x0, standSince:0 };
    const reset = t => { p.start(idx, t); p.holding = true; w.x = WLK.x0; w.dir = 0; w.face = 1; w.standSince = t; };
    w.update = now => {
      if (idx < 0) return;
      if (w.last === null || now < w.last) reset(now);
      w.last = now;
      for (let guard = 0; guard < 200; guard++) {
        if (p.holding) {
          if (now - w.standSince < WLK.standMs) return;
          const t = w.standSince + WLK.standMs, far = WLK.x1 - p.a.w * 2;
          w.target = w.x - WLK.x0 < (far - WLK.x0) / 2 ? far : WLK.x0;
          if (w.target === w.x) { w.standSince = t; continue; }
          w.dir = w.face = w.target > w.x ? 1 : -1;
          p.frame = 0; p.frameStart = t; p.hold = holdOf(pak, p.a, 0);
          p.holding = false; p.loopRelease = false; p.inLoop = false;
        }
        const r = p.step(now, 0);
        if (!r) return;
        if (r.completed) { p.holding = true; w.standSince = r.at; continue; }
        if (w.dir && p.inLoop) {
          w.x += w.dir * crabCells(p.frame, r.fromLoop) * 2;
          if ((w.dir > 0 && w.x >= w.target) || (w.dir < 0 && w.x <= w.target)) { w.x = w.target; w.dir = 0; p.loopRelease = true; }
        }
      }
    };
    w.draw = c => {
      if (idx < 0) return;
      const a = p.a, top = WLK.feet - a.h * 2;
      for (let gy = 0; gy < a.h; gy++)
        for (let gx = 0; gx < a.w; gx++) {
          const v = cellOf(pak, a, p.frame, w.face < 0 ? a.w - 1 - gx : gx, gy);
          if (v) { c.fillStyle = a.pal[v]; c.fillRect(w.x + gx * 2, top + gy * 2, 2, 2); }
        }
    };
    return w;
  }
  let sprite = null;                                    // { mascot, walker } once setPak got a pack
  function setPak(b64) {
    const pak = typeof b64 === 'string' ? parsePak(b64) : null;
    if (pak && pak.find('walking') >= 0) sprite = { mascot:mascotSprite(pak), walker:walkerSprite(pak) };
    return Boolean(sprite);
  }
  const hasPak = () => Boolean(sprite);
  // Clawd in the corner window (56 x 42 at x, y, the firmware's place), or the app icon.
  function clawd(c, x, y, now) {
    if (!sprite) { mascot(c, x + 8, y + 2); return; }
    sprite.mascot.update(now);
    sprite.mascot.draw(c, x, y);
  }

  function mascot(c, x, y) {          // the app icon: orange square, white clock face
    rr(c, x, y, 34, 34, 8, C.accent);
    c.strokeStyle = '#fff'; c.lineWidth = 2.4; c.lineCap = 'round';
    c.beginPath(); c.arc(x + 17, y + 17, 9, 0, Math.PI * 2); c.stroke();
    c.beginPath(); c.moveTo(x + 17, y + 17); c.lineTo(x + 17, y + 11); c.moveTo(x + 17, y + 17); c.lineTo(x + 21, y + 19); c.stroke();
  }
  function header(c, d, now) {
    clawd(c, 6, 2, now);
    const x = Math.max(64, W / 2 + 8 - tw(c, d.clock, F.t34) / 2);
    tx(c, d.clock, x, 2, F.t34, C.text);
  }
  const SPIN = ['·', '✻', '✽', '✶', '✳', '✢'];
  function classic(c, d, now) {
    header(c, d, now);
    const card = (y, label, pct, reset) => {
      rr(c, 8, y, 224, 74, 8, C.panel);
      const pw = tw(c, label, F.s14) + 16;
      rr(c, 222 - pw, y + 7, pw, 18, 99, C.track);
      tx(c, label, 230 - pw, y + 9, F.s14, C.text);
      tx(c, `${pct}%`, 18, y + 5, F.s24, C.text);
      bar(c, 18, y + 36, 204, 12, pct, pctColor(pct));
      tx(c, `Resets in ${fmtReset(reset)}`, 18, y + 53, F.s14, C.dim);
    };
    card(44, 'Current', d.s, d.sr);
    card(124, 'Weekly', d.w, d.wr);
    const ph = Math.floor(now / 160) % SPIN.length;
    tx(c, `${SPIN[ph]} Clauding…`, W / 2, H - 30, F.m16, C.accent, 'center');
  }
  function paced(c, d, now) {
    header(c, d, now);
    const card = (y, label, pct, reset, win) => {
      rr(c, 8, y, 224, 92, 8, C.panel);
      const num = `${pct}%`;
      tx(c, num, 18, y + 6, F.s24, C.text);
      tx(c, label, 26 + tw(c, num, F.s24), y + 14, F.s14, C.dim);
      const p = pace(pct, reset, win);
      const pw = tw(c, p.label, F.s14) + 14;
      rr(c, 222 - pw, y + 9, pw, 20, 99, p.color);
      tx(c, p.label, 229 - pw, y + 12, F.s14, p.color === C.green ? C.text : '#141413');
      bar(c, 18, y + 38, 204, 12, pct, pctColor(pct));
      rr(c, 18 + Math.round(204 * p.t) - 1, y + 34, 2, 20, 1, C.text);
      tx(c, `Resets in ${fmtReset(reset)}`, 18, y + 56, F.s14, C.dim);
      const f = forecast(pct, reset, win, d.clock);
      tx(c, f.text, 18, y + 72, F.s14, f.color);
    };
    card(44, '5-hour', d.s, d.sr, WIN_S);
    card(142, 'weekly', d.w, d.wr, WIN_W);
  }
  const spanTxt = m => (m < 0 ? '---' : fmtReset(m));
  const pctI = p => Math.round(Math.max(0, p));
  const clampT = t => Math.min(1, Math.max(0, t));
  function hero(c, d, now) {
    clawd(c, 6, 2, now);
    tx(c, d.clock, 232, 2, F.t34, C.text, 'right');
    tx(c, '5-HOUR', 12, 50, F.s14, C.dim);
    const rst = `resets ${spanTxt(d.sr)}`;
    tx(c, rst, 228, 50, F.s14, C.dim, 'right');
    const num = String(pctI(d.s)), p = pace(d.s, d.sr, WIN_S);
    tx(c, num, 10, 62, F.s48, C.text);
    tx(c, '%', 14 + tw(c, num, F.s48), 90, F.s24, C.text);
    const pw = tw(c, p.label, F.s14) + 26, px = 228 - pw;
    rr(c, px, 96, pw, 20, 99, C.track);
    rr(c, px + 8, 103, 6, 6, 3, p.color);
    tx(c, p.label, px + 18, 99, F.s14, C.text);
    bar(c, 12, 132, 216, 14, d.s, pctColor(d.s));
    rr(c, 12 + Math.round(216 * p.t) - 1, 127, 2, 24, 1, C.text);
    const f = forecast(d.s, d.sr, WIN_S, d.clock);
    tx(c, f.text, 12, 156, F.s14, f.color);
    rr(c, 8, 184, 224, 48, 8, C.panel);
    tx(c, 'Weekly', 18, 192, F.s14, C.dim);
    tx(c, spanTxt(d.wr), 26 + tw(c, 'Weekly', F.s14), 192, F.s14, C.dim);
    tx(c, `${pctI(d.w)}%`, 222, 187, F.s24, C.text, 'right');
    bar(c, 18, 216, 204, 6, d.w, pctColor(d.w));
  }
  // angles in degrees, 0 = 12 o'clock, clockwise; r is the outer radius
  function arc(c, cx, cy, r, lw, a0, sweep, col) {
    if (sweep <= 0) return;
    const rad = a => (a - 90) * Math.PI / 180;
    c.strokeStyle = col; c.lineWidth = lw; c.lineCap = 'round';
    c.beginPath(); c.arc(cx, cy, r - lw / 2, rad(a0), rad(a0 + sweep)); c.stroke();
  }
  function dot(c, cx, cy, r, a, col) {
    const x = cx + r * Math.sin(a * Math.PI / 180), y = cy - r * Math.cos(a * Math.PI / 180);
    c.fillStyle = col; c.beginPath(); c.arc(x, y, 5, 0, Math.PI * 2); c.fill();
  }
  function rings(c, d, now) {
    const CX = 120, CY = 120, v = p => Math.min(100, Math.max(0, p));
    arc(c, CX, CY, 110, 12, 0, 359.9, C.track);
    arc(c, CX, CY, 110, 12, 0, 3.6 * v(d.s), pctColor(d.s));
    arc(c, CX, CY, 94, 8, 0, 359.9, C.track);
    arc(c, CX, CY, 94, 8, 0, 3.6 * v(d.w), pctColor(d.w));
    // the dots sit on the ring's centre line (outer radius minus half the stroke)
    dot(c, CX, CY, 104, 360 * pace(d.s, d.sr, WIN_S).t, C.text);
    dot(c, CX, CY, 90, 360 * pace(d.w, d.wr, WIN_W).t, C.text);
    clawd(c, 92, 36, now);
    tx(c, d.clock, CX, 80, F.t34, C.text, 'center');
    tx(c, `${pctI(d.s)}%`, CX - 8, 126, F.s24, pctColor(d.s), 'right');
    tx(c, `${pctI(d.w)}%`, CX + 8, 126, F.s24, pctColor(d.w));
    rr(c, CX - 1, 130, 2, 16, 1, C.track);
    tx(c, '5-hour', CX - 8, 152, F.s14, C.dim, 'right');
    tx(c, 'weekly', CX + 8, 152, F.s14, C.dim);
    tx(c, `resets ${spanTxt(d.sr)}`, CX, 176, F.s14, C.dim, 'center');
  }
  function gauges(c, d, now) {
    clawd(c, 6, 2, now);
    const x0 = Math.max(64, W / 2 + 8 - tw(c, d.clock, F.t34) / 2);
    tx(c, d.clock, x0, 2, F.t34, C.text);
    const card = (x, label, pct, reset) => {
      const cx = x + 54, cy = 102, v = Math.min(100, Math.max(0, pct));
      rr(c, x, 46, 108, 156, 10, C.panel);
      arc(c, cx, cy, 40, 8, 225, 270, C.track);
      arc(c, cx, cy, 40, 8, 225, 2.7 * v, pctColor(v));
      tx(c, `${pctI(pct)}%`, cx, 91, F.s24, C.text, 'center');
      tx(c, label, cx, 150, F.s14, C.text, 'center');
      tx(c, spanTxt(reset), cx, 172, F.s14, C.dim, 'center');
    };
    card(8, 'Current', d.s, d.sr);
    card(124, 'Weekly', d.w, d.wr);
    const ph = Math.floor(now / 160) % SPIN.length;
    tx(c, `${SPIN[ph]} Clauding…`, W / 2, H - 30, F.m16, C.accent, 'center');
  }
  function pixel(c, d, now) {
    tx(c, d.clock, W / 2, 4, F.m32, C.text, 'center');
    const row = (y, label, pct, reset) => {
      tx(c, label, 11, y, F.m18, C.dim);
      tx(c, `${pctI(pct)}%`, 229, y, F.m18, C.text, 'right');
      const lit = Math.round(Math.min(100, Math.max(0, pct)) / 5), col = pctColor(pct);
      for (let i = 0; i < 20; i++) rr(c, 11 + i * 11, y + 24, 9, 18, 0, i < lit ? col : C.track);
      tx(c, `rst ${spanTxt(reset).replace(/ /g, '')}`, 11, y + 48, F.m18, C.dim);
    };
    row(52, '5H', d.s, d.sr);
    row(124, '7D', d.w, d.wr);
    for (let x = 4; x < W - 4; x += 4) { c.fillStyle = C.track; c.fillRect(x, 234, 2, 2); }
    if (sprite) { sprite.walker.update(now); sprite.walker.draw(c); return; }
    // Without the pack the icon strolls to the right edge and back, faster the higher the usage
    const span = W - 8 - 34, speed = 0.02 + 0.06 * clampT(d.s / 100);
    const u = (now * speed) % (2 * span), x = 4 + (u < span ? u : 2 * span - u);
    mascot(c, Math.round(x), 234 - 34 - (Math.floor(now / 160) % 2));
  }
  const LAYOUTS = [
    { id:0, name:'Hiện tại', desc:'Hai thẻ Current / Weekly và dòng trạng thái.', draw:classic },
    { id:1, name:'Nhịp & dự báo', desc:'Cho biết đang dùng nhanh hay chậm, và lúc nào hết.', draw:paced },
    { id:2, name:'Tiêu điểm', desc:'Một số lớn cho phiên 5 giờ, tuần nằm gọn bên dưới.', draw:hero },
    { id:3, name:'Vòng đôi', desc:'Hai vòng lồng nhau, đồng hồ nằm giữa.', draw:rings },
    { id:4, name:'Đồng hồ đôi', desc:'Hai mặt đồng hồ tròn cạnh nhau.', draw:gauges },
    { id:5, name:'Pixel retro', desc:'Thanh 20 ô kiểu game, Clawd đi dạo bên dưới.', draw:pixel },
  ];
  function waiting(c, now) {
    clawd(c, 92, 66, now);
    tx(c, 'Ch\u01b0a c\u00f3 d\u1eef li\u1ec7u usage', W / 2, 130, F.s14, C.dim, 'center');
    tx(c, 'App s\u1ebd g\u1eedi sau v\u00e0i gi\u00e2y', W / 2, 152, F.s14, C.dim, 'center');
  }
  function paint(canvas, layout, d, now) {
    const c = canvas.getContext('2d');
    c.setTransform(1, 0, 0, 1, 0, 0);
    c.fillStyle = C.bg; c.fillRect(0, 0, W, H);
    if (!d) { waiting(c, now); return; }
    (LAYOUTS[layout] || LAYOUTS[0]).draw(c, d, now);
  }
  return { LAYOUTS, paint, fromPayload, setPak, hasPak };
})();
