/// `events.read_hid_idle()`: seconds since the last keyboard/mouse input, `None` if unknown.
/// A locked screen gets no input either, so this also covers "locked". Windows:
/// `GetLastInputInfo` (+ `GetTickCount`); macOS: `ioreg -c IOHIDSystem` `HIDIdleTime`;
/// elsewhere `None`.
pub fn read_hid_idle() -> Option<f64> {
    platform_idle()
}

/// `(now - last_input) & 0xFFFFFFFF` milliseconds as seconds: tick counts wrap every 49.7 days.
#[cfg_attr(not(windows), allow(dead_code))]
fn wrapped_idle_secs(now_ticks: u32, last_input_ticks: u32) -> f64 {
    f64::from(now_ticks.wrapping_sub(last_input_ticks)) / 1000.0
}

#[cfg(windows)]
fn platform_idle() -> Option<f64> {
    use windows::Win32::System::SystemInformation::GetTickCount;
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};

    let mut info = LASTINPUTINFO {
        cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
        dwTime: 0,
    };
    // SAFETY: `info` is a valid LASTINPUTINFO with `cbSize` set; GetTickCount takes nothing.
    unsafe {
        if !GetLastInputInfo(&mut info).as_bool() {
            return None;
        }
        Some(wrapped_idle_secs(GetTickCount(), info.dwTime))
    }
}

#[cfg(target_os = "macos")]
fn platform_idle() -> Option<f64> {
    use std::process::Command;

    let out = Command::new("ioreg")
        .args(["-c", "IOHIDSystem", "-d", "4"])
        .output()
        .ok()?;
    parse_hid_idle(&String::from_utf8_lossy(&out.stdout))
}

#[cfg(not(any(windows, target_os = "macos")))]
fn platform_idle() -> Option<f64> {
    None
}

/// `"HIDIdleTime" = <nanoseconds>` in `ioreg` output, as seconds.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn parse_hid_idle(ioreg_output: &str) -> Option<f64> {
    let rest = ioreg_output.split("\"HIDIdleTime\"").nth(1)?;
    let rest = rest.trim_start().strip_prefix('=')?.trim_start();
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    digits.parse::<u64>().ok().map(|ns| ns as f64 / 1e9)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tick_counts_wrap_like_python_mask() {
        assert_eq!(wrapped_idle_secs(5_000, 2_000), 3.0);
        assert_eq!(wrapped_idle_secs(1_000, u32::MAX - 999), 2.0);
    }

    #[test]
    fn ioreg_output_is_parsed() {
        let out = "    | |   \"HIDIdleTime\" = 2500000000\n    | |   \"x\" = 1";
        assert_eq!(parse_hid_idle(out), Some(2.5));
        assert_eq!(parse_hid_idle("nothing"), None);
        assert_eq!(parse_hid_idle("\"HIDIdleTime\" = abc"), None);
    }

    #[cfg(windows)]
    #[test]
    fn windows_reports_a_plausible_idle_time() {
        // Session 0 / CI without an interactive desktop may return None; both are fine.
        if let Some(secs) = read_hid_idle() {
            assert!(secs >= 0.0);
        }
    }
}
