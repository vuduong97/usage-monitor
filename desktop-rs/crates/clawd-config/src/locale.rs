use std::time::Duration;

use crate::command::CommandRunner;
use crate::pytext::strip;

/// `subprocess.run(["defaults", ...], timeout=3)`.
const DEFAULTS_TIMEOUT: Duration = Duration::from_secs(3);

/// `usage.detect_hour_format()`: best-effort 12h/24h detection for the host, `12` or `24`
/// (default 24). `defaults read -g AppleICUForce24HourTime` then `AppleICUForce12HourTime`
/// (through `runner`; a program that is missing, hangs or exits non-zero is skipped, as on
/// Windows where Python gets `FileNotFoundError`); the first one printing `1` decides.
///
/// Python's last resort, the C library's `T_FMT` of the process locale, is NOT ported: it needs
/// libc `nl_langinfo` (absent on Windows, where Python also answers 24) and a GUI-launched macOS
/// app runs in the `C` locale, which reads as 24 too.
pub fn detect_hour_format(runner: &dyn CommandRunner) -> u8 {
    for (key, result) in [
        ("AppleICUForce24HourTime", 24),
        ("AppleICUForce12HourTime", 12),
    ] {
        if let Ok(out) = runner.run("defaults", &["read", "-g", key], DEFAULTS_TIMEOUT) {
            if strip(&out.stdout) == "1" {
                return result;
            }
        }
    }
    24
}
