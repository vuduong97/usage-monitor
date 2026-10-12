// Canvas mock of the clock's usage screens (240x240), sample data only.
// Mirrors the firmware layouts in smalltv/src/features/clawd/ClawdUi.cpp:
// 0 = classic (two cards + status line), 1 = pace (taller cards, pace pill,
// time marker on the bar, forecast line). System fonts stand in for the
// clock's own; the mascot is the app's clock icon.
'use strict';
const PV = (() => {
  const C = { bg:'#000', panel:'#1f1f1e', text:'#faf9f5', dim:'#b0aea5', accent:'#d97757',
              green:'#788c5d', amber:'#d97757', red:'#c0392b', track:'#2a2a28' };
  const SANS = '-apple-system, BlinkMacSystemFont, "Segoe UI", system-ui, sans-serif';
  const F = { s14:`14px ${SANS}`, s24:`500 24px ${SANS}`, t34:'34px "New York", "Iowan Old Style", Georgia, serif',
              m16:'16px ui-monospace, Menlo, Consolas, monospace' };
  const W = 240, H = 240, WIN_S = 300, WIN_W = 10080;
  const SAMPLES = {
    normal: { s:42, sr:134, w:67, wr:3960, clock:'14:05' },
    hot:    { s:88, sr:38,  w:93, wr:540,  clock:'14:05' },
    fresh:  { s:4,  sr:291, w:18, wr:8200, clock:'09:12' },
  };
  const pctColor = p => p >= 80 ? C.red : p >= 50 ? C.amber : C.green;
  function fmtReset(m) {
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
  function mascot(c, x, y) {          // the app icon: orange square, white clock face
    rr(c, x, y, 34, 34, 8, C.accent);
    c.strokeStyle = '#fff'; c.lineWidth = 2.4; c.lineCap = 'round';
    c.beginPath(); c.arc(x + 17, y + 17, 9, 0, Math.PI * 2); c.stroke();
    c.beginPath(); c.moveTo(x + 17, y + 17); c.lineTo(x + 17, y + 11); c.moveTo(x + 17, y + 17); c.lineTo(x + 21, y + 19); c.stroke();
  }
  function header(c, d) {
    mascot(c, 14, 4);
    const x = Math.max(64, W / 2 + 8 - tw(c, d.clock, F.t34) / 2);
    tx(c, d.clock, x, 2, F.t34, C.text);
  }
  const SPIN = ['·', '✻', '✽', '✶', '✳', '✢'];
  function classic(c, d, now) {
    header(c, d);
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
  function paced(c, d) {
    header(c, d);
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
  const LAYOUTS = [
    { id:0, name:'Hiện tại', desc:'Hai thẻ Current / Weekly và dòng trạng thái.', draw:classic },
    { id:1, name:'Nhịp & dự báo', desc:'Cho biết đang dùng nhanh hay chậm, và lúc nào hết.', draw:paced },
  ];
  function paint(canvas, layout, sample, now) {
    const c = canvas.getContext('2d');
    c.setTransform(1, 0, 0, 1, 0, 0);
    c.fillStyle = C.bg; c.fillRect(0, 0, W, H);
    (LAYOUTS[layout] || LAYOUTS[0]).draw(c, SAMPLES[sample] || SAMPLES.normal, now);
  }
  return { LAYOUTS, paint };
})();
