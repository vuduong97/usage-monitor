//! Ports of `test_tray.py:64,150,157` (the typed-address dialog) and checks of the native dialog
//! command lines. Nothing here opens a dialog.

use std::cell::{Cell, RefCell};

use clawd_app::dialogs::{
    answer_of, applescript_escape, manual, osascript_alert_script, osascript_ask_script,
    ManualOutcome, ADDRESS_PROMPT, POWERSHELL_INPUT_BOX,
};

#[test]
fn menu_dialog_uses_the_typed_ip() {
    let asked = RefCell::new(Vec::new());
    let alerts = RefCell::new(Vec::new());
    let set = RefCell::new(Vec::new());
    let outcome = manual(
        |prompt| {
            asked.borrow_mut().push(prompt.to_owned());
            Some("10.97.40.207".to_owned())
        },
        |text| {
            set.borrow_mut().push(text.to_owned());
            true
        },
        |t| alerts.borrow_mut().push(t.to_owned()),
    );
    assert_eq!(outcome, ManualOutcome::Set);
    assert_eq!(*asked.borrow(), [ADDRESS_PROMPT]);
    assert_eq!(*set.borrow(), ["10.97.40.207"]);
    assert!(alerts.borrow().is_empty());
}

#[test]
fn menu_dialog_reports_a_bad_answer_and_ignores_cancel() {
    let alerts = RefCell::new(Vec::new());
    let outcome = manual(
        |_| Some("not a host:99999".to_owned()),
        |_| false,
        |t| alerts.borrow_mut().push(t.to_owned()),
    );
    let notice = "“not a host:99999” không phải địa chỉ hợp lệ.";
    assert_eq!(outcome, ManualOutcome::Invalid(notice.to_owned()));
    assert_eq!(*alerts.borrow(), [notice]);

    for answer in [None, Some(String::new())] {
        let touched = Cell::new(false);
        let outcome = manual(
            |_| answer.clone(),
            |_| {
                touched.set(true);
                true
            },
            |_| touched.set(true),
        );
        assert_eq!(outcome, ManualOutcome::Cancelled);
        assert!(!touched.get(), "a cancelled dialog changes nothing");
    }
}

#[test]
fn applescript_text_is_escaped_not_spliced() {
    assert_eq!(applescript_escape(r#"a"b\c"#), r#"a\"b\\c"#);
    let script = osascript_ask_script("say \"hi\"", "x\\y");
    assert_eq!(
        script,
        "text returned of (display dialog \"say \\\"hi\\\"\" default answer \"x\\\\y\" with title \"Clawd\")"
    );
    assert_eq!(
        osascript_alert_script("bad \"ip\""),
        "display alert \"Clawd\" message \"bad \\\"ip\\\"\""
    );
}

#[test]
fn the_windows_input_box_takes_text_from_the_environment() {
    assert!(POWERSHELL_INPUT_BOX.contains("$env:CLAWD_PROMPT"));
    assert!(POWERSHELL_INPUT_BOX.contains("$env:CLAWD_DEFAULT"));
    assert!(!POWERSHELL_INPUT_BOX.contains(ADDRESS_PROMPT));
}

#[test]
fn an_answer_is_read_like_ask_text() {
    assert_eq!(
        answer_of(true, " 10.0.0.5\r\n", true),
        Some("10.0.0.5".into())
    );
    assert_eq!(
        answer_of(true, "\r\n", true),
        None,
        "Windows: empty is cancel"
    );
    assert_eq!(
        answer_of(true, "\n", false),
        Some(String::new()),
        "macOS: empty text"
    );
    assert_eq!(
        answer_of(false, "10.0.0.5", false),
        None,
        "macOS: cancel exits non-zero"
    );
}
