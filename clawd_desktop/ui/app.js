// The settings window. Talks to Python through window.pywebview.api (settings_api.py).
'use strict';
const $ = id => document.getElementById(id);
const el = (tag, cls, text) => { const e = document.createElement(tag); if (cls) e.className = cls; if (text != null) e.textContent = text; return e; };
const svgIcon = id => { const s = document.createElementNS('http://www.w3.org/2000/svg', 'svg'); s.setAttribute('class', 'i');
  const u = document.createElementNS('http://www.w3.org/2000/svg', 'use'); u.setAttribute('href', '#' + id); s.append(u); return s; };
// Deep copy of plain data (the settings are JSON from Python); no parsing, so nothing to throw.
function clone(o) {
  if (Array.isArray(o)) return o.map(clone);
  if (o && typeof o === 'object') return Object.fromEntries(Object.entries(o).map(([k, v]) => [k, clone(v)]));
  return o;
}

// IANA -> POSIX TZ (same table as the clock's web page, plus Vietnam).
const TZMAP = {
  'UTC':'UTC0', 'Asia/Ho_Chi_Minh':'<+07>-7', 'Asia/Bangkok':'<+07>-7', 'Asia/Jakarta':'WIB-7',
  'Asia/Shanghai':'CST-8', 'Asia/Hong_Kong':'HKT-8', 'Asia/Singapore':'<+08>-8', 'Asia/Taipei':'CST-8',
  'Asia/Tokyo':'JST-9', 'Asia/Seoul':'KST-9', 'Asia/Dubai':'<+04>-4', 'Asia/Karachi':'PKT-5', 'Asia/Kolkata':'IST-5:30',
  'Asia/Dhaka':'<+06>-6', 'Australia/Perth':'AWST-8', 'Australia/Sydney':'AEST-10AEDT,M10.1.0,M4.1.0/3',
  'Australia/Adelaide':'ACST-9:30ACDT,M10.1.0,M4.1.0/3', 'Australia/Brisbane':'AEST-10',
  'Pacific/Auckland':'NZST-12NZDT,M9.5.0,M4.1.0/3', 'Pacific/Honolulu':'HST10',
  'Europe/London':'GMT0BST,M3.5.0/1,M10.5.0', 'Europe/Dublin':'GMT0IST,M3.5.0/1,M10.5.0', 'Europe/Lisbon':'WET0WEST,M3.5.0/1,M10.5.0',
  'Europe/Paris':'CET-1CEST,M3.5.0,M10.5.0/3', 'Europe/Berlin':'CET-1CEST,M3.5.0,M10.5.0/3', 'Europe/Madrid':'CET-1CEST,M3.5.0,M10.5.0/3',
  'Europe/Rome':'CET-1CEST,M3.5.0,M10.5.0/3', 'Europe/Amsterdam':'CET-1CEST,M3.5.0,M10.5.0/3', 'Europe/Brussels':'CET-1CEST,M3.5.0,M10.5.0/3',
  'Europe/Zurich':'CET-1CEST,M3.5.0,M10.5.0/3', 'Europe/Vienna':'CET-1CEST,M3.5.0,M10.5.0/3', 'Europe/Warsaw':'CET-1CEST,M3.5.0,M10.5.0/3',
  'Europe/Prague':'CET-1CEST,M3.5.0,M10.5.0/3', 'Europe/Stockholm':'CET-1CEST,M3.5.0,M10.5.0/3', 'Europe/Oslo':'CET-1CEST,M3.5.0,M10.5.0/3',
  'Europe/Copenhagen':'CET-1CEST,M3.5.0,M10.5.0/3', 'Europe/Athens':'EET-2EEST,M3.5.0/3,M10.5.0/4',
  'Europe/Helsinki':'EET-2EEST,M3.5.0/3,M10.5.0/4', 'Europe/Bucharest':'EET-2EEST,M3.5.0/3,M10.5.0/4',
  'Europe/Kyiv':'EET-2EEST,M3.5.0/3,M10.5.0/4', 'Europe/Istanbul':'<+03>-3', 'Europe/Moscow':'MSK-3',
  'America/New_York':'EST5EDT,M3.2.0,M11.1.0', 'America/Toronto':'EST5EDT,M3.2.0,M11.1.0', 'America/Chicago':'CST6CDT,M3.2.0,M11.1.0',
  'America/Denver':'MST7MDT,M3.2.0,M11.1.0', 'America/Phoenix':'MST7', 'America/Los_Angeles':'PST8PDT,M3.2.0,M11.1.0',
  'America/Anchorage':'AKST9AKDT,M3.2.0,M11.1.0', 'America/Sao_Paulo':'<-03>3', 'America/Mexico_City':'CST6',
  'America/Bogota':'<-05>5', 'America/Argentina/Buenos_Aires':'<-03>3',
};
const STEP_TEXT = { check:'Đang kiểm tra đồng hồ…', pause:'Tạm dừng gửi dữ liệu tới đồng hồ…',
  upload:'Đang gửi firmware (khoảng 30 giây)…', reboot:'Đồng hồ đang khởi động lại…', done:'Xong.' };
