//! Settings commands: skill animation rules ("while skill X runs, Clawd plays Y"). Rust-only, no
//! Python origin. The rules live in `animation-rules.json` on this machine (never on the clock);
//! the engine's Brain follows the file, so a save applies without a restart.
//! Each takes the positional JS `args` and returns `{"ok": true, ...}` or
//! `{"ok": false, "code", "error"}`; `Err` rejects the promise.

use std::path::{Path, PathBuf};
use std::time::Duration;

use clawd_config::read_config_value;
use clawd_contracts::{AppPaths, Env};
use clawd_events::skill_rules::{
    find_skills, load_rules, parse_rules, rules_from_value, rules_json, save_rules, DESK_ANIMS,
    RULE_TEXT_MAX,
};
use clawd_events::{ascii_text, read_event_port};
use serde_json::{json, Value};

use super::backup::{cancelled, pick_json};
use super::session::{arg, fail, text_of, Bridge};
use super::CommandCtx;

/// An exported rules file is a few KB; anything bigger is not one.
const RULES_FILE_MAX: u64 = 256 * 1024;
/// How long the ▶ button's animation plays on the clock, seconds.
const PREVIEW_SECS: f64 = 5.0;
const POKE_TIMEOUT: Duration = Duration::from_secs(2);
const FILE_LABEL: &str = "Luật animation (*.json)";

/// Where installed skills live: Claude Code (`CLAUDE_CONFIG_DIR` or `~/.claude`), the shared
/// `~/.agents/skills`, and pi (`PI_CODING_AGENT_DIR` or `~/.pi/agent`).
fn skill_dirs(paths: &AppPaths, env: &Env) -> Vec<PathBuf> {
    let home = paths.home();
    let claude = env
        .get("CLAUDE_CONFIG_DIR")
        .filter(|d| !d.trim().is_empty())
        .map_or_else(|| home.join(".claude"), PathBuf::from);
    let pi = env
        .get("PI_CODING_AGENT_DIR")
        .filter(|d| !d.trim().is_empty())
        .map_or_else(|| home.join(".pi").join("agent"), PathBuf::from);
    vec![
        claude.join("skills"),
        home.join(".agents").join("skills"),
        pi.join("skills"),
    ]
}

/// What the settings list needs: the saved rules, the problems the file has, the clock's
/// animations, the ones an older desk.pak lacks, and the skills found on this machine.
fn rules_state(paths: &AppPaths, env: &Env) -> Value {
    let (rules, problems) = load_rules(&paths.skill_rules_file());
    json!({
        "ok": true,
        "rules": rules.rules(),
        "problems": problems,
        "anims": DESK_ANIMS,
        "newAnims": clawd_events::pet::IDLE_PICKS_NEW,
        "skills": find_skills(&skill_dirs(paths, env)),
        "textMax": RULE_TEXT_MAX,
    })
}

/// Save the window's list (`[{skill, anim, text}]`): all rules valid, or nothing is written.
fn save_list(paths: &AppPaths, list: &Value) -> Value {
    let rules = match rules_from_value(list) {
        Ok(rules) => rules,
        Err(why) => return fail("bad", &why),
    };
    if let Err(e) = save_rules(&paths.skill_rules_file(), &rules) {
        return fail("error", &format!("Không lưu được: {e}"));
    }
    json!({"ok": true, "rules": rules.rules()})
}

/// Replace the rules with those of an exported file; a file with any bad rule changes nothing.
fn import_from(paths: &AppPaths, file: &Path) -> Value {
    const NOT_RULES: &str = "File này không phải file luật animation của Clawd.";
    let too_big = std::fs::metadata(file).map_or(true, |m| m.len() > RULES_FILE_MAX);
    let Some(text) = (!too_big)
        .then(|| std::fs::read_to_string(file).ok())
        .flatten()
    else {
        return fail("bad", NOT_RULES);
    };
    let (rules, problems) = parse_rules(&text);
    if let Some(problem) = problems.into_iter().next() {
        return fail("bad", &problem);
    }
    if let Err(e) = save_rules(&paths.skill_rules_file(), &rules) {
        return fail("error", &format!("Không lưu được: {e}"));
    }
    json!({"ok": true, "rules": rules.rules()})
}

/// Write the saved rules to `file`.
fn export_to(paths: &AppPaths, file: &Path) -> Value {
    let (rules, _) = load_rules(&paths.skill_rules_file());
    if rules.is_empty() {
        return fail("empty", "Chưa có luật nào để xuất.");
    }
    match std::fs::write(file, rules_json(&rules)) {
        Ok(()) => json!({"ok": true, "path": file.display().to_string()}),
        Err(e) => fail("error", &format!("Lỗi: {e}")),
    }
}

/// The `play` poke for the ▶ button, or why not.
fn preview_cmd(anim: &str, text: &str) -> Result<Value, Value> {
    let anim = anim.trim();
    if !DESK_ANIMS.contains(&anim) {
        return Err(fail(
            "bad",
            &format!("Đồng hồ không có animation \"{anim}\""),
        ));
    }
    let text = ascii_text(text, RULE_TEXT_MAX);
    Ok(json!({"cmd": "play", "anim": anim, "text": text, "secs": PREVIEW_SECS}))
}

/// `skill_rules()`.
pub async fn skill_rules(ctx: &CommandCtx<'_>, _args: Vec<Value>) -> Result<Value, String> {
    let b = Bridge::of(ctx);
    let (paths, env) = (b.paths().clone(), b.env());
    Ok(b.blocking(move |_| rules_state(&paths, &env)).await)
}

