//! Agents that load a plugin file from a directory (pi extensions, opencode plugins):
//! `agents/plugin.py`.
//!
//! The app carries the file ([`crate::assets::integration`]) and copies it in, so it works on a
//! machine that never saw this repo. A copy older than the app's is replaced at the next install
//! or app start (`refresh`). A symlink there is a developer's link to the repo
//! (`integrations/pi/install.sh`): it counts as installed and is left alone. A file of the same
//! name that is not ours is never overwritten.

use std::path::Path;

use clawd_contracts::AgentError;

/// `PluginAgent.is_installed()`: the file at `target` exists and contains `mark` (a line only
/// our plugin has).
pub fn is_installed(target: &Path, mark: &str) -> bool {
    todo!("phase 05")
}

/// `PluginAgent.needs_update()`: installed (not a symlink) and different from the bundled copy
/// `bundled` (a `paths.integration()` name such as `pi/clawd-home.ts`).
pub fn needs_update(target: &Path, mark: &str, bundled: &str) -> bool {
    todo!("phase 05")
}

/// `PluginAgent.install()`: copy the bundled plugin to `target` (atomic: temp file + replace).
/// Refuses (`AgentError`) a foreign file or a foreign symlink in the way.
pub fn install(target: &Path, mark: &str, bundled: &str) -> Result<String, AgentError> {
    todo!("phase 05")
}

/// `PluginAgent.uninstall()`: remove `target` when it is ours (or our symlink).
pub fn uninstall(target: &Path, mark: &str) -> Result<String, AgentError> {
    todo!("phase 05")
}

/// `PluginAgent.refresh()`: replace an older copy with the app's; true when it did.
pub fn refresh(target: &Path, mark: &str, bundled: &str) -> bool {
    todo!("phase 05")
}
