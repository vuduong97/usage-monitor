//! `autostart.py`: start the app at login, per user and without admin rights (red team #1).
//!
//! * Windows: `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`, value `Clawd` = `"<exe>"`
//!   with **no arguments** (and no `--autostart`).
//! * macOS: `~/Library/LaunchAgents/vn.clawd.desktop.plist`, a byte-copy of Python's
//!   [`launch_agent`] dict: `Label`, `ProgramArguments = [exe]`, `RunAtLoad`,
//!   `KeepAlive {SuccessfulExit: false}`, `ThrottleInterval 30`, `ProcessType Interactive`.
//!
//! Written by hand (`windows` crate registry calls, `plist`); no `auto-launch` crate (it also
//! writes `StartupApproved` and would add a second launcher). The exe path does not change from
//! 0.1.4 (`{localappdata}\Programs\Clawd\Clawd.exe`, `/Applications/Clawd.app/Contents/MacOS/
//! Clawd`), so no migration step exists.
//!
//! Test isolation (red team #14): the plist goes under the injected `home`; the Windows Run
//! value goes under the real `Run` key only for the production value name (`Clawd`). Any other
//! name (every `OsStoreNames::for_test`) lives under `HKCU\Software\ClawdTest\<name>\Run`, a
//! key nothing launches from; [`Autostart::remove_sandbox`] deletes it.

use std::io;
use std::path::PathBuf;

use clawd_contracts::{AppPaths, OsStoreNames, Platform, RUN_VALUE};
use plist::{Dictionary, Value};

/// `autostart.command()`: how to start the app: `[<this executable>]`.
pub fn command() -> Vec<String> {
    match std::env::current_exe() {
        Ok(exe) => vec![exe.to_string_lossy().into_owned()],
        Err(_) => Vec::new(),
    }
}

/// `autostart.launch_agent(cmd)` for LaunchAgent label `label`. Unlike Python it never adds
/// `WorkingDirectory` (the Rust app is always a packaged binary).
pub fn launch_agent(label: &str, cmd: &[String]) -> plist::Dictionary {
    let mut keep_alive = Dictionary::new();
    keep_alive.insert("SuccessfulExit".to_owned(), Value::Boolean(false));
    let mut agent = Dictionary::new();
    agent.insert("Label".to_owned(), Value::String(label.to_owned()));
    agent.insert(
        "ProgramArguments".to_owned(),
        Value::Array(cmd.iter().cloned().map(Value::String).collect()),
    );
    agent.insert("RunAtLoad".to_owned(), Value::Boolean(true));
    // Restart after a crash, not after "Thoát" (a clean exit).
    agent.insert("KeepAlive".to_owned(), Value::Dictionary(keep_alive));
    agent.insert("ThrottleInterval".to_owned(), Value::Integer(30.into()));
    agent.insert(
        "ProcessType".to_owned(),
        Value::String("Interactive".to_owned()),
    );
    agent
}

/// `plistlib.dumps(agent)` (XML format, tab indent, keys sorted, trailing newline): the exact
/// bytes `autostart.enable` writes to the plist.
pub fn plist_xml(agent: &Dictionary) -> io::Result<Vec<u8>> {
    let mut out = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <plist version=\"1.0\">\n",
    );
    write_plist_value(&mut out, &Value::Dictionary(agent.clone()), 0)?;
    out.push_str("</plist>\n");
    Ok(out.into_bytes())
}

/// `plistlib._PlistWriter.write_value`: one tab per level, dict keys sorted, text escaped.
fn write_plist_value(out: &mut String, value: &Value, depth: usize) -> io::Result<()> {
    let pad = "\t".repeat(depth);
    match value {
        Value::Boolean(true) => out.push_str(&format!("{pad}<true/>\n")),
        Value::Boolean(false) => out.push_str(&format!("{pad}<false/>\n")),
        Value::String(s) => out.push_str(&format!("{pad}<string>{}</string>\n", xml_escape(s))),
        Value::Integer(i) => out.push_str(&format!("{pad}<integer>{i}</integer>\n")),
        Value::Array(items) => {
            out.push_str(&format!("{pad}<array>\n"));
            for item in items {
                write_plist_value(out, item, depth + 1)?;
            }
            out.push_str(&format!("{pad}</array>\n"));
        }
        Value::Dictionary(dict) => {
            out.push_str(&format!("{pad}<dict>\n"));
            let mut keys: Vec<&String> = dict.keys().collect();
            keys.sort();
            for key in keys {
                out.push_str(&format!("{pad}\t<key>{}</key>\n", xml_escape(key)));
                if let Some(item) = dict.get(key) {
                    write_plist_value(out, item, depth + 1)?;
                }
            }
            out.push_str(&format!("{pad}</dict>\n"));
        }
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "unsupported plist value",
            ))
        }
    }
    Ok(())
}

