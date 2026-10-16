//! `tray.open_path` moved here so `settings` code does not depend on the tray (scout §1: break
//! the `settings_api -> tray` edge).

use clawd_contracts::Platform;

/// The program and arguments that open `path` on a platform with an opener command
/// (`subprocess.Popen(["open", path])`, `["xdg-open", path]`); `None` on Windows, which uses
/// `ShellExecute` (`os.startfile`).
pub fn opener_command(platform: Platform, path: &str) -> Option<(&'static str, Vec<String>)> {
    match platform {
        Platform::Darwin => Some(("open", vec![path.to_owned()])),
        Platform::Linux => Some(("xdg-open", vec![path.to_owned()])),
        Platform::Win32 => None,
    }
}

/// `tray.open_path(path)`: open a file or folder with the OS (`open` / `ShellExecute` /
/// `xdg-open`). Errors are swallowed. Callers pass only paths the app owns or a known URL
/// (`open_web` is limited to the clock's page and GitHub).
pub fn open_path(path: &str) {
    match opener_command(Platform::current(), path) {
        Some((program, args)) => {
            if let Ok(mut child) = std::process::Command::new(program).args(args).spawn() {
                // reap it without blocking the caller (Popen leaves it running)
                std::thread::spawn(move || {
                    let _ = child.wait();
                });
            }
        }
        None => shell_open(path),
    }
}

#[cfg(windows)]
fn shell_open(path: &str) {
    use windows::core::{w, PCWSTR};
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let target: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: `target` is NUL-terminated and outlives the call; the verb is a static string.
    // The result is an HINSTANCE-style code that Python's `os.startfile` also ignores here.
    let _ = unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            PCWSTR(target.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
}

#[cfg(not(windows))]
fn shell_open(_path: &str) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opener_commands_per_platform() {
        assert_eq!(
            opener_command(Platform::Darwin, "/tmp/x"),
            Some(("open", vec!["/tmp/x".to_owned()]))
        );
        assert_eq!(
            opener_command(Platform::Linux, "/tmp/x"),
            Some(("xdg-open", vec!["/tmp/x".to_owned()]))
        );
        assert_eq!(opener_command(Platform::Win32, r"C:\x"), None);
    }
}
