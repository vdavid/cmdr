//! The walk an INCREMENTAL rescore uses: read only the changed subtrees out of the
//! index, instead of the whole volume.
//!
//! [`super::walk`] reads every directory and every file row of a volume — ~5.5 s
//! over a 611,699-folder root index — and the rescore that follows then throws
//! almost all of it away. This reads the same folders for the batch's subtrees
//! alone, and produces a [`WalkedFolders`] that is byte-identical to the full walk's
//! over those folders, so everything downstream is unchanged.
//!
//! **How the two whole-tree propagations survive being scoped** (the reason this was
//! deferred for a while, and the reason it is now safe):
//!
//! - `under_floored_ancestor` is exact from a folder's own path. A folder's ancestors
//!   are the prefixes of its absolute path, so `classify::under_floored_ancestor`
//!   sees a flooring `node_modules` far above the subtree root without reading it.
//! - `has_marker_below` is exact INSIDE a subtree: a subtree is downward-closed, so
//!   every descendant that could raise a folder is present.
//! - The one signal that crosses the boundary is a subtree's marker presence raising
//!   ancestors ABOVE the subtree. That is detected exactly rather than estimated: a
//!   subtree's marker presence is `has_project_marker` on its own row, so comparing
//!   the stored value against the fresh one says whether any ancestor outside could
//!   have moved. If it did, the pass takes the full walk instead.
//!
//! Depth and the accepted lossiness: `DETAILS.md` § The scoped incremental walk.

use std::collections::HashMap;

use super::walk::{
    ExtensionGroup, IndexFolder, WalkedFolders, folded_is_project_marker, lowercased_extension_into,
    propagate_marker_to_ancestors,
};
use crate::importance::classify::under_floored_ancestor;
use crate::importance::signals::ChildAggregate;
use crate::indexing::store::{ARENA_FULL, DirTree, IndexStore, ROOT_ID};

/// The most (de-duplicated) origins a batch may carry and still be worth scoping.
///
/// Each origin costs its own path resolution plus ancestor-chain read before any of
/// its subtree is read, and a batch this wide is usually a scan-shaped event whose
/// subtrees overlap most of the volume anyway. Past it, the full walk is both
/// simpler and cheaper.
pub(super) const SCOPED_WALK_MAX_ORIGINS: usize = 64;

/// The most directories a scoped walk may read before it gives up and lets the full
/// walk run instead.
///
/// Checked DURING the descent, so an origin that turns out to sit above most of the
/// volume costs a bounded probe rather than a slower-than-full-walk crawl. Sized so
/// the probe stays a small fraction of a full walk's cost on a real index: the full
/// walk is ~9 µs per folder (5.5 s over 611,699 folders, measured 2026-07-27), and a
/// level-batched descent runs at a comparable per-directory cost, so 20,000
/// directories is a few hundred milliseconds at worst.
pub(super) const SCOPED_WALK_MAX_DIRS: usize = 20_000;

/// Parent ids per batched child query. Well under SQLite's default 999-parameter
/// ceiling, and big enough that a wide level is a handful of queries.
const PARENT_CHUNK: usize = 256;

/// What [`try_scoped_walk`] came back with.
pub(super) enum ScopedWalkOutcome {
    /// The scoped walk holds every folder in the changed subtrees, with the same
    /// values the full walk would have produced for them.
    Scoped(WalkedFolders),
    /// The scoped walk can't stand in for the full one this pass. The caller runs
    /// the full walk (which is also the differential oracle) and rescores with the
    /// ancestor chain included.
    FullWalkNeeded(FullWalkReason),
}

/// Why a pass fell back to the full walk. Typed, never a message match
/// (`no-string-matching`); the `Display` is for the debug log only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FullWalkReason {
    /// The batch carried more than [`SCOPED_WALK_MAX_ORIGINS`] distinct subtrees.
    TooManyOrigins,
    /// The subtrees together held more than [`SCOPED_WALK_MAX_DIRS`] directories.
    SubtreesTooLarge,
    /// A subtree's marker presence changed, so ancestors ABOVE it may have to move
    /// with it — and only a whole-tree walk knows what else is below them.
    MarkerPresenceFlipped,
    /// An origin has no stored row to compare its marker presence against (a folder
    /// that appeared since the last pass covering it, or a store with no pass yet),
    /// so the comparison can't rule the ancestors out.
    MarkerPresenceUnknown,
    /// The index's name arena ran out of address space — the same bail the full walk
    /// takes.
    ArenaFull,
}

