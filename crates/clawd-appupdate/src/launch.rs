//! Starting the installer / the swap script **detached**: the process must outlive the app,
//! which quits right after `Updater::apply` returns `(true, "")`. Behind a trait so tests record
//! the command instead of running it.

use std::io;
use std::path::PathBuf;

/// What is being started (decides how it is detached).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchKind {
    /// The Inno Setup installer (Windows): `DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP`.
    Installer,
    /// `/bin/sh swap.sh ...` (macOS): started with `process_group(0)` (its own process group,
    /// not a new session as Python's `start_new_session`), so launchd does not take it down
    /// with the app. Phase 14 must verify on a real Mac that it survives the app quitting.
    SwapScript,
}

/// `DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP` (`0x8 | 0x200`), the Windows creation flags.
pub const WINDOWS_DETACHED_FLAGS: u32 = 0x0000_0008 | 0x0000_0200;

/// One program to start: no shell, no inherited stdio.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchSpec {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub kind: LaunchKind,
}

/// Starts a program and does not wait for it.
pub trait Launcher: Send + Sync {
    fn spawn_detached(&self, spec: &LaunchSpec) -> io::Result<()>;
}

/// The real thing: `std::process::Command` with stdio closed.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemLauncher;

impl Launcher for SystemLauncher {
    fn spawn_detached(&self, spec: &LaunchSpec) -> io::Result<()> {
        use std::process::{Command, Stdio};

        let mut command = Command::new(&spec.program);
        command
            .args(&spec.args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            if spec.kind == LaunchKind::Installer {
                command.creation_flags(WINDOWS_DETACHED_FLAGS);
            }
        }
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        command.spawn().map(|_child| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_flags_are_detached_process_and_new_process_group() {
        assert_eq!(WINDOWS_DETACHED_FLAGS, 0x208);
    }
}
