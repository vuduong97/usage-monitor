use std::path::{Component, Path, PathBuf};

use clawd_contracts::AppPaths;

use crate::config_file::config_entries;
use crate::pytext::strip;

/// `usage.read_config_dirs()`: the Claude config dirs to poll, from the `config_dirs` option
/// (comma list, `~` expanded); `[~/.claude]` when unset or empty. Also what the Claude Code
/// hook installer edits (`settings.json` in each). The LAST `config_dirs` line wins (Python
/// keeps looping), unlike [`crate::read_config_value`].
pub fn read_config_dirs(paths: &AppPaths) -> Vec<PathBuf> {
    let raw = config_entries(paths)
        .into_iter()
        .filter(|(key, _)| key == "config_dirs")
        .next_back()
        .map(|(_, value)| value)
        .unwrap_or_default();
    let dirs: Vec<PathBuf> = raw
        .split(',')
        .map(strip)
        .filter(|part| !part.is_empty())
        .map(|part| expand_user(paths, part))
        .collect();
    if dirs.is_empty() {
        vec![paths.default_claude_dir()]
    } else {
        dirs
    }
}

/// `Path(text).expanduser()` with the injected home: `~`, `~/x` (and `~\x` on Windows) lead
/// with the home dir; `~name` stays as written. Like `pathlib` the result drops `.` parts and
/// repeated separators.
fn expand_user(paths: &AppPaths, text: &str) -> PathBuf {
    let rest = text.strip_prefix('~').filter(|rest| {
        rest.is_empty() || rest.starts_with('/') || (cfg!(windows) && rest.starts_with('\\'))
    });
    match rest {
        Some(rest) => {
            let mut path = paths.home().to_path_buf();
            path.extend(Path::new(rest.trim_start_matches(['/', '\\'])).components());
            normalize(&path)
        }
        None => normalize(Path::new(text)),
    }
}

/// `pathlib` spelling of a path: no `.` components (but `.` for the empty path).
fn normalize(path: &Path) -> PathBuf {
    let normalized: PathBuf = path
        .components()
        .filter(|component| *component != Component::CurDir)
        .collect();
    if normalized.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        normalized
    }
}

/// `Path == Path`: same components (case-insensitive on Windows, like `PureWindowsPath`).
pub(crate) fn same_path(a: &Path, b: &Path) -> bool {
    let key = |path: &Path| -> Vec<String> {
        normalize(path)
            .components()
            .map(|component| {
                let part = component.as_os_str().to_string_lossy();
                if cfg!(windows) {
                    part.to_lowercase()
                } else {
                    part.into_owned()
                }
            })
            .collect()
    };
    key(a) == key(b)
}
