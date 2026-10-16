//! The capability files are the first lock on the two commands (the label checks in
//! `commands/mod.rs` are the second): the settings window may call `py_api` and not
//! `popover_msg`; the popover the other way round. Neither gets anything but the event API.

use std::path::Path;

use serde_json::Value;

fn capability(file: &str) -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("capabilities")
        .join(file);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn strings(v: &Value) -> Vec<&str> {
    v.as_array()
        .expect("an array")
        .iter()
        .map(|s| s.as_str().expect("a string"))
        .collect()
}

#[test]
fn only_the_settings_window_may_call_py_api() {
    let settings = capability("default.json");
    assert_eq!(strings(&settings["windows"]), ["settings"]);
    assert_eq!(
        strings(&settings["permissions"]),
        ["core:event:default", "allow-py-api"]
    );
    let popover = capability("popover.json");
    assert!(!strings(&popover["permissions"]).contains(&"allow-py-api"));
}

#[test]
fn only_the_popover_may_call_popover_msg() {
    let popover = capability("popover.json");
    assert_eq!(strings(&popover["windows"]), ["popover"]);
    assert_eq!(
        strings(&popover["permissions"]),
        ["core:event:default", "allow-popover-msg"]
    );
    let settings = capability("default.json");
    assert!(!strings(&settings["permissions"]).contains(&"allow-popover-msg"));
}

#[test]
fn no_capability_is_granted_to_every_window() {
    for file in ["default.json", "popover.json"] {
        let windows = strings(&capability(file)["windows"]).len();
        assert_eq!(windows, 1, "{file} must name exactly one window");
    }
}
