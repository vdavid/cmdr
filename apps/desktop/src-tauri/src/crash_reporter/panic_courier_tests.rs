//! Tests for in-session delivery of a survived panic.
//!
//! The property that matters most here is negative: **the reporting path must not turn a
//! panic the app survived into a hard crash**. `a_panic_in_the_delivery_path_does_not_escape_the_courier`
//! pins it directly by driving the courier with work that panics.
//!
//! Anything touching the auto-dispatcher's process-global state holds
//! `auto_dispatcher::TEST_LOCK`, the same mutex `auto_dispatcher_tests.rs` serializes on.

use super::panic_courier::{
    PANIC_LOG_TARGET, PanicNotice, courier_running_for_test, deliver_for_test, headline_for_test, notify,
    spawn_courier_for_test,
};
use crate::error_reporter::auto_dispatcher::{TEST_LOCK, reset_for_test, set_enabled, snapshot_for_test};
use std::sync::{Mutex, MutexGuard, mpsc};

/// Serializes the tests that spawn real couriers. `COURIER_RUNNING` is process-global, so
/// two of them in parallel would each see the other's courier and refuse to spawn.
static COURIER_TEST_LOCK: Mutex<()> = Mutex::new(());

fn lock_couriers() -> MutexGuard<'static, ()> {
    let guard = COURIER_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    wait_for_courier_to_finish();
    guard
}

fn notice() -> PanicNotice {
    PanicNotice {
        message: Some("called `unwrap()` on an `Err` value".to_string()),
        thread_name: Some("mtp-poll".to_string()),
        backtrace_frames: vec!["cmdr_lib::mtp::poll".to_string(), "std::sys::thread".to_string()],
        crash_file_short_id: Some("CRASH-A2345".to_string()),
    }
}

/// Waits for the courier to release the flag. Called only after a `join()`, so the store
/// has already happened; this just spins out any store-visibility lag rather than sleeping.
fn wait_for_courier_to_finish() {
    for _ in 0..1_000 {
        if !courier_running_for_test() {
            return;
        }
        std::thread::yield_now();
    }
    panic!("courier never released COURIER_RUNNING");
}

#[test]
fn a_panic_in_the_delivery_path_does_not_escape_the_courier() {
    let _guard = lock_couriers();

    let handle =
        spawn_courier_for_test(|| panic!("the reporting path itself blew up")).expect("no courier should be running");
    // `Ok` proves the courier caught the unwind. Without the catch this is `Err`, and in
    // the real hook a panic that escaped here would have aborted the process long before.
    assert!(
        handle.join().is_ok(),
        "a panic inside the courier must be caught, not propagated"
    );
    wait_for_courier_to_finish();

    // And the guard is released, so the NEXT panic still gets delivered.
    let second = spawn_courier_for_test(|| {}).expect("the flag must be released after a panicking courier");
    second.join().expect("second courier runs normally");
    wait_for_courier_to_finish();
}

#[test]
fn only_one_courier_runs_at_a_time() {
    let _guard = lock_couriers();

    let (release_tx, release_rx) = mpsc::channel::<()>();
    let (started_tx, started_rx) = mpsc::channel::<()>();
    let first = spawn_courier_for_test(move || {
        started_tx.send(()).ok();
        release_rx.recv().ok();
    })
    .expect("no courier should be running");
    started_rx.recv().expect("first courier started");

    assert!(
        spawn_courier_for_test(|| panic!("this work must never run")).is_none(),
        "a second courier must be refused while one is alive (this is the reentrancy guard)"
    );

    release_tx.send(()).ok();
    first.join().expect("first courier finishes");
    wait_for_courier_to_finish();
}

#[test]
fn notify_returns_quietly_whether_or_not_a_courier_slot_is_free() {
    // The hook calls `notify` mid-panic, where ANY panic aborts the process outright. It
    // has to come back cleanly from both branches. Holds the dispatcher lock too, because
    // the courier it spawns reaches `on_error_logged`.
    let _dispatcher = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    reset_for_test();
    let _guard = lock_couriers();

    notify(notice());
    notify(notice()); // slot taken by the first: a no-op, still no panic
    wait_for_courier_to_finish();

    assert!(
        snapshot_for_test().is_none(),
        "opt-in is off, so the courier must not have opened a window"
    );
}

#[test]
fn delivering_a_panic_opens_a_flow_b_window() {
    let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    reset_for_test();
    set_enabled(true);

    deliver_for_test(&notice());

    let (category, message, count, _scheduled) =
        snapshot_for_test().expect("a survived panic must open a Flow B window in-session");
    assert_eq!(category, PANIC_LOG_TARGET);
    assert!(
        message.contains("mtp-poll"),
        "the note names the panicking thread: {message}"
    );
    assert!(
        message.contains("called `unwrap()` on an `Err` value"),
        "the note carries the sanitized panic message: {message}"
    );
    assert!(
        message.contains("CRASH-A2345"),
        "the note carries the crash file's short id so the two reports pair up: {message}"
    );
    assert_eq!(count, 1);

    reset_for_test();
}

#[test]
fn delivering_a_panic_sends_nothing_when_error_reports_are_off() {
    let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    reset_for_test();
    // `reset_for_test` leaves the opt-in off, which is also the shipped default.

    deliver_for_test(&notice());

    assert!(
        snapshot_for_test().is_none(),
        "no opt-in means no window, so nothing can leave the machine"
    );
}

#[test]
fn the_headline_falls_back_when_the_panic_carried_no_strings() {
    let bare = PanicNotice {
        message: None,
        thread_name: None,
        backtrace_frames: Vec::new(),
        crash_file_short_id: None,
    };
    let headline = headline_for_test(&bare);
    assert!(headline.contains("<unnamed>"), "{headline}");
    assert!(headline.contains("(no panic message)"), "{headline}");
}
