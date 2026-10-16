//! The settings pages ship without inline `style=` attributes and without a meta CSP (phase 13
//! contract gate: the real policy lives in tauri.conf.json, so the markup must not need
//! `'unsafe-inline'`).

use std::fs;
use std::path::Path;

fn ui_html_files() -> Vec<std::path::PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("ui");
    let mut files: Vec<_> = fs::read_dir(dir)
        .expect("app/ui exists")
        .map(|entry| entry.expect("dir entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "html"))
        .collect();
    files.sort();
    files
}

fn has_style_attribute(html: &str) -> bool {
    let bytes = html.as_bytes();
    let mut from = 0;
    while let Some(found) = html[from..].find("style=") {
        let at = from + found;
        let word_start = at == 0
            || !(bytes[at - 1].is_ascii_alphanumeric()
                || bytes[at - 1] == b'_'
                || bytes[at - 1] == b'-');
        if word_start {
            return true;
        }
        from = at + "style=".len();
    }
    false
}

#[test]
fn ui_html_has_no_inline_style_attribute() {
    let files = ui_html_files();
    assert!(!files.is_empty());
    for file in files {
        let html = fs::read_to_string(&file).expect("readable html");
        assert!(
            !has_style_attribute(&html),
            "{} has a style= attribute",
            file.display()
        );
    }
}

#[test]
fn settings_index_has_no_meta_csp() {
    let html = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("ui/index.html"))
        .expect("index.html");
    assert!(!html.contains("Content-Security-Policy"));
}
