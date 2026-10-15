use crate::consts::{KEYRING_SERVICE, LAUNCH_AGENT_LABEL, RUN_VALUE};

/// The three names the app leaves in OS-wide stores. Production values are frozen; tests inject
/// unique ones so they never read or write the developer's real keychain, Run key or
/// LaunchAgents (red team #14). `clawd-store` tests refuse to run when [`Self::is_production`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OsStoreNames {
    /// Keyring service (Windows: credential TargetName). Production `vn.clawd.desktop`.
    pub keyring_service: String,
    /// HKCU Run value name. Production `Clawd`.
    pub run_value: String,
    /// LaunchAgent label / plist file stem. Production `vn.clawd.desktop`.
    pub launch_label: String,
}

impl OsStoreNames {
    /// The frozen production names.
    pub fn production() -> Self {
        Self {
            keyring_service: KEYRING_SERVICE.to_owned(),
            run_value: RUN_VALUE.to_owned(),
            launch_label: LAUNCH_AGENT_LABEL.to_owned(),
        }
    }

    /// Names for one test run: `vn.clawd.desktop.test-<suffix>`, `ClawdTest-<suffix>`,
    /// `vn.clawd.desktop.test-<suffix>` (`suffix` is a uuid or any unique string).
    pub fn for_test(suffix: &str) -> Self {
        Self {
            keyring_service: format!("{KEYRING_SERVICE}.test-{suffix}"),
            run_value: format!("ClawdTest-{suffix}"),
            launch_label: format!("{LAUNCH_AGENT_LABEL}.test-{suffix}"),
        }
    }

    /// True when any name equals the production one.
    pub fn is_production(&self) -> bool {
        self.keyring_service == KEYRING_SERVICE
            || self.run_value == RUN_VALUE
            || self.launch_label == LAUNCH_AGENT_LABEL
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_names_are_never_production() {
        assert!(OsStoreNames::production().is_production());
        assert!(!OsStoreNames::for_test("abc").is_production());
        assert_eq!(OsStoreNames::for_test("abc").run_value, "ClawdTest-abc");
    }
}
