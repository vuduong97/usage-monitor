//! The command line (`cli.py`), parsed by hand (no argument-parsing dependency):
//!
//! ```text
//! Clawd                     tray app (default)
//! Clawd run                 headless: same work, no tray (Ctrl+C stops)
//! Clawd find [--seconds N]  list the clocks that answer mDNS on this network (no port scan)
//! Clawd hooks on|off        install / remove the Claude Code hooks
//! Clawd autostart on|off    start at login, or not
//! Clawd cleanup             what the uninstaller runs: autostart off, every agent disconnected
//! Clawd settings [--clock http://<clock>] [--key <name>] [--tab update|agents|setup]
//!                           the settings window alone (no tray, no engine, no event server)
//! Clawd --version
//! ```
//!
//! `-psn_*` (macOS passes it when Finder opens an app on old systems) is dropped. There is no
//! `--autostart` flag. Subcommands that print attach to the parent console first (Windows GUI
//! executable).

use std::io::Write;

use clawd_contracts::{AppPaths, ClockTarget, APP_NAME, APP_VERSION};
use serde_json::Value;

/// Default of `find --seconds`.
pub const FIND_SECONDS: f64 = 10.0;
/// `settings --tab` choices (`""` = none).
pub const TABS: [&str; 4] = ["", "update", "agents", "setup"];

/// A parsed command line.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    Tray,
    Run,
    Find {
        seconds: f64,
    },
    Hooks {
        on: bool,
    },
    Autostart {
        on: bool,
    },
    Cleanup,
    /// `clock`/`key` are `""` when not given (the remembered clock is used).
    Settings {
        clock: String,
        key: String,
        tab: String,
    },
}

/// Parsing ended without a command to run: print `text` (stderr when `stderr`) and exit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exit {
    pub code: i32,
    pub text: String,
    pub stderr: bool,
}

impl Exit {
    fn usage_error(message: String) -> Self {
        Self {
            code: 2,
            text: format!("{}\nClawd: error: {message}", usage()),
            stderr: true,
        }
    }
}

/// The `--help` text.
pub fn usage() -> String {
    "usage: Clawd [--version] [tray|run|find|hooks|autostart|cleanup|settings] ...\n\
     \n\
     \x20 Clawd                     tray app (default)\n\
     \x20 Clawd run                 headless: same work, no tray (Ctrl+C stops)\n\
     \x20 Clawd find [--seconds N]  list the clocks that answer mDNS on this network\n\
     \x20 Clawd hooks on|off        install / remove the Claude Code hooks\n\
     \x20 Clawd autostart on|off    start at login, or not\n\
     \x20 Clawd cleanup             what the uninstaller runs: autostart off, every agent disconnected\n\
     \x20 Clawd settings            the settings window (--clock, --key, --tab update|agents|setup)"
        .to_owned()
}

/// The value after a flag: `--flag value` or `--flag=value`. `Ok(None)` when `arg` is not `flag`.
fn flag_value<'a>(
    arg: &'a str,
    flag: &str,
    rest: &mut std::slice::Iter<'a, String>,
) -> Result<Option<&'a str>, Exit> {
    if arg == flag {
        return match rest.next() {
            Some(v) => Ok(Some(v.as_str())),
            None => Err(Exit::usage_error(format!(
                "argument {flag}: expected one argument"
            ))),
        };
    }
    Ok(arg.strip_prefix(flag).and_then(|r| r.strip_prefix('=')))
}

fn on_off(args: &[String], name: &str) -> Result<bool, Exit> {
    match args {
        [state] if state == "on" => Ok(true),
        [state] if state == "off" => Ok(false),
        [state] => Err(Exit::usage_error(format!(
            "argument state: invalid choice: '{state}' (choose from 'on', 'off')"
        ))),
        [] => Err(Exit::usage_error(format!(
            "the following arguments are required: state ({name})"
        ))),
        [_, extra, ..] => Err(Exit::usage_error(format!(
            "unrecognized arguments: {extra}"
        ))),
    }
}