impl std::fmt::Display for FullWalkReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let text = match self {
            Self::TooManyOrigins => "too many changed subtrees",
            Self::SubtreesTooLarge => "the changed subtrees cover too much of the volume",
            Self::MarkerPresenceFlipped => "a subtree's project-marker presence changed",
            Self::MarkerPresenceUnknown => "an origin has no stored row to compare against",
            Self::ArenaFull => ARENA_FULL,
        };
        f.write_str(text)
    }
}

/// Read the folders of `origins`' subtrees out of `conn`, or say why the pass has to
/// take the full walk instead.
///
/// `origins` must already be sanitized and de-duplicated
/// ([`super::recompute::sanitize_incremental_batch`] then
/// [`super::recompute::dedupe_nested_origins`]), so no origin floors and none is
/// under another. `previous_markers` maps an origin's path to the
/// `has_project_marker` its stored row carries; a missing entry means it has no row.
pub(super) fn try_scoped_walk(
    conn: &rusqlite::Connection,
    home: &str,
    origins: &[String],
    previous_markers: &HashMap<String, bool>,
) -> Result<ScopedWalkOutcome, String> {
    if origins.len() > SCOPED_WALK_MAX_ORIGINS {
        return Ok(ScopedWalkOutcome::FullWalkNeeded(FullWalkReason::TooManyOrigins));
    }

    let mut rows = ScopedRows::default();
    // Origin path → its resolved directory id, for the marker comparison below. An
    // origin that doesn't resolve was deleted between the publish and this pass.
    let mut resolved: Vec<(&str, Option<i64>)> = Vec::new();

    for origin in origins {
        let Some(id) = resolve_directory(conn, origin)? else {
            resolved.push((origin, None));
            continue;
        };
        resolved.push((origin, Some(id)));
        collect_ancestor_chain(conn, id, &mut rows)?;
        if !descend_subtree(conn, id, &mut rows)? {
            return Ok(ScopedWalkOutcome::FullWalkNeeded(FullWalkReason::SubtreesTooLarge));
        }
    }

    let Some(mut folders) = rows.into_walk(conn)? else {
        return Ok(ScopedWalkOutcome::FullWalkNeeded(FullWalkReason::ArenaFull));
    };

    // The floor comes from each folder's own path (no propagation pass), and the
    // marker rises through the scoped tree exactly as it does through the full one —
    // an ancestor-chain row isn't a folder here, so the climb simply stops being
    // recorded once it leaves the subtree.
    folders.for_each_mut(|folder, path| {
        folder.under_floored_ancestor = under_floored_ancestor(path, home);
    });
    propagate_marker_to_ancestors(&mut folders);

    // The guard: an ancestor outside every subtree can only have moved if some
    // subtree's marker presence did.
    for (origin, id) in resolved {
        let fresh = id
            .and_then(|id| folders.folder_of_dir_id(id))
            .is_some_and(|index| folders.marker_presence_at(index));
        match previous_markers.get(origin) {
            Some(&stored) if stored == fresh => {}
            Some(_) => {
                return Ok(ScopedWalkOutcome::FullWalkNeeded(FullWalkReason::MarkerPresenceFlipped));
            }
            // No stored row and nothing in the index either: the folder never scored
            // and still doesn't, so no ancestor above it can have moved.
            None if id.is_none() => {}
            None => {
                return Ok(ScopedWalkOutcome::FullWalkNeeded(FullWalkReason::MarkerPresenceUnknown));
            }
        }
    }

    Ok(ScopedWalkOutcome::Scoped(folders))
}

impl WalkedFolders {
    /// Whether the folder at `index` has a project marker in it or below it — the
    /// `has_project_marker` signal, and exactly what a subtree contributes to its
    /// ancestors' `has_marker_below`.
    fn marker_presence_at(&self, index: usize) -> bool {
        let folder = self.folder_at(index);
        folder.children.has_direct_marker || folder.has_marker_below
    }
}