/// `plistlib._escape`: `&`, `<`, `>`.
fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// The Run value data for `cmd`: every element quoted and joined by a space (`"<exe>"`), as
/// `" ".join(f'"{c}"' for c in cmd)` in `autostart.enable`.
pub fn run_value(cmd: &[String]) -> String {
    cmd.iter()
        .map(|c| format!("\"{c}\""))
        .collect::<Vec<_>>()
        .join(" ")
}

/// One autostart entry of one user, with every OS-visible name and path injectable.
#[derive(Debug, Clone)]
pub struct Autostart {
    pub names: OsStoreNames,
    pub home: PathBuf,
    pub platform: Platform,
    /// What the entry launches (`[exe]`).
    pub command: Vec<String>,
}

impl Autostart {
    /// This machine and this executable: `platform = Platform::current()`, `command = command()`.
    pub fn new(paths: &AppPaths, names: OsStoreNames) -> Self {
        Self {
            names,
            home: paths.home().to_path_buf(),
            platform: Platform::current(),
            command: command(),
        }
    }

    /// `autostart.PLIST`: `<home>/Library/LaunchAgents/<label>.plist`.
    pub fn plist_path(&self) -> PathBuf {
        AppPaths::new(&self.home).launch_agent_plist(&self.names.launch_label)
    }

    /// The registry key holding the Run value: the real `Run` key for the production value name,
    /// a sandbox key for any other (see the module docs).
    pub fn run_key(&self) -> String {
        if self.names.run_value == RUN_VALUE {
            clawd_contracts::RUN_KEY.to_owned()
        } else {
            format!(r"Software\ClawdTest\{}\Run", self.names.run_value)
        }
    }

    /// `autostart.supported(platform)`: macOS and Windows only.
    pub fn supported(&self) -> bool {
        matches!(self.platform, Platform::Darwin | Platform::Win32)
    }

    /// `autostart.is_enabled(platform)`: the plist exists / the Run value exists.
    pub fn is_enabled(&self) -> bool {
        match self.platform {
            Platform::Darwin => self.plist_path().exists(),
            Platform::Win32 => registry::value_exists(&self.run_key(), &self.names.run_value),
            Platform::Linux => false,
        }
    }

