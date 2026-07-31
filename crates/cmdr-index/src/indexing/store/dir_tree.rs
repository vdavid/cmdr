//! The compact directory tree a whole-index walk reconstructs folder paths from.
//!
//! A volume's file rows stream by (they're the bulk of an index and are never all resident),
//! but the DIRECTORIES have to be held for the whole walk: rebuilding a folder's absolute
//! path means following parent pointers up to the root, in any order. That makes the
//! directory side the walk's floor, and how it's stored decides whether a big NAS index
//! costs tens of MB or hundreds.
//!
//! So this holds the minimum: one name arena plus a fixed 24-byte record per directory,
//! sorted by id and binary-searched. No per-directory heap allocation, no hash map, and
//! none of the metadata (sizes, times, inode, flags) that path reconstruction never reads.
//!
//! Both whole-index walks build on it: `media_index`'s image walk and `importance`'s
//! recompute walk, the latter hanging its own per-folder record off the row indices here.
//!
//! Depth, measurements, and the alternatives weighed: `media_index/scheduler/DETAILS.md` § The
//! walk and its memory floor.

use crate::indexing::store::{IndexStore, ROOT_ID};

/// One directory: its id, its parent's id, and the slice of [`DirTree::names`] holding its
/// name. 24 bytes, `Copy`, no owned allocation.
#[derive(Clone, Copy)]
struct DirRow {
    id: i64,
    parent_id: i64,
    name_start: u32,
    name_len: u32,
}

/// How many parent hops a path reconstruction will follow before giving up. Well past any
/// real tree (macOS caps a whole path at 1,024 bytes), so it only ever fires on an index
/// whose parent pointers form a cycle, where the alternative is spinning forever.
const MAX_DEPTH: usize = 4_096;

/// The message an arena that ran out of address space reports. Its own constant because
/// [`load`](DirTree::load) and every caller building a tree row by row raise the same one.
pub(crate) const ARENA_FULL: &str = "this index holds more folder-name bytes than the path arena can address";

/// Every directory in one volume's index, in the shape path reconstruction actually needs.
///
/// Build it with [`load`](DirTree::load) (the whole directory set in one query) or row by row
/// with [`push`](DirTree::push) when the caller's own query already streams the rows, then ask
/// for a folder's absolute path with [`path_into`](DirTree::path_into).
pub(crate) struct DirTree {
    /// Every directory name, concatenated back to back with no separators. One growable
    /// buffer instead of one heap `String` per directory: the same bytes, without hundreds of
    /// thousands of individual allocations and their per-block overhead.
    names: String,
    /// One record per directory, in `id` order (the query's `ORDER BY`), so a lookup is a
    /// binary search. A hash map would cost another ~24 bytes per directory (hashbrown
    /// rounds capacity up to a power of two) to save hops a walk makes only once per
    /// folder-with-files.
    rows: Vec<DirRow>,
    /// Reusable ancestor-chain scratch, so reconstructing a path allocates nothing after the
    /// first deep folder. It's why [`path_into`](DirTree::path_into) takes `&mut self`.
    chain: Vec<u32>,
}

impl DirTree {
    /// An empty tree, to fill with [`push`](DirTree::push).
    pub(crate) fn new() -> Self {
        Self {
            names: String::new(),
            rows: Vec::new(),
            chain: Vec::new(),
        }
    }

    /// Read every directory row of `conn`'s index into the compact form.
    ///
    /// Streams the rows ([`IndexStore::for_each_directory`]) rather than collecting them, so
    /// the transient peak is the compact structure itself, never a full `Vec<EntryRow>` on
    /// top of it.
    pub(crate) fn load(conn: &rusqlite::Connection) -> Result<Self, String> {
        let mut tree = Self::new();
        let mut arena_full = false;
        IndexStore::for_each_directory(conn, |id, parent_id, name, _modified_at| {
            if !tree.push(id, parent_id, name) {
                arena_full = true;
            }
        })
        .map_err(|e| e.to_string())?;
        if arena_full {
            return Err(ARENA_FULL.to_string());
        }
        Ok(tree)
    }

    /// Append one directory, returning `false` when the name arena is full.
    ///
    /// Rows MUST arrive in ascending `id` order (what every directory query's `ORDER BY id`
    /// gives): the id lookup is a binary search, so an out-of-order push silently breaks path
    /// reconstruction rather than failing loudly.
    ///
    /// The arena addresses names with a `u32`, which caps it at 4 GiB. No real index comes near
    /// that (a 13.5M-row NAS index holds 8 MB of directory names), but the caller bails honestly
    /// on `false` rather than silently dropping folders from the tree if one ever does.
    #[must_use]
    pub(crate) fn push(&mut self, id: i64, parent_id: i64, name: &str) -> bool {
        let (Ok(name_start), Ok(name_len)) = (u32::try_from(self.names.len()), u32::try_from(name.len())) else {
            return false;
        };
        self.names.push_str(name);
        self.rows.push(DirRow {
            id,
            parent_id,
            name_start,
            name_len,
        });
        true
    }

    /// How many directories the tree holds (the root sentinel included).
    pub(crate) fn len(&self) -> usize {
        self.rows.len()
    }

    /// The entry id of the directory at `index`.
    pub(crate) fn id_at(&self, index: usize) -> i64 {
        self.rows[index].id
    }

    /// The parent entry id of the directory at `index`.
    pub(crate) fn parent_at(&self, index: usize) -> i64 {
        self.rows[index].parent_id
    }

