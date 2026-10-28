//! Skill animation rules: "while skill X runs, Clawd plays animation Y with text Z". Rust-only
//! (no Python origin). Rules live in `animation-rules.json` in the config dir, the settings
//! window edits them, and the Brain re-reads the file when it changes (no restart).
//!
//! A rule never outranks what tells the user something real: questions, errors, compaction,
//! many agents, ultrathink, test runs, debug turns and reading keep their animations
//! (`Tracker::session_view` decides where a rule applies). With no rules, nothing changes.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::time::SystemTime;

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::pet::IDLE_PICKS_NEW;
use crate::vocab::ascii_text;

/// The file's schema version.
pub const RULES_VERSION: u32 = 1;
/// The longest rule text (the clock shows ~20 characters on one line).
pub const RULE_TEXT_MAX: usize = 20;
/// At most this many rules (the settings list stays readable; the file stays tiny).
pub const MAX_RULES: usize = 64;
/// How often the Brain looks at the rules file's modification time, seconds.
pub const RULES_CHECK_EVERY: f64 = 2.0;
/// The rules file is never bigger than this (64 rules are ~6 KB).
const MAX_FILE: u64 = 256 * 1024;

/// The desk.pak's animations (`PROTOCOL.md`), in the order the settings list shows them.
pub const DESK_ANIMS: [&str; 31] = [
    "thinking",
    "ultrathink",
    "typing",
    "typing boss",
    "building",
    "wizard",
    "debugger",
    "headphones",
    "juggling",
    "error",
    "dizzy",
    "happy",
    "notification",
    "sweeping",
    "carrying",
    "reading",
    "idle",
    "look",
    "bubble",
    "coffee hand",
    "shy",
    "outlaw",
    "yawn",
    "sleeping",
    "wake",
    "low battery",
    "double jump",
    "heart eyes",
    "react left",
    "react right",
    "annoyed",
];

/// A skill name as written in a prompt, a `Skill` tool call or a skill folder.
static SKILL_TOKEN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z0-9][A-Za-z0-9_.:-]{0,79}$").expect("SKILL_TOKEN"));

/// The one spelling a skill is compared by: lower case, no leading `/` or `skill:`, and `:`
/// written as `-` (`/ak:cook`, `ak-cook`, `skill:ak-cook` are all `ak-cook`). Empty when the
/// name does not look like a skill.
pub fn normalize_skill(name: &str) -> String {
    let name = name.trim().trim_start_matches('/');
    let name = name
        .get(..6)
        .filter(|p| p.eq_ignore_ascii_case("skill:"))
        .map_or(name, |_| &name[6..]);
    if !SKILL_TOKEN.is_match(name) {
        return String::new();
    }
    name.to_ascii_lowercase().replace(':', "-")
}

/// One rule as stored and as the settings window sends it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillRule {
    pub skill: String,
    pub anim: String,
    /// Empty = the text Clawd would show anyway ("Editing", "Running", "Thinking"...).
    #[serde(default)]
    pub text: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct RulesFile {
    version: u32,
    #[serde(default)]
    rules: Vec<SkillRule>,
}

/// Check and clean one rule: the skill normalized, a desk animation, the text folded to ASCII
/// and capped. `Err` says what is wrong (Vietnamese, for the settings window).
pub fn clean_rule(rule: &SkillRule) -> Result<SkillRule, String> {
    let skill = normalize_skill(&rule.skill);
    if skill.is_empty() {
        return Err(format!("Tên skill không hợp lệ: \"{}\"", rule.skill.trim()));
    }
    let anim = rule.anim.trim();
    if !DESK_ANIMS.contains(&anim) {
        return Err(format!(
            "Đồng hồ không có animation \"{anim}\" (skill {skill})"
        ));
    }
    Ok(SkillRule {
        skill,
        anim: anim.to_owned(),
        text: ascii_text(&rule.text, RULE_TEXT_MAX),
    })
}

/// The rules the Tracker looks up: one per skill (the first wins).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SkillRules {
    rules: Vec<SkillRule>,
    by_skill: HashMap<String, usize>,
}

