//! Clawd: tray-only app, no window at start (`app.windows: []`). `commands::register` installs the
//! one `invoke_handler` (phase 02, frozen); `cli::main` parses the command line and runs the
//! tray, the headless engine, a console subcommand or the settings-only window.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = clawd_app::cli::main(&args, || tauri::generate_context!());
    std::process::exit(code);
}
