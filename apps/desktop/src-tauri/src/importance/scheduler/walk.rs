//! The index walk every recompute starts with: read a volume's folders out of the
//! index, fold each one's children into the small aggregate scoring needs, and
//! propagate the two subtree flags.
//!
//! Split from [`super::recompute`] (which scores and writes what this produces)
//! because the walk's memory shape is a subject of its own: it runs over
//! multi-million-row NAS indexes, so what it keeps per folder decides whether a
//! pass costs tens of MB or hundreds. Depth: `../DETAILS.md` § The walk.

use crate::importance::classify::{is_project_marker, self_floors};
use crate::importance::signals::ChildAggregate;
use crate::indexing::store::{ARENA_FULL, DirTree, IndexStore, ROOT_ID};

/// One folder the walk found, in the shape scoring needs and nothing more.
///
/// `Copy`, no owned allocation: what a folder is NAMED by (its id, parent, and name)
/// lives once in the walk's shared [`DirTree`], which this points into by row index,
/// and its absolute path is reconstructed on demand rather than stored. Both matter
/// at NAS scale — a full entry row plus a materialized path per folder is what made
/// this walk cost hundreds of MB.
#[derive(Clone, Copy)]
pub(crate) struct IndexFolder {
    /// This folder's row in the walk's tree: its id, parent id, and name.
    dir_index: u32,
    /// The folder's own modification time — the only column scoring reads off its
    /// entry row, so the only one kept.
    pub(crate) modified_at: Option<u64>,
    /// What this folder's direct children collapse to. Folded from the streamed file
    /// rows plus the sibling directory rows; the child rows themselves are never
    /// resident.
    pub(crate) children: ChildAggregate,
    /// `true` when a project marker sits in a DESCENDANT of this folder (a `.git`
    /// deep in a tree marks the whole path above it).
    pub(crate) has_marker_below: bool,
    /// `true` when a self-flooring ancestor (a denylisted, hidden, or system folder)
    /// sits above this one — so the whole subtree under a `node_modules` or a cache
    /// floors, not just the named folder. The downward twin of `has_marker_below`'s
    /// upward marker propagation.
    pub(crate) under_floored_ancestor: bool,
}

/// Every folder of one volume, plus the compact directory tree their paths come from.
///
/// The walk's whole output, and the only thing that stays resident while a pass
/// scores. Consumers reach the folders through [`for_each`](WalkedFolders::for_each),
/// which hands each one its reconstructed absolute path in a reused buffer.
pub(crate) struct WalkedFolders {
    /// Every directory in the index, the root sentinel included (paths reconstruct
    /// through it).
    tree: DirTree,
    /// One record per real folder (the root sentinel excluded), in the tree's id
    /// order — so `dir_index` ascends and a folder is findable by binary search.
    folders: Vec<IndexFolder>,
}

impl WalkedFolders {
    /// How many folders the walk found (the root sentinel isn't one).
    pub(crate) fn len(&self) -> usize {
        self.folders.len()
    }

    /// Whether the volume's index held no folder at all (an empty or unscanned index).
    pub(crate) fn is_empty(&self) -> bool {
        self.folders.is_empty()
    }

    /// Visit every folder with its reconstructed absolute path, in walk order.
    ///
    /// The path lands in one buffer reused across the whole visit, so iterating a
    /// NAS-sized volume allocates once rather than once per folder. Takes `&mut self`
    /// for that buffer and the tree's own ancestor scratch, not to mutate the walk.
    pub(crate) fn for_each(&mut self, mut visit: impl FnMut(&IndexFolder, &str)) {
        let mut path = String::new();
        for index in 0..self.folders.len() {
            // Copied out (it's a small `Copy` record) so the tree can borrow mutably
            // for its path scratch while the visitor reads the folder.
            let folder = self.folders[index];
            self.tree.path_at_into(folder.dir_index as usize, &mut path);
            visit(&folder, &path);
        }
    }

    /// The first `max` folders' absolute paths, reconstructing no more than that.
    ///
    /// For the `kMDItemLastUsedDate` sample, which queries at most its own cap and takes
    /// the first paths it's given: handing it the whole volume's paths would cost one
    /// heap `String` per folder, and reconstructing them all would cost a second full
    /// path pass, both for 500 paths' worth of use.
    pub(crate) fn first_paths(&mut self, max: usize) -> Vec<String> {
        let mut out = Vec::new();
        let mut path = String::new();
        for index in 0..self.folders.len().min(max) {
            self.tree
                .path_at_into(self.folders[index].dir_index as usize, &mut path);
            out.push(path.clone());
        }
        out
    }

