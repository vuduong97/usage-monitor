use std::fmt;

/// The three values `sys.platform` takes in the Python code. Injected wherever Python takes a
/// `platform: str = sys.platform` argument (hook commands, autostart, golden fixtures).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Platform {
    /// `"win32"`
    Win32,
    /// `"darwin"`
    Darwin,
    /// `"linux"` (non-goal; only so `cargo check` on Ubuntu has a value)
    Linux,
}

impl Platform {
    /// The platform this binary was compiled for.
    pub const fn current() -> Self {
        if cfg!(target_os = "windows") {
            Platform::Win32
        } else if cfg!(target_os = "macos") {
            Platform::Darwin
        } else {
            Platform::Linux
        }
    }

    /// The Python `sys.platform` spelling (`win32`, `darwin`, `linux`).
    pub const fn as_str(self) -> &'static str {
        match self {
            Platform::Win32 => "win32",
            Platform::Darwin => "darwin",
            Platform::Linux => "linux",
        }
    }

    /// Parse the Python spelling.
    pub fn from_sys_platform(s: &str) -> Option<Self> {
        match s {
            "win32" => Some(Platform::Win32),
            "darwin" => Some(Platform::Darwin),
            "linux" => Some(Platform::Linux),
            _ => None,
        }
    }
}

impl fmt::Display for Platform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        for p in [Platform::Win32, Platform::Darwin, Platform::Linux] {
            assert_eq!(Platform::from_sys_platform(p.as_str()), Some(p));
        }
        assert_eq!(Platform::from_sys_platform("cygwin"), None);
    }
}