    /// `autostart.enable(platform)`: write the plist / Run value. `Ok(false)` where unsupported;
    /// `Err` carries the OS error text Python's `OSError` showed (`settings_api` returns it to the
    /// page, `first_run` logs it).
    pub fn enable(&self) -> io::Result<bool> {
        match self.platform {
            Platform::Darwin => {
                let path = self.plist_path();
                if let Some(dir) = path.parent() {
                    std::fs::create_dir_all(dir)?;
                }
                let agent = launch_agent(&self.names.launch_label, &self.command);
                std::fs::write(&path, plist_xml(&agent)?)?;
                Ok(true)
            }
            Platform::Win32 if registry::AVAILABLE => {
                registry::set_value(
                    &self.run_key(),
                    &self.names.run_value,
                    &run_value(&self.command),
                )?;
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    /// `autostart.disable(platform)`: remove them. `Ok(false)` where unsupported.
    pub fn disable(&self) -> io::Result<bool> {
        match self.platform {
            Platform::Darwin => {
                match std::fs::remove_file(self.plist_path()) {
                    Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
                    _ => {}
                }
                Ok(true)
            }
            Platform::Win32 if registry::AVAILABLE => {
                // Python swallows the OSError of a missing key or value
                registry::delete_value(&self.run_key(), &self.names.run_value);
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    /// `autostart.set_enabled(on, platform)`.
    pub fn set_enabled(&self, on: bool) -> io::Result<bool> {
        if on {
            self.enable()
        } else {
            self.disable()
        }
    }

    /// For tests: delete the sandbox registry key of non-production names. No-op for the
    /// production value name and where there is no registry.
    pub fn remove_sandbox(&self) {
        if self.names.run_value != RUN_VALUE {
            registry::delete_tree(&format!(r"Software\ClawdTest\{}", self.names.run_value));
        }
    }
}

#[cfg(windows)]
mod registry {
    use std::io;

    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{ERROR_SUCCESS, WIN32_ERROR};
    use windows::Win32::System::Registry::{
        RegCloseKey, RegCreateKeyExW, RegDeleteTreeW, RegDeleteValueW, RegOpenKeyExW,
        RegQueryValueExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE,
        KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_SZ,
    };

    pub(super) const AVAILABLE: bool = true;

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn check(status: WIN32_ERROR) -> io::Result<()> {
        if status == ERROR_SUCCESS {
            Ok(())
        } else {
            Err(io::Error::from_raw_os_error(status.0 as i32))
        }
    }

    /// An open HKCU subkey, closed on drop.
    struct Key(HKEY);

    impl Drop for Key {
        fn drop(&mut self) {
            // SAFETY: the handle was opened by this module and is closed once.
            let _ = unsafe { RegCloseKey(self.0) };
        }
    }

    fn open(
        path: &str,
        access: windows::Win32::System::Registry::REG_SAM_FLAGS,
    ) -> io::Result<Key> {
        let path = wide(path);
        let mut key = HKEY::default();
        // SAFETY: `path` is NUL-terminated and outlives the call; `key` receives the handle.
        check(unsafe {
            RegOpenKeyExW(
                HKEY_CURRENT_USER,
                PCWSTR(path.as_ptr()),
                None,
                access,
                &mut key,
            )
        })?;
        Ok(Key(key))
    }

    /// `winreg.OpenKey` + `QueryValueEx`: does the value exist?
    pub(super) fn value_exists(path: &str, name: &str) -> bool {
        let Ok(key) = open(path, KEY_QUERY_VALUE) else {
            return false;
        };
        let name = wide(name);
        // SAFETY: `name` is NUL-terminated; every out parameter is null (existence check only).
        let status =
            unsafe { RegQueryValueExW(key.0, PCWSTR(name.as_ptr()), None, None, None, None) };
        status == ERROR_SUCCESS
    }

    /// `winreg.CreateKey` + `SetValueEx(.., REG_SZ, data)`.
    pub(super) fn set_value(path: &str, name: &str, data: &str) -> io::Result<()> {
        let path = wide(path);
        let mut key = HKEY::default();
        // SAFETY: as in `open`; the created key is wrapped and closed on drop.
        check(unsafe {
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                PCWSTR(path.as_ptr()),
                None,
                PCWSTR::null(),
                REG_OPTION_NON_VOLATILE,
                KEY_WRITE,
                None,
                &mut key,
                None,
            )
        })?;
        let key = Key(key);
        let name = wide(name);
        // REG_SZ data: UTF-16LE including the terminating NUL (as `winreg.SetValueEx`)
        let bytes: Vec<u8> = wide(data).into_iter().flat_map(u16::to_le_bytes).collect();
        // SAFETY: `name` is NUL-terminated; `bytes` outlives the call.
        check(unsafe { RegSetValueExW(key.0, PCWSTR(name.as_ptr()), None, REG_SZ, Some(&bytes)) })
    }

    /// `winreg.DeleteValue`; errors (no key, no value) are the caller's to ignore.
    pub(super) fn delete_value(path: &str, name: &str) {
        let Ok(key) = open(path, KEY_SET_VALUE) else {
            return;
        };
        let name = wide(name);
        // SAFETY: `name` is NUL-terminated and outlives the call.
        let _ = unsafe { RegDeleteValueW(key.0, PCWSTR(name.as_ptr())) };
    }

    pub(super) fn delete_tree(path: &str) {
        let path = wide(path);
        // SAFETY: `path` is NUL-terminated and outlives the call.
        let _ = unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, PCWSTR(path.as_ptr())) };
    }

    /// Raw read of a REG_SZ value for tests: `(type, text)`.
    #[cfg(test)]
    pub(super) fn read_string(path: &str, name: &str) -> Option<(u32, String)> {
        let key = open(path, KEY_QUERY_VALUE).ok()?;
        let name = wide(name);
        let mut kind = windows::Win32::System::Registry::REG_VALUE_TYPE(0);
        let mut size = 0u32;
        // SAFETY: first call sizes the buffer, second fills it; all pointers outlive the calls.
        unsafe {
            check(RegQueryValueExW(
                key.0,
                PCWSTR(name.as_ptr()),
                None,
                Some(&mut kind),
                None,
                Some(&mut size),
            ))
            .ok()?;
            let mut buf = vec![0u8; size as usize];
            check(RegQueryValueExW(
                key.0,
                PCWSTR(name.as_ptr()),
                None,
                Some(&mut kind),
                Some(buf.as_mut_ptr()),
                Some(&mut size),
            ))
            .ok()?;
            let units: Vec<u16> = buf
                .chunks_exact(2)
                .map(|p| u16::from_le_bytes([p[0], p[1]]))
                .collect();
            let end = units.iter().position(|u| *u == 0).unwrap_or(units.len());
            Some((kind.0, String::from_utf16_lossy(&units[..end])))
        }
    }
}

#[cfg(not(windows))]
mod registry {
    use std::io;

    pub(super) const AVAILABLE: bool = false;

    pub(super) fn value_exists(_path: &str, _name: &str) -> bool {
        false
    }
    pub(super) fn set_value(_path: &str, _name: &str, _data: &str) -> io::Result<()> {
        Ok(())
    }
    pub(super) fn delete_value(_path: &str, _name: &str) {}
    pub(super) fn delete_tree(_path: &str) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `plistlib.dumps(autostart.launch_agent([exe]))` with `paths.frozen()` true.
    const PYTHON_PLIST: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\">\n<dict>\n\t<key>KeepAlive</key>\n\t<dict>\n\t\t<key>SuccessfulExit</key>\n\t\t<false/>\n\t</dict>\n\t<key>Label</key>\n\t<string>vn.clawd.desktop</string>\n\t<key>ProcessType</key>\n\t<string>Interactive</string>\n\t<key>ProgramArguments</key>\n\t<array>\n\t\t<string>/Applications/Clawd.app/Contents/MacOS/Clawd</string>\n\t</array>\n\t<key>RunAtLoad</key>\n\t<true/>\n\t<key>ThrottleInterval</key>\n\t<integer>30</integer>\n</dict>\n</plist>\n";

    fn sandbox(tag: &str) -> (tempfile::TempDir, Autostart) {
        let dir = tempfile::tempdir().expect("temp dir");
        let paths = AppPaths::new(dir.path().join("home"));
        let names = OsStoreNames::for_test(&format!("autostart-{tag}-{}", std::process::id()));
        assert!(!names.is_production());
        let mut auto = Autostart::new(&paths, names);
        auto.command = vec!["/Applications/Clawd.app/Contents/MacOS/Clawd".to_owned()];
        (dir, auto)
    }

    #[test]
    fn plist_bytes_match_python_plistlib() {
        let agent = launch_agent(
            "vn.clawd.desktop",
            &["/Applications/Clawd.app/Contents/MacOS/Clawd".to_owned()],
        );
        assert_eq!(
            String::from_utf8(plist_xml(&agent).unwrap()).unwrap(),
            PYTHON_PLIST
        );
    }

    #[test]
    fn plist_escapes_and_sorts_like_plistlib() {
        // plistlib.dumps({'Label': 'a&<b>', 'ProgramArguments': ['/x y/\u00e9']})
        let mut d = Dictionary::new();
        d.insert(
            "ProgramArguments".into(),
            Value::Array(vec![Value::String("/x y/\u{e9}".into())]),
        );
        d.insert("Label".into(), Value::String("a&<b>".into()));
        let want = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\">\n<dict>\n\t<key>Label</key>\n\t<string>a&amp;&lt;b&gt;</string>\n\t<key>ProgramArguments</key>\n\t<array>\n\t\t<string>/x y/\u{e9}</string>\n\t</array>\n</dict>\n</plist>\n";
        assert_eq!(String::from_utf8(plist_xml(&d).unwrap()).unwrap(), want);
    }

    #[test]
    fn run_value_quotes_every_element() {
        assert_eq!(
            run_value(&[r"C:\Apps\Clawd\Clawd.exe".to_owned()]),
            r#""C:\Apps\Clawd\Clawd.exe""#
        );
        assert_eq!(
            run_value(&["a b".to_owned(), "-m".to_owned()]),
            r#""a b" "-m""#
        );
        assert_eq!(run_value(&[]), "");
    }

    #[test]
    fn command_is_this_executable() {
        let cmd = command();
        assert_eq!(cmd.len(), 1);
        assert!(std::path::Path::new(&cmd[0]).is_absolute());
    }

    #[test]
    fn mac_enable_writes_the_plist_under_the_injected_home() {
        let (dir, mut auto) = sandbox("mac");
        auto.platform = Platform::Darwin;
        assert!(auto.supported());
        assert!(!auto.is_enabled());
        assert!(auto.enable().unwrap());
        assert!(auto.is_enabled());
        let path = auto.plist_path();
        assert!(path.starts_with(dir.path()));
        assert_eq!(
            path.file_name().unwrap().to_string_lossy(),
            format!("{}.plist", auto.names.launch_label)
        );
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains(&format!("<string>{}</string>", auto.names.launch_label)));
        assert!(text.contains("<key>SuccessfulExit</key>\n\t\t<false/>"));
        assert!(auto.set_enabled(false).unwrap());
        assert!(!auto.is_enabled());
        assert!(auto.disable().unwrap()); // already gone: still Ok(true)
    }

    #[test]
    fn linux_is_unsupported() {
        let (_dir, mut auto) = sandbox("linux");
        auto.platform = Platform::Linux;
        assert!(!auto.supported());
        assert!(!auto.is_enabled());
        assert!(!auto.enable().unwrap());
        assert!(!auto.disable().unwrap());
        assert!(!auto.set_enabled(true).unwrap());
    }

    #[test]
    fn sandbox_key_is_used_for_test_names_and_the_real_one_for_production() {
        let (_dir, auto) = sandbox("key");
        assert!(auto.run_key().starts_with(r"Software\ClawdTest\ClawdTest-"));
        let mut prod = auto.clone();
        prod.names = OsStoreNames::production();
        assert_eq!(prod.run_key(), clawd_contracts::RUN_KEY);
    }

    #[cfg(windows)]
    #[test]
    fn windows_run_value_is_the_quoted_exe_and_goes_away_on_disable() {
        let (_dir, mut auto) = sandbox("win");
        auto.platform = Platform::Win32;
        auto.command = vec![r"C:\Users\x y\AppData\Local\Programs\Clawd\Clawd.exe".to_owned()];
        struct Cleanup(Autostart);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                self.0.remove_sandbox();
            }
        }
        let guard = Cleanup(auto.clone());
        assert!(!auto.is_enabled());
        assert!(auto.enable().unwrap());
        assert!(auto.is_enabled());
        let (kind, data) = registry::read_string(&auto.run_key(), &auto.names.run_value).unwrap();
        assert_eq!(kind, 1, "REG_SZ");
        assert_eq!(
            data,
            r#""C:\Users\x y\AppData\Local\Programs\Clawd\Clawd.exe""#
        );
        assert!(auto.set_enabled(false).unwrap());
        assert!(!auto.is_enabled());
        assert!(auto.disable().unwrap(), "disabling twice is fine");
        drop(guard);
        assert!(registry::read_string(&auto.run_key(), &auto.names.run_value).is_none());
    }

    #[cfg(not(windows))]
    #[test]
    fn win32_without_a_registry_is_unavailable() {
        let (_dir, mut auto) = sandbox("nowin");
        auto.platform = Platform::Win32;
        assert!(auto.supported());
        assert!(!auto.is_enabled());
        assert!(!auto.enable().unwrap());
    }
}
