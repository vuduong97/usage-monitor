// The Agent tab, the usage source picker and the setup wizard. Loaded after app.js and uses
// its helpers ($, el, msg, showTab, load) and `api` (settings_api.py: agents, set_agent,
// set_agents, disconnect_all, usage_sources, set_usage_source, setup_state, finish_setup,
// setup_clocks, setup_use_clock, set_clock).
'use strict';

const SIGNAL_TEXT = { think: 'Suy nghĩ', tool: 'Công cụ', subagent: 'Subagent', error: 'Lỗi', attention: 'Cần bạn' };
let AG = null;          // last api.agents()

function signalChips(a) {
  const box = el('div', 'sig');
  for (const s of ['think', 'tool', 'subagent', 'error', 'attention']) {
    box.append(el('span', 'chip ' + (a.signals.includes(s) ? 'ok' : 'no'), SIGNAL_TEXT[s]));
  }
  if (a.beta) box.append(el('span', 'chip fw', 'Beta'));
  return box;
}
function agentStatus(a) {
  if (a.error) return a.error;
  if (a.installed) return 'Đã nối' + (a.note ? ` · ${a.note}` : '');
  return a.found ? 'Có trên máy, chưa nối' : 'Không thấy trên máy này';
}

// ---------- Agent tab ----------
function renderAgents() {
  const list = $('agentList'); list.replaceChildren();
  if (!AG) return;
  for (const a of AG.agents) {
    const row = el('div', 'ag' + (a.found || a.installed ? '' : ' off'));
    const nm = el('div', 'nm');
    nm.append(el('b', null, a.name), el('div', 'st' + (a.error ? ' err' : ''), agentStatus(a)), signalChips(a));
    const sw = el('button', 'sw');
    sw.setAttribute('role', 'switch');
    sw.setAttribute('aria-label', `Nối ${a.name}`);
    sw.setAttribute('aria-checked', String(!!a.installed));
    sw.disabled = !a.found && !a.installed;
    sw.title = a.where || '';
    sw.addEventListener('click', async () => {
      sw.disabled = true;
      const r = await api.set_agent(a.id, !a.installed);
      if (r.agents) AG.agents = r.agents;
      if (r.ok) msg(`${a.name}: ${r.msg}`, 'good'); else msg(r.error, 'err');
      renderAgents(); renderAgentSum();
    });
    row.append(nm, sw);
    list.append(row);
  }
  $('wslNote').classList.toggle('hide', !AG.wsl);
}
function renderAgentSum() {
  const on = AG ? AG.agents.filter(a => a.installed).map(a => a.name) : [];
  $('agentSum').textContent = on.length ? on.join(', ') : 'chưa nối agent nào';
}
async function loadAgents() {
  AG = await api.agents();
  renderAgents(); renderAgentSum();
}
$('agentsRescan').addEventListener('click', () => { loadAgents(); msg('Đã quét lại.'); });
$('disconnectAll').addEventListener('click', async () => {
  const r = await api.disconnect_all();
  if (r.agents) AG.agents = r.agents;
  renderAgents(); renderAgentSum();
  msg('Đã ngắt mọi agent khỏi Clawd.', 'good');
});
$('goAgents').addEventListener('click', () => showTab('agents'));
$('rerunSetup').addEventListener('click', () => openWizard(true));

// ---------- usage source (Usage tab) ----------
let US = null;
function renderUsageSrc() {
  const sel = $('usageSrc'); sel.replaceChildren();
  if (!US) return;
  for (const o of US.options) {
    const opt = el('option', null, o.label);
    opt.value = o.id; opt.disabled = !o.ok && o.id !== US.current;
    sel.append(opt);
  }
  sel.value = US.current;
  const cur = US.options.find(o => o.id === US.current);
  $('usageSrcNote').textContent = cur ? cur.detail || '' : '';
}
async function loadUsageSrc() { US = await api.usage_sources(); renderUsageSrc(); }
$('usageSrc').addEventListener('change', async e => {
  const r = await api.set_usage_source(e.target.value);
  if (r.ok) { US.current = r.current; msg('Đã đổi nguồn usage: đồng hồ cập nhật trong giây lát.', 'good'); }
  else msg(r.error, 'err');
  renderUsageSrc();
});