    /// The name of the directory at `index`, borrowed straight out of the arena.
    pub(crate) fn name_at(&self, index: usize) -> &str {
        let row = self.rows[index];
        let start = row.name_start as usize;
        &self.names[start..start + row.name_len as usize]
    }

    /// The tree index of directory `id`, or `None` for an id the tree doesn't hold (the
    /// root's parent, or a file's parent that vanished between queries).
    ///
    /// A binary search over the id-ordered rows: no hash map, and far more cache-friendly
    /// than one at this size.
    pub(crate) fn index_of(&self, id: i64) -> Option<usize> {
        self.rows.binary_search_by_key(&id, |row| row.id).ok()
    }

    /// Write the absolute path of directory `id` into `out`, replacing whatever it held.
    ///
    /// The root is `"/"`, and so is an id the tree doesn't know (a directory that vanished
    /// between the two queries). A chain that breaks partway yields the part it could follow,
    /// rooted at the break.
    ///
    /// Takes `&mut self` for the reusable chain buffer, not to mutate the tree: a whole walk
    /// reconstructs one path per folder-with-files, and each of those would otherwise allocate.
    pub(crate) fn path_into(&mut self, id: i64, out: &mut String) {
        match self.index_of(id) {
            Some(index) => self.path_at_into(index, out),
            None => {
                out.clear();
                out.push('/');
            }
        }
    }

    /// [`path_into`](DirTree::path_into) for a caller that already holds the row index —
    /// a whole-tree walk, which visits every row in order and would otherwise pay a
    /// binary search to find what it just iterated past.
    pub(crate) fn path_at_into(&mut self, index: usize, out: &mut String) {
        out.clear();
        self.chain.clear();
        let mut cursor = Some(index);
        while let Some(index) = cursor {
            if self.rows[index].id == ROOT_ID || self.chain.len() >= MAX_DEPTH {
                break;
            }
            self.chain.push(index as u32);
            cursor = self.index_of(self.rows[index].parent_id);
        }
        if self.chain.is_empty() {
            out.push('/');
            return;
        }
        for &index in self.chain.iter().rev() {
            let row = self.rows[index as usize];
            out.push('/');
            let start = row.name_start as usize;
            out.push_str(&self.names[start..start + row.name_len as usize]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build an index holding just the given `(id, parent_id, name)` directories, then load
    /// it into a [`DirTree`].
    fn tree_of(dirs: &[(i64, i64, &str)]) -> (tempfile::TempDir, DirTree) {
        let dir = tempfile::tempdir().expect("temp");
        let index_path = dir.path().join("index-root.db");
        let store = IndexStore::open(&index_path).expect("open index");
        for (id, parent_id, name) in dirs {
            IndexStore::insert_entry_v2_with_id(
                store.read_conn(),
                *id,
                *parent_id,
                name,
                true,
                false,
                None,
                None,
                None,
                None,
            )
            .expect("insert dir");
        }
        let tree = DirTree::load(store.read_conn()).expect("load");
        (dir, tree)
    }

    fn path_of(tree: &mut DirTree, id: i64) -> String {
        let mut out = String::new();
        tree.path_into(id, &mut out);
        out
    }

    #[test]
    fn a_nested_folder_resolves_to_its_absolute_path() {
        let (_dir, mut tree) = tree_of(&[
            (10, ROOT_ID, "photos"),
            (11, 10, "2024"),
            (12, 11, "trip to Åre"),
            (13, ROOT_ID, "docs"),
        ]);
        assert_eq!(path_of(&mut tree, 12), "/photos/2024/trip to Åre");
        assert_eq!(path_of(&mut tree, 10), "/photos");
        assert_eq!(path_of(&mut tree, 13), "/docs");
    }

    #[test]
    fn the_root_and_an_unknown_folder_are_both_the_root_path() {
        // An id the tree never saw means the folder vanished between the directory query
        // and the file query; treating it as the root keeps its files countable instead of
        // dropping them, which is what the walk did before the tree was compacted.
        let (_dir, mut tree) = tree_of(&[(10, ROOT_ID, "photos")]);
        assert_eq!(path_of(&mut tree, ROOT_ID), "/");
        assert_eq!(path_of(&mut tree, 999), "/");
    }

    #[test]
    fn a_folder_whose_ancestor_is_missing_resolves_from_the_break() {
        // Parent 50 is absent, so the chain stops there and the path starts at the deepest
        // ancestor the tree does hold.
        let (_dir, mut tree) = tree_of(&[(51, 50, "orphan"), (52, 51, "child")]);
        assert_eq!(path_of(&mut tree, 52), "/orphan/child");
    }

    #[test]
    fn reusing_the_output_buffer_never_leaves_a_previous_path_behind() {
        let (_dir, mut tree) = tree_of(&[(10, ROOT_ID, "a-very-long-folder-name"), (11, ROOT_ID, "b")]);
        let mut out = String::new();
        tree.path_into(10, &mut out);
        tree.path_into(11, &mut out);
        assert_eq!(out, "/b");
    }

    #[test]
    fn a_cycle_in_the_parent_pointers_terminates_instead_of_spinning() {
        // A corrupt index can point two folders at each other. The walk has to come back
        // with something rather than hang the pass thread forever.
        let (_dir, mut tree) = tree_of(&[(20, 21, "left"), (21, 20, "right")]);
        let path = path_of(&mut tree, 20);
        assert!(
            path.starts_with('/'),
            "a cycle still yields a rooted path, got {path:?}"
        );
    }
}
