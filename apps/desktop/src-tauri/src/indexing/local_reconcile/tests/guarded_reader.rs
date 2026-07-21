//! The per-read wall-clock guard: a hung directory read is abandoned near the
//! timeout and the reader serves the next one.

use super::*;

#[test]
fn guarded_reader_returns_a_quick_result() {
    let read_fn: ReadFn = Arc::new(|_p| Some(vec![]));
    let mut reader = GuardedReader::with_read_fn(Duration::from_secs(5), read_fn);
    assert!(
        reader.read(Path::new("/x")).0.is_some(),
        "a fast read returns its result"
    );
}

#[test]
fn guarded_reader_abandons_a_hung_read_and_recovers() {
    use std::sync::atomic::AtomicUsize;
    // Only the FIRST read hangs; later reads are fast. This proves both that the
    // hung read is abandoned near the timeout (not waited out) AND that the reader
    // recovers — respawns a worker — for the next read.
    let calls = Arc::new(AtomicUsize::new(0));
    let read_fn: ReadFn = {
        let calls = Arc::clone(&calls);
        Arc::new(move |_p| {
            if calls.fetch_add(1, Ordering::SeqCst) == 0 {
                std::thread::sleep(Duration::from_secs(2));
            }
            Some(vec![])
        })
    };
    let mut reader = GuardedReader::with_read_fn(Duration::from_millis(50), read_fn);

    let start = Instant::now();
    assert!(reader.read(Path::new("/hang")).0.is_none(), "a hung read returns None");
    assert!(
        start.elapsed() < Duration::from_secs(1),
        "must abandon near the timeout, not wait out the ~2s hang (elapsed {:?})",
        start.elapsed()
    );
    assert!(
        reader.read(Path::new("/ok")).0.is_some(),
        "the reader recovers after a timeout and serves the next read",
    );
}