// ---------- the setup wizard ----------
const STEP_NAME = { clock: 'Đồng hồ', agents: 'Agent', usage: 'Usage', done: 'Xong' };
const W = { steps: [], i: 0, done: [], timer: 0, clocks: null, picked: '', agents: null, ticks: new Set(),
            usage: null, choice: '', since: 0, busy: false, loading: false, clockOk: false };
// "Tiếp" works unless a step is still loading, a request runs, or no clock answers yet.
function nextState() {
  const s = W.steps[W.i];
  $('wizNext').disabled = W.busy || W.loading || (s === 'clock' && !W.clockOk);
}

async function openWizard(all) {
  const st = await api.setup_state();
  W.steps = [...(all ? st.steps : st.pending), 'done'];
  W.i = 0; W.done = []; W.ticks = new Set(); W.agents = null; W.usage = null; W.clocks = null;
  $('wiz').classList.add('on');
  showStep();
}
function closeWizard() {
  clearInterval(W.timer); W.timer = 0;
  $('wiz').classList.remove('on');
  load(); loadAgents(); loadUsageSrc();
}
window.clawdOpenSetup = () => openWizard(false);
window.clawdLoadAgents = () => loadAgents();

function wizMsg(t, kind = '') { const m = $('wizMsg'); m.textContent = t; m.className = 'msg' + (kind ? ' ' + kind : ''); }
function renderSteps() {
  const box = $('wizSteps'); box.replaceChildren();
  W.steps.forEach((s, j) => box.append(el('span', j === W.i ? 'on' : j < W.i ? 'done' : '', `${j + 1}. ${STEP_NAME[s]}`)));
}
function option(kind, checked, title, detail, disabled, onClick) {
  const b = el('button', 'opt');
  b.setAttribute('role', kind);
  b.setAttribute('aria-checked', String(!!checked));
  if (disabled) b.setAttribute('aria-disabled', 'true');
  const nm = el('span', 'nm');
  nm.append(el('span', null, title));
  if (detail) nm.append(typeof detail === 'string' ? el('small', null, detail) : detail);
  b.append(el('span', 'ck'), nm);
  if (!disabled) b.addEventListener('click', onClick);
  return b;
}
function showStep() {
  clearInterval(W.timer); W.timer = 0;
  renderSteps(); wizMsg('');
  const s = W.steps[W.i], body = $('wizBody');
  body.replaceChildren();
  $('wizBack').classList.toggle('hide', W.i === 0 || s === 'done');
  $('wizSkip').classList.toggle('hide', s === 'done');
  $('wizClose').classList.toggle('hide', s === 'done');
  $('wizNext').textContent = s === 'done' ? 'Xong' : 'Tiếp';
  W.loading = false; nextState();
  if (s === 'clock') stepClock(body);
  else if (s === 'agents') stepAgents(body);
  else if (s === 'usage') stepUsage(body);
  else stepDone(body);
}

