//! Recent-listing-errors ring buffer for the `cmdr://state` resource.
//!
//! Surfaces the last few directory-listing failures (volume not found, permission
//! denied, I/O error, SMB protocol error) to MCP-driven tests so they can assert
//! "no error since timestamp X" without grepping the on-disk log file. Populated
//! from `file_system::listing::streaming` whenever it emits `listing-error`.
//!
//! Ring-buffered to 20 entries: enough to triage a problem, small enough that
//! every `cmdr://state` read pays a bounded YAML cost.

use std::collections::VecDeque;
use std::sync::{LazyLock, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

/// One recorded listing error.
#[derive(Debug, Clone)]
pub struct RecentListingError {
    /// Wall-clock millis since UNIX epoch; matches the format used by JS `Date.now()`.
    pub at_unix_ms: u64,
    pub listing_id: String,
    pub volume_id: String,
    pub path: String,
    /// Raw error text. Matches what the FE sees on the `listing-error` event.
    pub message: String,
}

const CAPACITY: usize = 20;

static BUFFER: LazyLock<Mutex<VecDeque<RecentListingError>>> =
    LazyLock::new(|| Mutex::new(VecDeque::with_capacity(CAPACITY)));

/// Record a listing error. Called from the streaming event sink right after it
/// emits the `listing-error` Tauri event, so MCP sees what the FE saw.
pub fn record(listing_id: &str, volume_id: &str, path: &str, message: &str) {
    let at_unix_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let entry = RecentListingError {
        at_unix_ms,
        listing_id: listing_id.to_string(),
        volume_id: volume_id.to_string(),
        path: path.to_string(),
        message: message.to_string(),
    };
    let mut buf = match BUFFER.lock() {
        Ok(b) => b,
        Err(poisoned) => poisoned.into_inner(),
    };
    if buf.len() == CAPACITY {
        buf.pop_front();
    }
    buf.push_back(entry);
}

/// Returns a snapshot of recorded errors, oldest first. Cheap; clones each entry.
pub fn snapshot() -> Vec<RecentListingError> {
    let buf = match BUFFER.lock() {
        Ok(b) => b,
        Err(poisoned) => poisoned.into_inner(),
    };
    buf.iter().cloned().collect()
}

/// Returns only entries with `at_unix_ms > since_ms`. Convenience for tests that
/// want to assert "no errors since I started this scenario." Kept under
/// `#[cfg(test)]` until a production caller needs it; the unit tests below
/// pin the semantics.
#[cfg(test)]
pub fn snapshot_since(since_ms: u64) -> Vec<RecentListingError> {
    snapshot().into_iter().filter(|e| e.at_unix_ms > since_ms).collect()
}

#[cfg(test)]
pub fn clear_for_test() {
    if let Ok(mut buf) = BUFFER.lock() {
        buf.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ignore_poison::IgnorePoison;

    /// `BUFFER` is process-global, so these tests can't run concurrently: each one clears it
    /// and then asserts on exactly what it recorded. Under a thread-per-test runner they
    /// interleave and clobber each other (they pass under nextest, which is process-per-test,
    /// so `pnpm check` stays green while a bare `cargo test` fails ~4 runs in 5). Every test
    /// here takes this lock for its whole body.
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn record_pushes_to_buffer_and_snapshot_returns_in_order() {
        let _guard = TEST_LOCK.lock_ignore_poison();
        clear_for_test();
        record("l1", "v1", "/a", "boom");
        record("l2", "v1", "/b", "kaboom");
        let snap = snapshot();
        assert_eq!(snap.len(), 2);
        assert_eq!(snap[0].listing_id, "l1");
        assert_eq!(snap[1].listing_id, "l2");
    }

    #[test]
    fn buffer_drops_oldest_past_capacity() {
        let _guard = TEST_LOCK.lock_ignore_poison();
        clear_for_test();
        for i in 0..(CAPACITY + 5) {
            record(&format!("l{i}"), "v", "/p", "err");
        }
        let snap = snapshot();
        assert_eq!(snap.len(), CAPACITY);
        // First retained entry should be `l5` (5 oldest dropped).
        assert_eq!(snap.first().unwrap().listing_id, "l5");
        // Last retained entry should be the newest push.
        assert_eq!(snap.last().unwrap().listing_id, format!("l{}", CAPACITY + 4));
    }

    #[test]
    fn snapshot_since_filters_by_timestamp() {
        let _guard = TEST_LOCK.lock_ignore_poison();
        clear_for_test();
        record("l1", "v", "/p", "err");
        let mid = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64;
        // Sleep just past the millisecond boundary so the next `record` definitely
        // gets a strictly later timestamp than `mid` (otherwise the test is flaky
        // on machines where SystemTime resolution is coarse enough that two
        // back-to-back calls land in the same millisecond).
        // allowed-test-sleep: crossing a millisecond boundary is the subject. `record` stamps with
        // the wall clock, so only elapsed real time can make `l2` strictly later than `mid`
        std::thread::sleep(std::time::Duration::from_millis(2));
        record("l2", "v", "/p", "err");
        let recent = snapshot_since(mid);
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].listing_id, "l2");
    }
}
