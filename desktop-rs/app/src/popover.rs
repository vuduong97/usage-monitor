//! The macOS popover under the menu-bar icon (`popover.py`): STUB owned by phase 12, which creates
//! the frameless `popover` webview on tray click and destroys it when it loses focus.

use serde_json::Value;
use tauri::AppHandle;

/// Handle one message from `ui/popover.html` (`invoke('popover_msg', {msg})`), after
/// `commands::popover_msg` has checked that the caller is the `popover` window. `msg` is the
/// JSON `{cmd, ...}` of `popover.py:262-280`: `ready`, `size {h: 40..400}`, `settings`, `close`.
pub fn handle(app: &AppHandle, msg: Value) -> Result<(), String> {
    todo!("phase 12")
}