/// Parse the arguments after the program name.
pub fn parse(args: &[String]) -> Result<Command, Exit> {
    let args: Vec<String> = args
        .iter()
        .filter(|a| !a.starts_with("-psn_"))
        .cloned()
        .collect();
    let Some(first) = args.first() else {
        return Ok(Command::Tray);
    };
    let rest = &args[1..];
    match first.as_str() {
        "--version" => Err(Exit {
            code: 0,
            text: format!("{APP_NAME} {APP_VERSION}"),
            stderr: false,
        }),
        "-h" | "--help" => Err(Exit {
            code: 0,
            text: usage(),
            stderr: false,
        }),
        "tray" if rest.is_empty() => Ok(Command::Tray),
        "run" if rest.is_empty() => Ok(Command::Run),
        "cleanup" if rest.is_empty() => Ok(Command::Cleanup),
        "tray" | "run" | "cleanup" => Err(Exit::usage_error(format!(
            "unrecognized arguments: {}",
            rest.join(" ")
        ))),
        "hooks" => Ok(Command::Hooks {
            on: on_off(rest, "hooks")?,
        }),
        "autostart" => Ok(Command::Autostart {
            on: on_off(rest, "autostart")?,
        }),
        "find" => {
            let mut seconds = FIND_SECONDS;
            let mut it = rest.iter();
            while let Some(arg) = it.next() {
                match flag_value(arg, "--seconds", &mut it)? {
                    Some(v) => {
                        seconds = v.parse().map_err(|_| {
                            Exit::usage_error(format!(
                                "argument --seconds: invalid float value: '{v}'"
                            ))
                        })?;
                    }
                    None => {
                        return Err(Exit::usage_error(format!("unrecognized arguments: {arg}")))
                    }
                }
            }
            Ok(Command::Find { seconds })
        }
        "settings" => {
            let (mut clock, mut key, mut tab) = (String::new(), String::new(), String::new());
            let mut it = rest.iter();
            while let Some(arg) = it.next() {
                if arg == "--piped" {
                    continue; // the Python tray's pipe flag; accepted, nothing to do
                }
                if let Some(v) = flag_value(arg, "--clock", &mut it)? {
                    clock = v.to_owned();
                } else if let Some(v) = flag_value(arg, "--key", &mut it)? {
                    key = v.to_owned();
                } else if let Some(v) = flag_value(arg, "--tab", &mut it)? {
                    if !TABS.contains(&v) {
                        return Err(Exit::usage_error(format!(
                            "argument --tab: invalid choice: '{v}' (choose from '', 'update', 'agents', 'setup')"
                        )));
                    }
                    tab = v.to_owned();
                } else {
                    return Err(Exit::usage_error(format!("unrecognized arguments: {arg}")));
                }
            }
            Ok(Command::Settings { clock, key, tab })
        }
        other => Err(Exit::usage_error(format!(
            "argument cmd: invalid choice: '{other}' (choose from 'tray', 'run', 'find', 'hooks', 'autostart', 'cleanup', 'settings')"
        ))),
    }
}

// =============================================================================
// Pure pieces of the subcommands
// =============================================================================

/// `cli.remembered_clock` on the contents of clock.json: `(base URL, keychain key)` of the clock
/// the tray last used, or `("", "")`.
pub fn remembered_clock(data: &Value) -> (String, String) {
    let text = |k: &str| match data.get(k) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    };
    let (manual, host, port) = (text("manual_host"), text("host"), text("port"));
    let mut raw = if manual.is_empty() {
        host.clone()
    } else {
        manual.clone()
    };
    if !host.is_empty() && manual.is_empty() && !port.is_empty() && port != "0" {
        raw = format!("{host}:{port}");
    }
    let clock = (!raw.is_empty())
        .then(|| clawd_clock::manual_clock(&raw))
        .flatten();
    match clock {
        None => (String::new(), String::new()),
        Some(c) => {
            let key = if !manual.is_empty() {
                manual
            } else if !text("name").is_empty() {
                text("name")
            } else {
                c.host.clone()
            };
            (c.base(), key)
        }
    }
}

/// One line of `Clawd find`.
pub fn find_line(clock: &ClockTarget) -> String {
    format!(
        "mDNS  {:<20} {:<28} v{}",
        clock.name,
        clock.base(),
        clock.version
    )
}

/// What `find` says when nothing answered.
pub const FIND_NOTHING: &str =
    "mDNS: nothing. Type the IP the clock shows on its screen in Cài đặt → App → Nhập địa chỉ.";

