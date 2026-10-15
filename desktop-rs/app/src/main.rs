//! Clawd: tray-only app, no window at start (`app.windows: []`). `commands::register` installs the
//! one `invoke_handler` (phase 02, frozen); phase 12 adds the builder setup around it.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    clawd_app::commands::register(tauri::Builder::default())
        .run(tauri::generate_context!())
        .expect("error while running Clawd");
}