/// `save_skill_rules(rules)`.
pub async fn save_skill_rules(ctx: &CommandCtx<'_>, args: Vec<Value>) -> Result<Value, String> {
    let b = Bridge::of(ctx);
    let (paths, list) = (b.paths().clone(), arg(&args, 0).clone());
    Ok(b.blocking(move |_| save_list(&paths, &list)).await)
}

/// `skill_anim_preview(anim, text)`: play it on the clock for a few seconds, through the Clawd
/// daemon on this machine (the one that pushes the clock's state).
pub async fn skill_anim_preview(ctx: &CommandCtx<'_>, args: Vec<Value>) -> Result<Value, String> {
    let b = Bridge::of(ctx);
    let cmd = match preview_cmd(&text_of(arg(&args, 0)), &text_of(arg(&args, 1))) {
        Ok(cmd) => cmd,
        Err(failure) => return Ok(failure),
    };
    let (paths, env) = (b.paths().clone(), b.env());
    let port = b
        .blocking(move |_| read_event_port(&env, &|k| read_config_value(&paths, k)))
        .await;
    Ok(match clawd_events::poke(port, &cmd, POKE_TIMEOUT).await {
        Ok(answer) => answer,
        Err(_) => fail(
            "offline",
            "Clawd chưa nhận sự kiện agent (tắt trong cài đặt hoặc chưa chạy), nên chưa phát thử được.",
        ),
    })
}

/// `export_skill_rules()`: save the rules to a file the user picks.
pub async fn export_skill_rules(ctx: &CommandCtx<'_>, _args: Vec<Value>) -> Result<Value, String> {
    let b = Bridge::of(ctx);
    let Some(file) = pick_json(
        ctx.app,
        Some("clawd-skill-animations.json".into()),
        FILE_LABEL,
    )
    .await
    else {
        return Ok(cancelled());
    };
    let paths = b.paths().clone();
    Ok(b.blocking(move |_| export_to(&paths, &file)).await)
}

/// `import_skill_rules()`: replace the rules with a file the user picks.
pub async fn import_skill_rules(ctx: &CommandCtx<'_>, _args: Vec<Value>) -> Result<Value, String> {
    let b = Bridge::of(ctx);
    let Some(file) = pick_json(ctx.app, None, FILE_LABEL).await else {
        return Ok(cancelled());
    };
    let paths = b.paths().clone();
    Ok(b.blocking(move |_| import_from(&paths, &file)).await)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn temp() -> (tempfile::TempDir, AppPaths) {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(dir.path());
        (dir, paths)
    }

    #[test]
    fn save_then_state_then_export_and_import() {
        let (dir, paths) = temp();
        let skills = dir.path().join(".agents").join("skills").join("ak-cook");
        std::fs::create_dir_all(&skills).unwrap();
        std::fs::write(skills.join("SKILL.md"), "x").unwrap();

        let bad = save_list(&paths, &json!([{"skill": "ak-cook", "anim": "moonwalk"}]));
        assert_eq!(bad["code"], "bad");
        assert!(!paths.skill_rules_file().exists(), "nothing written");

        let ok = save_list(
            &paths,
            &json!([{"skill": "/ak:cook", "anim": "wizard", "text": "Nấu"}]),
        );
        assert_eq!(
            ok["rules"],
            json!([{"skill": "ak-cook", "anim": "wizard", "text": "Nau"}])
        );

        let state = rules_state(&paths, &Env::empty());
        assert_eq!(state["rules"], ok["rules"]);
        assert_eq!(state["skills"], json!(["ak-cook"]));
        assert_eq!(state["anims"].as_array().unwrap().len(), 31);
        assert_eq!(state["problems"], json!([]));

        let out = dir.path().join("out.json");
        assert_eq!(export_to(&paths, &out)["ok"], true);
        save_list(&paths, &json!([]));
        assert!(!paths.skill_rules_file().exists(), "no rules = no file");
        assert_eq!(export_to(&paths, &out)["code"], "empty");
        assert_eq!(import_from(&paths, &out)["rules"], ok["rules"]);

        std::fs::write(
            &out,
            "{\"version\":1,\"rules\":[{\"skill\":\"x\",\"anim\":\"nope\"}]}",
        )
        .unwrap();
        assert_eq!(import_from(&paths, &out)["code"], "bad");
        assert_eq!(
            rules_state(&paths, &Env::empty())["rules"],
            ok["rules"],
            "a bad import changes nothing"
        );
        assert_eq!(
            import_from(&paths, &dir.path().join("missing"))["code"],
            "bad"
        );
    }

    #[test]
    fn skill_dirs_follow_agent_env_overrides() {
        let paths = AppPaths::new("/h");
        let env = Env::from_map(HashMap::from([
            ("CLAUDE_CONFIG_DIR".to_owned(), "/c".to_owned()),
            ("PI_CODING_AGENT_DIR".to_owned(), "/p".to_owned()),
        ]));
        assert_eq!(
            skill_dirs(&paths, &env),
            vec![
                PathBuf::from("/c/skills"),
                PathBuf::from("/h/.agents/skills"),
                PathBuf::from("/p/skills")
            ]
        );
        assert_eq!(
            skill_dirs(&paths, &Env::empty())[0],
            PathBuf::from("/h/.claude/skills")
        );
    }

    #[test]
    fn preview_plays_a_desk_animation_for_a_few_seconds() {
        assert_eq!(
            preview_cmd(" wizard ", "Nấu ăn").unwrap(),
            json!({"cmd": "play", "anim": "wizard", "text": "Nau an", "secs": 5.0})
        );
        assert_eq!(preview_cmd("moonwalk", "").unwrap_err()["code"], "bad");
    }
}
