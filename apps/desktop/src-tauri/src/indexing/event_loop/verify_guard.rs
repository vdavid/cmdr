//! The cost guard for post-replay verification: two pure decisions that bound
//! what one pathological directory can cost a cold start.
//!
//! Both decisions are pure and threshold-injected (the `reconciler/rescan_route.rs`
//! shape), so the boundaries are pinned in the ms-scale unit tier and the
//! integration test can drive `verify_affected_dirs` with a tiny threshold instead
//! of a million-file fixture.
//!
//! Why two teeth and not one: `verify_affected_dirs` materialises every affected
//! path's DB children into a `HashMap<String, (i64, Vec<EntryRow>)>` BEFORE any
//! per-child work, and its Phase-2 loop `continue`s past DB-known children before
//! doing anything with them. So a directory that is already fully indexed costs
//! ~zero upserts while costing a full snapshot plus a full `read_dir` iteration —
//! an upsert cap would be a no-op on exactly the measured incident.
//!
//! - **Tooth 1** ([`classify_db_children`]) runs on a `LIMIT threshold + 1` probe
//!   BEFORE `list_children_on`, so an oversized path is never snapshotted.
//! - **Tooth 2** ([`classify_iteration`]) caps `read_dir` ITERATIONS in Phase 2,
//!   which also covers the directory that is small in the DB but huge on disk (it
//!   passes any DB-side count).
//!
//! ❌ A declined directory must NOT be marked unlisted. See `indexing/DETAILS.md`
//! § "Bounding verification cost (the two teeth)".

/// Child count above which background verification declines a directory.
///
/// Both teeth share it: "how many children make a directory pathological" is one
/// question, and answering it twice would let the DB-side and disk-side answers
/// drift. Sized between the two ends of the measured distribution on David's
/// machine (verified on macOS 15, index-DB child-count query, 2026-07-19): the
/// largest legitimate directory found was ~119k children, and the incident
/// directory was 1.14M (a Google Drive File Stream `fetch_temp` full of empty
/// files). 200k sits ~1.7× above the legitimate case and ~6× below the
/// pathological one.
///
/// The `huge_dirs_seen` census (`indexing/events.rs`) is the instrument for
/// revisiting this: it counts every directory listing at or over
/// `HUGE_DIR_CHILD_FLOOR` across the guarded walker, the full-rescan walk, and
/// the small-scope reconcile walk, so the constant can be re-derived from real
/// machines instead of one.
pub(in crate::indexing) const HUGE_DIR_CHILDREN: usize = 200_000;

/// What verification should do with a directory (or with one more iteration of
/// its `read_dir` loop).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::indexing) enum VerifyVerdict {
    /// Within budget: diff it.
    Diff,
    /// Over budget: skip the remaining work for this directory.
    Decline,
}

/// The SQL `LIMIT` for the tooth-1 probe. `threshold + 1` is the whole point: it
/// answers "more than the threshold?" while reading at most one row past it,
/// where a `COUNT(*)` would scan all 1.14M index rows.
pub(in crate::indexing) fn probe_limit(threshold: usize) -> i64 {
    threshold.saturating_add(1) as i64
}

/// Tooth 1: decide from the probed DB child count, BEFORE snapshotting.
///
/// `child_count` is the probe's row count, so it saturates at `threshold + 1`;
/// anything above the threshold declines. Exactly `threshold` children is still
/// diffed (the threshold is a ceiling we honour, not the first refusal).
pub(in crate::indexing) fn classify_db_children(child_count: usize, threshold: usize) -> VerifyVerdict {
    if child_count > threshold {
        VerifyVerdict::Decline
    } else {
        VerifyVerdict::Diff
    }
}

/// Tooth 2: decide whether the Phase-2 `read_dir` loop may take one more
/// iteration, given how many it has already taken.
///
/// Declines once `iterations_done` reaches the threshold, so a directory yields
/// at most `threshold` iterations — the same ceiling tooth 1 applies to the DB
/// side.
pub(in crate::indexing) fn classify_iteration(iterations_done: usize, threshold: usize) -> VerifyVerdict {
    if iterations_done >= threshold {
        VerifyVerdict::Decline
    } else {
        VerifyVerdict::Diff
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_limit_reads_exactly_one_row_past_the_threshold() {
        // The probe must distinguish "at the threshold" from "over it" without a
        // COUNT(*) over every child row.
        assert_eq!(probe_limit(0), 1);
        assert_eq!(probe_limit(200_000), 200_001);
    }

    #[test]
    fn a_directory_under_the_threshold_is_diffed() {
        assert_eq!(classify_db_children(0, 3), VerifyVerdict::Diff);
        assert_eq!(classify_db_children(2, 3), VerifyVerdict::Diff);
    }

    #[test]
    fn a_directory_exactly_at_the_threshold_is_diffed() {
        assert_eq!(classify_db_children(3, 3), VerifyVerdict::Diff);
        assert_eq!(
            classify_db_children(HUGE_DIR_CHILDREN, HUGE_DIR_CHILDREN),
            VerifyVerdict::Diff
        );
    }

    #[test]
    fn a_directory_over_the_threshold_is_declined() {
        assert_eq!(classify_db_children(4, 3), VerifyVerdict::Decline);
        // What the probe returns for the measured incident: it stops at
        // `threshold + 1` rows, so 1.14M children present as 200,001.
        assert_eq!(
            classify_db_children(HUGE_DIR_CHILDREN + 1, HUGE_DIR_CHILDREN),
            VerifyVerdict::Decline
        );
    }

    #[test]
    fn the_iteration_cap_allows_exactly_threshold_iterations() {
        // 0..threshold-1 proceed; the threshold-th call stops the loop, so the
        // directory yields exactly `threshold` iterations.
        assert_eq!(classify_iteration(0, 3), VerifyVerdict::Diff);
        assert_eq!(classify_iteration(2, 3), VerifyVerdict::Diff);
        assert_eq!(classify_iteration(3, 3), VerifyVerdict::Decline);
        assert_eq!(classify_iteration(4, 3), VerifyVerdict::Decline);
    }

    #[test]
    fn a_zero_threshold_declines_everything_with_children() {
        // Degenerate but reachable if the constant is ever mis-tuned; it must
        // decline rather than panic or wrap.
        assert_eq!(classify_db_children(0, 0), VerifyVerdict::Diff);
        assert_eq!(classify_db_children(1, 0), VerifyVerdict::Decline);
        assert_eq!(classify_iteration(0, 0), VerifyVerdict::Decline);
    }
}
