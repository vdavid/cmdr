//! Tests for `contain_panics`: a panic inside the closure comes back as `None` and the
//! hook writes no crash file and starts no courier for it; a value flows through; the mark
//! is gone afterwards so the next panic on the thread is reported as usual.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use super::contain::panic_is_contained;
use super::*;

/// Install the production hook body (minus the process-wide statics) as THE panic hook
/// while `f` runs, then put the previous hook back. Returns how many times the hook saw a
/// panic it reported (vs. contained).
fn with_hook_recording_to<T>(
    crash_path: &Path,
    already_written: &Arc<AtomicBool>,
    f: impl FnOnce() -> T,
) -> (T, usize) {
    let reported = Arc::new(AtomicUsize::new(0));
    let previous = std::panic::take_hook();
    {
        let crash_path = crash_path.to_path_buf();
        let already_written = Arc::clone(already_written);
        let reported = Arc::clone(&reported);
        std::panic::set_hook(Box::new(move |info| {
            if handle_panic(info, Some(&crash_path), &already_written) == PanicDisposition::Reported {
                reported.fetch_add(1, Ordering::SeqCst);
            }
        }));
    }
    let out = f();
    std::panic::set_hook(previous);
    (out, reported.load(Ordering::SeqCst))
}

#[test]
fn a_contained_panic_returns_none_and_the_hook_writes_no_crash_file() {
    let dir = tempfile::tempdir().unwrap();
    let crash_path = dir.path().join(CRASH_FILE_NAME);
    let written = Arc::new(AtomicBool::new(false));
    let couriers_before = panic_courier::couriers_started_for_test();

    let (outcome, reported) = with_hook_recording_to(&crash_path, &written, || {
        contain_panics(|| -> u8 { panic!("a deliberate contained panic") })
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