impl SkillRules {
    /// Clean every rule; a bad or duplicate rule is skipped and reported, never the whole set.
    pub fn from_rules(rules: &[SkillRule]) -> (Self, Vec<String>) {
        let mut out = Self::default();
        let mut problems = Vec::new();
        for rule in rules {
            if out.rules.len() >= MAX_RULES {
                problems.push(format!("Tối đa {MAX_RULES} luật; bỏ qua phần còn lại"));
                break;
            }
            match clean_rule(rule) {
                Ok(rule) if out.by_skill.contains_key(&rule.skill) => {
                    problems.push(format!("Skill {} có hai luật; dùng luật đầu", rule.skill));
                }
                Ok(rule) => {
                    out.by_skill.insert(rule.skill.clone(), out.rules.len());
                    out.rules.push(rule);
                }
                Err(why) => problems.push(why),
            }
        }
        (out, problems)
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    pub fn rules(&self) -> &[SkillRule] {
        &self.rules
    }

    /// The rule for a (normalized) skill. `desk_new` false (a clock with an older desk.pak):
    /// a rule naming an animation that pak lacks does not apply, so the clock never falls back
    /// to its first animation.
    pub fn get(&self, skill: &str, desk_new: bool) -> Option<&SkillRule> {
        let rule = &self.rules[*self.by_skill.get(skill)?];
        if !desk_new && IDLE_PICKS_NEW.contains(&rule.anim.as_str()) {
            return None;
        }
        Some(rule)
    }
}

/// Parse the rules file's text: `(rules, problems)`. Unreadable JSON or another version gives no
/// rules and one problem.
pub fn parse_rules(text: &str) -> (SkillRules, Vec<String>) {
    match serde_json::from_str::<RulesFile>(text) {
        Ok(file) if file.version == RULES_VERSION => SkillRules::from_rules(&file.rules),
        Ok(file) => (
            SkillRules::default(),
            vec![format!(
                "File luật phiên bản {} chưa được hỗ trợ",
                file.version
            )],
        ),
        Err(e) => (SkillRules::default(), vec![format!("File luật hỏng: {e}")]),
    }
}

/// Read the rules file; a missing file is no rules and no problem.
pub fn load_rules(path: &Path) -> (SkillRules, Vec<String>) {
    match std::fs::metadata(path) {
        Err(_) => return (SkillRules::default(), Vec::new()),
        Ok(m) if m.len() > MAX_FILE => {
            return (SkillRules::default(), vec!["File luật quá lớn".to_owned()]);
        }
        Ok(_) => {}
    }
    match std::fs::read_to_string(path) {
        Ok(text) => parse_rules(&text),
        Err(e) => (
            SkillRules::default(),
            vec![format!("Không đọc được file luật: {e}")],
        ),
    }
}

/// The file text for a set of rules (pretty JSON, ends with a newline).
pub fn rules_json(rules: &SkillRules) -> String {
    let file = RulesFile {
        version: RULES_VERSION,
        rules: rules.rules.clone(),
    };
    let mut text = serde_json::to_string_pretty(&file).unwrap_or_default();
    text.push('\n');
    text
}

/// Write the rules file (through a temp file and a rename, so a reader never sees half of it).
/// No rules: the file is removed.
pub fn save_rules(path: &Path, rules: &SkillRules) -> std::io::Result<()> {
    if rules.is_empty() {
        return match std::fs::remove_file(path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        };
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, rules_json(rules))?;
    std::fs::rename(&tmp, path)
}

/// Rules from the settings window's JSON (`[{skill, anim, text}]`): all good, or the first
/// problem. The window must not save a rule the Tracker would then skip.
pub fn rules_from_value(value: &Value) -> Result<SkillRules, String> {
    let list: Vec<SkillRule> = serde_json::from_value(value.clone())
        .map_err(|_| "Dữ liệu luật không hợp lệ".to_owned())?;
    let (rules, problems) = SkillRules::from_rules(&list);
    match problems.into_iter().next() {
        Some(problem) => Err(problem),
        None => Ok(rules),
    }
}

/// The skills installed in `dirs` (each a folder of `<name>/SKILL.md`), normalized, sorted,
/// without duplicates. For the settings window's suggestions; a user may type any other name.
pub fn find_skills(dirs: &[PathBuf]) -> Vec<String> {
    let mut found: Vec<String> = dirs
        .iter()
        .filter_map(|dir| std::fs::read_dir(dir).ok())
        .flatten()
        .filter_map(Result::ok)
        .filter(|entry| entry.path().join("SKILL.md").is_file())
        .map(|entry| normalize_skill(&entry.file_name().to_string_lossy()))
        .filter(|name| !name.is_empty())
        .collect();
    found.sort();
    found.dedup();
    found
}

/// The Brain's view of the rules file: re-read when its modification time or size changes,
/// looked at no more than every [`RULES_CHECK_EVERY`] seconds.
#[derive(Debug)]
pub struct RulesWatch {
    path: PathBuf,
    checked: f64,
    stamp: Option<(SystemTime, u64)>,
}

impl RulesWatch {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            checked: f64::NEG_INFINITY,
            stamp: None,
        }
    }

    /// New rules when the file changed since the last look (`Some(empty)` once it is removed).
    pub fn poll(&mut self, now: f64) -> Option<SkillRules> {
        if now - self.checked < RULES_CHECK_EVERY && now >= self.checked {
            return None;
        }
        self.checked = now;
        let stamp = std::fs::metadata(&self.path)
            .ok()
            .map(|m| (m.modified().unwrap_or(SystemTime::UNIX_EPOCH), m.len()));
        if stamp == self.stamp {
            return None;
        }
        self.stamp = stamp;
        let (rules, problems) = load_rules(&self.path);
        for problem in problems {
            tracing::warn!("Skill animation rules: {problem}");
        }
        Some(rules)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn rule(skill: &str, anim: &str, text: &str) -> SkillRule {
        SkillRule {
            skill: skill.into(),
            anim: anim.into(),
            text: text.into(),
        }
    }

    #[test]
    fn skill_names_compare_in_one_spelling() {
        for (raw, want) in [
            ("ak-cook", "ak-cook"),
            ("/ak:cook", "ak-cook"),
            ("AK:Cook", "ak-cook"),
            ("skill:ak-cook", "ak-cook"),
            ("/skill:ak-cook", "ak-cook"),
            (" plugin:review ", "plugin-review"),
            ("", ""),
            ("/", ""),
            ("/Users/me/x", ""),
            ("a b", ""),
            ("-x", ""),
            (&"a".repeat(81), ""),
        ] {
            assert_eq!(normalize_skill(raw), want, "{raw:?}");
        }
    }

    #[test]
    fn rules_are_cleaned_and_bad_ones_skipped() {
        let (rules, problems) = SkillRules::from_rules(&[
            rule("/ak:cook", " wizard ", "Nấu ăn thật lâu lắm luôn đó nha"),
            rule("ak-cook", "happy", ""),
            rule("x y", "happy", ""),
            rule("ak-plan", "moonwalk", ""),
            rule("ak-plan", "carrying", ""),
        ]);
        assert_eq!(
            rules.rules(),
            &[
                rule("ak-cook", "wizard", "Nau an that lau l..."),
                rule("ak-plan", "carrying", ""),
            ]
        );
        assert_eq!(problems.len(), 3, "{problems:?}");
        assert_eq!(rules.get("ak-cook", true).unwrap().anim, "wizard");
        assert!(rules.get("ak-advise", true).is_none());
    }

    #[test]
    fn newer_animations_wait_for_a_newer_desk_pak() {
        let (rules, _) = SkillRules::from_rules(&[rule("ak-cook", "coffee hand", "")]);
        assert!(rules.get("ak-cook", true).is_some());
        assert!(rules.get("ak-cook", false).is_none());
    }

    #[test]
    fn file_round_trip_and_versions() {
        let (rules, _) = SkillRules::from_rules(&[rule("ak-cook", "wizard", "Cooking")]);
        let text = rules_json(&rules);
        assert_eq!(parse_rules(&text), (rules, Vec::new()));
        let (none, problems) = parse_rules(r#"{"version":2,"rules":[]}"#);
        assert!(none.is_empty() && problems.len() == 1);
        let (none, problems) = parse_rules("not json");
        assert!(none.is_empty() && problems.len() == 1);
    }

    #[test]
    fn settings_payload_is_all_or_nothing() {
        let ok = rules_from_value(&json!([{"skill": "ak:cook", "anim": "wizard"}])).unwrap();
        assert_eq!(ok.rules(), &[rule("ak-cook", "wizard", "")]);
        assert!(rules_from_value(&json!([{"skill": "ak:cook", "anim": "nope"}])).is_err());
        assert!(rules_from_value(&json!({"skill": "x"})).is_err());
        assert!(rules_from_value(&json!([])).unwrap().is_empty());
    }

    #[test]
    fn save_load_watch_and_find() {
        let dir = std::env::temp_dir().join(format!("clawd-rules-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("animation-rules.json");
        let mut watch = RulesWatch::new(path.clone());
        assert_eq!(watch.poll(0.0), None, "no file yet: the empty rules stand");
        assert_eq!(watch.poll(1.0), None, "checked recently");
        assert_eq!(watch.poll(3.0), None, "still missing");

        let (rules, _) = SkillRules::from_rules(&[rule("ak-cook", "wizard", "Cooking")]);
        save_rules(&path, &rules).unwrap();
        assert_eq!(load_rules(&path), (rules.clone(), Vec::new()));
        assert_eq!(watch.poll(6.0), Some(rules));
        assert_eq!(watch.poll(9.0), None, "unchanged");
        save_rules(&path, &SkillRules::default()).unwrap();
        assert!(!path.exists());
        assert_eq!(watch.poll(12.0), Some(SkillRules::default()));

        let skills = dir.join("skills");
        for name in ["ak-cook", "ak:plan", "notes"] {
            std::fs::create_dir_all(skills.join(name)).unwrap();
        }
        std::fs::write(skills.join("ak-cook").join("SKILL.md"), "x").unwrap();
        std::fs::write(skills.join("ak:plan").join("SKILL.md"), "x").unwrap();
        assert_eq!(
            find_skills(&[skills.clone(), skills, dir.join("missing")]),
            vec!["ak-cook".to_owned(), "ak-plan".to_owned()]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
