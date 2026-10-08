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

use crate::indexing::paths::path_prefix;
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

/// One subtree rescan a removal storm escalates to.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct StormAnchor {
    /// Where the rescan walks.
    pub(super) path: PathBuf,
    /// How many of the batch's removals fall under it, for the log line.
    pub(super) removals: usize,
}

/// Given the batch's removal event paths (absolute, canonical), return the
/// rescans the batch's storms escalate to. A storm is a depth-capped grouping
/// prefix with more than [`REMOVAL_STORM_THRESHOLD`] removals under it, and
/// [`cluster_anchors`] decides how tightly each one anchors.
///
/// Sorted by path, so the log reads the same for the same batch; callers queue
/// every anchor, and `queue_must_scan_sub_dirs` dedups + ancestor-collapses.
pub(super) fn detect_storm_anchors(removal_paths: &[&str]) -> Vec<StormAnchor> {
    let mut groups: HashMap<String, Vec<&str>> = HashMap::new();
    for &p in removal_paths {
        let key = path_prefix::capped_prefix(p, STORM_GROUP_PREFIX_DEPTH);
        groups.entry(key).or_default().push(p);
    }

    let mut anchors = Vec::new();
    for members in groups.into_values() {
        if members.len() > REMOVAL_STORM_THRESHOLD {
            cluster_anchors(&members, &mut anchors);
        }
    }
    anchors.sort_by(|a, b| a.path.cmp(&b.path));
    anchors
}

/// Anchor one storm (`members.len() > REMOVAL_STORM_THRESHOLD`) as tightly as
/// it allows.
///
/// The members' deepest common ancestor covers them all, but one stray delete
/// elsewhere in the same batch can stretch it from `target/debug` to the whole
/// worktree, and the rescan then re-lists everything beneath (measured
/// 2026-10-05: a 225 s walk of a worktree root that changed 781 rows). So the
/// members split by the child of that ancestor they fall under. Every child
/// that's a storm on its own anchors separately, recursively, and the rest go
/// per-file, as long as they're no more than a threshold's worth. Past that,
/// the leftovers would be a storm of per-file deletes themselves, so the whole
/// storm keeps the common ancestor. Each split level lets at most
/// [`REMOVAL_STORM_THRESHOLD`] removals through to the per-file path.
fn cluster_anchors(members: &[&str], out: &mut Vec<StormAnchor>) {
    let Some(common) = path_prefix::deepest_common_ancestor(members.iter().copied()) else {
        return;
    };
    let mut by_child: HashMap<&str, Vec<&str>> = HashMap::new();
    // The ancestor's own removal (its `rmdir`) has no child to fall under.
    let mut per_file = 0;
    for &path in members {
        match child_under(path, &common) {
            Some(child) => by_child.entry(child).or_default().push(path),
            None => per_file += 1,
        }
    }
    let (storms, quiet): (Vec<_>, Vec<_>) = by_child
        .into_values()
        .partition(|children| children.len() > REMOVAL_STORM_THRESHOLD);
    per_file += quiet.iter().map(Vec::len).sum::<usize>();

    if storms.is_empty() || per_file > REMOVAL_STORM_THRESHOLD {
        out.push(StormAnchor {
            path: PathBuf::from(common),
            removals: members.len(),
        });
        return;
    }
    for storm in storms {
        cluster_anchors(&storm, out);
    }
}

