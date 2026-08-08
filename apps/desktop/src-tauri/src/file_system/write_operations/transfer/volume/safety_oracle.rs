//! The safety oracle every merge and grid suite asserts through: ONE statement
//! of what a finished copy, move, or delete must have left behind, so the copy
//! matrix, the move matrix, and the coverage grid can't drift into asserting
//! different things about the same promise.
//!
//! ## The three clauses
//!
//! 1. **No byte the user didn't approve is gone from either side.** Every source
//!    file's content is readable from the source tree or the destination tree.
//!    Searched by CONTENT over whole trees, never by path: the Rename policy
//!    relocates a clashing item to a `name (1)` sibling, and for a clashing
//!    directory that shifts every file inside it. Fixture contents are unique per
//!    file, so presence in the bag is an honest "the data still exists". ❌ Don't
//!    "simplify" this into a path assertion.
//! 2. **Every byte the user did approve is at the destination**, at the path
//!    they'd go looking for it. A caller lists only the deliveries that hold
//!    under EVERY policy it drives (source-only files, typically); a clashing
//!    file's landing spot is a policy question, and clause 1 already covers it.
//! 3. **Every dest-only file the source didn't shadow is untouched**, byte for
//!    byte. That's the merge invariant.
//!
//! An operation with no destination (delete) has no clause 2: see
//! `safety_grid_tests.rs`, which says so per cell rather than passing an empty
//! list and calling it covered.
//!
//! ## Why the two merge fixtures stay separate
//!
//! `merge_tests.rs::make_rich_merge` and `move_merge_tests.rs`'s
//! `build_merge_source_tree` / `build_merge_dest_tree` are DIFFERENT TREES, not
//! two spellings of one. The clash contents differ (`b"SRC-clash-larger"` versus
//! `b"SRC-c"`), which is what decides whether `OverwriteSmaller` reduces to an
//! overwrite or to a skip, and the move fixture adds the second cross-type clash
//! (`/album/swap2`). Unifying them would quietly weaken a policy assertion on one
//! side, so this module extracts the ASSERTION only. `DETAILS.md` § "The safety
//! oracle".

use crate::file_system::volume::Volume;
use std::path::Path;
use std::sync::Arc;

/// Reads a whole file off a volume, or `None` when it isn't there.
pub(super) async fn try_read_all(vol: &Arc<dyn Volume>, path: &str) -> Option<Vec<u8>> {
    let mut stream = vol.open_read_stream(Path::new(path)).await.ok()?;
    let mut out = Vec::new();
    while let Some(Ok(chunk)) = stream.next_chunk().await {
        out.extend_from_slice(&chunk);
    }
    Some(out)
}

/// Every path reachable under `root`, walked recursively, directories suffixed
/// with `/`. Diagnostic only: it makes a failure message show the tree that was
/// actually there instead of just naming the byte that went missing.
pub(super) async fn collect_paths(vol: &Arc<dyn Volume>, root: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_string()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = vol.list_directory(Path::new(&dir), None).await else {
            continue;
        };
        for entry in entries {
            if entry.is_directory {
                stack.push(entry.path.clone());
                out.push(format!("{}/", entry.path));
            } else {
                out.push(entry.path.clone());
            }
        }
    }
    out.sort();
    out
}

/// Every file content reachable under `root`, walked recursively.
///
/// Clause 1 searches this bag rather than guessing paths, because Rename moves
/// files out from under any path a test could predict.
pub(super) async fn collect_contents(vol: &Arc<dyn Volume>, root: &str) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_string()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = vol.list_directory(Path::new(&dir), None).await else {
            continue;
        };
        for entry in entries {
            if entry.is_directory {
                stack.push(entry.path.clone());
            } else if let Some(bytes) = try_read_all(vol, &entry.path).await {
                out.push(bytes);
            }
        }
    }
    out
}

/// What a finished operation must have left behind, stated once.
///
/// Every path is RELATIVE to its root (`"/keep.txt"` under a `dest_root` of
/// `"/dest/album"`), so the same const table drives the cross-volume case and
/// the same-volume one, where both trees live on a single volume under
/// different prefixes.
pub(super) struct SafetySpec<'a> {
    /// Prefixed to every failure message, so a table-driven cell names itself.
    pub label: &'a str,
    /// The tree the operation read from, on the source volume.
    pub source_root: &'a str,
    /// The tree the operation wrote to, on the destination volume.
    pub dest_root: &'a str,
    /// Clause 1: every file the source started with, by content.
    pub source_files: &'a [(&'a str, &'a [u8])],
    /// Clause 2: every file that must be readable at the destination afterwards,
    /// under every policy the caller drives.
    pub delivered: &'a [(&'a str, &'a [u8])],
    /// Clause 3: every dest-only file that must still read back byte-identical.
    pub untouched_dest: &'a [(&'a str, &'a [u8])],
}

/// Asserts all three oracle clauses over a finished operation.
///
/// Pass the same volume twice for a same-volume operation; the two roots keep
/// the sides apart.
pub(super) async fn assert_operation_was_safe(source: &Arc<dyn Volume>, dest: &Arc<dyn Volume>, spec: &SafetySpec<'_>) {
    let label = spec.label;

    // Clause 1: no byte is gone from BOTH sides.
    let mut surviving = collect_contents(source, spec.source_root).await;
    surviving.extend(collect_contents(dest, spec.dest_root).await);
    for (rel, content) in spec.source_files {
        assert!(
            surviving.iter().any(|c| c == content),
            "{label}: source file {}{rel} is gone from BOTH sides — data destroyed.\n  source tree: {:?}\n  dest tree: {:?}",
            spec.source_root,
            collect_paths(source, spec.source_root).await,
            collect_paths(dest, spec.dest_root).await,
        );
    }

    // Clause 2: everything the user approved arrived, where they'd look for it.
    for (rel, content) in spec.delivered {
        let path = format!("{}{rel}", spec.dest_root);
        assert_eq!(
            try_read_all(dest, &path).await.as_deref(),
            Some(*content),
            "{label}: {path} didn't arrive at the destination"
        );
    }

    // Clause 3: THE MERGE INVARIANT — a dest-only file is never touched.
    for (rel, content) in spec.untouched_dest {
        let path = format!("{}{rel}", spec.dest_root);
        assert_eq!(
            try_read_all(dest, &path).await.as_deref(),
            Some(*content),
            "{label}: dest-only {path} was clobbered"
        );
    }
}
