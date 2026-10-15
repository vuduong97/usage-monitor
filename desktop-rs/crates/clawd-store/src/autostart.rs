//! `autostart.py`: start the app at login, per user and without admin rights (red team #1).
//!
//! * Windows: `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`, value `Clawd` = `"<exe>"`
//!   with **no arguments** (and no `--autostart`).
//! * macOS: `~/Library/LaunchAgents/vn.clawd.desktop.plist`, a byte-copy of Python's
//!   [`launch_agent`] dict: `Label`, `ProgramArguments = [exe]`, `RunAtLoad`,
//!   `KeepAlive {SuccessfulExit: false}`, `ThrottleInterval 30`, `ProcessType Interactive`.
//!
//! Written by hand (`windows` crate registry calls, `plist`); no `auto-launch` crate (it also
//! writes `StartupApproved` and would add a second launcher). The exe path does not change from
//! 0.1.4 (`{localappdata}\Programs\Clawd\Clawd.exe`, `/Applications/Clawd.app/Contents/MacOS/
//! Clawd`), so no migration step exists.

use std::path::PathBuf;

use clawd_contracts::{AppPaths, OsStoreNames, Platform};

/// `autostart.command()`: how to start the app: `[<this executable>]`.
pub fn command() -> Vec<String> {
    todo!("phase 03")
}

/// `autostart.launch_agent(cmd)` for LaunchAgent label `label`. Unlike Python it never adds
/// `WorkingDirectory` (the Rust app is always a packaged binary).
pub fn launch_agent(label: &str, cmd: &[String]) -> plist::Dictionary {
    todo!("phase 03")
}

/// The Run value data for `cmd`: every element quoted and joined by a space (`"<exe>"`), as
/// `" ".join(f'"{c}"' for c in cmd)` in `autostart.enable`.
pub fn run_value(cmd: &[String]) -> String {
    todo!("phase 03")
}

/// One autostart entry of one user, with every OS-visible name and path injectable.
#[derive(Debug, Clone)]
pub struct Autostart {
    pub names: OsStoreNames,
    pub home: PathBuf,
    pub platform: Platform,
    /// What the entry launches (`[exe]`).
    pub command: Vec<String>,
}

impl Autostart {
    /// This machine and this executable: `platform = Platform::current()`, `command = command()`.
    pub fn new(paths: &AppPaths, names: OsStoreNames) -> Self {
        todo!("phase 03")
    }

    /// `autostart.supported(platform)`: macOS and Windows only.
    pub fn supported(&self) -> bool {
        todo!("phase 03")
    }

    /// `autostart.is_enabled(platform)`: the plist exists / the Run value exists.
    pub fn is_enabled(&self) -> bool {
        todo!("phase 03")
    }

    /// `autostart.enable(platform)`: write the plist / Run value. False where unsupported.
    pub fn enable(&self) -> bool {
        todo!("phase 03")
    }

    /// `autostart.disable(platform)`: remove them. False where unsupported.
    pub fn disable(&self) -> bool {
        todo!("phase 03")
    }

    /// `autostart.set_enabled(on, platform)`.
    pub fn set_enabled(&self, on: bool) -> bool {
        todo!("phase 03")
    }
}