/// `cli.main` for `cleanup`: autostart off, then every agent disconnected, one line each with a
/// `[HH:MM:SS]` stamp, then "cleanup done". An autostart failure is reported and does not stop
/// the agents from being disconnected (the uninstaller must still clean what it can).
pub fn cleanup_with(
    disable_autostart: impl FnOnce() -> std::io::Result<bool>,
    uninstall_agents: impl FnOnce() -> Vec<String>,
    stamp: impl Fn() -> String,
    out: &mut impl Write,
) {
    if let Err(e) = disable_autostart() {
        let _ = writeln!(out, "[{}] autostart: {e}", stamp());
    }
    for line in uninstall_agents() {
        let _ = writeln!(out, "[{}] {line}", stamp());
    }
    let _ = writeln!(out, "[{}] cleanup done", stamp());
}

/// `cli.main` for `autostart on|off`: the line printed and the exit code.
pub fn autostart_report(on: bool, result: std::io::Result<bool>) -> (String, i32) {
    match result {
        Ok(true) => (format!("autostart {}", if on { "on" } else { "off" }), 0),
        Ok(false) => ("autostart: not supported here".to_owned(), 1),
        Err(e) => (format!("autostart: {e}"), 1),
    }
}

// =============================================================================
// Entry point
// =============================================================================

fn print_exit(exit: &Exit) {
    if exit.stderr {
        eprintln!("{}", exit.text);
    } else {
        println!("{}", exit.text);
    }
}

fn stamp() -> String {
    use clawd_contracts::Clock;
    crate::tray::local_hhmmss(clawd_contracts::SystemClock.now())
}

fn find(seconds: f64) -> i32 {
    use clawd_clock::{MdnsBrowser, MdnsSource};
    let browser = MdnsBrowser::new();
    browser.start();
    std::thread::sleep(std::time::Duration::from_secs_f64(seconds.max(0.0)));
    let names = browser.names();
    let mut clocks: Vec<ClockTarget> = std::thread::scope(|scope| {
        let handles: Vec<_> = names
            .iter()
            .map(|name| {
                let browser = &browser;
                scope.spawn(move || browser.resolve(name))
            })
            .collect();
        handles
            .into_iter()
            .filter_map(|h| h.join().ok().flatten())
            .collect()
    });
    browser.stop();
    clocks.sort_by(|a, b| a.name.cmp(&b.name));
    for clock in &clocks {
        println!("{}", find_line(clock));
    }
    if clocks.is_empty() {
        println!("{FIND_NOTHING}");
    }
    0
}

fn services_for_cli() -> (AppPaths, crate::engine::EngineConfig) {
    let cfg = crate::engine::EngineConfig::system();
    (cfg.paths.clone(), cfg)
}

/// The program: parse, run, return the exit code. `context` builds the Tauri context and is only
/// called by the two modes that open a window (`tray`, `settings`).
pub fn main(args: &[String], context: impl FnOnce() -> tauri::Context<tauri::Wry>) -> i32 {
    let command = match parse(args) {
        Ok(command) => command,
        Err(exit) => {
            crate::lifecycle::attach_parent_console();
            print_exit(&exit);
            return exit.code;
        }
    };
    match command {
        Command::Tray => crate::lifecycle::run_tray(context()),
        Command::Settings { clock, key, tab } => {
            crate::lifecycle::run_settings_only(context(), clock, key, tab)
        }
        console_command => {
            crate::lifecycle::attach_parent_console();
            run_console(console_command)
        }
    }
}

fn run_console(command: Command) -> i32 {
    let (paths, cfg) = services_for_cli();
    match command {
        Command::Run => crate::lifecycle::run_headless(cfg),
        Command::Find { seconds } => find(seconds),
        Command::Hooks { on } => {
            let lines = clawd_agents::hooks::apply(
                &paths,
                &cfg.env,
                on,
                None,
                clawd_contracts::Platform::current(),
                None,
            );
            for line in lines {
                println!("[{}] {line}", stamp());
            }
            0
        }
        Command::Autostart { on } => {
            let autostart = clawd_store::autostart::Autostart::new(&paths, cfg.names.clone());
            let (line, code) = autostart_report(on, autostart.set_enabled(on));
            println!("{line}");
            code
        }
        Command::Cleanup => {
            let autostart = clawd_store::autostart::Autostart::new(&paths, cfg.names.clone());
            let registry = clawd_agents::Registry::new(cfg.agents.clone());
            cleanup_with(
                || autostart.disable(),
                || clawd_agents::uninstall_all(&registry),
                stamp,
                &mut std::io::stdout(),
            );
            0
        }
        Command::Tray | Command::Settings { .. } => 2,
    }
}
