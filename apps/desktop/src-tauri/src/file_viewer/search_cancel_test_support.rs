//! Shared scaffolding for the per-match search-cancel tests.
//!
//! `LineIndexBackend` and `ByteSeekBackend` both promise to notice a cancel flag between individual
//! matches, not merely between lines or chunks. Proving that needs a canceller racing a live scan,
//! so both backends drive the same helper here rather than each rolling its own threads.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use super::search_matcher::Matcher;
use super::{FileViewerBackend, MAX_SEARCH_MATCHES, SearchMatch};
use crate::test_support::wait_until;

/// Asserts that `backend.search` stops on the cancel flag mid-scan, and stops promptly.
///
/// The canceller flips the flag the moment the scan reports its first match, so the run always
/// lands inside `scan_line_with_matcher` rather than at one of the cheaper per-line or per-chunk
/// checks. Call it with a corpus holding far more than `MAX_SEARCH_MATCHES` matches: the final
/// count then separates the two possible endings, and a scan that outruns the canceller fails
/// loudly instead of passing on a cancel that never mattered.
pub(super) fn assert_search_stops_on_per_match_cancel(backend: &impl FileViewerBackend, matcher: &Matcher) {
    let cancel = Arc::new(AtomicBool::new(false));
    let matches: Arc<Mutex<Vec<SearchMatch>>> = Arc::new(Mutex::new(Vec::new()));
    let progress = Mutex::new(0u64);

    let cancel_setter = Arc::clone(&cancel);
    let watched = Arc::clone(&matches);
    let cancelled_at: Arc<Mutex<Option<Instant>>> = Arc::new(Mutex::new(None));
    let stamp = Arc::clone(&cancelled_at);
    let canceller = thread::spawn(move || {
        wait_until(Duration::from_secs(5), "the scan to report its first match", || {
            !watched.lock().unwrap().is_empty()
        });
        *stamp.lock().unwrap() = Some(Instant::now());
        cancel_setter.store(true, Ordering::Relaxed);
    });

    let _ = backend.search(matcher, &cancel, &matches, &progress);
    let returned_at = Instant::now();
    canceller.join().expect("the canceller must reach the cancel flag");

    let found = matches.lock().unwrap().len();
    assert!(found > 0, "the scan must report matches before it gets cancelled");
    assert!(
        found < MAX_SEARCH_MATCHES,
        "the scan collected all {MAX_SEARCH_MATCHES} matches it was allowed, so it ended on the limit \
         rather than on cancel and says nothing about cancellation"
    );

    let cancelled_at = cancelled_at.lock().unwrap().expect("the canceller must stamp its time");
    let observed_in = returned_at.saturating_duration_since(cancelled_at);
    assert!(
        observed_in < Duration::from_millis(800),
        "search must return once cancel is set; it took {observed_in:?}"
    );
}

/// A corpus with 1,000 matches on each of 1,000 lines: two orders of magnitude past
/// `MAX_SEARCH_MATCHES`, so a scan that runs to its natural end always ends on the limit.
pub(super) fn many_matches_corpus() -> String {
    let line: String = "a".repeat(1_000) + "\n";
    line.repeat(1_000)
}
