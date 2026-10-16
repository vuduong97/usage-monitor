//! Small native dialogs (`tray.py:101-135`, `:304-310`): no webview, so they work when WebView2
//! is missing. The text the user types never reaches a shell: it travels as an environment
//! variable (Windows) or inside an AppleScript string whose quotes and backslashes are escaped
//! (macOS), exactly as in Python.

use std::process::Command;

use clawd_contracts::APP_NAME;

/// `tray.ADDRESS_PROMPT`.
pub const ADDRESS_PROMPT: &str =
    "Địa chỉ IP của đồng hồ (hiện trên màn hình đồng hồ lúc khởi động), ví dụ 192.168.1.48:";

/// Both dialogs give up after this long (`timeout=300` in Python).
const DIALOG_TIMEOUT_SECS: u64 = 300;

/// `CREATE_NO_WINDOW`: a console child of a GUI process must not flash a window.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// The PowerShell command of the Windows input box. The prompt and default are read from the
/// environment (`CLAWD_PROMPT`, `CLAWD_DEFAULT`), never spliced into this text.
pub const POWERSHELL_INPUT_BOX: &str = "Add-Type -AssemblyName Microsoft.VisualBasic; \
[Microsoft.VisualBasic.Interaction]::InputBox($env:CLAWD_PROMPT, 'Clawd', $env:CLAWD_DEFAULT)";

/// AppleScript string escape: backslash first, then the double quote.
pub fn applescript_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// The `osascript -e` program of the macOS input dialog.
pub fn osascript_ask_script(prompt: &str, default: &str) -> String {
    format!(
        "text returned of (display dialog \"{}\" default answer \"{}\" with title \"{APP_NAME}\")",
        applescript_escape(prompt),
        applescript_escape(default)
    )
}

/// The `osascript -e` program of the macOS alert.
pub fn osascript_alert_script(text: &str) -> String {
    format!(
        "display alert \"{APP_NAME}\" message \"{}\"",
        applescript_escape(text)
    )
}

/// `ask_text` result of the process output: macOS needs exit code 0 and gives the text (empty
/// allowed); Windows gives the text and `None` for empty (cancel and empty look the same).
pub fn answer_of(success: bool, stdout: &str, empty_is_cancel: bool) -> Option<String> {
    if !success {
        return None;
    }
    let text = stdout.trim().to_owned();
    if empty_is_cancel && text.is_empty() {
        None
    } else {
        Some(text)
    }
}

fn run_with_timeout(mut command: Command) -> Option<(bool, String)> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    let mut child = command.spawn().ok()?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(DIALOG_TIMEOUT_SECS);
    let mut stdout = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut out = String::new();
        let _ = std::io::Read::read_to_string(&mut stdout, &mut out);
        out
    });
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if std::time::Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(100)),
            Err(_) => return None,
        }
    };
    Some((status.success(), reader.join().unwrap_or_default()))
}

/// `tray.ask_text`: one-line text input; `None` when cancelled or unsupported. Blocks for as
/// long as the dialog is open: call it from a thread of its own.
pub fn ask_text(prompt: &str, default: &str) -> Option<String> {
    if cfg!(target_os = "macos") {
        let mut command = Command::new("osascript");
        command.arg("-e").arg(osascript_ask_script(prompt, default));
        let (ok, out) = run_with_timeout(command)?;
        answer_of(ok, &out, false)
    } else if cfg!(windows) {
        let mut command = Command::new("powershell");
        command
            .args(["-NoProfile", "-Command", POWERSHELL_INPUT_BOX])
            .env("CLAWD_PROMPT", prompt)
            .env("CLAWD_DEFAULT", default);
        let (ok, out) = run_with_timeout(command)?;
        // PowerShell exits 0 even on cancel: the text decides.
        answer_of(ok, &out, true)
    } else {
        None
    }
}

#[cfg(windows)]
fn message_box(text: &str) {
    #[link(name = "user32")]
    extern "system" {
        fn MessageBoxW(hwnd: isize, text: *const u16, caption: *const u16, flags: u32) -> i32;
    }
    const MB_ICONINFORMATION: u32 = 0x40;
    let wide = |s: &str| s.encode_utf16().chain(Some(0)).collect::<Vec<u16>>();
    let (text, caption) = (wide(text), wide(APP_NAME));
    // SAFETY: both buffers are NUL-terminated and outlive the call; a null owner is allowed.
    unsafe {
        MessageBoxW(0, text.as_ptr(), caption.as_ptr(), MB_ICONINFORMATION);
    }
}

/// `tray.alert`: a modal notice. Blocks until dismissed (call it off the main thread).
pub fn alert(text: &str) {
    #[cfg(target_os = "macos")]
    {
        let mut command = Command::new("osascript");
        command.arg("-e").arg(osascript_alert_script(text));
        let _ = run_with_timeout(command);
    }
    #[cfg(windows)]
    message_box(text);
    #[cfg(not(any(target_os = "macos", windows)))]
    println!("{text}");
}

/// What "Nhập địa chỉ đồng hồ…" came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManualOutcome {
    /// Cancelled or left empty: nothing changes.
    Cancelled,
    /// The finder took the address.
    Set,
    /// Not a host: the notice shown to the user.
    Invalid(String),
}

/// `Tray.manual`: ask, hand the answer to the finder (`set_manual` says whether it was a valid
/// address), tell the user when it was not. The three effects are injected.
pub fn manual(
    ask: impl FnOnce(&str) -> Option<String>,
    set_manual: impl FnOnce(&str) -> bool,
    alert: impl FnOnce(&str),
) -> ManualOutcome {
    match ask(ADDRESS_PROMPT).filter(|t| !t.is_empty()) {
        None => ManualOutcome::Cancelled,
        Some(text) if set_manual(&text) => ManualOutcome::Set,
        Some(text) => {
            let notice = format!("“{text}” không phải địa chỉ hợp lệ.");
            alert(&notice);
            ManualOutcome::Invalid(notice)
        }
    }
}
