use std::path::PathBuf;

use clawd_contracts::AppPaths;

/// `usage.read_config_dirs()`: the Claude config dirs to poll, from the `config_dirs` option
/// (comma list, `~` expanded); `[~/.claude]` when unset or empty. Also what the Claude Code
/// hook installer edits (`settings.json` in each).
pub fn read_config_dirs(paths: &AppPaths) -> Vec<PathBuf> {
    todo!("phase 06")
}