// -- step: the clock
function stepClock(body) {
  W.since = Date.now(); W.clockOk = false; nextState();
  body.append(el('h2', 'wizTitle', 'Tìm đồng hồ Clawd'),
    el('p', 'sub', 'Đồng hồ cần đang chạy và đã vào cùng mạng Wi‑Fi với máy tính này.'));
  const list = el('div'); list.id = 'wizClocks';
  const manual = el('div', 'f');
  const inp = el('input'); inp.type = 'text'; inp.placeholder = '192.168.1.48'; inp.spellcheck = false;
  inp.setAttribute('aria-label', 'Địa chỉ đồng hồ'); inp.style.flex = '1';
  const use = el('button', 'btn ghost small', 'Dùng');
  use.addEventListener('click', async () => {
    const v = inp.value.trim(); if (!v) return;
    wizMsg('Đang kết nối…');
    const r = await api.set_clock(v);
    if (r.ok || r.code === 'auth') { W.picked = v; wizMsg('Đã tìm thấy đồng hồ.', 'good'); pollClocks(); }
    else wizMsg(r.error, 'err');
  });
  const row = el('div', 'row'); row.style.flexWrap = 'nowrap'; row.append(inp, use);
  manual.append(el('span', null, 'Hoặc nhập IP'), row);
  const help = el('div', 'box hide'); help.id = 'wizHelp';
  help.append(el('h3', null, 'Đồng hồ chưa vào Wi‑Fi?'));
  const g = el('div', 'guide');
  g.append(guideArt());
  const ol = el('ol');
  for (const t of ['Cắm điện đồng hồ. Chưa có Wi‑Fi thì màn hình hiện tên điểm phát (Clawd-Setup hoặc Clawd-XXXX) và địa chỉ 192.168.4.1.',
                   'Trên điện thoại, vào Wi‑Fi đó. Trang cài đặt tự mở (nếu không, mở 192.168.4.1).',
                   'Chọn Wi‑Fi nhà bạn, nhập mật khẩu, lưu. Đồng hồ khởi động lại và vào mạng đó.',
                   'Quay lại đây: đồng hồ sẽ hiện trong danh sách.']) ol.append(el('li', null, t));
  g.append(ol);
  help.append(g, el('p', 'note', 'Máy tính và đồng hồ phải cùng một mạng Wi‑Fi. Clawd không đổi Wi‑Fi của máy tính.'));
  body.append(list, manual, help);
  pollClocks();
  W.timer = setInterval(pollClocks, 2000);
}
function guideArt() {      // a clock showing its setup network: no image files to ship
  const ns = 'http://www.w3.org/2000/svg', svg = document.createElementNS(ns, 'svg');
  svg.setAttribute('viewBox', '0 0 120 120'); svg.setAttribute('width', '120'); svg.setAttribute('aria-hidden', 'true');
  svg.innerHTML = '<rect x="18" y="14" width="84" height="84" rx="10" fill="#1f1f1e" stroke="#55534d" stroke-width="2"/>' +
    '<rect x="28" y="24" width="64" height="56" rx="3" fill="#0b0b0a"/>' +
    '<path d="M44 46a22 22 0 0 1 32 0M50 52a13 13 0 0 1 20 0" stroke="#d97757" stroke-width="3" fill="none" stroke-linecap="round"/>' +
    '<circle cx="60" cy="58" r="3" fill="#d97757"/>' +
    '<text x="60" y="73" font-size="7" fill="#faf9f5" text-anchor="middle" font-family="monospace">Clawd-Setup</text>' +
    '<rect x="50" y="98" width="20" height="8" fill="#55534d"/>';
  return svg;
}
async function pollClocks() {
  let r;
  try { r = await api.setup_clocks(); } catch (e) { return; }
  W.clocks = r;
  const list = $('wizClocks'); if (!list) return;
  list.replaceChildren();
  const cur = r.current;
  if (cur && cur.answers) list.append(el('p', 'note', `Đang dùng: ${cur.name || cur.host} (${cur.host})`));
  for (const c of r.clocks) {
    const chosen = cur && cur.host === c.host;
    list.append(option('radio', chosen, c.name || c.host, `${c.host}${c.version ? ' · firmware ' + c.version : ''}`, false, async () => {
      const u = await api.setup_use_clock(c.name);
      if (u.ok) { W.picked = c.name; wizMsg(`Đã chọn ${c.name}.`, 'good'); pollClocks(); } else wizMsg(u.error, 'err');
    }));
  }
  if (!r.clocks.length) list.append(el('p', 'note', 'Đang tìm đồng hồ trên mạng…'));
  const ok = Boolean(cur && cur.answers);
  W.clockOk = ok; nextState();
  $('wizHelp').classList.toggle('hide', ok || (r.clocks.length > 0) || Date.now() - W.since < 6000);
}

