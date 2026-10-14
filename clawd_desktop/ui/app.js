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
const STEP_TEXT = { check:'Đang kiểm tra đồng hồ…', download:'Đang tải bản cập nhật…',
  pause:'Tạm dừng gửi dữ liệu tới đồng hồ…', upload:'Đang gửi firmware (khoảng 30 giây)…',
  reboot:'Đồng hồ đang khởi động lại…', clawdpak:'Đang gửi gói hoạt hình Clawd…',
  deskpak:'Đang gửi gói hoạt hình bàn làm việc (vài phút)…', style:'Đang gửi kiểu màn hình…', done:'Xong.' };
const STEP_ORDER = ['download', 'pause', 'upload', 'reboot', 'clawdpak', 'deskpak'];
const PACK_NAME = { fw:'firmware', clawdpak:'gói Clawd', deskpak:'gói bàn làm việc' };

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
  ambercrt:'Màn hình CRT màu hổ phách, vòng 24 ô cho phiên 5 giờ.',
  vt100tui:'Terminal VT100 xanh lá, thanh tiến độ kiểu htop.',
  mac84:'Hai cửa sổ Macintosh 1984 đen trắng trên nền gạch chéo.',
  win95:'Hộp thoại Windows 95, thanh tiến độ dạng khối.',
  biossetup:'Màn hình cài đặt BIOS nền xanh, Clawd ở khung trợ giúp.',
  c64:'Commodore 64, Clawd đi sang phải khi phiên đầy dần.',
  zxspec:'ZX Spectrum, cột ô màu cầu vồng, viền đổi màu theo trạng thái.',
  casiolcd:'Đồng hồ Casio LCD, chữ số đè lên vạch 8 mờ.',
  calc:'Máy tính bỏ túi, số lớn trên màn LCD.',
  hifivfd:'Màn VFD dàn hi-fi, hai cột đo và phổ âm.',
  tetris:'Tetris: chồng khối cao dần theo mức dùng 5 giờ.',
  pinballdmd:'Pinball: số lớn trên bảng chấm điểm DMD.',
  pacmaze:'Pac-Man: Clawd ăn dần chấm trong mê cung, chấm còn lại là phần 5 giờ còn lại.',
  jrpgstatus:'Bảng trạng thái JRPG, hai thanh máu cho 5 giờ và tuần.',
  hiscore:'Bảng điểm cao arcade, xếp hạng và dòng chữ nhấp nháy.',
  invaders:'Space Invaders: số kẻ địch bị bắn là mức dùng 5 giờ, pháo của Clawd chạy theo tuần.',
  eggpet:'Thú ảo hình trứng: Clawd là con thú, hai cột đo 5 giờ và tuần.',
  platformer:'Màn chơi đi cảnh: Clawd chạy tới lá cờ là lúc phiên 5 giờ reset.',
  chessclock:'Đồng hồ cờ vua, hai mặt cho 5 giờ và tuần, kim chạy theo mức dùng.',
  racinghud:'HUD đua xe: đồng hồ 20 vạch cho 5 giờ, hàng đèn chuyển số cho tuần.',
  vumeter:'Đồng hồ VU analog, kim chỉ mức dùng 5 giờ.',
  scope:'Máy hiện sóng: đường vẽ tới con trỏ NOW là thời gian đã trôi.',
  pfd:'Màn bay PFD, hai thang đo dọc cho 5 giờ và tuần.',
  fuel:'Đồng hồ xăng ô tô: vòng E–F là phần 5 giờ còn lại.',
  monitor:'Máy theo dõi bệnh nhân: sóng bên trái, ô chỉ số bên phải.',
  rack:'Tủ rack máy chủ, mỗi tầng một thanh đèn trạng thái.',
  weather:'Trạm thời tiết: số lớn kiểu chữ có chân, biểu tượng dự báo, áp kế cho tuần.',
  powerbank:'Pin dự phòng: hai viên pin vơi dần khi dùng nhiều.',
  lab:'Dụng cụ thí nghiệm: cốc là 5 giờ, ống nghiệm là tuần, mực nước là mức dùng.',
  sonar:'Màn sonar: Clawd là mục tiêu tiến vào tâm khi phiên 5 giờ đầy lên.',
  hairline:'Tối giản nét mảnh: hai con số, đường kẻ mảnh, nhiều khoảng trắng.',
  widget:'Widget điện thoại: một thẻ rộng và hai thẻ nhỏ trên nền xám nhạt.',
  tonal:'Material tonal: hai thanh viên thuốc dày, nút Clawd tròn.',
  brutal:'Neo-brutalist: khối đen cứng, màu phẳng, dòng chữ chạy.',
  soft:'Soft UI: vòng tròn nổi, tâm lõm, viền sáng tối 1 px.',
  oled:'OLED đen: hai thanh mỏng cho mức dùng và thời gian, Clawd đi trên thanh 5 giờ.',
  ink:'Máy đọc sách mực điện tử: trang sách, số lớn chữ có chân, tiến độ tuần kiểu Kindle.',
  tufte:'Tufte: các dải chấm có chú thích, mỗi giá trị một chấm, vạch là thời gian đã trôi.',
  frost:'Kính mờ: thẻ kính xếp lớp trên nền hoàng hôn 4 dải.',
  bigtype:'Chữ khổng lồ: hai chữ số bo tròn chiếm gần hết màn hình.',
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
  if (t === 'wifi' && !scan.nets && !scan.err) doScan();   // the clock's scan blocks it ~3 s: once per window
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
  // The preview keeps full brightness: the slider only changes what the clock does.
  for (const e of [cv, img]) {
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
  renderConn(); renderDisplay(); renderGallery(); renderWifi(); renderAppUpdate(); renderUpdate(); renderApp(); renderBar();
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
// The WiFi tab: "Đã lưu" (edit.wifi) above "Mạng quanh đây" (the clock's own scan, run when
// the tab opens). The clock joins the strongest saved network it hears, so there is no order
// to edit. One row at a time is open for a password: wifiOpen = {list:'saved'|'near', ssid}.
const MAX_NETS = 4;
let scan = { busy: false, nets: null, err: '' };
let wifiOpen = null;
function bars(rssi, cur) {   // 4 bars from dBm; none lit when the clock does not hear it
  const n = rssi == null ? 0 : rssi >= -55 ? 4 : rssi >= -65 ? 3 : rssi >= -75 ? 2 : 1;
  const b = el('span', 'bars' + (cur ? ' cur' : ''));
  for (let k = 0; k < 4; k++) b.append(el('i', k < n ? 'on' : null));
  b.setAttribute('role', 'img');
  b.setAttribute('aria-label', n ? `Sóng ${n}/4` : 'Không thấy quanh đồng hồ');
  return b;
}
function heard(ssid) {       // the clock's rssi for a network: its live link first, then the scan
  const net = S && S.net;
  if (net && net.ssid === ssid && net.rssi != null) return net.rssi;
  const n = scan.nets && scan.nets.find(x => x.ssid === ssid);
  return n ? n.rssi : null;
}
function iconBtn(icon, label, cls = 'ib') { const b = el('button', cls); b.append(svgIcon(icon)); b.setAttribute('aria-label', label); b.title = label; return b; }
function pwField(label, placeholder, value, oninput) {
  const w = el('div', 'pwf'), pw = el('input');
  pw.type = 'password'; pw.autocomplete = 'off'; pw.spellcheck = false; pw.value = value || '';
  pw.placeholder = placeholder; pw.setAttribute('aria-label', label); pw.dataset.focus = '1';
  pw.addEventListener('input', () => oninput(pw.value));
  const eye = iconBtn('eye', 'Hiện mật khẩu');
  eye.addEventListener('click', () => { const show = pw.type === 'password'; pw.type = show ? 'text' : 'password';
    eye.setAttribute('aria-label', show ? 'Ẩn mật khẩu' : 'Hiện mật khẩu'); eye.title = eye.getAttribute('aria-label'); });
  w.append(pw, eye);
  return { w, pw };
}
function toggleOpen(list, ssid) {
  wifiOpen = wifiOpen && wifiOpen.list === list && wifiOpen.ssid === ssid ? null : { list, ssid };
  renderWifi();
  const f = $('panes').querySelector('.wr.open [data-focus]'); if (f) f.focus();
}
function addNet(ssid, pass) {
  if (!edit || edit.wifi.length >= MAX_NETS || edit.wifi.some(r => r.ssid === ssid)) return;
  edit.wifi.push(pass ? { ssid, pass } : { ssid });
  wifiOpen = null; renderWifi(); renderBar();
}
function renderWifi() {
  if (!edit) return;
  const isOpen = (list, ssid) => wifiOpen && wifiOpen.list === list && wifiOpen.ssid === ssid;
  const saved = new Set(orig ? orig.wifi.map(r => r.ssid) : []), full = edit.wifi.length >= MAX_NETS;
  $('wifiCount').textContent = `${edit.wifi.length} / ${MAX_NETS}`;

  const box = $('nets'); box.replaceChildren();
  edit.wifi.forEach((r, i) => {
    const open = isOpen('saved', r.ssid), cur = S && S.net && S.net.ssid === r.ssid;
    const row = el('div', 'wr' + (open ? ' open' : ''));
    const tag = cur ? el('span', 'tag', 'Đang dùng') : !saved.has(r.ssid) ? el('span', 'tag new', 'Mới') : el('span');
    const tog = iconBtn(open ? 'x' : 'more', open ? 'Đóng' : `Sửa ${r.ssid}`);
    tog.setAttribute('aria-expanded', String(open));
    tog.addEventListener('click', () => toggleOpen('saved', r.ssid));
    row.append(bars(heard(r.ssid), cur), el('span', 'nm', r.ssid), tag, tog);
    if (open) {
      const ed = el('div', 'ed');
      const { w } = pwField(`Mật khẩu ${r.ssid}`, saved.has(r.ssid) ? 'Mật khẩu mới' : 'Mật khẩu', r.pass,
        v => { r.pass = v; renderBar(); });
      const rm = iconBtn('trash', `Xoá ${r.ssid}`);
      rm.disabled = edit.wifi.length <= 1;
      if (rm.disabled) rm.title = 'Cần giữ ít nhất một mạng';
      rm.addEventListener('click', () => { edit.wifi.splice(i, 1); wifiOpen = null; renderWifi(); renderBar(); });
      ed.append(w, rm); row.append(ed);
    }
    box.append(row);
  });

  const ctl = $('scanCtl'); ctl.replaceChildren();
  if (scan.busy) { const sp = el('span', 'spin'); sp.setAttribute('role', 'status'); sp.setAttribute('aria-label', 'Đang dò mạng'); ctl.append(sp); }
  else { const rb = iconBtn('refresh', 'Dò lại'); rb.disabled = !S || !S.ok; rb.addEventListener('click', doScan); ctl.append(rb); }

  const near = $('near'); near.replaceChildren();
  if (scan.err) { near.append(el('p', 'wnote', scan.err)); return; }
  if (!scan.nets) return;
  const have = new Set(edit.wifi.map(r => r.ssid));
  const list = scan.nets.filter(n => !have.has(n.ssid));
  if (!list.length && !scan.busy) near.append(el('p', 'wnote', 'Không thấy mạng nào khác.'));
  list.forEach(n => {
    const open = isOpen('near', n.ssid);
    const row = el('div', 'wr' + (open ? ' open' : ''));
    const lk = n.enc ? svgIcon('lock') : el('span');
    if (n.enc) lk.classList.add('lk');
    let act;
    if (open) { act = iconBtn('x', 'Đóng'); act.addEventListener('click', () => toggleOpen('near', n.ssid)); }
    else {
      act = iconBtn('plus', `Thêm ${n.ssid}`, 'plus'); act.disabled = full;
      if (full) act.title = `Đồng hồ nhớ tối đa ${MAX_NETS} mạng`;
      act.addEventListener('click', () => n.enc ? toggleOpen('near', n.ssid) : addNet(n.ssid));
    }
    row.append(bars(n.rssi), el('span', 'nm', n.ssid), lk, act);
    if (open) {
      const ed = el('div', 'ed'), ok = el('button', 'btn small', 'Thêm');
      const { w, pw } = pwField(`Mật khẩu ${n.ssid}`, 'Mật khẩu', '', v => { ok.disabled = v.length < 8; });
      ok.disabled = true;
      ok.addEventListener('click', () => addNet(n.ssid, pw.value));
      pw.addEventListener('keydown', e => { if (e.key === 'Enter' && !ok.disabled) ok.click(); });
      ed.append(w, ok); row.append(ed);
    }
    near.append(row);
  });
}
async function doScan() {
  if (!api || scan.busy || !S || !S.ok) return;
  scan = { ...scan, busy: true, err: '' }; renderWifi();
  const r = await api.scan();
  scan = r.ok ? { busy: false, nets: r.networks, err: '' } : { busy: false, nets: null, err: r.error };
  if (wifiOpen && wifiOpen.list === 'near' && !(scan.nets || []).some(n => n.ssid === wifiOpen.ssid)) wifiOpen = null;
  renderWifi();
}
// ---------- the app's own update (appupdate.py) ----------
let appUpTimer = 0, appUpIdle = 0;
function renderAppUpdate() {
  const u = S && S.app && S.app.update;
  $('appUpBox').classList.toggle('hide', !u);
  if (!u) return;
  const kv = $('appUpKv'); kv.replaceChildren();
  const add = (k, v) => kv.append(el('span', null, k), el('span', null, v));
  add('Đang chạy', `Clawd ${u.current}`);
  if (u.version && u.status !== 'current') add('Bản mới', `Clawd ${u.version}`);
  if (u.version && u.notes && u.status !== 'current') add('Ghi chú', u.notes.replace(/\s+/g, ' ').slice(0, 200));
  const pct = Math.round((u.progress || 0) * 100);
  const manual = u.version && !u.canApply && u.status !== 'checking' && u.status !== 'downloading';
  $('appUpText').textContent = manual ? `Có bản ${u.version}. ${u.why} Bạn có thể tải bản cài đặt từ trang phát hành.` : {
    checking: 'Đang hỏi GitHub xem có bản mới không…',
    downloading: `Đang tải bản ${u.version || 'mới'}${pct ? ' · ' + pct + '%' : ''}…`,
    ready: `Đã tải xong bản ${u.version} và kiểm tra tính toàn vẹn. Khởi động lại Clawd để dùng bản này (cài đặt và mật khẩu được giữ nguyên).`,
    available: `Có bản ${u.version}.`,
    current: `Clawd ${u.current} là bản mới nhất.`,
    error: u.error || 'Không cập nhật được.',
    idle: u.checked ? `Clawd ${u.current} là bản mới nhất.` : 'Chưa kiểm tra.',
  }[u.status] || '';
  $('appUpBar').classList.toggle('hide', u.status !== 'downloading');
  $('appUpFill').style.width = pct + '%';
  const btn = $('appUpBtn');
  btn.textContent = manual ? 'Mở trang tải' : 'Khởi động lại để cập nhật';
  btn.disabled = manual ? false : !(u.status === 'ready' && u.canApply) || busy;
  $('appUpCheck').disabled = u.status === 'checking' || u.status === 'downloading';
  pollAppUpdate();
}
function pollAppUpdate() {       // while it is checking or downloading, ask again every second
  clearTimeout(appUpTimer);
  const u = S && S.app && S.app.update;
  if (!u) return;
  const active = u.status === 'checking' || u.status === 'downloading';
  if (!active && !(u.status === 'idle' && !u.checked && ++appUpIdle < 20)) return;
  appUpTimer = setTimeout(async () => {
    const r = api && await api.app_update();
    if (r && r.ok && S && S.app) { S.app.update = r; renderAppUpdate(); }
    else pollAppUpdate();
  }, 1000);
}
$('appUpBtn').addEventListener('click', async () => {
  const u = S && S.app && S.app.update; if (!u) return;
  if (u.status === 'ready' && u.canApply) {
    $('appUpBtn').disabled = true; msg('Đang khởi động lại để cập nhật…');
    const r = await api.app_update_apply();
    if (!r.ok) { msg(r.error, 'err'); renderAppUpdate(); }
  } else api.app_update_page();
});
$('appUpCheck').addEventListener('click', async () => {
  const r = await api.app_update_check();
  if (r.ok && S && S.app) { S.app.update = r; appUpIdle = 0; renderAppUpdate(); } else if (!r.ok) msg(r.error, 'err');
});
function renderUpdate() {
  const kv = $('fwKv'); kv.replaceChildren();
  const h = (S && S.heap) || {}, fw = S && S.fw;
  const add = (k, v) => kv.append(el('span', null, k), el('span', null, v));
  add('Đồng hồ đang chạy', h.version ? `${h.version} (bản #${h.fwSeq || 0}) · bật ${uptime(h.uptime || 0)}` : '—');
  if (h.version) add('Bộ nhớ', `heap ${kb(h.heap || 0)} · loop ${h.loopMaxMs ?? '?'} ms`);
  const b = fw && fw.bundle, feed = b && b.source === 'feed';
  add(feed ? 'Bản mới nhất' : 'Đi kèm app', b ? `bản #${b.seq}${b.date ? ' · ' + b.date : ''}${b.commit ? ' · ' + b.commit : ''} · ${kb(b.size)}` : 'không có');
  const plan = (fw && fw.plan) || {};
  const todo = Object.keys(plan).filter(k => plan[k] !== 'ok').map(k => PACK_NAME[k] + (plan[k] === 'wait' ? ' (đồng hồ đang kiểm tra)' : ''));
  if (todo.length) add('Sẽ cài', todo.join(' · '));
  if (feed && b.notes) add('Ghi chú', b.notes);
  const st = fw ? fw.update : 'none';
  const from = feed ? 'Có bản mới' : 'Có bản mới hơn đi kèm app';
  $('fwText').textContent = {
    available: from + (fw && fw.styles && fw.styles.state === 'update' ? ': thêm các kiểu màn hình usage mới' : '') + '. Cài mất vài phút, cài đặt trên đồng hồ được giữ nguyên.',
    packs: 'Firmware đã mới nhất, nhưng gói hoạt hình trên đồng hồ khác bản mới nhất. Bấm Cập nhật để gửi lại.',
    current: 'Đồng hồ đang chạy bản mới nhất.',
    none: (fw && fw.feedError) ? 'Chưa đọc được nguồn cập nhật. Kiểm tra mạng rồi thử lại.' : 'Chưa có bản firmware nào để cài.',
    incompatible: 'Đồng hồ này không chạy firmware Clawd nên app không cập nhật được.',
  }[st] || '';
  $('fwBtn').disabled = busy || (st !== 'available' && st !== 'packs') || !S || !S.ok;
  $('fwBtn').textContent = st === 'packs' ? 'Cập nhật gói hoạt hình' : 'Cập nhật firmware';
  $('fwCheck').disabled = busy || !S || !S.ok;
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
$('fwCheck').addEventListener('click', async () => {
  $('fwCheck').disabled = true; msg('Đang hỏi nguồn cập nhật…');
  try {
    const r = await api.check_update();
    if (!r.ok) return msg(r.error, 'err');
    S.fw = r.fw; S.heap = r.heap; renderAll();
    msg(r.fw.update === 'available' || r.fw.update === 'packs' ? 'Có bản cập nhật.' : r.fw.feedError ? 'Chưa đọc được nguồn cập nhật.' : 'Đồng hồ đang chạy bản mới nhất.', r.fw.feedError ? 'err' : 'good');
  } finally { renderUpdate(); }
});
window.clawdShowTab = t => showTab(t);
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
  if (r.ok) { orig = view(r.config); edit = mkEdit(orig); style = activeStyle(); wifiOpen = null; msg('Không có thay đổi chưa lưu.'); }
  else if (r.code === 'auth') { askAuth(); msg(r.error, 'err'); }
  else { msg(r.error, 'err'); if (r.code === 'noclock') showTab('app'); }
  renderAll();
  if (r.tab) showTab(r.tab);                 // the tray's "new firmware" item opens the Update tab
  if (r.ok && tab === 'wifi' && !scan.nets && !scan.busy) doScan();   // opened on #wifi before the clock answered
  if (r.ok && r.fw && r.fw.feedBusy) watchFeed();
}
// The update feed is read in the background (settings_api._kick_feed), so the window shows the
// clock at once with the packs the app carries. Ask again until the feed is done, then show
// what it brought (a newer release, its packs).
let feedWatch = false;
async function watchFeed() {
  if (feedWatch) return;
  feedWatch = true;
  try {
    for (let i = 0; i < 120 && S && S.fw && S.fw.feedBusy; i++) {   // up to about 90 s
      await new Promise(res => setTimeout(res, 750));
      const r = await api.fw();
      if (r && r.ok && S) S.fw = r.fw;
    }
  } catch (err) { console.warn('feed check failed', err); }
  finally { feedWatch = false; }
  if (S && S.fw) S.fw.feedBusy = false;
  renderGallery(); renderUpdate(); refreshLive();
}
async function load() { msg('Đang đọc cài đặt từ đồng hồ…'); apply(await api.state()); }

window.addEventListener('pywebviewready', () => {
  api = window.pywebview.api; load();
  const h = location.hash.slice(1);
  if (h !== tab && document.querySelector(`#tabs [data-tab="${CSS.escape(h)}"]`)) showTab(h);   // refreshes for usage
  else refreshLive();
});
requestAnimationFrame(loop);
