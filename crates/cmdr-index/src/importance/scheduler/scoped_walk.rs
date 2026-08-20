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
//! **An origin bigger than the budget is DEMOTED, not descended.** A change to a file
//! sitting directly inside an origin can move that origin's own signals and propagate
//! up its ancestors, but it cannot move any DESCENDANT's, so reading the subtree only
//! to rewrite it identically is waste. [`plan_incremental_batch`] asks the index's own
//! `recursive_dir_count` how much of the volume an origin covers — one indexed
//! primary-key lookup, before any of it is read — and an over-budget origin is
//! rescored ALONE, with its subtree's stored rows left untouched.
//!
//! Depth and the accepted lossiness: `DETAILS.md` § The scoped walk.

use std::collections::{HashMap, HashSet};

use super::recompute::{RescoreScope, dedupe_nested_origins};
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

/// The most directories one origin's subtree may hold and still be worth descending.
///
/// Checked TWICE, cheaply then honestly. [`plan_incremental_batch`] compares it
/// against the index's own `recursive_dir_count` before reading anything, so an origin
/// sitting above most of the volume is demoted for one primary-key lookup rather than
/// a 31 ms abandoned probe. The descent then counts against the same number as it
/// goes, which is what catches an origin whose `dir_stats` row is missing or stale.
///
/// Sized so a descent stays a small fraction of a full walk's cost on a real index:
/// the full walk is ~9 µs per folder (5.5 s over 611,699 folders, measured
/// 2026-07-27), and a level-batched descent runs at a comparable per-directory cost,
/// so 20,000 directories is a few hundred milliseconds at worst.
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

/// Why a pass fell back to the full walk. Typed, never a message match;
/// the `Display` is for the debug log only.
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

/// One origin of a sanitized batch, and how the pass will read it.
struct PlannedOrigin {
    /// The origin's absolute path, exactly as the batch spelled it (which is the key
    /// the clear list and the marker map use).
    path: String,
    /// The directory id it resolved to, or `None` when the index has no such directory
    /// — deleted between the publish and this pass, whose rows still have to be
    /// cleared.
    id: Option<i64>,
    /// `true` when the origin's subtree is past [`SCOPED_WALK_MAX_DIRS`], so the pass
    /// rescores the origin ALONE and leaves every row beneath it untouched.
    demoted: bool,
}

/// The origins one incremental pass acts on, each with how the pass will read it.
///
/// Built before the walk because the two decisions are entangled: an origin's size
/// decides whether it's descended, and THAT decides whether it absorbs the origins
/// nested under it (a demoted origin doesn't read or clear its subtree, so it can't
/// stand in for them).
pub(super) struct BatchPlan {
    origins: Vec<PlannedOrigin>,
}

impl BatchPlan {
    /// The two path lists a pass acts on, given the walk it ended up taking: the
    /// origins whose subtree it CLEARS and re-inserts, and the demoted ones it
    /// rescores alone.
    ///
    /// A full walk read the whole volume, so demotion is moot there and every origin
    /// is cleared exactly as it was before the bound existed.
    pub(super) fn lists_for(&self, scope: RescoreScope) -> (Vec<String>, Vec<String>) {
        match scope {
            RescoreScope::WithAncestors => (self.origins.iter().map(|o| o.path.clone()).collect(), Vec::new()),
            RescoreScope::ChangedSubtreesOnly => (
                self.origins
                    .iter()
                    .filter(|o| !o.demoted)
                    .map(|o| o.path.clone())
                    .collect(),
                self.origins
                    .iter()
                    .filter(|o| o.demoted)
                    .map(|o| o.path.clone())
                    .collect(),
            ),
        }
    }
}

/// Resolve each sanitized origin, decide which are too big to descend, and drop the
/// ones another origin still covers.
///
/// The size question is answered by the index's own `recursive_dir_count`: one indexed
/// primary-key lookup per origin, before a single directory of the subtree is read.
/// It replaces the descend-until-the-cap probe entirely for the case that actually
/// fires — a dotfile write in `$HOME`, whose subtree is 83% of a real root volume.
pub(super) fn plan_incremental_batch(conn: &rusqlite::Connection, origins: &[String]) -> Result<BatchPlan, String> {
    let mut ids: HashMap<&str, Option<i64>> = HashMap::new();
    let mut demoted: HashSet<String> = HashSet::new();
    for origin in origins {
        if ids.contains_key(origin.as_str()) {
            continue;
        }
        let id = resolve_directory(conn, origin)?;
        if let Some(id) = id
            && subtree_is_over_budget(conn, id)?
        {
            demoted.insert(origin.clone());
        }
        ids.insert(origin, id);
    }

    let kept = dedupe_nested_origins(origins, &demoted);
    Ok(BatchPlan {
        origins: kept
            .into_iter()
            .map(|path| PlannedOrigin {
                id: ids.get(path.as_str()).copied().flatten(),
                demoted: demoted.contains(&path),
                path,
            })
            .collect(),
    })
}

