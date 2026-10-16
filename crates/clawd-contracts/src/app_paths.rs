use std::path::{Path, PathBuf};

use crate::consts::*;

/// Every path the app uses, rooted so tests can inject a temp dir (red team #14: no test and no
/// golden exporter may touch the real home).
///
/// `home` is Python's `Path.home()`; `config_dir` is `CONFIG_DIR` =
/// `<home>/.config/claude-usage-monitor` on every OS. Agent config dirs
/// (`~/.codex`, `~/.cursor`, ...) are derived from `home` by `clawd-agents`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppPaths {
    home: PathBuf,
    config_dir: PathBuf,
}

impl AppPaths {
    /// Everything under `home` (`<home>/.config/claude-usage-monitor/...`).
    pub fn new(home: impl Into<PathBuf>) -> Self {
        let home = home.into();
        let config_dir = home.join(".config").join(CONFIG_DIR_NAME);
        Self { home, config_dir }
    }

    /// Like [`Self::new`] but with the config dir set explicitly (tests that only care about it).
    pub fn with_config_dir(home: impl Into<PathBuf>, config_dir: impl Into<PathBuf>) -> Self {
        Self {
            home: home.into(),
            config_dir: config_dir.into(),
        }
    }

    /// The real machine: `%USERPROFILE%` on Windows (as Python's `Path.home()`), `$HOME`
    /// elsewhere. Falls back to the current directory when neither is set.
    pub fn system() -> Self {
        let var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
        let home = std::env::var_os(var)
            .or_else(|| std::env::var_os("HOME"))
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        Self::new(home)
    }

    /// `Path.home()`.
    pub fn home(&self) -> &Path {
        &self.home
    }

    /// `paths.CONFIG_DIR`.
    pub fn config_dir(&self) -> &Path {
        &self.config_dir
    }

    fn in_config(&self, name: &str) -> PathBuf {
        self.config_dir.join(name)
    }

    /// `paths.CONFIG_FILE`: optional `key = value` overrides (README), read by `clawd-config`.
    pub fn config_file(&self) -> PathBuf {
        self.in_config(CONFIG_FILE_NAME)
    }
    /// `paths.CLOCK_FILE`: the remembered clock.
    pub fn clock_file(&self) -> PathBuf {
        self.in_config(CLOCK_FILE_NAME)
    }
    /// `paths.PREFS_FILE`: `desktop.json`.
    pub fn prefs_file(&self) -> PathBuf {
        self.in_config(PREFS_FILE_NAME)
    }
    /// The lock file Python locks (`desktop.lock`); lock this one, never the JSON itself.
    pub fn prefs_lock_file(&self) -> PathBuf {
        self.in_config(PREFS_LOCK_FILE_NAME)
    }
    /// `paths.PAYLOAD_FILE`.
    pub fn payload_file(&self) -> PathBuf {
        self.in_config(PAYLOAD_FILE_NAME)
    }
    /// `paths.LOG_FILE`.
    pub fn log_file(&self) -> PathBuf {
        self.in_config(LOG_FILE_NAME)
    }
    /// The rotated log (`clawd-desktop.log.1`).
    pub fn log_rotated_file(&self) -> PathBuf {
        self.in_config(LOG_ROTATED_FILE_NAME)
    }
    /// `paths.PAK_FILE`: the clock's Clawd sprites read from the clock (preview).
    pub fn pak_file(&self) -> PathBuf {
        self.in_config(PAK_FILE_NAME)
    }
    /// `paths.RENDERER_DIR`.
    pub fn renderer_dir(&self) -> PathBuf {
        self.in_config(RENDERER_DIR_NAME)
    }
    /// `paths.HOOK_SCRIPT`: `clawd-event.sh`, copied here so app updates never break hooks.
    pub fn hook_script(&self) -> PathBuf {
        self.in_config(HOOK_SCRIPT_NAME)
    }
    /// `paths.OTA_BUSY_FILE`: exists while the settings window installs firmware (stale after
    /// 900 s). Read by the app updater only.
    pub fn ota_busy_file(&self) -> PathBuf {
        self.in_config(OTA_BUSY_FILE_NAME)
    }
    /// `pause.PAUSE_FILE`: the engine's push lease (<= 300 s). Not `ota.busy`.
    pub fn pause_file(&self) -> PathBuf {
        self.in_config(PAUSE_FILE_NAME)
    }
    /// `usage.ATTENTION_DIR` (same as the config dir).
    pub fn attention_dir(&self) -> &Path {
        &self.config_dir
    }
    /// `usage.ATTENTION_FLAG`.
    pub fn attention_flag(&self) -> PathBuf {
        self.in_config(ATTENTION_FILE_NAME)
    }
    /// The pet's per-day counters (`ATTENTION_DIR / "pet.json"`).
    pub fn pet_file(&self) -> PathBuf {
        self.in_config(PET_FILE_NAME)
    }
    /// `feed.CACHE_DIR`.
    pub fn feed_cache_dir(&self) -> PathBuf {
        self.in_config(FEED_CACHE_DIR_NAME)
    }
    /// `appupdate.STAGE_DIR` (`ready.json`, `swap.sh`, `apply.log`, `failed`).
    pub fn app_update_stage_dir(&self) -> PathBuf {
        self.in_config(APP_UPDATE_DIR_NAME)
    }

    /// `usage.DEFAULT_CONFIG_DIR`: `~/.claude`.
    pub fn default_claude_dir(&self) -> PathBuf {
        self.home.join(".claude")
    }
    /// `usage.ANTHROPIC_STORE_DIR`: `~/.config/anthropic` (Claude Code "storage v5").
    pub fn anthropic_store_dir(&self) -> PathBuf {
        self.home.join(".config").join("anthropic")
    }
    /// `autostart.PLIST` for a LaunchAgent label: `~/Library/LaunchAgents/<label>.plist`.
    pub fn launch_agent_plist(&self, label: &str) -> PathBuf {
        self.home
            .join("Library")
            .join("LaunchAgents")
            .join(format!("{label}.plist"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_file_lives_in_the_injected_root() {
        let p = AppPaths::new("/tmp/h");
        let cfg = Path::new("/tmp/h/.config/claude-usage-monitor");
        assert_eq!(p.config_dir(), cfg);
        assert_eq!(p.prefs_file(), cfg.join("desktop.json"));
        assert_eq!(p.prefs_lock_file(), cfg.join("desktop.lock"));
        assert_eq!(p.pause_file(), cfg.join("pause.json"));
        assert_eq!(p.ota_busy_file(), cfg.join("ota.busy"));
        assert_ne!(p.pause_file(), p.ota_busy_file());
        assert_eq!(
            p.anthropic_store_dir(),
            Path::new("/tmp/h/.config/anthropic")
        );
        assert_eq!(
            p.launch_agent_plist("vn.clawd.desktop"),
            Path::new("/tmp/h/Library/LaunchAgents/vn.clawd.desktop.plist")
        );
    }
}
