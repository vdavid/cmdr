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
use crate::indexing::store::{EXCLUDED_SUBTREES_PRUNED_KEY, IndexStore};

use super::MutationTracker;

/// Delete every row beneath a directory named in `excluded_dir_names`, then record
/// `fingerprint` as the list this DB is pruned against.
///
/// The marker is written ONLY after the whole prune succeeded, so an interrupted
/// or failing run simply re-prunes on the next load (the work is idempotent).
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

    if roots.is_empty() {
        record_fingerprint(conn, fingerprint, signal);
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

    // A dir an older index DID list carries a non-zero `listed_epoch`; left alone,
    // the now-childless dir would roll up as an EXACT `0 B`. It was never listed
    // under today's rules, so 0 (unknown) is the truthful value.
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

    if deleted > 0 {
        mutation_tracker.bump();
        log::info!(
            "Writer: pruned {} beneath {} that the scanner doesn't walk",
            crate::pluralize::pluralize_with(deleted, "row", "rows"),
            crate::pluralize::pluralize(roots.len() as u64, "system dir"),
        );
    }
    record_fingerprint(conn, fingerprint, signal);
}

/// Record which exclusion list this DB is now pruned against.
fn record_fingerprint(conn: &rusqlite::Connection, fingerprint: &str, signal: &IndexFailureSignal) {
    if let Err(e) = IndexStore::update_meta(conn, EXCLUDED_SUBTREES_PRUNED_KEY, fingerprint) {
        signal.note(&e, "prune_excluded_subtrees: record fingerprint");
    }
}

#[cfg(test)]
mod tests;