/// Whether `id`'s subtree holds more directories than a scoped descent may read.
///
/// Reads `dir_stats.recursive_dir_count`, which the aggregator already maintains
/// exactly (it read 574,006 for a real `$HOME` against the 574,007 a full walk
/// counted, itself excluded). A directory with no `dir_stats` row yet reads as within
/// budget; the running count inside [`descend_subtree`] is the backstop for that and
/// for a stale one.
fn subtree_is_over_budget(conn: &rusqlite::Connection, id: i64) -> Result<bool, String> {
    let stats = IndexStore::get_dir_stats_by_id(conn, id).map_err(|e| e.to_string())?;
    Ok(stats.is_some_and(|s| s.recursive_dir_count > SCOPED_WALK_MAX_DIRS as u64))
}

/// Read the folders of `plan`'s subtrees out of `conn`, or say why the pass has to
/// take the full walk instead.
///
/// `plan` comes from [`plan_incremental_batch`] over an already-sanitized batch
/// ([`super::recompute::sanitize_incremental_batch`]), so no origin floors and none is
/// under a DESCENDED one. `previous_markers` maps an origin's path to the
/// `has_project_marker` its stored row carries; a missing entry means it has no row.
pub(super) fn try_scoped_walk(
    conn: &rusqlite::Connection,
    home: &str,
    plan: &BatchPlan,
    previous_markers: &HashMap<String, bool>,
) -> Result<ScopedWalkOutcome, String> {
    if plan.origins.len() > SCOPED_WALK_MAX_ORIGINS {
        return Ok(ScopedWalkOutcome::FullWalkNeeded(FullWalkReason::TooManyOrigins));
    }

    let mut rows = ScopedRows::default();
    for origin in &plan.origins {
        let Some(id) = origin.id else { continue };
        collect_ancestor_chain(conn, id, &mut rows)?;
        if origin.demoted {
            read_origin_alone(conn, id, &mut rows)?;
        } else if !descend_subtree(conn, id, &mut rows)? {
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
    carry_marker_below_for_demoted(&mut folders, plan, previous_markers);

    // The guard: an ancestor outside every subtree can only have moved if some
    // subtree's marker presence did.
    for origin in &plan.origins {
        let fresh = origin
            .id
            .and_then(|id| folders.folder_of_dir_id(id))
            .is_some_and(|index| folders.marker_presence_at(index));
        match previous_markers.get(&origin.path) {
            Some(&stored) if stored == fresh => {}
            Some(_) => {
                return Ok(ScopedWalkOutcome::FullWalkNeeded(FullWalkReason::MarkerPresenceFlipped));
            }
            // No stored row and nothing in the index either: the folder never scored
            // and still doesn't, so no ancestor above it can have moved.
            None if origin.id.is_none() => {}
            None => {
                return Ok(ScopedWalkOutcome::FullWalkNeeded(FullWalkReason::MarkerPresenceUnknown));
            }
        }
    }

    Ok(ScopedWalkOutcome::Scoped(folders))
}

/// Give each demoted origin back the `has_marker_below` its stored row carries.
///
/// **Decision/Why the stored value is the honest source.** Nothing under a demoted
/// origin was read, so the propagation above can't produce its marker-below flag — and
/// nothing under it CHANGED either: a change deeper in the tree makes that directory
/// its own origin (the `dir-changed` contract in
/// `../../indexing/lifecycle/CLAUDE.md`), where it is either descended or demoted in
/// its own right. Its own direct children ARE read, so a marker landing directly in
/// the origin still shows up in `has_direct_marker` and flips the guard above.
///
/// `has_project_marker` is `has_direct_marker || has_marker_below`, so seeding the
/// stored value here is exactly "the origin keeps the marker presence it had unless
/// its own listing gained one". ❌ It is deliberately ONE-directional: the last marker
/// disappearing from deep inside a demoted origin's subtree leaves the origin reading
/// project-adjacent until the next full pass. That is the same bounded staleness the
/// batch gate already accepts for a marker inside a floored subtree
/// (`DETAILS.md` § The batch gate), in the same advisory signal.
fn carry_marker_below_for_demoted(
    folders: &mut WalkedFolders,
    plan: &BatchPlan,
    previous_markers: &HashMap<String, bool>,
) {
    for origin in plan.origins.iter().filter(|o| o.demoted) {
        if previous_markers.get(&origin.path) != Some(&true) {
            continue;
        }
        if let Some(id) = origin.id
            && let Some(index) = folders.folder_of_dir_id(id)
        {
            folders.folder_at_mut(index).has_marker_below = true;
        }
    }
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

/// Read a DEMOTED origin: the directory itself, in scope, plus its direct child
/// directories, which are not.
///
/// The child directories are read because a project marker is often one
/// (`.git`/`.hg`/`.svn`), and [`fold_direct_children`] derives the origin's
/// `has_direct_marker` from the rows the walk holds. They are not in scope, so nothing
/// rescores them and nothing under them is read: one origin costs one level of
/// listing instead of its whole subtree.
fn read_origin_alone(conn: &rusqlite::Connection, root_id: i64, rows: &mut ScopedRows) -> Result<(), String> {
    let Some(root) = IndexStore::get_entry_by_id(conn, root_id).map_err(|e| e.to_string())? else {
        return Ok(());
    };
    rows.insert(root.id, root.parent_id, &root.name, root.modified_at, true);
    rows.scored_dirs += 1;
    IndexStore::for_each_child_directory_of(conn, &[root_id], |id, parent_id, name, modified_at| {
        rows.insert(id, parent_id, name, modified_at, false);
    })
    .map_err(|e| e.to_string())
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
