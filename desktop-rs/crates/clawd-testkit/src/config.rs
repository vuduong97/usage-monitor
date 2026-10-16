//! Fakes for `clawd-config` tests (phase 06 owns this file): a scripted [`CommandRunner`] for the
//! macOS `security` / `defaults` CLIs. Use it from `tests/*.rs` only (PORT-MAP rule: a crate's
//! own testkit fakes never go into its `#[cfg(test)]` modules).

use std::io;
use std::sync::Mutex;
use std::time::Duration;

use clawd_config::{CommandOutput, CommandRunner};

/// What a scripted program answers.
#[derive(Debug, Clone)]
pub enum FakeReply {
    /// The program ran: exit status, stdout, stderr.
    Output {
        status: i32,
        stdout: String,
        stderr: String,
    },
    /// The program could not run (`NotFound` = not installed, `TimedOut` = killed after the timeout).
    Error(io::ErrorKind),
}

impl FakeReply {
    /// Exit 0 printing `stdout`.
    pub fn ok(stdout: &str) -> Self {
        Self::Output {
            status: 0,
            stdout: stdout.to_owned(),
            stderr: String::new(),
        }
    }

    /// A non-zero exit (`security` answers 44 for "item not found").
    pub fn exit(status: i32, stderr: &str) -> Self {
        Self::Output {
            status,
            stdout: String::new(),
            stderr: stderr.to_owned(),
        }
    }

    /// The program does not exist on this machine.
    pub fn not_found() -> Self {
        Self::Error(io::ErrorKind::NotFound)
    }

    /// The program hung until the timeout.
    pub fn timed_out() -> Self {
        Self::Error(io::ErrorKind::TimedOut)
    }
}

/// One call a [`FakeRunner`] saw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedCommand {
    pub program: String,
    pub args: Vec<String>,
    pub timeout: Duration,
}

type Matcher = Box<dyn Fn(&str, &[&str]) -> bool + Send + Sync>;

/// A [`CommandRunner`] that never starts a process: the first rule whose matcher accepts
/// `(program, args)` answers; with no matching rule the program "does not exist"
/// (`NotFound`, like `security` on Windows). Every call is recorded.
#[derive(Default)]
pub struct FakeRunner {
    rules: Mutex<Vec<(Matcher, FakeReply)>>,
    calls: Mutex<Vec<RecordedCommand>>,
}

impl FakeRunner {
    pub fn new() -> Self {
        Self::default()
    }

    /// Answer `reply` to exactly `program args...`.
    pub fn on(self, program: &str, args: &[&str], reply: FakeReply) -> Self {
        let program = program.to_owned();
        let args: Vec<String> = args.iter().map(|a| (*a).to_owned()).collect();
        self.on_match(
            move |p, a| p == program && a.iter().copied().eq(args.iter().map(String::as_str)),
            reply,
        )
    }

    /// Answer `reply` when `matcher(program, args)`.
    pub fn on_match(
        self,
        matcher: impl Fn(&str, &[&str]) -> bool + Send + Sync + 'static,
        reply: FakeReply,
    ) -> Self {
        self.rules
            .lock()
            .expect("rules")
            .push((Box::new(matcher), reply));
        self
    }

    /// `security find-generic-password -s <service> -a <account> -w` answers `reply`
    /// (Claude Code's own item: [`clawd_config::tokens::KEYCHAIN_SERVICE`]).
    pub fn keychain_item(self, service: &str, account: &str, reply: FakeReply) -> Self {
        self.on(
            "security",
            &["find-generic-password", "-s", service, "-a", account, "-w"],
            reply,
        )
    }

    /// `security find-generic-password -s <service> -w` (no account: the CLIProxy key).
    pub fn keychain_service(self, service: &str, reply: FakeReply) -> Self {
        self.on(
            "security",
            &["find-generic-password", "-s", service, "-w"],
            reply,
        )
    }

    /// `defaults read -g <key>` answers `reply`.
    pub fn defaults(self, key: &str, reply: FakeReply) -> Self {
        self.on("defaults", &["read", "-g", key], reply)
    }

    /// Every call so far, in order.
    pub fn calls(&self) -> Vec<RecordedCommand> {
        self.calls.lock().expect("calls").clone()
    }

    /// The arguments of every call to `program`, in order.
    pub fn args_of(&self, program: &str) -> Vec<Vec<String>> {
        self.calls()
            .into_iter()
            .filter(|call| call.program == program)
            .map(|call| call.args)
            .collect()
    }
}

impl CommandRunner for FakeRunner {
    fn run(&self, program: &str, args: &[&str], timeout: Duration) -> io::Result<CommandOutput> {
        self.calls.lock().expect("calls").push(RecordedCommand {
            program: program.to_owned(),
            args: args.iter().map(|a| (*a).to_owned()).collect(),
            timeout,
        });
        let rules = self.rules.lock().expect("rules");
        let reply = rules
            .iter()
            .find(|(matcher, _)| matcher(program, args))
            .map(|(_, reply)| reply.clone())
            .unwrap_or_else(FakeReply::not_found);
        match reply {
            FakeReply::Output {
                status,
                stdout,
                stderr,
            } => Ok(CommandOutput {
                status,
                stdout,
                stderr,
            }),
            FakeReply::Error(kind) => Err(io::Error::new(kind, "fake runner")),
        }
    }
}
