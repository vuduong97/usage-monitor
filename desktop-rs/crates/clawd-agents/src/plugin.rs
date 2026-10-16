//! Agents that load a plugin file from a directory (pi extensions, opencode plugins):
//! `agents/plugin.py`.
//!
//! The app carries the file ([`crate::assets::integration`]) and copies it in, so it works on a
//! machine that never saw this repo. A copy older than the app's is replaced at the next install
//! or app start (`refresh`). A symlink there is a developer's link to the repo
//! (`integrations/pi/install.sh`): it counts as installed and is left alone. A file of the same
//! name that is not ours is never overwritten.

use std::fs;
use std::path::Path;

use clawd_contracts::AgentError;

use crate::assets::integration;
use crate::hooks::universal_newlines;

/// `PluginAgent._text(p)`: the file as text with universal newlines, `None` when unreadable.
fn text_of(path: &Path) -> Option<String> {
    let bytes = fs::read(path).ok()?;
    String::from_utf8(bytes)
        .ok()
        .map(|t| universal_newlines(&t))
}

/// The bundled copy (`PluginAgent.source()` read as text).
fn bundled_text(bundled: &str) -> Option<String> {
    let bytes = integration(bundled)?;
    std::str::from_utf8(bytes).ok().map(universal_newlines)
}

fn is_symlink(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink())
}

/// `PluginAgent.is_installed()`: the file at `target` exists and contains `mark` (a line only
/// our plugin has).
pub fn is_installed(target: &Path, mark: &str) -> bool {
    target.exists() && text_of(target).is_some_and(|t| t.contains(mark))
}

/// `PluginAgent.needs_update()`: installed (not a symlink) and different from the bundled copy
/// `bundled` (a `paths.integration()` name such as `pi/clawd-home.ts`).
pub fn needs_update(target: &Path, mark: &str, bundled: &str) -> bool {
    if is_symlink(target) || !is_installed(target, mark) {
        return false;
    }
    text_of(target) != bundled_text(bundled)
}

/// `PluginAgent.install()`: copy the bundled plugin to `target` (atomic: temp file + replace).
/// Refuses (`AgentError`) a foreign file or a foreign symlink in the way.
pub fn install(target: &Path, mark: &str, bundled: &str) -> Result<String, AgentError> {
    let body = bundled_text(bundled)
        .ok_or_else(|| AgentError(format!("Bản cài thiếu file {bundled}.")))?;
    if is_symlink(target) {
        if is_installed(target, mark) {
            return Ok("Đã nối (liên kết tới mã nguồn).".to_owned());
        }
        return Err(AgentError(format!(
            "{} là một liên kết khác, Clawd không ghi đè.",
            target.display()
        )));
    }
    if target.exists() {
        let current = text_of(target);
        if !current.as_deref().is_some_and(|c| c.contains(mark)) {
            return Err(AgentError(format!(
                "{} đã có và không phải của Clawd, Clawd không ghi đè.",
                target.display()
            )));
        }
        if current.as_deref() == Some(body.as_str()) {
            return Ok("Đã nối.".to_owned());
        }
    }
    let io_err = |e: std::io::Error| AgentError(format!("{}: {e}", target.display()));
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(io_err)?;
    }
    let mut name = target.file_name().unwrap_or_default().to_owned();
    name.push(".clawd-tmp");
    let tmp = target.with_file_name(name);
    fs::write(&tmp, body).map_err(io_err)?;
    fs::rename(&tmp, target).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        io_err(e)
    })?;
    Ok("Đã nối.".to_owned())
}

/// `PluginAgent.uninstall()`: remove `target` when it is ours (or our symlink).
pub fn uninstall(target: &Path, mark: &str) -> Result<String, AgentError> {
    if is_symlink(target) || is_installed(target, mark) {
        match fs::remove_file(target) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(AgentError(format!("{}: {e}", target.display()))),
        }
        return Ok("Đã ngắt.".to_owned());
    }
    Ok("Chưa nối.".to_owned())
}

/// `PluginAgent.refresh()`: replace an older copy with the app's; true when it did.
pub fn refresh(target: &Path, mark: &str, bundled: &str) -> bool {
    needs_update(target, mark, bundled) && install(target, mark, bundled).is_ok()
}
