use crate::command::CommandRunner;

/// `usage.detect_hour_format()`: best-effort 12h/24h detection for the host, `12` or `24`
/// (default 24). macOS: `defaults read -g AppleICUForce24HourTime|AppleICUForce12HourTime`
/// (through `runner`), then the locale; Windows and others: 24, exactly like Python (`defaults` is missing and
/// `nl_langinfo` raises there): no behaviour change in the port.
pub fn detect_hour_format(runner: &dyn CommandRunner) -> u8 {
    todo!("phase 06")
}
