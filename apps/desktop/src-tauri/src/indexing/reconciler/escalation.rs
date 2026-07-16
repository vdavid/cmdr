//! Missing-parent escalation for the reconciler (Leak B).
//!
//! A live create/modify event whose parent chain isn't in the index used to be
//! dropped, leaking the credit (design § "Fix per leak" → Leak B). Instead we
//! resolve the deepest ancestor that IS in the index AS A DIRECTORY and queue a
//! rescan of the highest MISSING dir (the child of that deepest existing dir), so
//! `reconcile_subtree` anchors at an existing parent and discovers the whole
//! missing chain (it recurses into new dirs, credits via per-entry propagation,
//! and stamps coverage via `MarkDirsListed`).

use rusqlite::Connection;
use std::path::PathBuf;

use crate::indexing::IndexPathSpace;
use crate::indexing::store::IndexStore;

use super::compute_parent_path;

/// Resolve the escalation anchor for a `target_path` whose chain is (partly)
/// missing from the index: the rescan root that lets `reconcile_subtree` heal the
/// gap. Returns `None` when the whole chain already exists as directories.
///
/// Walks `target_path`'s ancestor chain from the volume root down, resolving each
/// prefix via `space.resolve_abs` (index-only, cheap). It stops at the first
/// GAP — a component that's absent, OR one that resolves to a FILE row (a file
/// counts as missing, else the escalation would parent new rows under a file id:
/// the type-change orphan class). The anchor is then:
///
/// - the highest MISSING dir (the child of the deepest existing dir) when the gap
///   is an absent component — `reconcile_subtree`'s root-not-in-DB fallback
///   creates it under the verified-dir parent and recurses; or
/// - the deepest existing dir itself when the gap is a FILE row — re-listing that
///   parent lets the diff delete the stale file row and insert the dir, healing
///   the type change (anchoring at the file would parent under it).
pub(super) fn resolve_escalation_anchor(
    space: &IndexPathSpace,
    conn: &Connection,
    target_path: &str,
) -> Option<PathBuf> {
    // The volume root always exists as a directory (index `ROOT_ID`); never walk
    // above it. For a mount-rooted drive `/Volumes/X` maps to the index root, and
    // paths above it (`/Volumes`, `/`) resolve to nothing — so the chain must stop
    // at the volume root or the walk would break before reaching any real prefix.
    let volume_root = space.volume_root_string();

    // Build the ancestor chain shallow→deep, including `target_path` itself.
    let mut chain: Vec<String> = Vec::new();
    let mut current = target_path.to_string();
    loop {
        chain.push(current.clone());
        if current == volume_root {
            break;
        }
        let parent = compute_parent_path(&current);
        if parent == current || parent.is_empty() {
            break;
        }
        current = parent;
    }
    chain.reverse();

    // Walk shallow→deep; track the deepest prefix that resolves to a DIRECTORY.
    let mut deepest_dir_idx: Option<usize> = None;
    let mut gap_is_file = false;
    for (i, prefix) in chain.iter().enumerate() {
        match space.resolve_abs(conn, prefix) {
            Ok(Some(id)) => {
                let is_dir = matches!(
                    IndexStore::get_entry_by_id(conn, id),
                    Ok(Some(entry)) if entry.is_directory
                );
                if is_dir {
                    deepest_dir_idx = Some(i);
                    continue;
                }
                // Resolves to a FILE row: treat as missing, stop here.
                gap_is_file = true;
                break;
            }
            // Absent, or a resolve error: stop. Everything below is missing.
            _ => break,
        }
    }

    let idx = deepest_dir_idx?;
    if gap_is_file {
        // Re-list the deepest existing dir to heal the stale file→dir type change.
        Some(PathBuf::from(&chain[idx]))
    } else if idx + 1 < chain.len() {
        // Highest missing dir: the child right below the deepest existing dir.
        Some(PathBuf::from(&chain[idx + 1]))
    } else {
        // The whole chain resolves as directories — nothing to escalate.
        None
    }
}
