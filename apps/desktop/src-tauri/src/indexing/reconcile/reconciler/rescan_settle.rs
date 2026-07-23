//! Settle delay for brand-new subtree anchors.
//!
//! An app updater unpacking a bundle, an installer staging a payload, a build
//! writing a fresh output tree: each creates a directory, fills it, and often
//! deletes it again within seconds. Every one of those raises `MustScanSubDirs`,
//! and walking the subtree immediately indexes rows for data that is gone before
//! the walk finishes.
//!
//! The [`super::rescan_throttle`] can't catch this: its signal is REPETITION, and
//! every one of these paths is unique, so no anchor ever reaches a second strike.
//! The signal here is YOUTH. An anchor whose directory was created less than
//! [`NEW_SUBTREE_SETTLE_DELAY`] ago is not walked yet; it stays queued and becomes
//! eligible once it has settled. Nothing is dropped or forgotten.
//!
//! ## Where the syscall lives
//!
//! The throttle engine is pure and clock-injected, which is why every one of its
//! rules is deterministically unit-testable. So the `stat` happens HERE, at the
//! enqueue call site, and only the resulting deadline crosses into the throttle,
//! exactly like `now` and `walk_cost` do. One `symlink_metadata` per enqueue on
//! the anchor itself, never a walk, and never with the throttle lock held.
//!
//! ## Birthtime, and failing open
//!
//! "Brand new" is a CREATION-time question, so this reads birthtime, not mtime: a
//! busy-but-established directory must not be delayed. Where birthtime is
//! unavailable (a filesystem or platform that doesn't record it, or the directory
//! already vanished) there is no deadline and the anchor walks as it does today.
//! Failing open is the only safe direction: a missing birthtime must never stall
//! an anchor.

use super::rescan_throttle::RescanThrottle;
use super::*;
use std::time::SystemTime;

/// How long a directory must have existed before its subtree is worth walking.
/// Long enough to outlive an updater unpacking and deleting a bundle, short
/// enough that a folder a person actually created shows its size promptly.
pub(in crate::indexing) const NEW_SUBTREE_SETTLE_DELAY: Duration = Duration::from_secs(30);

/// Record `path`'s settle deadline on `throttle`, so every eligibility question
/// (pick, hourglass hold, sweep tick) sees it. No deadline recorded = nothing to
/// wait for.
///
/// The `stat` runs with NO lock held: the throttle lock is taken once to read the
/// policy and once to store the result, never across a syscall.
pub(super) fn note_settle_deadline(throttle: &Mutex<RescanThrottle>, path: &Path, now: Instant) {
    let delay = throttle.lock_ignore_poison().settle_delay();
    let Some(deadline) = settle_deadline(path, now, delay) else {
        return;
    };
    throttle.lock_ignore_poison().note_settle_deadline(path, deadline);
}

/// The instant `path` has settled enough to walk, or `None` for "walk it now".
///
/// `None` covers every fail-open case: the directory is older than `delay`, its
/// birthtime is unreadable (no `st_birthtime` on this filesystem), it already
/// vanished, or the wall clock moved backwards and the directory reads as created
/// in the future.
fn settle_deadline(path: &Path, now: Instant, delay: Duration) -> Option<Instant> {
    let created = std::fs::symlink_metadata(path).ok()?.created().ok()?;
    let age = SystemTime::now().duration_since(created).ok()?;
    remaining_settle(age, delay).map(|remaining| now + remaining)
}

/// How much of `delay` a directory of `age` still has to sit out, or `None` once
/// it has settled. Pure, so the boundary is testable without a filesystem.
fn remaining_settle(age: Duration, delay: Duration) -> Option<Duration> {
    delay.checked_sub(age).filter(|remaining| !remaining.is_zero())
}

#[cfg(test)]
mod tests {
    use super::super::tests::{ensure_path_in_db, non_excluded_tempdir, setup_test_writer};
    use super::*;

