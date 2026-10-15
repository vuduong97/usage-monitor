//! FROZEN (phase 02, plan.md rule 2). The two commands are registered in the app manifest so that
//! `capabilities/*.json` can allow them per window (`allow-py-api` for `settings`,
//! `allow-popover-msg` for `popover`); without it every window could call every command
//! (red team #7). Adding a command = a Contract CR.
fn main() {
    let attributes = tauri_build::Attributes::new()
        .app_manifest(tauri_build::AppManifest::new().commands(&["py_api", "popover_msg"]));
    tauri_build::try_build(attributes).expect("failed to run tauri-build");
}