/// The first component of `path` below `ancestor`, or `None` when `path` is
/// `ancestor` itself (or not under it).
fn child_under<'a>(path: &'a str, ancestor: &str) -> Option<&'a str> {
    let rest = if ancestor == "/" {
        path.strip_prefix('/')?
    } else {
        path.strip_prefix(ancestor)?.strip_prefix('/')?
    };
    rest.split('/').next().filter(|child| !child.is_empty())
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

    /// A git worktree at the depth David's live in, so the grouping cap lands on
    /// the worktree root exactly as it does in the field.
    const WORKTREE: &str = "/Users/me/projects-git/me/cmdr/.claude/worktrees/wt";

    fn anchor_paths(removals: &[&str]) -> Vec<String> {
        detect_storm_anchors(removals)
            .into_iter()
            .map(|a| a.path.to_string_lossy().into_owned())
            .collect()
    }

    /// `count` removals spread over `count / 10` subfolders of `dir`, the way a
    /// compiler clears its fingerprint and incremental dirs.
    fn removals_under(dir: &str, count: usize) -> Vec<String> {
        (0..count).map(|i| format!("{dir}/unit-{}/file{i}.o", i / 10)).collect()
    }

    #[test]
    fn detects_a_group_over_threshold_and_anchors_at_dca() {
        // A deep tree with THRESHOLD+1 files sharing a common ancestor deeper
        // than the grouping cap. The anchor must be the DCA, not the cap.
        let base = "/Users/x/projects/repo/worktrees/e2e/target/debug";
        let owned: Vec<String> = (0..=REMOVAL_STORM_THRESHOLD)
            .map(|i| format!("{base}/file{i}.o"))
            .collect();
        let refs: Vec<&str> = owned.iter().map(String::as_str).collect();

        assert_eq!(anchor_paths(&refs), vec![base.to_string()]);
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

    /// The field case behind a 225-second walk (2026-10-05): cargo clears
    /// thousands of files under `target/debug` while something else deletes a few
    /// dozen elsewhere in the worktree in the SAME batch. One deepest common
    /// ancestor over both is the worktree root, so the rescan re-listed the whole
    /// worktree (`node_modules` and all) for a delete that lived in `target/debug`.
    /// The stray few go per-file, and the storm anchors at its own cluster.
    #[test]
    fn a_few_stray_removals_dont_widen_the_anchor_to_the_worktree() {
        let target = format!("{WORKTREE}/target/debug");
        let mut owned = removals_under(&target, 1_400);
        owned.extend(removals_under(
            &format!("{WORKTREE}/apps/desktop/.svelte-kit/output"),
            60,
        ));
        let refs: Vec<&str> = owned.iter().map(String::as_str).collect();

        assert_eq!(
            detect_storm_anchors(&refs),
            vec![StormAnchor {
                path: PathBuf::from(&target),
                removals: 1_400,
            }]
        );
    }

    /// Two storms in one worktree in one batch are two anchors, each tight.
    #[test]
    fn two_storms_in_one_worktree_anchor_separately() {
        let target = format!("{WORKTREE}/target/debug");
        let modules = format!("{WORKTREE}/node_modules/.pnpm");
        let mut owned = removals_under(&target, 500);
        owned.extend(removals_under(&modules, 300));
        let refs: Vec<&str> = owned.iter().map(String::as_str).collect();

        assert_eq!(anchor_paths(&refs), vec![modules, target]);
    }

    /// A delete spread thin over many folders, none of them a storm alone, still
    /// coalesces at their common ancestor. Splitting it would send every removal
    /// down the per-file path this module exists to avoid.
    #[test]
    fn a_storm_spread_thin_keeps_its_common_ancestor() {
        let mut owned = Vec::new();
        for child in 0..10 {
            owned.extend(removals_under(
                &format!("{WORKTREE}/target/debug/build/crate-{child}"),
                150,
            ));
        }
        let refs: Vec<&str> = owned.iter().map(String::as_str).collect();

        assert_eq!(anchor_paths(&refs), vec![format!("{WORKTREE}/target/debug/build")]);
    }

    /// The split never lets more than a threshold's worth of removals out of a
    /// storm to the per-file path. A big cluster beside a big scatter keeps the
    /// wide anchor, because the scatter alone is a storm's worth of per-file
    /// deletes.
    #[test]
    fn a_split_never_leaks_more_than_a_threshold_to_the_per_file_path() {
        let mut owned = removals_under(&format!("{WORKTREE}/target/debug"), 1_000);
        for child in 0..30 {
            owned.extend(removals_under(&format!("{WORKTREE}/scattered-{child}"), 10));
        }
        let refs: Vec<&str> = owned.iter().map(String::as_str).collect();

        assert_eq!(anchor_paths(&refs), vec![WORKTREE.to_string()]);
    }

    /// The tightening recurses: a storm inside `target/` that is really a storm
    /// inside `target/debug/incremental` anchors there.
    #[test]
    fn the_split_tightens_level_by_level() {
        let incremental = format!("{WORKTREE}/target/debug/incremental");
        let mut owned = removals_under(&incremental, 900);
        owned.extend(removals_under(&format!("{WORKTREE}/target/debug/deps"), 40));
        owned.extend(removals_under(&format!("{WORKTREE}/target/rustdoc"), 40));
        owned.extend(removals_under(&format!("{WORKTREE}/apps"), 40));
        let refs: Vec<&str> = owned.iter().map(String::as_str).collect();

        assert_eq!(anchor_paths(&refs), vec![incremental]);
    }

    #[test]
    fn child_under_names_the_next_component() {
        assert_eq!(child_under("/a/b/c/d", "/a/b"), Some("c"));
        assert_eq!(child_under("/a/b/c", "/a/b"), Some("c"));
        assert_eq!(child_under("/a/x", "/"), Some("a"));
        // The ancestor itself, and a sibling sharing a name prefix, have no child.
        assert_eq!(child_under("/a/b", "/a/b"), None);
        assert_eq!(child_under("/a/bc/d", "/a/b"), None);
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
