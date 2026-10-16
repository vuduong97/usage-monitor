//! `CommandRunner`: the real runner reports a missing program as `NotFound`, and the testkit
//! fake scripts answers and records calls.

use std::io::ErrorKind;
use std::time::Duration;

use clawd_config::{CommandRunner, SystemRunner};
use clawd_testkit::config::{FakeReply, FakeRunner};

const WAIT: Duration = Duration::from_secs(10);

#[test]
fn system_runner_reports_a_missing_program_as_not_found() {
    let err = SystemRunner
        .run("clawd-no-such-program-xyz", &["a"], WAIT)
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::NotFound);
}

#[test]
fn system_runner_captures_output_and_exit_status() {
    // the test binary itself: `--list` prints the test names and exits 0
    let exe = std::env::current_exe().unwrap();
    let out = SystemRunner
        .run(exe.to_str().unwrap(), &["--list"], WAIT)
        .unwrap();
    assert_eq!(out.status, 0);
    assert!(out
        .stdout
        .contains("system_runner_captures_output_and_exit_status"));
    // an argument libtest rejects: a non-zero exit is Ok, not an error
    let out = SystemRunner
        .run(exe.to_str().unwrap(), &["--no-such-flag"], WAIT)
        .unwrap();
    assert_ne!(out.status, 0);
    assert!(!out.stderr.is_empty());
}

#[test]
fn fake_runner_answers_first_matching_rule_and_records_calls() {
    let fake = FakeRunner::new()
        .on("tool", &["a"], FakeReply::ok("one\n"))
        .on_match(|program, _| program == "tool", FakeReply::exit(3, "bad"))
        .defaults("AppleICUForce24HourTime", FakeReply::timed_out());

    assert_eq!(fake.run("tool", &["a"], WAIT).unwrap().stdout, "one\n");
    let out = fake.run("tool", &["b"], WAIT).unwrap();
    assert_eq!((out.status, out.stderr.as_str()), (3, "bad"));
    let err = fake
        .run("defaults", &["read", "-g", "AppleICUForce24HourTime"], WAIT)
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::TimedOut);
    // unscripted program: not installed
    assert_eq!(
        fake.run("security", &[], WAIT).unwrap_err().kind(),
        ErrorKind::NotFound
    );
    assert_eq!(fake.calls().len(), 4);
    assert_eq!(fake.args_of("tool"), vec![vec!["a"], vec!["b"]]);
    assert_eq!(fake.calls()[0].timeout, WAIT);
}