/// Resolve an absolute path to the id of the DIRECTORY the index holds for it, or
/// `None` when the index has no such directory.
///
/// Descends component by component from the root sentinel, so it matches the index's
/// own folded-name comparison and costs one indexed point query per level.
fn resolve_directory(conn: &rusqlite::Connection, path: &str) -> Result<Option<i64>, String> {
    let mut id = ROOT_ID;
    for component in path.split('/').filter(|c| !c.is_empty()) {
        match IndexStore::resolve_component(conn, id, component).map_err(|e| e.to_string())? {
            Some(next) => id = next,
            None => return Ok(None),
        }
    }
    if id == ROOT_ID {
        // The bare root is never an origin (`sanitize_incremental_batch` drops it),
        // and it isn't a scorable folder either.
        return Ok(None);
    }
    match IndexStore::get_entry_by_id(conn, id).map_err(|e| e.to_string())? {
        Some(entry) if entry.is_directory => Ok(Some(id)),
        _ => Ok(None),
    }
}

/// One directory the scoped walk read.
struct ScopedRow {
    parent_id: i64,
    name: String,
    modified_at: Option<u64>,
    /// Whether this row is a folder to SCORE. Ancestor-chain rows are present so
    /// paths reconstruct with the index's own names, but they sit outside the
    /// changed subtrees, so nothing rescores them.
    in_scope: bool,
}

/// The directory rows one scoped walk collected, before they're ordered into a tree.
#[derive(Default)]
struct ScopedRows {
    /// Every directory read, by entry id.
    rows: HashMap<i64, ScopedRow>,
    /// How many in-scope directories the descent has read, against
    /// [`SCOPED_WALK_MAX_DIRS`].
    scored_dirs: usize,
}

impl ScopedRows {
    fn insert(&mut self, id: i64, parent_id: i64, name: &str, modified_at: Option<u64>, in_scope: bool) {
        self.rows
            .entry(id)
            .and_modify(|row| row.in_scope |= in_scope)
            .or_insert_with(|| ScopedRow {
                parent_id,
                name: name.to_string(),
                modified_at,
                in_scope,
            });
    }

    /// Order the rows by id, build the tree, and fold every in-scope folder's direct
    /// children into its aggregate. `None` when the name arena filled up.
    fn into_walk(self, conn: &rusqlite::Connection) -> Result<Option<WalkedFolders>, String> {
        let mut ordered: Vec<(i64, ScopedRow)> = self.rows.into_iter().collect();
        // `DirTree` binary-searches its rows, so they MUST be pushed id-ascending —
        // the same contract `for_each_directory`'s `ORDER BY id` gives the full walk.
        ordered.sort_unstable_by_key(|(id, _)| *id);

        let mut tree = DirTree::new();
        let mut folders: Vec<IndexFolder> = Vec::new();
        let mut scored_ids: Vec<i64> = Vec::new();
        for (index, (id, row)) in ordered.iter().enumerate() {
            if !tree.push(*id, row.parent_id, &row.name) {
                return Ok(None);
            }
            if row.in_scope && *id != ROOT_ID {
                folders.push(IndexFolder {
                    dir_index: index as u32,
                    modified_at: row.modified_at,
                    children: ChildAggregate::default(),
                    has_marker_below: false,
                    under_floored_ancestor: false,
                });
                scored_ids.push(*id);
            }
        }

        let mut walked = WalkedFolders::from_parts(tree, folders);
        fold_direct_children(conn, &mut walked, &ordered, &scored_ids)?;
        Ok(Some(walked))
    }
}

