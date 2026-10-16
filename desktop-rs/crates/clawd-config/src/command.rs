//! Running an external program (`security find-generic-password`, `defaults read`) behind a
//! trait so the token and locale readers are testable on every OS with a fake.

use std::io::{self, Read};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

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

/// `CREATE_NO_WINDOW`: a tray app must never flash a console.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

impl CommandRunner for SystemRunner {
    fn run(&self, program: &str, args: &[&str], timeout: Duration) -> io::Result<CommandOutput> {
        let mut command = Command::new(program);
        command
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(CREATE_NO_WINDOW);
        }
        let mut child = command.spawn()?;
        let stdout = drain(child.stdout.take());
        let stderr = drain(child.stderr.take());
        let deadline = Instant::now() + timeout;
        let status = loop {
            if let Some(status) = child.try_wait()? {
                break status;
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    format!("{program} timed out after {timeout:?}"),
                ));
            }
            thread::sleep(Duration::from_millis(10));
        };
        Ok(CommandOutput {
            status: status.code().unwrap_or(-1),
            stdout: join_text(stdout),
            stderr: join_text(stderr),
        })
    }
}

/// Read a pipe to its end on its own thread, so a chatty child never blocks on a full pipe.
fn drain<R: Read + Send + 'static>(pipe: Option<R>) -> thread::JoinHandle<Vec<u8>> {
    thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(mut pipe) = pipe {
            let _ = pipe.read_to_end(&mut bytes);
        }
        bytes
    })
}

fn join_text(reader: thread::JoinHandle<Vec<u8>>) -> String {
    String::from_utf8_lossy(&reader.join().unwrap_or_default()).into_owned()
}
