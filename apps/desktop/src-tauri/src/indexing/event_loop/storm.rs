//! Removal-storm coalescing for the live event loop (index-ledger plan, root
//! cause 7 — the 2–5 minute chew).
//!
//! `rm -rf` deletes depth-first (unlink every file, THEN rmdir each emptied
//! dir, the root LAST), and FSEvents reports that order faithfully. So the cheap
//! one-`DeleteSubtreeById` path fires only at the very END, after the reconciler
//! has already chewed through hundreds of thousands of per-file removals. When
//! the kernel doesn't coalesce a bulk delete for us, this module synthesizes the
//! coalescing: a per-batch detector escalates a removal burst to ONE subtree
//! rescan through the SAME machinery the coalesced case uses (`queue_must_scan_sub_dirs`),
//! and the caller drops the storm's strict-descendant per-file events.
//!
//! Pure helpers, unit-tested here; the stateful orchestration (queue the anchor,
//! read the reconciler's active-rescan scopes, drop + re-queue) lives in
//! `event_loop::process_live_batch`.

use crate::indexing::path_prefix;
use std::collections::HashMap;
use std::path::PathBuf;

/// Removals under one grouping prefix within a single ~1 s live batch that flip
/// that group from per-file processing to a coalesced subtree rescan. ~200 sits
/// well above organic per-batch delete rates (a handful to a few dozen) yet far
/// below storm scale (thousands per batch in the incident). Tune by measurement:
/// the negative-delta warn and the storm-detector log line are the tripwires.
/// Ties to the plan's design § "Removal-storm coalescing".
pub(super) const REMOVAL_STORM_THRESHOLD: usize = 200;

/// Depth cap (component count) for the storm GROUPING prefix. The cap only
/// decides which removals count as the same storm; the queued rescan anchors at
/// the group's DEEPEST COMMON ANCESTOR, which may reach far deeper (the incident
/// path is ~11 components). Anchoring at the cap would re-list a whole worktree
/// (`node_modules` and all) instead of just the deleted `target` — the exact
/// over-scope the cap was meant to prevent.
pub(super) const STORM_GROUP_PREFIX_DEPTH: usize = 8;

/// Given the batch's removal event paths (absolute, canonical), return the
/// anchors whose removal count under a depth-capped grouping prefix EXCEEDS
/// [`REMOVAL_STORM_THRESHOLD`]. Each anchor is that group's deepest common
/// ancestor — the tightest scope that still covers every removal in the group.
///
/// The result is deterministic-in-content but unordered (grouped via a
/// `HashMap`); callers queue every anchor, and `queue_must_scan_sub_dirs`
/// dedups + ancestor-collapses, so order doesn't matter.
pub(super) fn detect_storm_anchors(removal_paths: &[&str]) -> Vec<PathBuf> {
    let mut groups: HashMap<String, Vec<&str>> = HashMap::new();
    for &p in removal_paths {
        let key = path_prefix::capped_prefix(p, STORM_GROUP_PREFIX_DEPTH);
        groups.entry(key).or_default().push(p);
    }

    let mut anchors = Vec::new();
    for members in groups.into_values() {
        if members.len() > REMOVAL_STORM_THRESHOLD
            && let Some(anchor) = path_prefix::deepest_common_ancestor(members.iter().copied())
        {
            anchors.push(PathBuf::from(anchor));
        }
    }
    anchors
}

/// The rescan scope to re-queue when a removal event should be DROPPED (skipped
/// per-file) because it's a STRICT descendant of a queued-or-active rescan. The
/// scope's OWN removal event (path equal to a scope) is never dropped: it must
/// take the cheap `DeleteSubtreeById` path, since `reconcile_subtree` on a root
/// that's gone from disk deletes nothing and would strand the whole subtree.
///
/// Returns `Some(scope)` to drop-and-requeue, `None` to process per-file.
pub(super) fn scope_to_requeue<'a>(removal_path: &str, scopes: &'a [PathBuf]) -> Option<&'a PathBuf> {
    scopes
        .iter()
        .find(|s| path_prefix::is_strict_descendant(removal_path, &s.to_string_lossy()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_a_group_over_threshold_and_anchors_at_dca() {
        // A deep tree with THRESHOLD+1 files sharing a common ancestor deeper
        // than the grouping cap. The anchor must be the DCA, not the cap.
        let base = "/Users/x/projects/repo/worktrees/e2e/target/debug";
        let owned: Vec<String> = (0..=REMOVAL_STORM_THRESHOLD)
            .map(|i| format!("{base}/file{i}.o"))
            .collect();
        let refs: Vec<&str> = owned.iter().map(String::as_str).collect();

        let anchors = detect_storm_anchors(&refs);
        assert_eq!(anchors, vec![PathBuf::from(base)]);
    }

    #[test]
    fn below_threshold_yields_no_anchor() {
        let base = "/a/b/c/d/e/f/g/h/i";
        let owned: Vec<String> = (0..REMOVAL_STORM_THRESHOLD).map(|i| format!("{base}/f{i}")).collect();
        let refs: Vec<&str> = owned.iter().map(String::as_str).collect();
        assert!(detect_storm_anchors(&refs).is_empty());
    }

    #[test]
    fn scattered_removals_dont_trip_a_single_shallow_anchor() {
        // THRESHOLD+1 removals scattered across DIFFERENT capped-prefix groups:
        // no single group exceeds the threshold, so nothing coalesces (the cap
        // guards against re-listing a huge shared shallow ancestor).
        let owned: Vec<String> = (0..=REMOVAL_STORM_THRESHOLD)
            .map(|i| format!("/Users/x/proj{i}/target/debug/file.o"))
            .collect();
        let refs: Vec<&str> = owned.iter().map(String::as_str).collect();
        assert!(detect_storm_anchors(&refs).is_empty());
    }

    #[test]
    fn strict_descendant_drops_but_scope_root_survives() {
        let scopes = vec![PathBuf::from("/a/b/target")];
        // A strict descendant drops and re-queues the scope.
        assert_eq!(
            scope_to_requeue("/a/b/target/debug/x.o", &scopes),
            Some(&PathBuf::from("/a/b/target"))
        );
        // The scope's OWN removal (its rmdir arriving last) is never dropped.
        assert_eq!(scope_to_requeue("/a/b/target", &scopes), None);
        // An unrelated sibling isn't dropped.
        assert_eq!(scope_to_requeue("/a/b/other/x.o", &scopes), None);
        // An ancestor of the scope isn't dropped.
        assert_eq!(scope_to_requeue("/a/b", &scopes), None);
    }
}