const STEP_ORDER = ['pause', 'upload', 'reboot', 'done'];

let api = null;
let S = null;          // last state from Python
let orig = null;       // the clock's settings, in the shape we save
let edit = null;       // what the user has now
let tab = 'display';
let live = { exact: false, frames: {}, payload: null, sentAt: null, now: 0 };   // api.usage_preview()
let busy = false;      // a request or the firmware update is running
let style = 'classic'; // the usage screen style picked: 'classic' or a bundled pack id (/api/style)
// Hover text for each style ("Hiện tại" is the classic screen; names on the clock stay English).
const STYLE_DESC = {
  classic:'Hai thẻ Current / Weekly và dòng trạng thái.',
  gameboy:'Bốn tông xanh lá kiểu máy chơi game cầm tay.',
  synth:'Mặt trời neon, lưới chạy về chân trời.',
  paper:'Trang báo giấy, chữ có chân, tít in đậm.',
  blueprint:'Bản vẽ kỹ thuật, nét trắng trên nền xanh.',
  pastel:'Màu kẹo nhạt, Clawd nhún nhảy.',
  onebit:'Đen trắng 1-bit, chữ gõ từng ký tự.',
};

// ---------- helpers ----------
function view(cfg) {   // the editable part of /api/config (settings_api._clock_view)
  const u = cfg.usage || {}, k = cfg.clock || {};
  return {
    hostname: cfg.hostname || '', brightness: cfg.brightness ?? 50,
    rotation: cfg.rotation ?? 0,
    wifi: (cfg.wifi || []).filter(r => r && r.ssid).map(r => ({ ssid: r.ssid })),
    usage: { screen: u.screen ?? 0, anim: u.anim ?? -1, layout: u.layout ?? 0 },
    clock: { tz: k.tz || '', tzPosix: k.tzPosix || 'UTC0', nightEnabled: !!k.nightEnabled,
             nightStart: k.nightStart || '22:00', nightEnd: k.nightEnd || '07:00', nightLevel: k.nightLevel ?? 0 },
  };
}
// What the window shows for a clock: the "Clawd" main screen (2) is gone from the page, so a
// clock still set to it reads as Auto; so do the old usage layouts (1-5, replaced by style
// packs) read as classic. `orig` keeps the real values, so each is one unsaved change and
// Save writes 0.
function mkEdit(o) {
  const e = clone(o);
  if (e.usage.screen === 2) e.usage.screen = 0;
  e.usage.layout = 0;
  return e;
}
function diff(a, b) {  // same rule as clockapi.config_diff
  const out = {};
  for (const k of Object.keys(b)) {
    const x = a[k], y = b[k];
    if (y && typeof y === 'object' && !Array.isArray(y) && x && typeof x === 'object' && !Array.isArray(x)) {
      const d = diff(x, y); if (Object.keys(d).length) out[k] = d;
    } else if (JSON.stringify(x) !== JSON.stringify(y)) out[k] = y;
  }
  return out;
}
function leaves(o) { return Object.values(o).reduce((n, v) => n + (v && typeof v === 'object' && !Array.isArray(v) ? leaves(v) : 1), 0); }
function wifiForSave(rows) { return rows.map(r => r.pass ? { ssid: r.ssid, pass: r.pass } : { ssid: r.ssid }); }
function editForSave() { const e = clone(edit); e.wifi = wifiForSave(edit.wifi); return e; }
function pending() { return orig ? diff(orig, editForSave()) : {}; }
function activeStyle() { return (S && S.fw && S.fw.styles && S.fw.styles.active) || 'classic'; }
function styleChanged() { return Boolean(orig) && style !== activeStyle(); }
function styles() {    // the gallery: the classic screen, then the packs the app carries
  const packs = (S && S.fw && S.fw.styles && S.fw.styles.packs) || [];
  return [{ id:'classic', name:'Hiện tại', light:false }, ...packs];
}
function packOf(id) { return id === 'classic' ? null : styles().find(p => p.id === id) || null; }
function msg(text, kind = '') { const m = $('msg'); m.textContent = text; m.className = 'msg' + (kind ? ' ' + kind : ''); }
function setSeg(id, v) { $(id).querySelectorAll('button').forEach(b => b.setAttribute('aria-checked', String(b.dataset.v === String(v)))); }
function setSw(id, on) { $(id).setAttribute('aria-checked', String(!!on)); }
function styleState(id) {   // 'ready' | 'update' (needs the bundled firmware) | 'unavailable'
  if (id === 'classic') return 'ready';
  return (S && S.fw && S.fw.styles && S.fw.styles.state) || 'unavailable';
}
function kb(n) { return (n / 1024).toFixed(1) + ' KB'; }
function uptime(s) { if (s < 3600) return Math.floor(s / 60) + ' phút'; if (s < 86400) return Math.floor(s / 3600) + ' giờ'; return Math.floor(s / 86400) + ' ngày'; }

