//! Prune index rows that today's scanner would never produce.
//!
//! The `Volume`-trait scanner keeps a NAS snapshot/system pseudo-dir's own row but
//! never walks its subtree (`network_scanner/system_dirs.rs`). Nothing used to
//! remove rows an OLDER index had already put there: a reconcile diffs each dir it
//! LISTS, and this one is never listed, so its children were invisible to every
//! later pass. On a real QNAP that left 10 898 710 orphaned rows — 80% of a
//! 13.5M-row, 1.88 GB index — inflating a 10 TB NAS to an 89 TB roll-up and making
//! every O(entries) walk pay 5×.
//!
//! ## Why this can't delete real user data
//!
//! The prune set is derived from the SAME predicate the scanner skips on, so
//! anything it removes is by construction something the current scanner would
//! never have written. Three narrowing steps, in order:
//!
//! 1. **Volume kind**: only trait-scanned (SMB/MTP) volumes get the message at all
//!    — the local walker indexes these folders in full. Gated in the message's one
//!    constructor, `system_dirs::prune_message_for_kind`.
//! 2. **SQL narrows**: `lower(name) IN (…)` over directories only.
//! 3. **Rust confirms**: every candidate name is re-checked with
//!    `eq_ignore_ascii_case` against the caller's list before anything is deleted,
//!    so a SQL/Rust semantic drift can only ever prune LESS, never more.
//!
//! The excluded directory's OWN row always survives (it stays listed and
//! navigable, a deliberate invariant), and its `listed_epoch` is reset to 0 so it
//! reads as honestly unknown (`—`) rather than an exact `0 B`.

use crate::indexing::IndexFailureSignal;
use crate::indexing::store::{EXCLUDED_SUBTREES_PRUNE_STARTED_KEY, EXCLUDED_SUBTREES_PRUNED_KEY, IndexStore};

use super::MutationTracker;

/// Delete every row beneath a directory named in `excluded_dir_names`, then record
/// `fingerprint` as the list this DB is pruned against.
///
/// ## Surviving an interruption
///
/// This runs at startup and takes 20–30 s on a real NAS index, so a user quitting
/// mid-run is ordinary, not exotic. Three things make that safe:
///
/// 1. **The deletes are post-order** (`delete_descendants_by_id`), so no
///    interruption can sever the tree and leave rows unreachable from the root.
///    Re-descending from the same root finds whatever is left.
/// 2. **An in-progress marker is written BEFORE the first delete** and cleared
///    only after the whole run finished, so the next load re-runs the prune even
///    if the fingerprint already matched.
/// 3. **Every run ends with an orphan sweep**, which collects rows an older,
///    top-down delete already stranded on installs carrying that damage. Nothing
///    else can: stranded rows are invisible to any descent from the root.
///
/// The fingerprint gate keeps all of it to once per DB per exclusion-list
/// version; none of it is a per-launch cost.
pub(super) fn handle_prune_excluded_subtrees(
    conn: &rusqlite::Connection,
    excluded_dir_names: &[String],
    fingerprint: &str,
    mutation_tracker: &MutationTracker,
    signal: &IndexFailureSignal,
) {
    let lowercase: Vec<String> = excluded_dir_names.iter().map(|n| n.to_ascii_lowercase()).collect();
    let candidates = match IndexStore::find_dirs_named_any_of(conn, &lowercase) {
        Ok(rows) => rows,
        Err(e) => {
            signal.note(&e, "prune_excluded_subtrees: find candidates");
            return;
        }
    };

    // Re-confirm with the Rust matcher: SQL may only narrow the set, never widen it.
    let roots: Vec<i64> = candidates
        .into_iter()
        .filter(|(_, name)| {
            excluded_dir_names
                .iter()
                .any(|excluded| name.eq_ignore_ascii_case(excluded))
        })
        .map(|(id, _)| id)
        .collect();

    // Durable "started, not finished" mark, ahead of every delete. Autocommitted,
    // so a quit or crash from here on leaves it behind for the next load to see.
    if let Err(e) = IndexStore::update_meta(conn, EXCLUDED_SUBTREES_PRUNE_STARTED_KEY, "1") {
        signal.note(&e, "prune_excluded_subtrees: mark in progress");
        return;
    }

    // Nested excluded dirs overlap (a snapshot tree holds a copy of the whole
    // share, `@Recycle` included). Descending from the outer one already deletes
    // the inner one's row, and the inner pass then finds nothing — the outcome is
    // the same in either order, because a nested excluded dir IS a descendant of an
    // excluded root and must go.
    let mut deleted: u64 = 0;
    for root_id in &roots {
        match IndexStore::delete_descendants_by_id(conn, *root_id) {
            Ok(n) => deleted += n,
            Err(e) => {
                signal.note(&e, &format!("prune_excluded_subtrees: delete under id={root_id}"));
                return;
            }
        }
    }

    // Collect anything an older top-down run severed from the root. Post-order
    // deletion means this run can't have created such rows, but a DB that already
    // carries them has no other way back: they're unreachable, so re-descending
    // from the excluded roots walks straight past them.
    let swept = match IndexStore::sweep_orphaned_entries(conn) {
        Ok(n) => n,
        Err(e) => {
            signal.note(&e, "prune_excluded_subtrees: sweep stranded rows");
            return;
        }
    };

    if !roots.is_empty() {
        // A dir an older index DID list carries a non-zero `listed_epoch`; left
        // alone, the now-childless dir would roll up as an EXACT `0 B`. It was
        // never listed under today's rules, so 0 (unknown) is the truthful value.
        if let Err(e) = IndexStore::clear_listed_epoch(conn, &roots) {
            signal.note(&e, "prune_excluded_subtrees: clear listed_epoch");
            return;
        }
        // Drop the stale inflated `dir_stats` rows too; the caller's
        // `ComputeAllAggregates` rewrites them, and until it runs "no row" reads as
        // unknown, which beats a row claiming 83 TB.
        if let Err(e) = IndexStore::delete_dir_stats_by_ids(conn, &roots) {
            signal.note(&e, "prune_excluded_subtrees: clear dir_stats");
            return;
        }
    }

    if deleted > 0 || swept > 0 {
        mutation_tracker.bump();
        log::info!(
            "Writer: pruned {} beneath {} that the scanner doesn't walk, and swept {} an earlier interrupted run had stranded",
            crate::pluralize::pluralize_with(deleted, "row", "rows"),
            crate::pluralize::pluralize(roots.len() as u64, "system dir"),
            crate::pluralize::pluralize_with(swept, "row", "rows"),
        );
    }
    record_fingerprint(conn, fingerprint, signal);
}

/// Record which exclusion list this DB is now pruned against, and drop the
/// in-progress mark.
///
/// Fingerprint first: whichever of the two writes an interruption lands between,
/// the next load still sees work pending (a missing fingerprint OR a leftover
/// mark), and re-running a finished prune is a no-op.
fn record_fingerprint(conn: &rusqlite::Connection, fingerprint: &str, signal: &IndexFailureSignal) {
    if let Err(e) = IndexStore::update_meta(conn, EXCLUDED_SUBTREES_PRUNED_KEY, fingerprint) {
        signal.note(&e, "prune_excluded_subtrees: record fingerprint");
        return;
    }
    if let Err(e) = IndexStore::delete_meta(conn, EXCLUDED_SUBTREES_PRUNE_STARTED_KEY) {
        signal.note(&e, "prune_excluded_subtrees: clear in-progress mark");
    }
}

#[cfg(test)]
mod tests;