// -- step: agents
async function stepAgents(body) {
  body.append(el('h2', 'wizTitle', 'Clawd phản ứng với agent nào?'),
    el('p', 'sub', 'Clawd thêm một hook nhỏ vào cấu hình agent bạn chọn (có sao lưu, gỡ được bất cứ lúc nào trong tab Agent). Chưa bấm Tiếp thì chưa có gì thay đổi.'));
  const list = el('div'); body.append(list);
  W.loading = true; nextState();                // never send an empty list before it loaded
  if (!W.agents) {
    const r = await api.agents();
    W.agents = r;
    W.ticks = new Set(r.agents.filter(a => a.installed || a.found).map(a => a.id));
  }
  const found = W.agents.agents.filter(a => a.found || a.installed);
  const draw = () => {
    list.replaceChildren();
    for (const a of found) {
      const detail = el('span');
      detail.append(el('small', null, a.error || a.where), signalChips(a));
      list.append(option('checkbox', W.ticks.has(a.id), a.name, detail, false, () => {
        if (W.ticks.has(a.id)) W.ticks.delete(a.id); else W.ticks.add(a.id);
        draw();
      }));
    }
    if (!found.length) list.append(el('p', 'note', 'Chưa thấy agent nào trên máy này. Cài agent rồi bật trong tab Agent sau.'));
    const missing = W.agents.agents.filter(a => !a.found && !a.installed).map(a => a.name);
    if (missing.length) list.append(el('p', 'note', 'Không thấy trên máy: ' + missing.join(', ') + '. Cài sau thì Clawd sẽ hỏi.'));
    if (W.agents.wsl) list.append(el('p', 'note warn', 'Agent chạy trong WSL chưa được hỗ trợ.'));
  };
  draw();
  W.loading = false; nextState();
}

// -- step: usage source
async function stepUsage(body) {
  body.append(el('h2', 'wizTitle', 'Đồng hồ hiện usage nào?'),
    el('p', 'sub', 'Màn hình Usage hiện mức dùng 5 giờ và tuần của một nguồn. Đổi lại được trong tab Usage.'));
  const list = el('div'); body.append(list);
  W.loading = true; nextState();
  if (!W.usage) { W.usage = await api.usage_sources(); W.choice = W.usage.current; }
  const draw = () => {
    list.replaceChildren();
    for (const o of W.usage.options) {
      list.append(option('radio', W.choice === o.id, o.label, o.detail, !o.ok, () => { W.choice = o.id; draw(); }));
    }
  };
  draw();
  W.loading = false; nextState();
}

// -- step: done
function stepDone(body) {
  body.append(el('h2', 'wizTitle', 'Xong rồi'));
  const on = (W.agents ? W.agents.agents : (AG ? AG.agents : [])).filter(a => a.installed);
  if (on.length) {
    body.append(el('p', 'sub', 'Clawd sẽ phản ứng khi bạn dùng: ' + on.map(a => a.name).join(', ') + '.'));
    const notes = on.filter(a => a.note);
    if (notes.length) {
      const ul = el('ul', 'note');
      for (const a of notes) ul.append(el('li', null, `${a.name}: ${a.note}`));
      body.append(ul);
    }
    body.append(el('p', 'note', 'Agent đang mở cần khởi động lại một lần để nhận hook.'));
  } else {
    body.append(el('p', 'sub', 'Chưa nối agent nào: đồng hồ vẫn hiện giờ và usage. Bật agent trong tab Agent khi cần.'));
  }
}

async function commitStep(skip) {
  const s = W.steps[W.i];
  if (s === 'agents' && !skip) {
    wizMsg('Đang nối…');
    const r = await api.set_agents([...W.ticks]);
    W.agents = { ...W.agents, agents: r.agents };
    if (!r.ok) { wizMsg(r.error, 'err'); return false; }
  }
  if (s === 'usage' && !skip) {
    const r = await api.set_usage_source(W.choice);
    if (!r.ok) { wizMsg(r.error, 'err'); return false; }
  }
  await api.finish_setup([s]);
  return true;
}
$('wizNext').addEventListener('click', async () => {
  if (W.busy) return;
  if (W.steps[W.i] === 'done') { closeWizard(); return; }
  W.busy = true; nextState();
  try { if (await commitStep(false)) { W.i++; showStep(); } }
  finally { W.busy = false; nextState(); }
});
$('wizSkip').addEventListener('click', async () => {
  if (W.busy) return;
  W.busy = true; nextState();
  try { await commitStep(true); W.i++; showStep(); } finally { W.busy = false; nextState(); }
});
$('wizBack').addEventListener('click', () => { if (W.i > 0) { W.i--; showStep(); } });
$('wizClose').addEventListener('click', closeWizard);

window.addEventListener('pywebviewready', () => { loadAgents(); loadUsageSrc(); });