// ---------- tabs ----------
function showTab(t) {
  tab = t;
  history.replaceState(null, '', '#' + t);   // a reload (live reload in `make dev`) reopens this tab
  document.querySelectorAll('#tabs button').forEach(b => b.setAttribute('aria-selected', String(b.dataset.tab === t)));
  document.querySelectorAll('[data-pane]').forEach(p => p.classList.toggle('hide', p.dataset.pane !== t));
  if (t === 'usage') refreshLive();
}
$('tabs').addEventListener('click', e => { const b = e.target.closest('button'); if (b) showTab(b.dataset.tab); });
$('tabs').addEventListener('keydown', e => {
  const tabs = [...document.querySelectorAll('#tabs button')], i = tabs.findIndex(b => b.dataset.tab === tab);
  const j = e.key === 'ArrowRight' ? (i + 1) % tabs.length : e.key === 'ArrowLeft' ? (i + tabs.length - 1) % tabs.length : -1;
  if (j >= 0) { showTab(tabs[j].dataset.tab); tabs[j].focus(); }
});

// ---------- preview ----------
let lastPaint = 0;
const reduce = matchMedia('(prefers-reduced-motion: reduce)').matches;
// The page shows the frame the clock's own renderer drew (Python: usage_preview). Without that
// renderer (public build, or one built from another firmware) it draws its own approximation
// of the same screen, from the same real usage.
function ago(sec) {
  if (sec < 90) return 'vừa xong';
  if (sec < 3600) return Math.round(sec / 60) + ' phút trước';
  if (sec < 86400) return Math.round(sec / 3600) + ' giờ trước';
  return Math.round(sec / 86400) + ' ngày trước';
}
function setShot(img, url) { if (img.dataset.url !== url) { img.dataset.url = url; img.src = url; } }
function paintPreview(now) {
  const cv = $('big'), img = $('shot'), t = reduce ? 0 : now;
  const url = live.exact ? live.frames[style] : null;
  const data = PV.fromPayload(live.payload);
  img.classList.toggle('hide', !url);
  cv.classList.toggle('hide', Boolean(url));
  if (url) setShot(img, url); else PV.paint(cv, packOf(style), data, t);
  const br = edit ? edit.brightness : 70;
  for (const e of [cv, img]) {
    e.style.filter = `brightness(${(0.25 + br / 100 * 0.85).toFixed(2)})`;
    e.style.transform = `rotate(${(edit ? edit.rotation : 0) * 90}deg)`;
  }
  const name = (packOf(style) || { name:'Hiện tại' }).name;
  const how = url ? 'vẽ bằng firmware' : 'gần đúng';
  const when = live.sentAt ? `Usage ${ago(Math.max(0, live.now - live.sentAt))}` : 'chưa có usage thật';
  $('device').title = `${name} · ${how} · ${when}`;   // hover only: the pane shows just the clock
  [...$('gal').children].forEach(b => {             // one button per style: its canvas and its image
    const id = b.dataset.id, u = live.exact ? live.frames[id] : null;
    const c = b.querySelector('canvas.thumb'), im = b.querySelector('img.thumb');
    im.classList.toggle('hide', !u);
    c.classList.toggle('hide', Boolean(u));
    if (u) setShot(im, u); else if (tab === 'usage') PV.paint(c, packOf(id), data, t);
  });
}
async function refreshLive() {
  if (!api || document.hidden) return;
  try {
    const r = await api.usage_preview(null, !reduce);   // animated loops unless reduced motion
    if (r && r.ok) live = r;
    // The approximate canvas draws the real Clawd from the clock's own pack, once Python has it
    // (it reads the clock at most every few minutes, so asking on every refresh is cheap).
    if (!live.exact && !PV.hasPak()) {
      const p = await api.clawd_pak();
      if (p && p.ok) PV.setPak(p.pak);
    }
  } catch (err) { console.warn('usage_preview failed; keeping the last frame', err); }
  paintPreview(performance.now());
}
setInterval(refreshLive, 20000);
// While the clock does not answer, ask again now and then: the tray may have found it at a
// new address meanwhile, and state() then follows it (settings_api._follow_store).
let relooking = false;
setInterval(async () => {
  if (!api || busy || relooking || document.hidden || !S || S.ok || S.code !== 'offline') return;
  relooking = true;
  try { const r = await api.state(); if (r && (r.ok || r.code !== 'offline')) apply(r); else if (r) { S = r; renderConn(); } }
  finally { relooking = false; }
}, 20000);
function loop(now) {
  if (!document.hidden && now - lastPaint > (reduce ? 1000 : 80)) { lastPaint = now; paintPreview(now); }
  requestAnimationFrame(loop);
}

