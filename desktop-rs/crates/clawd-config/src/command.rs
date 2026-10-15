//! Running an external program (`security find-generic-password`, `defaults read`) behind a
//! trait so the token and locale readers are testable on every OS with a fake.

use std::io;
use std::time::Duration;

/// What a finished program printed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    /// Exit code (`-1` when killed by a signal).
    pub status: i32,
    pub stdout: String,
    pub stderr: String,
}

/// `subprocess.run(args, capture_output=True, text=True, timeout=...)`.
pub trait CommandRunner: Send + Sync {
    /// Run `program args...`. `Err(NotFound)` when the program does not exist,
    /// `Err(TimedOut)` after `timeout` (the child is killed). A non-zero exit is `Ok`.
    fn run(&self, program: &str, args: &[&str], timeout: Duration) -> io::Result<CommandOutput>;
}

/// The real thing: `std::process::Command`, no console window on Windows.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemRunner;

impl CommandRunner for SystemRunner {
    fn run(&self, program: &str, args: &[&str], timeout: Duration) -> io::Result<CommandOutput> {
        todo!("phase 06")
    }
}
