//! Tests for `contain_panics`: a panic inside the closure comes back as `None` and the
//! hook writes no crash file and starts no courier for it; a value flows through; the mark
//! is gone afterwards so the next panic on the thread is reported as usual.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use super::contain::panic_is_contained;
use super::*;

/// The panic hook is process-wide, so the tests that install one must not overlap: under
/// plain `cargo test` (one process, parallel threads) two of them racing on `set_hook` /
/// `take_hook` leave one asserting against the other's hook. Nextest runs each test in its own
/// process and never sees this; the lock keeps both runners green. A poisoned lock (a test
/// that panicked while holding it) is still a usable lock. The lock can't cover a panicking
/// test in ANOTHER module, so each installed hook also ignores panics from other threads.
static PANIC_HOOK: Mutex<()> = Mutex::new(());

fn hold_the_hook() -> std::sync::MutexGuard<'static, ()> {
    PANIC_HOOK.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Install the production hook body (minus the process-wide statics) as THE panic hook
/// while `f` runs, then put the previous hook back. Returns how many times the hook saw a
/// panic it reported (vs. contained).
fn with_hook_recording_to<T>(
    crash_path: &Path,
    already_written: &Arc<AtomicBool>,
    f: impl FnOnce() -> T,
) -> (T, usize) {
    let _serialized = hold_the_hook();
    let reported = Arc::new(AtomicUsize::new(0));
    let previous = std::panic::take_hook();
    {
        let crash_path = crash_path.to_path_buf();
        let already_written = Arc::clone(already_written);
        let reported = Arc::clone(&reported);
        let this_thread = std::thread::current().id();
        std::panic::set_hook(Box::new(move |info| {
            if std::thread::current().id() != this_thread {
                return;
            }
            if handle_panic(info, Some(&crash_path), &already_written) == PanicDisposition::Reported {
                reported.fetch_add(1, Ordering::SeqCst);
            }
        }));
    }
    let out = f();
    std::panic::set_hook(previous);
    // A reported panic detached a courier whose counter increment lands on its own thread;
    // wait for it here, while the lock is still held, so it can't land in the next test.
    panic_courier::join_last_courier_for_test();
    (out, reported.load(Ordering::SeqCst))
}

#[test]
fn a_contained_panic_returns_none_and_the_hook_writes_no_crash_file() {
    let dir = tempfile::tempdir().unwrap();
    let crash_path = dir.path().join(CRASH_FILE_NAME);
    let written = Arc::new(AtomicBool::new(false));

    // The courier count is read INSIDE the hook window: by then the lock is held and the
    // previous test's courier (if any) has been joined, so the baseline can't move under us.
    let ((outcome, couriers_before), reported) = with_hook_recording_to(&crash_path, &written, || {
        let couriers_before = panic_courier::couriers_started_for_test();
        (
            contain_panics(|| -> u8 { panic!("a deliberate contained panic") }),
            couriers_before,
        )
    });

    assert_eq!(outcome, None, "the panic is the closure's `None`");
    assert_eq!(reported, 0, "the hook saw the panic and contained it");
    assert!(!crash_path.exists(), "no crash file for a contained panic");
    assert!(
        !written.load(Ordering::SeqCst),
        "the session's one crash-file write is still available to a real panic"
    );
    assert_eq!(
        panic_courier::couriers_started_for_test(),
        couriers_before,
        "no in-session courier for a contained panic"
    );
    assert!(!panic_is_contained(), "the mark is gone once the closure is over");
}

#[test]
fn a_value_flows_through_and_the_mark_is_scoped_to_the_closure() {
    assert!(!panic_is_contained());
    let seen_inside = contain_panics(|| {
        let inside = panic_is_contained();
        // A nested closure that panics leaves the outer mark standing.
        let nested = contain_panics(|| -> () { panic!("nested") });
        (inside, nested, panic_is_contained())
    });
    assert_eq!(seen_inside, Some((true, None, true)));
    assert!(!panic_is_contained());
}

#[test]
fn a_panic_outside_the_closure_is_still_reported() {
    // The exemption is the closure, not the thread: the same thread panicking right after
    // goes through the full path (here: the crash file lands).
    let dir = tempfile::tempdir().unwrap();
    let crash_path = dir.path().join(CRASH_FILE_NAME);
    let written = Arc::new(AtomicBool::new(false));

    let (_, reported) = with_hook_recording_to(&crash_path, &written, || {
        let contained = contain_panics(|| -> () { panic!("contained") });
        assert_eq!(contained, None);
        let plain = std::panic::catch_unwind(|| panic!("a real one"));
        assert!(plain.is_err());
    });

    assert_eq!(reported, 1, "exactly the uncontained panic was reported");
    assert!(written.load(Ordering::SeqCst));
    let report = read_crash_report(&crash_path).expect("the uncontained panic wrote its report");
    assert_eq!(report.panic_message.as_deref(), Some("a real one"));
}

/// `cmdr.log` rides error reports, and a `pdf-extract` panic message can quote bytes of the
/// PDF's object dump (`expect` on a dictionary lookup formats the object it was looking at).
/// So the warning names the thread and nothing the file could have put there.
#[test]
fn the_contained_panic_warning_carries_no_panic_message() {
    let _serialized = hold_the_hook();
    let captured: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let previous = std::panic::take_hook();
    {
        let captured = Arc::clone(&captured);
        std::panic::set_hook(Box::new(move |info| {
            if std::thread::current().name() == Some("pdf-parse-worker") {
                *captured.lock().unwrap() = Some(contained_panic_warning(info));
            }
        }));
    }
    let outcome = std::thread::Builder::new()
        .name("pdf-parse-worker".into())
        .spawn(|| contain_panics(|| -> () { panic!("<< /Type /Page /Contents SECRET-OBJECT-DUMP >>") }))
        .unwrap()
        .join()
        .unwrap();
    std::panic::set_hook(previous);

    assert_eq!(outcome, None);
    let line = captured.lock().unwrap().clone().expect("the hook saw the panic");
    assert!(
        line.contains("pdf-parse-worker"),
        "the thread name is the one thing worth logging: {line}"
    );
    assert!(
        !line.contains("SECRET-OBJECT-DUMP") && !line.contains("/Type"),
        "the panic message must never reach the log: {line}"
    );
}