/// Fold each in-scope folder's direct children into its [`ChildAggregate`]: the
/// marker flag from its child DIRECTORIES (a `.git` is a directory), and the file
/// count, distinct extensions, and marker flag from its child FILES.
fn fold_direct_children(
    conn: &rusqlite::Connection,
    walked: &mut WalkedFolders,
    ordered: &[(i64, ScopedRow)],
    scored_ids: &[i64],
) -> Result<(), String> {
    let mut folded = String::new();

    // Directory children come from the rows already read: the descent read every
    // directory under every origin, so a marker directory is always present.
    for (_, row) in ordered {
        if !folded_is_project_marker(&row.name, &mut folded) {
            continue;
        }
        if let Some(folder) = walked.folder_of_dir_id(row.parent_id) {
            walked.folder_at_mut(folder).children.has_direct_marker = true;
        }
    }

    // File children stream in per-parent groups, exactly as the full walk folds
    // them, so one reusable accumulator serves the whole scan.
    let mut group_parent: Option<i64> = None;
    let mut group_folder: Option<usize> = None;
    let mut extensions = ExtensionGroup::default();
    let mut file_count: u32 = 0;
    let mut has_marker = false;
    let mut extension = String::new();

    for chunk in scored_ids.chunks(PARENT_CHUNK) {
        IndexStore::for_each_child_file_of(conn, chunk, |parent_id, name| {
            if group_parent != Some(parent_id) {
                close_group(walked, group_folder, file_count, extensions.len(), has_marker);
                group_parent = Some(parent_id);
                group_folder = walked.folder_of_dir_id(parent_id);
                extensions.reset();
                file_count = 0;
                has_marker = false;
            }
            file_count += 1;
            lowercased_extension_into(name, &mut extension);
            extensions.insert(&extension);
            if folded_is_project_marker(name, &mut folded) {
                has_marker = true;
            }
        })
        .map_err(|e| e.to_string())?;
        // A chunk boundary is also a group boundary (each parent id sits in exactly
        // one chunk), so close the open group before the next query.
        close_group(walked, group_folder, file_count, extensions.len(), has_marker);
        group_parent = None;
        group_folder = None;
        extensions.reset();
        file_count = 0;
        has_marker = false;
    }
    Ok(())
}

/// Write one directory's finished file-group aggregate onto its folder.
fn close_group(
    walked: &mut WalkedFolders,
    folder: Option<usize>,
    file_count: u32,
    distinct_extension_count: u32,
    has_marker: bool,
) {
    let Some(folder) = folder else { return };
    let children = &mut walked.folder_at_mut(folder).children;
    children.file_count = file_count;
    children.distinct_extension_count = distinct_extension_count;
    // A marker DIRECTORY child may already have set this; a file marker only adds.
    children.has_direct_marker |= has_marker;
}

/// Read the chain of directories from `id` up to the root into `rows`, so every
/// scoped folder's path reconstructs in full — and from the index's OWN names, not
/// the batch's spelling of them (paths resolve on the folded name, so the two can
/// differ in case, and the stored `path` column has to match what a full pass
/// writes).
///
/// Chain rows are not in scope: they sit outside the changed subtrees, so nothing
/// rescores them.
fn collect_ancestor_chain(conn: &rusqlite::Connection, id: i64, rows: &mut ScopedRows) -> Result<(), String> {
    let mut cursor = id;
    loop {
        let Some(entry) = IndexStore::get_entry_by_id(conn, cursor).map_err(|e| e.to_string())? else {
            return Ok(());
        };
        if cursor != id {
            rows.insert(entry.id, entry.parent_id, &entry.name, entry.modified_at, false);
        }
        if entry.id == ROOT_ID || rows.rows.contains_key(&entry.parent_id) {
            return Ok(());
        }
        cursor = entry.parent_id;
    }
}

/// Read every directory at or under `root_id` into `rows`, level by level.
///
/// Returns `false` when the descent passed [`SCOPED_WALK_MAX_DIRS`], which means the
/// caller should take the full walk instead.
///
/// **Floored subtrees are descended too, deliberately.** A `node_modules` or a
/// `.git` scores nothing itself, but a project marker inside one still raises the
/// folders above it (`propagate_marker_to_ancestors` doesn't stop at a floor), so
/// skipping them would make this walk disagree with the full one. The size bail is
/// what keeps that affordable.
fn descend_subtree(conn: &rusqlite::Connection, root_id: i64, rows: &mut ScopedRows) -> Result<bool, String> {
    let Some(root) = IndexStore::get_entry_by_id(conn, root_id).map_err(|e| e.to_string())? else {
        return Ok(true);
    };
    rows.insert(root.id, root.parent_id, &root.name, root.modified_at, true);
    rows.scored_dirs += 1;

    let mut level = vec![root_id];
    while !level.is_empty() {
        let mut next: Vec<i64> = Vec::new();
        for chunk in level.chunks(PARENT_CHUNK) {
            let mut over_budget = false;
            IndexStore::for_each_child_directory_of(conn, chunk, |id, parent_id, name, modified_at| {
                rows.insert(id, parent_id, name, modified_at, true);
                rows.scored_dirs += 1;
                if rows.scored_dirs > SCOPED_WALK_MAX_DIRS {
                    over_budget = true;
                    return;
                }
                next.push(id);
            })
            .map_err(|e| e.to_string())?;
            if over_budget {
                return Ok(false);
            }
        }
        level = next;
    }
    Ok(true)
}