    /// The folder index of directory `id`, or `None` when the id isn't a folder here
    /// (the root sentinel, or a file's parent that vanished between queries).
    ///
    /// Two binary searches, no hash map: id → tree row, then tree row → folder. The
    /// walk runs it once per folder-with-files, never per file.
    fn folder_of_dir_id(&self, id: i64) -> Option<usize> {
        self.folder_of_dir_index(self.tree.index_of(id)?)
    }

    /// The folder index of the directory at tree row `dir_index`, for a caller that
    /// already resolved the row. `None` for the root sentinel, the one directory that
    /// isn't a folder.
    fn folder_of_dir_index(&self, dir_index: usize) -> Option<usize> {
        let dir_index = u32::try_from(dir_index).ok()?;
        self.folders.binary_search_by_key(&dir_index, |f| f.dir_index).ok()
    }
}

/// Walk every directory in a volume's index and build each folder's record: its
/// mtime, its children's aggregated summary, and the two subtree flags.
///
/// **The memory shape is the point.** On a multi-million-entry NAS the directories
/// are a small fraction of the rows, so the walk holds ONLY them, and holds them
/// compactly: a shared [`DirTree`] (name arena plus a 24-byte record per directory)
/// plus one small `Copy` [`IndexFolder`] per folder. Paths are reconstructed on
/// demand from the tree rather than stored. File rows STREAM by
/// ([`for_each_file_child_by_parent`](IndexStore::for_each_file_child_by_parent)),
/// grouped by parent, so each directory's distinct-extension set lives in one reused
/// accumulator that closes at the group boundary — no per-folder set, and no file row
/// resident. So a pass costs O(dirs) in a small constant, not O(entries) and not the
/// hundreds of MB a row-per-folder shape reached on exactly the NAS-sized volumes SMB
/// scoring enables.
///
/// Directory children still come from the directory set itself (a `.git`/`.hg`
/// marker is a directory), so the direct-marker flag folds both the streamed file
/// children and the sibling directory children. `has_marker_below` is a single upward
/// propagation after the walk, so a `.git` deep in a tree raises its ancestors (plan
/// Decision 3); `under_floored_ancestor` is its downward twin.
pub(crate) fn walk_index_folders(conn: &rusqlite::Connection, home: &str) -> Result<WalkedFolders, String> {
    let mut walked = WalkedFolders {
        tree: DirTree::new(),
        folders: Vec::new(),
    };

    // The directory rows, ascending by id: one tree row each (the root sentinel
    // included, since paths reconstruct through it) and one folder record each.
    let mut arena_full = false;
    let mut folded = String::new();
    IndexStore::for_each_directory(conn, |id, parent_id, name, modified_at| {
        if !walked.tree.push(id, parent_id, name) {
            arena_full = true;
            return;
        }
        if id != ROOT_ID {
            walked.folders.push(IndexFolder {
                dir_index: (walked.tree.len() - 1) as u32,
                modified_at,
                children: ChildAggregate::default(),
                has_marker_below: false,
                under_floored_ancestor: false,
            });
        }
    })
    .map_err(|e| e.to_string())?;
    if arena_full {
        return Err(ARENA_FULL.to_string());
    }

    // Directory children first: a `.git`/`.hg`/`.svn` marker is a DIRECTORY, so fold
    // the directory set into each parent's direct-marker flag. (Directories never
    // contribute to the extension count or file count.)
    for dir_index in 0..walked.tree.len() {
        if walked.tree.id_at(dir_index) == ROOT_ID
            || !folded_is_project_marker(walked.tree.name_at(dir_index), &mut folded)
        {
            continue;
        }
        if let Some(folder) = walked.folder_of_dir_id(walked.tree.parent_at(dir_index)) {
            walked.folders[folder].children.has_direct_marker = true;
        }
    }

    fold_file_children(conn, &mut walked, &mut folded)?;
    propagate_floor_to_descendants(&mut walked, home);
    propagate_marker_to_ancestors(&mut walked);

    Ok(walked)
}