    /// The case that pays for this module: a directory created a moment ago waits
    /// out (nearly) the whole delay before anything walks it.
    #[test]
    fn a_brand_new_directory_waits_out_the_delay() {
        let dir = tempfile::tempdir().expect("temp dir");
        let now = Instant::now();

        let deadline = settle_deadline(dir.path(), now, NEW_SUBTREE_SETTLE_DELAY).expect("a just-created dir settles");

        let remaining = deadline.duration_since(now);
        assert!(
            remaining > NEW_SUBTREE_SETTLE_DELAY - Duration::from_secs(5) && remaining <= NEW_SUBTREE_SETTLE_DELAY,
            "a dir created just now waits out about the full delay, got {remaining:?}"
        );
    }

    /// The guard: an ESTABLISHED directory is walked immediately, exactly as
    /// before. A zero delay is the same question as "older than the delay", asked
    /// of a real directory with a real birthtime.
    #[test]
    fn a_directory_older_than_the_delay_is_not_delayed() {
        let dir = tempfile::tempdir().expect("temp dir");
        assert!(
            settle_deadline(dir.path(), Instant::now(), Duration::ZERO).is_none(),
            "a directory past the delay walks now, with no deadline recorded"
        );
        assert_eq!(
            remaining_settle(
                NEW_SUBTREE_SETTLE_DELAY + Duration::from_secs(1),
                NEW_SUBTREE_SETTLE_DELAY
            ),
            None,
            "an age past the delay leaves nothing to sit out"
        );
        assert_eq!(
            remaining_settle(NEW_SUBTREE_SETTLE_DELAY, NEW_SUBTREE_SETTLE_DELAY),
            None,
            "exactly at the delay is settled, not a zero-length wait"
        );
        assert_eq!(
            remaining_settle(Duration::from_secs(10), NEW_SUBTREE_SETTLE_DELAY),
            Some(Duration::from_secs(20)),
            "a 10 s old dir sits out the remaining 20 s"
        );
    }

    /// Fail open, never closed: no birthtime to read (the path is gone, or the
    /// filesystem doesn't record one) must never stall an anchor.
    #[test]
    fn an_unreadable_birthtime_does_not_delay_the_anchor() {
        let dir = tempfile::tempdir().expect("temp dir");
        let missing = dir.path().join("vanished-before-we-looked");
        assert!(
            settle_deadline(&missing, Instant::now(), NEW_SUBTREE_SETTLE_DELAY).is_none(),
            "no birthtime means no delay: the anchor walks as it does today"
        );
    }

    /// The designed outcome: by the time an ephemeral subtree settles it is
    /// usually GONE. Walking a vanished root that was never indexed costs one
    /// failed stat and nothing else — no rows, and no escalation, so the drain
    /// takes the next anchor instead of re-queueing this one. Its DB rows (if it
    /// ever had any) are the FSEvents delete path's business, not the rescan
    /// drain's.
    #[test]
    fn a_vanished_anchor_walks_to_a_clean_no_op() {
        let (writer, dir, conn) = setup_test_writer();
        let db_path = dir.path().join("test-reconciler.db");
        let test_dir = non_excluded_tempdir();
        let anchor = test_dir.path().join("update.a1b2c3");
        std::fs::create_dir(&anchor).expect("create the ephemeral dir");
        // The parent is indexed (as after a scan); the ephemeral subtree never was.
        ensure_path_in_db(&db_path, &test_dir.path().to_string_lossy(), &writer);
        // ...and the updater deletes it while the anchor is still settling.
        std::fs::remove_dir_all(&anchor).expect("the updater cleans up");

        let cancelled = AtomicBool::new(false);
        let summary = reconcile_subtree(&anchor, &IndexPathSpace::root(), &conn, &writer, &cancelled).expect("walks");

        assert_eq!(
            (summary.added, summary.removed, summary.updated),
            (0, 0, 0),
            "a vanished root writes nothing"
        );
        assert!(
            summary.escalation.is_none(),
            "and re-queues nothing: there is no chain to heal"
        );
        writer.shutdown();
    }
}
