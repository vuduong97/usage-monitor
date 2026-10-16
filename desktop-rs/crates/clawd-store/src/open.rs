//! `tray.open_path` moved here so `settings` code does not depend on the tray (scout §1: break
//! the `settings_api -> tray` edge).
#![allow(unused_variables, dead_code)] // milestone stub: removed when implemented

/// `tray.open_path(path)`: open a file or folder with the OS (`open` / `ShellExecute` /
/// `xdg-open`). Errors are swallowed. Callers pass only paths the app owns or a known URL
/// (`open_web` is limited to the clock's page and GitHub).
pub fn open_path(path: &str) {
    todo!("phase 03")
}