/// Stream the volume's file rows into their parents' [`ChildAggregate`]s.
///
/// The rows arrive grouped by parent, so one reusable accumulator serves the whole
/// scan: each group's distinct extensions, file count, and marker flag land on the
/// folder when the group closes, and the accumulator resets for the next one.
fn fold_file_children(
    conn: &rusqlite::Connection,
    walked: &mut WalkedFolders,
    folded: &mut String,
) -> Result<(), String> {
    let mut group_parent: Option<i64> = None;
    let mut group_folder: Option<usize> = None;
    let mut extensions = ExtensionGroup::default();
    let mut file_count: u32 = 0;
    let mut has_marker = false;
    let mut extension = String::new();

    IndexStore::for_each_file_child_by_parent(conn, |parent_id, name| {
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
        if folded_is_project_marker(name, folded) {
            has_marker = true;
        }
    })
    .map_err(|e| e.to_string())?;
    // The last group has no following row to close it.
    close_group(walked, group_folder, file_count, extensions.len(), has_marker);
    Ok(())
}

/// Write one directory's finished file-group aggregate onto its folder. A group whose
/// parent isn't a folder here (a file whose directory vanished between the two
/// queries) is dropped, exactly as a folder lookup miss always was.
fn close_group(
    walked: &mut WalkedFolders,
    folder: Option<usize>,
    file_count: u32,
    distinct_extension_count: u32,
    has_marker: bool,
) {
    let Some(folder) = folder else { return };
    let children = &mut walked.folders[folder].children;
    children.file_count = file_count;
    children.distinct_extension_count = distinct_extension_count;
    // A marker DIRECTORY child may already have set this; a file marker only adds.
    children.has_direct_marker |= has_marker;
}

/// Propagate the floor DOWN to descendants: a folder under a self-flooring ancestor (a
/// denylisted / hidden / system folder) floors too, so a `node_modules`'s whole subtree
/// floors, not just the folder named `node_modules`. The downward twin of the
/// marker-below upward propagation.
///
/// Seeds the self-flooring directories (classified from each folder's own path via the
/// shared `classify` predicate), then marks every DESCENDANT of a seed by walking its
/// parent chain and checking whether any ancestor is a seed. The seed set is a flag per
/// TREE row, so an ancestor check is a binary search plus a byte, never a hash lookup.
fn propagate_floor_to_descendants(walked: &mut WalkedFolders, home: &str) {
    let mut self_floored = vec![false; walked.tree.len()];
    let mut any_floored = false;
    let mut path = String::new();
    for index in 0..walked.folders.len() {
        let dir_index = walked.folders[index].dir_index as usize;
        walked.tree.path_at_into(dir_index, &mut path);
        if self_floors(&path, walked.tree.name_at(dir_index), home) {
            self_floored[dir_index] = true;
            any_floored = true;
        }
    }
    if !any_floored {
        return;
    }

    for index in 0..walked.folders.len() {
        let mut cursor = walked.tree.parent_at(walked.folders[index].dir_index as usize);
        while cursor != ROOT_ID {
            let Some(dir_index) = walked.tree.index_of(cursor) else {
                break;
            };
            if self_floored[dir_index] {
                walked.folders[index].under_floored_ancestor = true;
                break;
            }
            cursor = walked.tree.parent_at(dir_index);
        }
    }
}

/// Propagate a direct project marker up to every ancestor: a `.git` deep in a subtree
/// marks the whole path above it as project-adjacent (plan Decision 3). Seeds from each
/// folder's own direct-marker flag, then walks parent pointers.
fn propagate_marker_to_ancestors(walked: &mut WalkedFolders) {
    let seeds: Vec<u32> = walked
        .folders
        .iter()
        .filter(|f| f.children.has_direct_marker)
        .map(|f| f.dir_index)
        .collect();
    for seed in seeds {
        let mut cursor = walked.tree.parent_at(seed as usize);
        while cursor != ROOT_ID {
            let Some(dir_index) = walked.tree.index_of(cursor) else {
                break;
            };
            if let Some(folder) = walked.folder_of_dir_index(dir_index) {
                walked.folders[folder].has_marker_below = true;
            }
            cursor = walked.tree.parent_at(dir_index);
        }
    }
}

