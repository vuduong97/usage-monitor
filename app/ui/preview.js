// Approximate canvas drawing of the clock's usage screens (240x240), from the real usage
// payload. The settings window shows the clock's own render when the app has it
// (preview.py); this is the fallback when it does not, so it is labelled "gần đúng".
// Mirrors the classic screen in clock/firmware/src/features/clawd/ClawdUi.cpp
// (two cards + status line). System fonts stand in for the clock's own. Clawd is
// the real sprite when the app could read /clawd.pak off the user's clock
// (settings_api.clawd_pak, setPak below), played like ClawdMascot in the firmware's
// ClawdAnim.cpp; without it, the app's clock icon. A style pack has no
// approximation: only the renderer draws one, so without it the page shows a
// labelled card in the pack's light or dark tone (placeholder below).
'use strict';
const PV = (() => {
  const C = { bg:'#000', panel:'#1f1f1e', text:'#faf9f5', dim:'#b0aea5', accent:'#d97757',
              green:'#788c5d', amber:'#d97757', red:'#c0392b', track:'#2a2a28' };
  const SANS = '-apple-system, BlinkMacSystemFont, "Segoe UI", system-ui, sans-serif';
  const F = { s14:`14px ${SANS}`, s24:`500 24px ${SANS}`, t34:'34px "New York", "Iowan Old Style", Georgia, serif',
              m16:'16px ui-monospace, Menlo, Consolas, monospace' };
  const W = 240, H = 240;
  // The usage payload the app sends the clock ({s, sr, w, wr, t, tf, ok}) as the drawers read it.
  // null when there is nothing to show (no payload yet, or the "no data" beat);
  // {login: true, clock} for {"ok": false, "why": "login"} (the clock says "Login expired").
  function clockText(p) {
    if (typeof p.t !== 'number') return '';       // local wall-clock seconds: read the UTC fields
    const d = new Date(p.t * 1000), h = d.getUTCHours(), m = String(d.getUTCMinutes()).padStart(2, '0');
    return p.tf === 12 ? `${h % 12 || 12}:${m} ${h < 12 ? 'AM' : 'PM'}` : `${String(h).padStart(2, '0')}:${m}`;
  }
  function fromPayload(p) {
    if (p && p.ok === false && p.why === 'login' && !p.src) return { login: true, clock: clockText(p) };
    if (!p || p.ok === false || typeof p.s !== 'number') return null;
    const n = (v, dflt) => (typeof v === 'number' && isFinite(v) ? v : dflt);
    return { s: n(p.s, 0), sr: Math.round(n(p.sr, -1)), w: n(p.w, 0), wr: Math.round(n(p.wr, -1)), clock: clockText(p) };
  }
  const pctColor = p => p >= 80 ? C.red : p >= 50 ? C.amber : C.green;
  function fmtReset(m) {
    if (m < 0) return '---';
    if (m < 60) return `${m}m`;
    if (m < 1440) return `${Math.floor(m / 60)}h ${m % 60}m`;
    return `${Math.floor(m / 1440)}d ${Math.floor((m % 1440) / 60)}h`;
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
  // Format: clock/firmware/tools/clawd_assets.js; reader: ClawdPack.cpp. The player
  // follows ClawdAnim.cpp (ClawdPlayer, ClawdMascot at usage-rate group 0): when the
  // firmware's timings or acts change, change them here too.
  const MAS = { gx0:15, gy0:16, gw:28, gh:21, actLoopMs:2500, stillMs:4000, acts:['pointing', 'waving'] };
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
  let sprite = null;                                    // { mascot } once setPak got a pack
  function setPak(b64) {
    const pak = typeof b64 === 'string' ? parsePak(b64) : null;
    if (pak && pak.find('walking') >= 0) sprite = { mascot:mascotSprite(pak) };
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
  // A style pack without the renderer: its name on a card in the pack's tone.
  function placeholder(c, name, light) {
    c.fillStyle = light ? '#efe9dc' : '#15181d'; c.fillRect(0, 0, W, H);
    const fg = light ? '#2b2a27' : C.text, dim = light ? '#77736a' : C.dim;
    tx(c, name, W / 2, 96, F.s24, fg, 'center');
    tx(c, 'Xem tr\u01b0\u1edbc c\u1ea7n b\u1ea3n app', W / 2, 132, F.s14, dim, 'center');
    tx(c, 'c\u00f3 k\u00e8m firmware', W / 2, 152, F.s14, dim, 'center');
  }
  // ClawdUi.cpp's idle view with why = login: time, corner Clawd, the status line (any style).
  function loginExpired(c, d, now) {
    header(c, d, now);                          // (the clock's cloud animation is not drawn here)
    const ph = Math.floor(now / 160) % SPIN.length;
    tx(c, `${SPIN[ph]} Login expired`, W / 2, H - 30, F.m16, C.accent, 'center');
  }
  function waiting(c, now) {
    clawd(c, 92, 66, now);
    tx(c, 'Ch\u01b0a c\u00f3 d\u1eef li\u1ec7u usage', W / 2, 130, F.s14, C.dim, 'center');
    tx(c, 'App s\u1ebd g\u1eedi sau v\u00e0i gi\u00e2y', W / 2, 152, F.s14, C.dim, 'center');
  }
  // `pack`: null for the classic screen, else {name, light} of a style pack.
  function paint(canvas, pack, d, now) {
    const c = canvas.getContext('2d');
    c.setTransform(1, 0, 0, 1, 0, 0);
    c.fillStyle = C.bg; c.fillRect(0, 0, W, H);
    if (d && d.login) { loginExpired(c, d, now); return; }   // the clock leaves a style pack for it
    if (pack) { placeholder(c, pack.name, pack.light); return; }
    if (!d) { waiting(c, now); return; }
    classic(c, d, now);
  }
  // drawClawd(ctx, x, y, now): Clawd in his 56 x 42 corner box, as on the clock (the menu-bar panel)
  return { paint, fromPayload, setPak, hasPak, drawClawd: clawd };
})();
