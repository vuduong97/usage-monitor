//! `clawd-config`: the "read the sources" half of `usage.py` (phase 06 implements it).
//!
//! Python origin:
//! `usage.py:44-330, 417-545, 753-780, 969-980` (see `desktop-rs/PORT-MAP.md`). No network here:
//! the poller (`clawd-usage`, phase 07) uses these to find a token, the engine and the agents
//! use them for ports, dirs and targets.
//!
//! Python read the module-global `CONFIG_FILE`; every function here takes the injected
//! [`clawd_contracts::AppPaths`] instead. Readers are forgiving: a read error yields `None` /
//! the default, never a panic.

pub mod cliproxy;
pub mod command;
pub mod config_file;
pub mod dirs;
pub mod http_targets;
pub mod locale;
pub mod tokens;

mod pytext;

pub use cliproxy::{read_cliproxy_key, read_cliproxy_url};
pub use command::{CommandOutput, CommandRunner, SystemRunner};
pub use config_file::{
    read_chime_setting, read_clock_setting, read_config_value, read_onoff_setting,
    read_usage_source,
};
pub use dirs::read_config_dirs;
pub use http_targets::{normalize_http_target, read_http_targets};
pub use locale::detect_hour_format;
pub use tokens::{has_claude_token, read_token_for};
