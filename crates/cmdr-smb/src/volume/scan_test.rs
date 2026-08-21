//! The copy-scan progress ticker's counting contract.

use super::*;

/// The counts a scan reports must be CUMULATIVE for the call: the caller shifts
/// them by its own baseline across several calls, so a per-entry (or per-path)
/// reset would make the dialog's counters jump backwards.
#[test]
fn scan_ticker_reports_running_totals() {
    use cmdr_fs::ignore_poison::IgnorePoison;
    use cmdr_fs::volume::ListingProgress;
    use std::sync::Mutex;

    let seen: Mutex<Vec<ListingProgress>> = Mutex::new(Vec::new());
    let record = |p: ListingProgress| seen.lock_ignore_poison().push(p);
    let ticker = ScanTicker::new(Some(&record));

    ticker.dir();
    ticker.file(1_000);
    ticker.file(24);

    let seen = seen.lock_ignore_poison();
    assert_eq!(seen.len(), 3, "every entry reports, so a slow walk still looks alive");
    assert_eq!((seen[0].files, seen[0].dirs, seen[0].bytes), (0, 1, 0));
    assert_eq!((seen[1].files, seen[1].dirs, seen[1].bytes), (1, 1, 1_000));
    assert_eq!((seen[2].files, seen[2].dirs, seen[2].bytes), (2, 1, 1_024));
}

/// A scan with nobody listening still counts, so `scan_for_copy` (which the
/// trait gives it no callback) can share one implementation with the batch path.
#[test]
fn scan_ticker_without_a_listener_still_counts() {
    let ticker = ScanTicker::new(None);
    ticker.dir();
    ticker.file(512);

    let counts = ticker.counts();
    assert_eq!((counts.files, counts.dirs, counts.bytes), (1, 1, 512));
}
