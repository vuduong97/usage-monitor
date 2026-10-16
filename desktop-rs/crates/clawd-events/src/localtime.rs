//! Local wall-clock time for the Pet's quiet hours and daily counters (`time.localtime`).
//! Injected into the Pet so tests (and goldens) can pin the hour.

/// The parts of `time.struct_time` the Pet reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalTime {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
}

impl LocalTime {
    /// `time.strftime("%Y-%m-%d", t)`.
    pub fn date(&self) -> String {
        format!("{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }

    /// `t.tm_hour * 60 + t.tm_min`.
    pub fn minute_of_day(&self) -> u32 {
        self.hour * 60 + self.minute
    }
}

/// `localtime(ts)`: the stand-in tests pass is any function of the timestamp.
pub type LocalTimeFn = Box<dyn Fn(f64) -> LocalTime + Send + Sync>;

/// The machine's local time at a Unix timestamp (`time.localtime`).
#[cfg(windows)]
pub fn system_local_time(timestamp: f64) -> LocalTime {
    use windows::Win32::Foundation::{FILETIME, SYSTEMTIME};
    use windows::Win32::System::Time::{FileTimeToSystemTime, SystemTimeToTzSpecificLocalTime};

    const UNIX_TO_FILETIME_SECS: f64 = 11_644_473_600.0;
    let ticks = ((timestamp + UNIX_TO_FILETIME_SECS) * 1e7).max(0.0) as u64;
    let file_time = FILETIME {
        dwLowDateTime: (ticks & 0xFFFF_FFFF) as u32,
        dwHighDateTime: (ticks >> 32) as u32,
    };
    let mut utc = SYSTEMTIME::default();
    let mut local = SYSTEMTIME::default();
    // SAFETY: both calls only read/write the stack values passed by reference.
    let ok = unsafe {
        FileTimeToSystemTime(&file_time, &mut utc).is_ok()
            && SystemTimeToTzSpecificLocalTime(None, &utc, &mut local).is_ok()
    };
    if !ok {
        local = utc;
    }
    LocalTime {
        year: i32::from(local.wYear),
        month: u32::from(local.wMonth),
        day: u32::from(local.wDay),
        hour: u32::from(local.wHour),
        minute: u32::from(local.wMinute),
    }
}

/// The machine's local time at a Unix timestamp (`time.localtime`).
#[cfg(unix)]
pub fn system_local_time(timestamp: f64) -> LocalTime {
    let secs = timestamp as libc::time_t;
    // SAFETY: `tm` is plain data; `localtime_r` fills it from `secs` and touches nothing else.
    let tm = unsafe {
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&secs, &mut tm);
        tm
    };
    LocalTime {
        year: tm.tm_year + 1900,
        month: (tm.tm_mon + 1) as u32,
        day: tm.tm_mday as u32,
        hour: tm.tm_hour as u32,
        minute: tm.tm_min as u32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn date_and_minutes_format_like_strftime() {
        let t = LocalTime {
            year: 2026,
            month: 3,
            day: 9,
            hour: 7,
            minute: 5,
        };
        assert_eq!(t.date(), "2026-03-09");
        assert_eq!(t.minute_of_day(), 425);
    }

    #[test]
    fn system_local_time_is_plausible() {
        let t = system_local_time(1_760_000_000.0); // 2025-10-09 08:53 UTC
        assert_eq!(t.year, 2025);
        assert!((1..=12).contains(&t.month) && (1..=31).contains(&t.day));
        assert!(t.hour < 24 && t.minute < 60);
    }
}
