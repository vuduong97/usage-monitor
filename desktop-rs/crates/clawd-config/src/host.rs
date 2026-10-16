//! The two facts about the machine the token readers need and Python took from globals:
//! `sys.platform == "darwin"` and `getpass.getuser()`. Injected so every OS can test the macOS
//! Keychain paths.

use clawd_contracts::Env;

/// Which OS the Keychain code paths run for, and the account name `security -a` is given.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Host {
    /// `sys.platform == "darwin"`.
    pub macos: bool,
    /// `getpass.getuser()`; empty when it cannot be determined (the Keychain read is then skipped).
    pub user: String,
}

impl Host {
    /// This machine: macOS by target OS, user from the process environment.
    pub fn system() -> Self {
        Self::from_env(cfg!(target_os = "macos"), &Env::system())
    }

    /// `getpass.getuser()`: the first non-empty of `LOGNAME`, `USER`, `LNAME`, `USERNAME`.
    pub fn from_env(macos: bool, env: &Env) -> Self {
        let user = ["LOGNAME", "USER", "LNAME", "USERNAME"]
            .iter()
            .find_map(|name| env.get(name))
            .unwrap_or_default();
        Self { macos, user }
    }

    /// An explicit host (tests).
    pub fn new(macos: bool, user: impl Into<String>) -> Self {
        Self {
            macos,
            user: user.into(),
        }
    }
}