#[cfg(test)]
impl WalkedFolders {
    /// Build a walk over `paths` alone, with no index behind it — for a test that
    /// wants a specific folder SET (a rename transition, a floored subtree) rather
    /// than a specific index.
    ///
    /// Each path becomes a folder with a couple of mixed files (so an unfloored one
    /// scores above zero); ancestors that aren't themselves in `paths` still become
    /// tree rows, so every path reconstructs in full. The two subtree flags come from
    /// the SAME propagation production runs, so a synthetic walk can't disagree with a
    /// real one about what floors.
    pub(super) fn synthetic(paths: &[&str], home: &str) -> Self {
        // Every path plus every ancestor, sorted: a path's ancestors are its prefixes,
        // so sorting puts each parent before its children, which is what the tree's
        // ascending-id requirement needs.
        let mut directories: Vec<&str> = Vec::new();
        for path in paths {
            let mut current = *path;
            loop {
                directories.push(current);
                match current.rfind('/') {
                    Some(0) | None => break,
                    Some(position) => current = &current[..position],
                }
            }
        }
        directories.sort_unstable();
        directories.dedup();

        let id_of = |path: &str| ROOT_ID + 1 + directories.binary_search(&path).unwrap_or(0) as i64;
        let mut walked = Self {
            tree: DirTree::new(),
            folders: Vec::new(),
        };
        for (index, directory) in directories.iter().enumerate() {
            let (parent, name) = directory.rsplit_once('/').unwrap_or(("", *directory));
            let parent_id = if parent.is_empty() { ROOT_ID } else { id_of(parent) };
            assert!(walked.tree.push(id_of(directory), parent_id, name), "arena");
            if paths.contains(directory) {
                walked.folders.push(IndexFolder {
                    dir_index: index as u32,
                    modified_at: Some(1_000_000_000),
                    children: ChildAggregate {
                        distinct_extension_count: 3,
                        file_count: 4,
                        has_direct_marker: false,
                    },
                    has_marker_below: false,
                    under_floored_ancestor: false,
                });
            }
        }

        propagate_floor_to_descendants(&mut walked, home);
        propagate_marker_to_ancestors(&mut walked);
        walked
    }
}

/// The distinct lowercased extensions of ONE directory's file group.
///
/// A reused set of small buffers rather than a `HashSet<String>` per directory: a
/// folder holds a handful of distinct extensions, so a linear scan beats hashing, and
/// resetting between groups reuses the same allocations for the whole walk. Holding one
/// of these per directory instead cost ~280 bytes a folder (the table plus a `String`
/// per extension), which on a NAS-sized volume is most of a walk's memory.
#[derive(Default)]
struct ExtensionGroup {
    /// The distinct extensions seen so far, then spare buffers past `len` left over
    /// from earlier (bigger) groups.
    slots: Vec<String>,
    /// How many of `slots` the current group has filled.
    len: usize,
}

impl ExtensionGroup {
    /// Start a new group, keeping the buffers the last one used.
    fn reset(&mut self) {
        self.len = 0;
    }

    /// Add one file's extension (`""` for a file with none, which counts as its own
    /// distinct extension — the long-standing shape the scorer's weights are tuned on).
    fn insert(&mut self, extension: &str) {
        if self.slots[..self.len].iter().any(|seen| seen == extension) {
            return;
        }
        if self.len == self.slots.len() {
            self.slots.push(String::new());
        }
        let slot = &mut self.slots[self.len];
        slot.clear();
        slot.push_str(extension);
        self.len += 1;
    }

    /// How many distinct extensions the group holds.
    fn len(&self) -> u32 {
        self.len as u32
    }
}

/// Write `name`'s lowercased extension into `out` (empty when it has none).
///
/// Folds ASCII in place, which is every real extension; only a non-ASCII one takes the
/// allocating `to_lowercase` path (whose Unicode special cases we must keep, so the
/// fast path is gated on `is_ascii`, where the two agree by definition).
fn lowercased_extension_into(name: &str, out: &mut String) {
    out.clear();
    let Some(extension) = std::path::Path::new(name).extension().and_then(|e| e.to_str()) else {
        return;
    };
    if extension.is_ascii() {
        out.extend(extension.chars().map(|c| c.to_ascii_lowercase()));
    } else {
        out.push_str(&extension.to_lowercase());
    }
}

/// Whether `name` folds to a project marker, using `buf` as scratch.
///
/// Same ASCII fast path as [`lowercased_extension_into`]: markers are ASCII, but a
/// non-ASCII name can still fold onto one (U+212A KELVIN SIGN lowercases to `k`), so
/// the slow path stays exact rather than assuming it can't.
fn folded_is_project_marker(name: &str, buf: &mut String) -> bool {
    if name.is_ascii() {
        buf.clear();
        buf.extend(name.chars().map(|c| c.to_ascii_lowercase()));
        is_project_marker(buf)
    } else {
        is_project_marker(&name.to_lowercase())
    }
}
