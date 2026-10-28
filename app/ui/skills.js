// "Nâng cao: Animation theo skill" on the Display tab: while a skill the user invoked runs, Clawd
// plays the animation picked here. Loaded after app.js and uses its helpers ($, el, iconBtn) and
// `api` (src/commands/skills.rs: skill_rules, save_skill_rules, skill_anim_preview,
// export_skill_rules, import_skill_rules). The rules live on this machine, never on the clock,
// and every change is saved at once (no "Lưu vào đồng hồ").
'use strict';

let SK = null;          // last api.skill_rules(): {rules, anims, newAnims, skills, textMax, problems}
let skillRows = [];     // what the list shows: [{skill, anim, text}], drafts (no skill yet) included
let skillSaveTimer = 0;
let skillClearArmed = false;

function skillMsg(text, kind = '') { const m = $('skillMsg'); m.textContent = text; m.className = 'note' + (kind ? ' ' + kind : ''); }

function animSelect(value) {
  const s = el('select');
  s.setAttribute('aria-label', 'Animation');
  for (const name of SK.anims) {
    const o = el('option', null, SK.newAnims.includes(name) ? `${name} (cần gói desk mới)` : name);
    o.value = name;
    s.append(o);
  }
  s.value = SK.anims.includes(value) ? value : 'wizard';
  return s;
}

function renderSkills() {
  const list = $('skillRules'); list.replaceChildren();
  if (!SK) return;
  const names = $('skillNames'); names.replaceChildren();
  for (const n of SK.skills) { const o = el('option'); o.value = n; names.append(o); }
  skillRows.forEach((r, i) => {
    const row = el('div', 'sr');
    const skill = el('input'); skill.type = 'text'; skill.value = r.skill; skill.placeholder = 'ak-cook';
    skill.setAttribute('list', 'skillNames'); skill.setAttribute('aria-label', 'Skill'); skill.spellcheck = false;
    skill.addEventListener('change', () => { r.skill = skill.value.trim(); queueSkillSave(); });
    const anim = animSelect(r.anim);
    r.anim = anim.value;
    anim.addEventListener('change', () => { r.anim = anim.value; queueSkillSave(); });
    const text = el('input'); text.type = 'text'; text.value = r.text; text.maxLength = SK.textMax;
    text.placeholder = 'mặc định'; text.setAttribute('aria-label', 'Chữ trên đồng hồ');
    text.addEventListener('change', () => { r.text = text.value; queueSkillSave(); });
    const play = el('button', 'btn ghost small play', '▶');
    play.title = 'Phát thử trên đồng hồ'; play.setAttribute('aria-label', 'Phát thử trên đồng hồ');
    play.addEventListener('click', () => previewSkill(r));
    const del = iconBtn('x', 'Xoá luật này');
    del.addEventListener('click', () => { skillRows.splice(i, 1); renderSkills(); queueSkillSave(); });
    row.append(skill, anim, text, play, del);
    list.append(row);
  });
  if (!skillRows.length) list.append(el('p', 'note', 'Chưa có luật nào: Clawd hiện như bình thường.'));
  const problems = SK.problems || [];
  $('skillProblems').textContent = problems.join(' · ');
  $('skillProblems').classList.toggle('hide', !problems.length);
  $('skillExport').disabled = !SK.rules.length;
  $('skillClear').disabled = !skillRows.length;
}

function applySkillState(r) {
  if (!r || !r.ok) { skillMsg((r && r.error) || 'Không đọc được luật.', 'warn'); return; }
  SK = r;
  skillRows = r.rules.map(x => ({ skill: x.skill, anim: x.anim, text: x.text }));
  renderSkills();
}

async function loadSkills() {
  try { applySkillState(await api.skill_rules()); } catch (err) { skillMsg(String(err), 'warn'); }
}

// Rows without a skill are drafts: kept on screen, not saved until named.
function queueSkillSave() {
  clearTimeout(skillSaveTimer);
  skillSaveTimer = setTimeout(saveSkills, 400);
}
async function saveSkills() {
  const list = skillRows.filter(r => r.skill.trim()).map(r => ({ skill: r.skill, anim: r.anim, text: r.text }));
  const r = await api.save_skill_rules(list);
  if (!r.ok) { skillMsg(r.error, 'warn'); return; }
  SK.rules = r.rules;
  SK.problems = [];
  // show the names as the app compares them (/ak:cook -> ak-cook), keep the drafts
  const drafts = skillRows.filter(x => !x.skill.trim());
  skillRows = r.rules.map(x => ({ skill: x.skill, anim: x.anim, text: x.text })).concat(drafts);
  renderSkills();
  skillMsg(r.rules.length ? 'Đã lưu. Clawd dùng luật mới trong vài giây.' : 'Đã lưu: không còn luật nào.');
}

async function previewSkill(r) {
  skillMsg('Đang phát thử…');
  const res = await api.skill_anim_preview(r.anim, r.text);
  skillMsg(res.ok ? `Đồng hồ đang phát “${r.anim}” vài giây.` : res.error, res.ok ? '' : 'warn');
}

$('skillAdd').addEventListener('click', () => {
  if (!SK) return;
  skillRows.push({ skill: '', anim: 'wizard', text: '' });
  renderSkills();
  const inputs = $('skillRules').querySelectorAll('.sr input[list]');
  if (inputs.length) inputs[inputs.length - 1].focus();
});
$('skillExport').addEventListener('click', async () => {
  const r = await api.export_skill_rules();
  if (r.ok) skillMsg('Đã xuất luật.'); else if (r.code !== 'cancel') skillMsg(r.error, 'warn');
});
$('skillImport').addEventListener('click', async () => {
  const r = await api.import_skill_rules();
  if (r.ok) { await loadSkills(); skillMsg(`Đã nhập ${r.rules.length} luật.`); }
  else if (r.code !== 'cancel') skillMsg(r.error, 'warn');
});
$('skillClear').addEventListener('click', () => {
  const b = $('skillClear');
  if (!skillClearArmed) {
    skillClearArmed = true; b.textContent = 'Bấm lần nữa để xoá';
    setTimeout(() => { skillClearArmed = false; b.textContent = 'Xoá hết'; }, 4000);
    return;
  }
  skillClearArmed = false; b.textContent = 'Xoá hết';
  skillRows = []; renderSkills(); saveSkills();
});
$('skillAdv').addEventListener('toggle', () => { if ($('skillAdv').open) loadSkills(); });
