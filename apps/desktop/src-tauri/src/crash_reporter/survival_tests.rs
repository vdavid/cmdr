//! Tests for the proof that the app outlived a panic.
//!
//! Every test here drives a crash file in its own `tempdir`, so nothing touches
//! `CRASH_PATH` or `SESSION_CRASH_FILE_WRITTEN`. Those are process-global `OnceLock`s
//! that a test can only burn once, and a burnt one would silently disarm the watchdog for
//! every other test in the binary.

use super::survival::{arm_after_for_test, confirm_survival};
use super::{AppFate, CRASH_FILE_NAME, CrashReport, read_crash_report, write_crash_report};
use std::path::Path;
use std::time::Duration;

fn write_report(path: &Path, fate: AppFate) {
    let report = CrashReport {
        version: super::CRASH_FILE_VERSION,
        timestamp: "2026-03-22T10:00:00+00:00".to_string(),
        signal: Some("panic".to_string()),
        panic_message: Some("called `unwrap()` on a `None` value".to_string()),
        backtrace_frames: vec!["cmdr_lib::indexing::scan".to_string()],
        thread_name: Some("cmdr-indexer".to_string()),
        thread_count: 12,
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        os_version: "test".to_string(),
        arch: "test".to_string(),
        uptime_secs: 61.0,
        active_settings: super::ActiveSettings::default(),
        possible_crash_loop: false,
        app_fate: fate,
        build_mode: Some("debug".to_string()),
        short_id: Some("CRASH-A2345".to_string()),
        diag_id: "diag_00000000-0000-4000-8000-000000000000".to_string(),
        email: None,
        system_snapshot: None,
        image_base: None,
    };
    write_crash_report(path, &report).unwrap();
}

#[test]
fn confirming_survival_upgrades_the_report_on_disk() {
    // Without this the next launch resolves the report to `Ended` and the dialog opens
    // with "Cmdr quit unexpectedly last time" about a session the user quit themselves.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(CRASH_FILE_NAME);
    write_report(&path, AppFate::Unconfirmed);

    confirm_survival(&path);

    assert_eq!(read_crash_report(&path).unwrap().app_fate, AppFate::KeptRunning);
}

#[test]
fn confirming_survival_twice_is_a_no_op() {
    // Both seams (the watchdog's timer and the app's quit path) record the same fact, and
    // in a session that panics and is then quit, BOTH of them fire.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(CRASH_FILE_NAME);
    write_report(&path, AppFate::Unconfirmed);

    confirm_survival(&path);
    confirm_survival(&path);

    assert_eq!(read_crash_report(&path).unwrap().app_fate, AppFate::KeptRunning);
}

#[test]
fn confirming_survival_never_rewrites_a_settled_fate() {
    // A settled `Ended` belongs to a report a PREVIOUS launch already resolved. Claiming
    // the app survived that one would be flatly false, and the guard makes it impossible
    // rather than merely unreachable.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(CRASH_FILE_NAME);
    write_report(&path, AppFate::Ended);

    confirm_survival(&path);

    assert_eq!(read_crash_report(&path).unwrap().app_fate, AppFate::Ended);
}

#[test]
fn confirming_survival_with_no_crash_file_is_a_quiet_no_op() {
    // The common case by far: the app quits after a session that never panicked, so the
    // quit path calls into here with nothing on disk.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(CRASH_FILE_NAME);

    confirm_survival(&path);

    assert!(!path.exists(), "confirming survival must never conjure a report");
}

#[test]
fn the_watchdog_confirms_survival_once_its_delay_is_up() {
    // The thread reaching its own second line is the whole proof: a panic that took the
    // app down would have taken this thread with it.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(CRASH_FILE_NAME);
    write_report(&path, AppFate::Unconfirmed);

    let watchdog = arm_after_for_test(&path, Duration::from_millis(10)).expect("the watchdog thread starts");
    watchdog.join().expect("the watchdog runs to completion");

    assert_eq!(read_crash_report(&path).unwrap().app_fate, AppFate::KeptRunning);
}

#[test]
fn the_watchdog_leaves_the_report_alone_until_its_delay_is_up() {
    // A watchdog that confirmed immediately would call every fatal panic a survival, which
    // is the one direction of wrongness this design must not have.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(CRASH_FILE_NAME);
    write_report(&path, AppFate::Unconfirmed);

    let watchdog = arm_after_for_test(&path, Duration::from_secs(600)).expect("the watchdog thread starts");

    assert_eq!(
        read_crash_report(&path).unwrap().app_fate,
        AppFate::Unconfirmed,
        "nothing may be claimed before the app has actually stayed alive"
    );
    drop(watchdog);
}