// ---------- render from state ----------
function renderAll() {
  renderConn(); renderDisplay(); renderGallery(); renderWifi(); renderUpdate(); renderApp(); renderBar();
}
function renderConn() {
  const dot = $('connDot'), t = $('connText');
  if (!S) return;
  const host = (S.config && S.config.hostname) || '';
  const addr = S.app ? S.app.clock.replace(/^https?:\/\//, '') : '';
  if (S.ok) { dot.className = 'dot ok'; t.textContent = `${host || 'đồng hồ'} · ${addr}`; }
  else if (S.code === 'auth') { dot.className = 'dot busy'; t.textContent = `${addr} · cần mật khẩu`; }
  else if (S.code === 'noclock') { dot.className = 'dot bad'; t.textContent = 'Chưa có đồng hồ'; }
  else { dot.className = 'dot bad'; t.textContent = `${addr} · không trả lời`; }
}
function renderDisplay() {
  if (!edit) return;
  $('brightness').value = edit.brightness; $('brightVal').textContent = edit.brightness + '%';
  setSeg('rotation', edit.rotation); setSeg('screen', edit.usage.screen);
  const tz = $('tz'); tz.replaceChildren();
  const zones = Object.keys(TZMAP).sort((a, b) => a === 'UTC' ? -1 : b === 'UTC' ? 1 : a.localeCompare(b));
  if (edit.clock.tz && !(edit.clock.tz in TZMAP)) zones.unshift(edit.clock.tz);
  zones.forEach(z => { const o = el('option', null, z === 'Asia/Ho_Chi_Minh' ? 'Asia/Ho_Chi_Minh (Việt Nam)' : z); o.value = z; tz.append(o); });
  tz.value = edit.clock.tz || 'UTC';
  setSw('nightEnabled', edit.clock.nightEnabled);
  $('nightRow').classList.toggle('hide', !edit.clock.nightEnabled);
  $('nightStart').value = edit.clock.nightStart; $('nightEnd').value = edit.clock.nightEnd;
  $('nightLevel').value = edit.clock.nightLevel; $('nightVal').textContent = edit.clock.nightLevel + '%';
}
function renderGallery() {
  const g = $('gal'), items = styles();
  if (g.children.length !== items.length) {          // the packs come with the first state
    g.replaceChildren();
    items.forEach(L => {
      const b = el('button'); b.setAttribute('role', 'radio'); b.dataset.id = L.id;
      b.title = STYLE_DESC[L.id] || '';                // hover only, like the preview's details
      const cv = el('canvas', 'thumb'); cv.width = 240; cv.height = 240;
      const im = el('img', 'thumb hide'); im.width = 240; im.height = 240; im.alt = '';
      b.append(cv, im, el('span', 'n', L.name), el('span', 'chip'));
      g.append(b);
    });
  }
  [...g.children].forEach(b => {
    const id = b.dataset.id, st = styleState(id), chip = b.querySelector('.chip');
    b.setAttribute('aria-checked', String(Boolean(edit) && style === id));
    b.setAttribute('aria-disabled', String(st === 'unavailable'));
    // only flag packs that need something; the selected frame already shows the choice
    chip.className = 'chip ' + (st === 'ready' ? 'hide' : st === 'update' ? 'fw' : 'no');
    chip.textContent = st === 'ready' ? '' : st === 'update' ? 'Cần cập nhật firmware' : 'Không hỗ trợ';
  });
  const need = edit && styleState(style) === 'update';
  const ent = live.payload && live.payload.acct === 'ent' && style !== 'classic';
  $('galNote').textContent = need ? 'Kiểu này cần firmware mới. Bấm “Cập nhật & áp dụng”: app cài firmware đi kèm (khoảng 1 phút), rồi bật kiểu này.'
    : ent ? 'Tài khoản Enterprise không có giới hạn 5 giờ / tuần nên đồng hồ vẫn hiện kiểu “Hiện tại”.' : '';
}
function renderWifi() {
  if (!edit) return;
  const box = $('nets'); box.replaceChildren();
  const saved = new Set(orig ? orig.wifi.map(r => r.ssid) : []);
  edit.wifi.forEach((r, i) => {
    const row = el('div', 'net');
    const name = el('div', 'ss', r.ssid);
    name.append(el('small', null, saved.has(r.ssid) ? (r.pass ? 'mật khẩu mới' : 'đã lưu mật khẩu') : 'mạng mới'));
    const pw = el('input'); pw.type = 'password'; pw.autocomplete = 'off'; pw.value = r.pass || '';
    pw.placeholder = saved.has(r.ssid) ? 'giữ mật khẩu cũ' : 'mật khẩu (≥ 8 ký tự)';
    pw.setAttribute('aria-label', `Mật khẩu ${r.ssid}`);
    pw.addEventListener('input', () => { r.pass = pw.value; renderBar(); });
    const acts = el('div', 'acts');
    const up = el('button', 'ib'); up.title = 'Ưu tiên lên trên'; up.setAttribute('aria-label', `Đưa ${r.ssid} lên trên`); up.append(svgIcon('up')); up.disabled = i === 0;
    up.addEventListener('click', () => { [edit.wifi[i - 1], edit.wifi[i]] = [edit.wifi[i], edit.wifi[i - 1]]; renderWifi(); renderBar(); });
    const rm = el('button', 'ib'); rm.title = 'Xoá mạng này'; rm.setAttribute('aria-label', `Xoá ${r.ssid}`); rm.append(svgIcon('x')); rm.disabled = edit.wifi.length <= 1;
    rm.addEventListener('click', () => { edit.wifi.splice(i, 1); renderWifi(); renderBar(); });
    acts.append(up, rm);
    row.append(el('span', 'pri', String(i + 1)), name, pw, acts);
    box.append(row);
  });
  $('hostname').value = edit.hostname; $('hostHint').textContent = edit.hostname || 'clawd';
}
function renderUpdate() {
  const kv = $('fwKv'); kv.replaceChildren();
  const h = (S && S.heap) || {}, fw = S && S.fw;
  const add = (k, v) => kv.append(el('span', null, k), el('span', null, v));
  add('Đồng hồ đang chạy', h.version ? `${h.version} (bản #${h.fwSeq || 0}) · bật ${uptime(h.uptime || 0)}` : '—');
  if (h.version) add('Bộ nhớ', `heap ${kb(h.heap || 0)} · loop ${h.loopMaxMs ?? '?'} ms`);
  add('Đi kèm app', fw && fw.bundle ? `bản #${fw.bundle.seq}${fw.bundle.commit ? ' · ' + fw.bundle.commit : ''} · ${kb(fw.bundle.size)}` : 'không có');
  const st = fw ? fw.update : 'none';
  $('fwText').textContent = {
    available: 'Có bản mới hơn đi kèm app' + (fw && fw.styles && fw.styles.state === 'update' ? ': thêm các kiểu màn hình usage mới' : '') + '. Cài mất khoảng 1 phút, cài đặt trên đồng hồ được giữ nguyên.',
    current: 'Đồng hồ đang chạy bản mới nhất.',
    none: 'Bản app này không kèm firmware.',
    incompatible: 'Đồng hồ này không chạy firmware Clawd nên app không cập nhật được.',
  }[st] || '';
  $('fwBtn').disabled = busy || st !== 'available' || !S || !S.ok;
}
function renderApp() {
  const a = S && S.app; if (!a) return;
  setSw('autostart', a.autostart); $('autostart').disabled = !a.autostartSupported;
  setSw('hooks', a.hooks);
  $('clockAddr').textContent = a.clock ? a.clock.replace(/^https?:\/\//, '') : 'chưa có';
  $('manualHost').placeholder = a.manual || '192.168.1.48';
  $('appVer').textContent = `Clawd ${a.version}`;
}
function renderBar() {
  const save = $('saveBtn'), undo = $('undoBtn');
  if (!S || !S.ok) {
    undo.disabled = true;
    save.textContent = 'Thử lại'; save.disabled = busy || S?.code === 'noclock';
    return;
  }
  const d = pending(), n = leaves(d) + (styleChanged() ? 1 : 0);
  const needFw = edit && styleChanged() && styleState(style) === 'update';
  undo.disabled = busy || n === 0;
  save.disabled = busy || n === 0;
  save.textContent = needFw ? 'Cập nhật & áp dụng' : 'Lưu vào đồng hồ';
  if (busy) return;
  if (n === 0) msg('Không có thay đổi chưa lưu.');
  else msg(`${n} thay đổi chưa lưu` + (d.wifi || d.hostname ? ' · đồng hồ sẽ khởi động lại' : ''));
}

// ---------- inputs ----------
function on(id, ev, fn) { $(id).addEventListener(ev, e => { if (edit) { fn(e); renderBar(); } }); }
on('brightness', 'input', e => { edit.brightness = +e.target.value; $('brightVal').textContent = edit.brightness + '%'; paintPreview(performance.now()); });
on('rotation', 'click', e => { const b = e.target.closest('button'); if (b) { edit.rotation = +b.dataset.v; setSeg('rotation', edit.rotation); paintPreview(performance.now()); } });
on('screen', 'click', e => { const b = e.target.closest('button'); if (b) { edit.usage.screen = +b.dataset.v; setSeg('screen', edit.usage.screen); } });
on('tz', 'change', e => { const z = e.target.value; edit.clock.tz = z === 'UTC' ? '' : z;
  edit.clock.tzPosix = TZMAP[z] || (S.config.clock && S.config.clock.tz === z ? S.config.clock.tzPosix : 'UTC0'); });
on('nightEnabled', 'click', () => { edit.clock.nightEnabled = !edit.clock.nightEnabled; setSw('nightEnabled', edit.clock.nightEnabled); $('nightRow').classList.toggle('hide', !edit.clock.nightEnabled); });
on('nightStart', 'change', e => { if (e.target.value) edit.clock.nightStart = e.target.value; });
on('nightEnd', 'change', e => { if (e.target.value) edit.clock.nightEnd = e.target.value; });
on('nightLevel', 'input', e => { edit.clock.nightLevel = +e.target.value; $('nightVal').textContent = edit.clock.nightLevel + '%'; });
on('hostname', 'input', e => { edit.hostname = e.target.value.trim().toLowerCase(); $('hostHint').textContent = edit.hostname || 'clawd'; });
$('gal').addEventListener('click', e => {
  const b = e.target.closest('button'); if (!b || !edit || b.getAttribute('aria-disabled') === 'true') return;
  style = b.dataset.id; renderGallery(); renderBar(); paintPreview(performance.now());
});
$('gal').addEventListener('keydown', e => {
  if (!edit || !['ArrowRight', 'ArrowLeft', 'ArrowDown', 'ArrowUp'].includes(e.key)) return;
  const ids = styles().map(L => L.id), n = ids.length, step = e.key === 'ArrowRight' || e.key === 'ArrowDown' ? 1 : n - 1;
  let i = Math.max(0, ids.indexOf(style));
  for (let k = 0; k < n; k++) { i = (i + step) % n; if (styleState(ids[i]) !== 'unavailable') break; }
  style = ids[i]; renderGallery(); renderBar(); paintPreview(performance.now()); $('gal').children[i].focus(); e.preventDefault();
});

// ---------- WiFi scan ----------
$('scanBtn').addEventListener('click', async () => {
  $('scanBtn').disabled = true; $('scanNote').textContent = 'Đồng hồ đang dò mạng (vài giây)…'; $('scanList').replaceChildren();
  const r = await api.scan();
  $('scanBtn').disabled = false;
  if (!r.ok) { $('scanNote').textContent = r.error; return; }
  $('scanNote').textContent = r.networks.length ? 'Bấm vào một mạng để thêm.' : 'Không thấy mạng nào.';
  r.networks.forEach(n => {
    const b = el('button'); b.append(el('span', null, n.ssid), el('small', null, `${n.enc ? 'có mật khẩu · ' : 'mạng mở · '}${n.rssi} dBm`));
    b.addEventListener('click', () => {
      if (!edit) return;
      let i = edit.wifi.findIndex(r => r.ssid === n.ssid);
      if (i < 0) {
        if (edit.wifi.length >= 4) { msg('Đồng hồ nhớ tối đa 4 mạng: xoá bớt một mạng trước.', 'err'); return; }
        edit.wifi.push({ ssid: n.ssid }); i = edit.wifi.length - 1;
      }
      renderWifi(); renderBar();
      const inp = $('nets').children[i].querySelector('input'); if (inp) inp.focus();
    });
    $('scanList').append(b);
  });
});

// ---------- save / undo ----------
$('undoBtn').addEventListener('click', () => { if (orig) { edit = mkEdit(orig); style = activeStyle(); renderAll(); } });
$('saveBtn').addEventListener('click', async () => {
  if (!S || !S.ok) { return load(); }
  const e = editForSave();
  const want = styleChanged() ? style : null;
  if (want && styleState(want) === 'update') return startUpdate(e, want);
  busy = true; renderBar(); msg('Đang gửi…');
  const r = await api.save(e, want);
  busy = false;
  if (!r.ok) { renderBar(); if (r.code === 'auth') return askAuth(); return msg(r.error, 'err'); }
  if (r.reboot) {
    msg('Đã lưu. Đồng hồ đang khởi động lại để vào mạng mới…', 'good');
    S.fw = r.fw; orig = clone(edit); orig.wifi = orig.wifi.map(w => ({ ssid: w.ssid })); edit = mkEdit(orig); style = activeStyle(); renderAll();
    msg('Đã lưu. Đồng hồ đang khởi động lại để vào mạng mới…', 'good');
    setTimeout(load, 20000);
    return;
  }
  S.config = r.config; S.fw = r.fw; orig = view(r.config); edit = mkEdit(orig); style = activeStyle(); renderAll();
  msg(`Đã lưu ${leaves(r.sent) + (r.styleSent ? 1 : 0)} thay đổi.`, 'good');
});

// ---------- firmware ----------
async function startUpdate(e, want) {
  const r = await api.update(e || null, want ?? null);
  if (!r.ok) return msg(r.error, 'err');
  busy = true; renderAll();
  showTab('update');
  $('steps').classList.remove('hide'); $('fwStep').classList.remove('hide');
  pollJob();
}
async function pollJob() {
  const j = await api.job();
  const i = STEP_ORDER.indexOf(j.step);
  [...$('steps').children].forEach((s, k) => s.className = k < i || j.done ? 'done' : k === i ? 'on' : '');
  $('fwStep').textContent = j.error ? '' : (STEP_TEXT[j.step] || '');
  msg(j.error ? j.error : (STEP_TEXT[j.step] || 'Đang cập nhật…'), j.error ? 'err' : '');
  if (j.running) { setTimeout(pollJob, 1000); return; }
  busy = false;
  if (j.done) { await load(); msg('Đã cập nhật firmware' + (edit && styleState(style) === 'ready' ? ' và áp dụng cài đặt.' : '.'), 'good'); $('fwStep').textContent = 'Xong.'; }
  else { renderAll(); msg(j.error || 'Cập nhật không thành công.', 'err'); }
}
$('fwBtn').addEventListener('click', () => startUpdate(null, null));
window.clawdBusyClose = () => msg('Đang cập nhật firmware: chờ xong rồi hãy đóng cửa sổ.', 'err');

// ---------- maintenance ----------
async function simple(call, okText) {
  busy = true; renderBar();
  const r = await call();
  busy = false; renderBar();
  if (r.ok) msg(okText, 'good'); else if (r.code !== 'cancel') { if (r.code === 'auth') askAuth(); else msg(r.error, 'err'); }
  return r;
}
$('exportBtn').addEventListener('click', () => simple(() => api.export_backup(), 'Đã lưu bản sao lưu.'));
$('importBtn').addEventListener('click', async () => { const r = await simple(() => api.import_backup(), 'Đã khôi phục. Đồng hồ đang khởi động lại…'); if (r.ok) setTimeout(load, 20000); });
$('rebootBtn').addEventListener('click', async () => { const r = await simple(() => api.reboot(), 'Đồng hồ đang khởi động lại…'); if (r.ok) setTimeout(load, 20000); });
$('factoryBtn').addEventListener('click', () => { $('facInput').value = ''; $('factoryVeil').classList.add('on'); $('facInput').focus(); });
$('facCancel').addEventListener('click', () => $('factoryVeil').classList.remove('on'));
$('facOk').addEventListener('click', async () => {
  const v = $('facInput').value.trim().toUpperCase();
  if (v !== 'XOA') { $('facInput').focus(); return; }
  $('factoryVeil').classList.remove('on');
  await simple(() => api.factory_reset('XOA'), 'Đã xoá. Đồng hồ khởi động lại ở chế độ cài đặt (điểm phát Clawd-Setup).');
});

// ---------- app tab ----------
$('autostart').addEventListener('click', async () => { const r = await api.set_autostart(!(S.app && S.app.autostart)); if (r.ok) { S.app = r.app; renderApp(); } else msg(r.error, 'err'); });
$('hooks').addEventListener('click', async () => { const r = await api.set_hooks(!(S.app && S.app.hooks)); if (r.ok) { S.app = r.app; renderApp(); } else msg(r.error, 'err'); });
$('logBtn').addEventListener('click', () => api.open_log());
$('webBtn').addEventListener('click', () => api.open_web());
$('rescanBtn').addEventListener('click', async () => { msg('Đang tìm đồng hồ trong mạng…'); $('rescanBtn').disabled = true; const r = await api.set_clock(''); $('rescanBtn').disabled = false; apply(r); });
$('manualBtn').addEventListener('click', async () => { const v = $('manualHost').value.trim(); if (!v) return; msg('Đang kết nối…'); apply(await api.set_clock(v)); });

// ---------- login ----------
function askAuth() { $('authErr').classList.add('hide'); $('authPass').value = ''; $('authVeil').classList.add('on'); $('authPass').focus(); }
async function doLogin() {
  const p = $('authPass').value; if (!p) return;
  $('authBtn').disabled = true;
  const r = await api.login(p);
  $('authBtn').disabled = false;
  if (!r.ok && r.code === 'auth') { $('authErr').textContent = r.error; $('authErr').classList.remove('hide'); return; }
  $('authVeil').classList.remove('on');
  apply(r);
  if (r.ok && r.saved === false) msg('Đăng nhập được, nhưng máy không cho lưu mật khẩu: lần sau phải nhập lại.', 'err');
}
$('authBtn').addEventListener('click', doLogin);
$('authPass').addEventListener('keydown', e => { if (e.key === 'Enter') doLogin(); });
document.addEventListener('keydown', e => {
  if (e.key === 'Escape') $('factoryVeil').classList.remove('on');
  if ((e.metaKey || e.ctrlKey) && e.key === 's') { e.preventDefault(); if (!$('saveBtn').disabled) $('saveBtn').click(); }
});

// ---------- load ----------
function apply(r) {
  S = r;
  if (r.ok) { orig = view(r.config); edit = mkEdit(orig); style = activeStyle(); msg('Không có thay đổi chưa lưu.'); }
  else if (r.code === 'auth') { askAuth(); msg(r.error, 'err'); }
  else { msg(r.error, 'err'); if (r.code === 'noclock') showTab('app'); }
  renderAll();
}
async function load() { msg('Đang đọc cài đặt từ đồng hồ…'); apply(await api.state()); }

window.addEventListener('pywebviewready', () => {
  api = window.pywebview.api; load();
  const h = location.hash.slice(1);
  if (h !== tab && document.querySelector(`#tabs [data-tab="${CSS.escape(h)}"]`)) showTab(h);   // refreshes for usage
  else refreshLive();
});
requestAnimationFrame(loop);
