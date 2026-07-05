//! The parsed archive index: a synthetic directory tree plus the query surface
//! the volume layer reads. Format-agnostic — zip, tar, and 7z all feed the same
//! tree.
//!
//! Each format's `parse` (in [`super::zip`] / [`super::tar`] / [`super::sevenz`])
//! produces a `Vec<(RawEntry, H)>` — a format-neutral [`RawEntry`] plus a
//! per-format read handle `H`. [`build_index`] then does the format-independent
//! work over any `H`:
//!
//! 1. sanitize each name (Zip Slip defense, see [`super::name`]), and
//! 2. [`build_tree`] synthesizes the directory hierarchy from the entry path
//!    prefixes — most archives carry no explicit directory entries, so the tree
//!    is inferred from `a/b/c.txt` implying `a/` and `a/b/`.
//!
//! The pruned handle map is wrapped into an [`EntryStore`] variant; [`ArchiveIndex::open_read`]
//! dispatches per format to open a streaming reader.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use rc_zip::Entry;

use super::error::ArchiveError;
use super::format::ArchiveFormat;
use super::name::{QuarantineReason, SanitizedName, sanitize_entry_name};
use super::read::ArchiveEntryReader;
use super::source::ArchiveByteSource;

/// Maximum number of nodes (files plus synthesized directories) in one archive's
/// tree. The backstop against the many-entries axis of memory amplification: the
/// per-entry `MAX_COMPONENT_DEPTH` cap (in `name.rs`) bounds each entry's cost,
/// but a central directory with a huge number of deep-ish
/// entries could still sum to an oversized tree from a modest input. Exceeding
/// this fails the whole parse with a typed error rather than risking an OOM.
///
/// 2,000,000 is well beyond real archives (the Linux kernel source is ~90k
/// files, a Chromium checkout ~400k), so a legitimate archive never trips it; it
/// only fires on a hostile blow-up. It bounds node *count*; per-node path length
/// is separately bounded by the zip name field (`u16`, 64 KB), so worst-case
/// tree memory is bounded too.
pub(super) const MAX_TREE_NODES: usize = 2_000_000;

/// One node in the synthetic tree: a file, a directory (explicit or implied),
/// or a symlink. Archive-native — the volume layer maps this onto `FileEntry`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveNode {
    /// Final path component (the display name).
    pub name: String,
    /// Full inner path relative to the archive root, `/`-separated, no leading
    /// or trailing slash. The root directory is the empty string.
    pub path: String,
    pub is_dir: bool,
    pub is_symlink: bool,
    /// Uncompressed size in bytes. `None` for directories (synthetic dirs have
    /// no size; explicit dir entries carry a zero size we still report as None).
    pub size: Option<u64>,
    /// Compressed size in bytes. `None` for directories.
    pub compressed_size: Option<u64>,
    /// Last-modified time as a Unix timestamp in seconds. `None` for synthetic
    /// directories (inferred, so no real timestamp).
    pub modified: Option<i64>,
    /// Whether extracting this entry is blocked because it's encrypted.
    pub encrypted: bool,
}

impl ArchiveNode {
    fn root() -> Self {
        Self {
            name: String::new(),
            path: String::new(),
            is_dir: true,
            is_symlink: false,
            size: None,
            compressed_size: None,
            modified: None,
            encrypted: false,
        }
    }
}

/// The lightweight, pure input to [`build_tree`]: one accepted entry, already
/// sanitized. Decoupled from `rc_zip::Entry` so the tree builder can be tested
/// with hand-built seeds (no zip bytes, no field-heavy `Entry` construction).
#[derive(Debug, Clone)]
struct NodeSeed {
    /// Sanitized inner path: non-empty, no leading/trailing slash, no `..`.
    path: String,
    is_dir: bool,
    is_symlink: bool,
    size: Option<u64>,
    compressed_size: Option<u64>,
    modified: Option<i64>,
    encrypted: bool,
}

/// A format-neutral parsed entry: the fields every format's parser produces for
/// the shared tree builder. The per-format read handle (`H` in
/// [`build_index`]) travels alongside it, not inside.
pub(super) struct RawEntry {
    /// Raw, attacker-controlled entry name (pre-sanitization). `\` and `..` and
    /// absolute paths are handled by [`sanitize_entry_name`], identically for
    /// every format — the Zip Slip defense is format-independent.
    pub name: String,
    pub is_dir: bool,
    pub is_symlink: bool,
    /// Uncompressed size (0 for a dir/symlink is fine — dirs report `None`).
    pub size: u64,
    pub compressed_size: u64,
    pub modified: Option<i64>,
    pub encrypted: bool,
}

/// The per-format read handles, keyed by sanitized inner path. Each variant
/// knows how to open a streaming reader for one entry; [`Self::open_read`]
/// dispatches. The zip store IS the rc-zip entry map; tar and 7z carry their own
/// location/metadata handles plus the format state their reader needs.
pub(super) enum EntryStore {
    Zip(HashMap<String, Entry>),
    Tar(super::tar::TarStore),
    SevenZ(super::sevenz::SevenZStore),
}

impl EntryStore {
    /// Opens a streaming reader for the file at the already-normalized `inner`
    /// path. The caller ([`ArchiveIndex::open_read`]) has confirmed it's a file,
    /// not a directory, and passes the tree's `size` so every format reports the
    /// same uncompressed total for progress. Maps a missing handle to `NotFound`.
    fn open_read(
        &self,
        inner: &str,
        size: u64,
        source: Arc<dyn ArchiveByteSource>,
    ) -> Result<ArchiveEntryReader, ArchiveError> {
        match self {
            EntryStore::Zip(files) => {
                let entry = files
                    .get(inner)
                    .ok_or_else(|| ArchiveError::NotFound(inner.to_string()))?;
                super::zip::open_read(entry, source)
            }
            EntryStore::Tar(store) => store.open_read(inner, size, source),
            EntryStore::SevenZ(store) => store.open_read(inner, size, source),
        }
    }
}

/// The output of the shared tree builder plus the pruned per-format handle map,
/// before it's wrapped into an [`ArchiveIndex`] with a concrete [`EntryStore`].
pub(super) struct BuiltIndex<H> {
    nodes: HashMap<String, ArchiveNode>,
    children: HashMap<String, Vec<String>>,
    quarantined: Vec<(String, QuarantineReason)>,
    has_encrypted: bool,
    /// Read handles for the entries that ended up as real file nodes.
    files: HashMap<String, H>,
}

impl<H> BuiltIndex<H> {
    /// Wraps the built tree into an [`ArchiveIndex`], turning the handle map into
    /// a concrete [`EntryStore`] via `wrap`.
    pub(super) fn into_index(self, wrap: impl FnOnce(HashMap<String, H>) -> EntryStore) -> ArchiveIndex {
        ArchiveIndex {
            nodes: self.nodes,
            children: self.children,
            quarantined: self.quarantined,
            has_encrypted: self.has_encrypted,
            store: wrap(self.files),
        }
    }
}

/// A parsed archive index: the synthetic tree plus the per-file read handles.
/// Format-agnostic — the tree query surface is shared, and the format-specific
/// read handles live behind [`EntryStore`]. Cheap to share (`Arc`) and immutable
/// once built.
pub struct ArchiveIndex {
    /// Every node (files and directories, including the root) by inner path.
    nodes: HashMap<String, ArchiveNode>,
    /// Directory path -> its children's inner paths, pre-sorted (directories
    /// first, then case-insensitive by name).
    children: HashMap<String, Vec<String>>,
    /// Raw names dropped by the sanitizer, with the reason. Kept for diagnostics
    /// and tests (a hostile `../evil` never reaches the tree, but we record it).
    quarantined: Vec<(String, QuarantineReason)>,
    has_encrypted: bool,
    /// The per-format read handles for readable (non-directory) entries.
    store: EntryStore,
}

impl std::fmt::Debug for ArchiveIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ArchiveIndex")
            .field("nodes", &self.nodes.len())
            .field("quarantined", &self.quarantined.len())
            .field("has_encrypted", &self.has_encrypted)
            .finish()
    }
}

impl ArchiveIndex {
    /// Parses `source` into an index for the given `format`: reads the format's
    /// directory (zip central directory, one tar scan, or the 7z header),
    /// sanitizes names identically for every format, and synthesizes the tree.
    pub fn parse(source: Arc<dyn ArchiveByteSource>, format: ArchiveFormat) -> Result<Self, ArchiveError> {
        match format {
            ArchiveFormat::Zip => {
                let entries = super::zip::parse(source.as_ref())?;
                Ok(build_index(entries)?.into_index(EntryStore::Zip))
            }
            ArchiveFormat::Tar(codec) => {
                let entries = super::tar::parse(source, codec)?;
                Ok(build_index(entries)?
                    .into_index(|members| EntryStore::Tar(super::tar::TarStore::new(codec, members))))
            }
            ArchiveFormat::SevenZ => {
                let (entries, meta) = super::sevenz::parse(source)?;
                Ok(build_index(entries)?
                    .into_index(|members| EntryStore::SevenZ(super::sevenz::SevenZStore::new(meta, members))))
            }
        }
    }

    /// Lists the children of the directory at `inner_path`. Returns `None` if
    /// the path doesn't exist or isn't a directory. An empty directory returns
    /// `Some(vec![])`. Children are directories-first, then name-sorted.
    pub fn list(&self, inner_path: &str) -> Option<Vec<ArchiveNode>> {
        let key = normalize_lookup(inner_path);
        // A path is a listable directory iff it has a directory node.
        if !self.nodes.get(key).is_some_and(|n| n.is_dir) {
            return None;
        }
        let child_paths = match self.children.get(key) {
            Some(paths) => paths,
            None => return Some(Vec::new()),
        };
        Some(child_paths.iter().filter_map(|p| self.nodes.get(p).cloned()).collect())
    }

    /// Metadata for the node at `inner_path` (a file or a directory), or `None`
    /// if nothing exists there. The archive root (`""`) is a directory node.
    pub fn get(&self, inner_path: &str) -> Option<ArchiveNode> {
        let key = normalize_lookup(inner_path);
        self.nodes.get(key).cloned()
    }

    /// Whether `inner_path` exists in the archive.
    pub fn exists(&self, inner_path: &str) -> bool {
        self.nodes.contains_key(normalize_lookup(inner_path))
    }

    /// `Some(true)` if `inner_path` is a directory, `Some(false)` if a file,
    /// `None` if it doesn't exist.
    pub fn is_directory(&self, inner_path: &str) -> Option<bool> {
        self.nodes.get(normalize_lookup(inner_path)).map(|n| n.is_dir)
    }

    /// Whether the archive contains any encrypted entry. Browsing still works
    /// (names live in the central directory); this lets the volume layer warn
    /// or gate extraction up front.
    pub fn has_encrypted_entries(&self) -> bool {
        self.has_encrypted
    }

    /// Names the sanitizer dropped, with the reason. Empty for a clean archive.
    pub fn quarantined(&self) -> &[(String, QuarantineReason)] {
        &self.quarantined
    }

    /// Opens a streaming, chunk-by-chunk reader over the decompressed bytes of
    /// the file at `inner_path`, pulling compressed bytes from `source`.
    ///
    /// Errors: `NotFound` (no such path), `IsADirectory` (path is a directory),
    /// `Encrypted` (we don't decrypt). Decompression runs off the async
    /// executor; see [`ArchiveEntryReader`].
    pub fn open_read(
        &self,
        inner_path: &str,
        source: Arc<dyn ArchiveByteSource>,
    ) -> Result<ArchiveEntryReader, ArchiveError> {
        let key = normalize_lookup(inner_path);
        if self.nodes.get(key).is_some_and(|n| n.is_dir) {
            return Err(ArchiveError::IsADirectory(key.to_string()));
        }
        let size = self.nodes.get(key).and_then(|n| n.size).unwrap_or(0);
        self.store.open_read(key, size, source)
    }

    /// The uncompressed size of the file at `inner_path`, or `None` for a missing
    /// path or a directory. Used by the sequential one-pass extractor to size a
    /// member from the tree without touching the store.
    pub(super) fn file_size(&self, inner_path: &str) -> Option<u64> {
        let node = self.nodes.get(normalize_lookup(inner_path))?;
        (!node.is_dir).then_some(node.size.unwrap_or(0))
    }
}

/// Trims leading/trailing slashes so a caller's inner path (which may arrive as
/// `/foo/` or `foo`) matches the stored keys (no surrounding slashes; root is
/// `""`).
fn normalize_lookup(inner_path: &str) -> &str {
    inner_path.trim_matches('/')
}

/// Sanitizes and classifies each parsed entry (format-neutrally), then hands the
/// accepted seeds to [`build_tree`]. Stashes each readable file's per-format read
/// handle (pruned to the entries that end up as real file nodes) and records
/// quarantined names. Generic over the handle type `H`, so every format shares
/// the Zip Slip defense, the synthetic-tree logic, and the DoS caps.
pub(super) fn build_index<H>(entries: Vec<(RawEntry, H)>) -> Result<BuiltIndex<H>, ArchiveError> {
    let mut seeds: Vec<NodeSeed> = Vec::with_capacity(entries.len());
    let mut files: HashMap<String, H> = HashMap::new();
    let mut quarantined: Vec<(String, QuarantineReason)> = Vec::new();
    let mut has_encrypted = false;

    for (raw, handle) in entries {
        has_encrypted |= raw.encrypted;

        match sanitize_entry_name(&raw.name) {
            SanitizedName::Quarantined(reason) => quarantined.push((raw.name, reason)),
            SanitizedName::Accepted(path) => {
                seeds.push(NodeSeed {
                    path: path.clone(),
                    is_dir: raw.is_dir,
                    is_symlink: raw.is_symlink,
                    size: if raw.is_dir { None } else { Some(raw.size) },
                    compressed_size: if raw.is_dir { None } else { Some(raw.compressed_size) },
                    modified: raw.modified,
                    encrypted: raw.encrypted,
                });
                if !raw.is_dir {
                    // Later duplicate wins (some archives carry repeat names).
                    files.insert(path, handle);
                }
            }
        }
    }

    let (nodes, children) = build_tree(seeds, MAX_TREE_NODES)?;
    // A file the tree dropped (shadowed by a directory, or blocked by a file
    // ancestor) must not stay readable via `open_read`: keep a handle only for
    // paths that ended up as real file nodes.
    files.retain(|path, _| nodes.get(path).is_some_and(|node| !node.is_dir));
    Ok(BuiltIndex {
        nodes,
        children,
        quarantined,
        has_encrypted,
        files,
    })
}

/// Builds the directory tree from accepted entry seeds. Pure — no I/O — so the
/// synthetic-dir logic (implied ancestors, explicit-vs-implied, collisions) is
/// unit-tested directly.
///
/// Returns the node map (every path, files and dirs, including the root `""`)
/// and the per-directory child lists, pre-sorted directories-first then by
/// case-insensitive name.
/// The built tree: every node by inner path, plus each directory's pre-sorted
/// child-path list.
type BuiltTree = (HashMap<String, ArchiveNode>, HashMap<String, Vec<String>>);

fn build_tree(seeds: Vec<NodeSeed>, max_nodes: usize) -> Result<BuiltTree, ArchiveError> {
    let mut nodes: HashMap<String, ArchiveNode> = HashMap::new();
    let mut child_paths: HashMap<String, Vec<String>> = HashMap::new();
    // Every path whose parent link is already recorded. A path has exactly one
    // parent, so this global set prevents double-linking.
    let mut linked: HashSet<String> = HashSet::new();

    nodes.insert(String::new(), ArchiveNode::root());

    for seed in seeds {
        // Path collisions are resolved first-writer-wins. If an ancestor path is
        // already a FILE, this entry can't be placed under it — drop it rather
        // than leaving it as an unreachable orphan.
        if !ensure_ancestors(&seed.path, &mut nodes, &mut child_paths, &mut linked) {
            continue;
        }

        if seed.is_dir {
            // Create the dir, or upgrade an implied one with its real mtime. If a
            // file already holds this exact path, `upsert_dir` returns false and
            // the file wins — drop the dir entry.
            upsert_dir(&seed.path, seed.modified, &mut nodes, &mut child_paths, &mut linked);
        } else {
            // Yield to an existing directory at this path (the directory, with
            // children, wins). Otherwise create the file node, or overwrite an
            // earlier file of the same name (a later duplicate wins).
            let occupied_by_dir = nodes.get(&seed.path).is_some_and(|n| n.is_dir);
            if !occupied_by_dir {
                let node = ArchiveNode {
                    name: leaf_name(&seed.path).to_string(),
                    path: seed.path.clone(),
                    is_dir: false,
                    is_symlink: seed.is_symlink,
                    size: seed.size,
                    compressed_size: seed.compressed_size,
                    modified: seed.modified,
                    encrypted: seed.encrypted,
                };
                link_child(&seed.path, &mut child_paths, &mut linked);
                nodes.insert(seed.path.clone(), node);
            }
        }

        // Backstop against the many-entries amplification axis: refuse an
        // oversized tree rather than risk an OOM. One seed adds at most
        // MAX_COMPONENT_DEPTH nodes, so we overshoot the cap by a bounded margin
        // before catching it here.
        if nodes.len() > max_nodes {
            return Err(ArchiveError::TooLarge(format!(
                "directory tree exceeds the {max_nodes}-node limit"
            )));
        }
    }

    sort_children(&mut child_paths, &nodes);
    Ok((nodes, child_paths))
}

/// Ensures every ancestor directory of `path` exists as a synthetic dir node,
/// shallowest-first, so a parent is always created before its child. Returns
/// `false` if an ancestor path is already occupied by a FILE (so `path` can't be
/// placed under it); the caller drops the entry.
fn ensure_ancestors(
    path: &str,
    nodes: &mut HashMap<String, ArchiveNode>,
    child_paths: &mut HashMap<String, Vec<String>>,
    linked: &mut HashSet<String>,
) -> bool {
    let parts: Vec<&str> = path.split('/').collect();
    for depth in 1..parts.len() {
        let dir_path = parts[..depth].join("/");
        if !upsert_dir(&dir_path, None, nodes, child_paths, linked) {
            return false;
        }
    }
    true
}

/// Inserts a directory node at `dir_path` (creating it if absent) and links it
/// into its parent. If it already exists as a synthetic dir and `modified` is
/// `Some` (an explicit entry), upgrades its timestamp. Returns `false` if a FILE
/// already holds this exact path (first-writer-wins: the file keeps it).
fn upsert_dir(
    dir_path: &str,
    modified: Option<i64>,
    nodes: &mut HashMap<String, ArchiveNode>,
    child_paths: &mut HashMap<String, Vec<String>>,
    linked: &mut HashSet<String>,
) -> bool {
    match nodes.get_mut(dir_path) {
        Some(node) if node.is_dir => {
            // Existing dir: let an explicit entry fill in the real mtime.
            if node.modified.is_none() {
                node.modified = modified;
            }
            true
        }
        Some(_) => false, // a file already claimed this path
        None => {
            let node = ArchiveNode {
                name: leaf_name(dir_path).to_string(),
                path: dir_path.to_string(),
                is_dir: true,
                is_symlink: false,
                size: None,
                compressed_size: None,
                modified,
                encrypted: false,
            };
            link_child(dir_path, child_paths, linked);
            nodes.insert(dir_path.to_string(), node);
            true
        }
    }
}

/// Records `path` as a child of its parent directory, once.
fn link_child(path: &str, child_paths: &mut HashMap<String, Vec<String>>, linked: &mut HashSet<String>) {
    if !linked.insert(path.to_string()) {
        return;
    }
    let parent = parent_path(path);
    child_paths
        .entry(parent.to_string())
        .or_default()
        .push(path.to_string());
}

/// Sorts every directory's children: directories first, then case-insensitive
/// by name. Deterministic output for a stable listing.
fn sort_children(child_paths: &mut HashMap<String, Vec<String>>, nodes: &HashMap<String, ArchiveNode>) {
    for paths in child_paths.values_mut() {
        paths.sort_by(|a, b| {
            let na = &nodes[a];
            let nb = &nodes[b];
            nb.is_dir
                .cmp(&na.is_dir)
                .then_with(|| na.name.to_lowercase().cmp(&nb.name.to_lowercase()))
                .then_with(|| na.name.cmp(&nb.name))
        });
    }
}

/// The final path component of an inner path.
fn leaf_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// The parent directory of an inner path (`""` for a top-level entry).
fn parent_path(path: &str) -> &str {
    match path.rfind('/') {
        Some(idx) => &path[..idx],
        None => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file_seed(path: &str, size: u64) -> NodeSeed {
        NodeSeed {
            path: path.to_string(),
            is_dir: false,
            is_symlink: false,
            size: Some(size),
            compressed_size: Some(size),
            modified: Some(1_700_000_000),
            encrypted: false,
        }
    }

    fn dir_seed(path: &str) -> NodeSeed {
        NodeSeed {
            path: path.to_string(),
            is_dir: true,
            is_symlink: false,
            size: None,
            compressed_size: None,
            modified: Some(1_700_000_500),
            encrypted: false,
        }
    }

    fn child_names(children: &HashMap<String, Vec<String>>, dir: &str) -> Vec<String> {
        children
            .get(dir)
            .map(|v| v.iter().map(|p| leaf_name(p).to_string()).collect())
            .unwrap_or_default()
    }

    /// Builds the tree with an effectively-unlimited node cap (the node cap is
    /// tested separately by passing a small one).
    fn build(seeds: Vec<NodeSeed>) -> BuiltTree {
        build_tree(seeds, usize::MAX).expect("uncapped build should not exceed the node cap")
    }

    #[test]
    fn synthesizes_implied_directories_from_file_prefixes() {
        // No explicit dir entries: `a/b/c.txt` must imply `a/` and `a/b/`.
        let (nodes, children) = build(vec![file_seed("a/b/c.txt", 10)]);

        assert!(nodes.get("a").is_some_and(|n| n.is_dir), "a/ should be synthesized");
        assert!(nodes.get("a/b").is_some_and(|n| n.is_dir), "a/b/ should be synthesized");
        assert!(nodes.get("a/b/c.txt").is_some_and(|n| !n.is_dir));

        // Synthetic dirs carry no timestamp.
        assert_eq!(nodes["a"].modified, None);
        assert_eq!(nodes["a/b"].modified, None);

        assert_eq!(child_names(&children, ""), vec!["a"]);
        assert_eq!(child_names(&children, "a"), vec!["b"]);
        assert_eq!(child_names(&children, "a/b"), vec!["c.txt"]);
    }

    #[test]
    fn explicit_dir_entry_upgrades_the_implied_one() {
        // File first (implies `docs/`), then the explicit `docs/` entry: the
        // explicit timestamp must win, and `docs/` must not be duplicated.
        let (nodes, children) = build(vec![file_seed("docs/readme.md", 20), dir_seed("docs")]);

        assert!(nodes["docs"].is_dir);
        assert_eq!(nodes["docs"].modified, Some(1_700_000_500));
        // One entry for `docs` under root, not two.
        assert_eq!(child_names(&children, ""), vec!["docs"]);
    }

    #[test]
    fn explicit_dir_before_its_child_is_not_duplicated() {
        // Order-independence: explicit dir first, then a file inside it.
        let (nodes, children) = build(vec![dir_seed("pics"), file_seed("pics/a.png", 5)]);
        assert!(nodes["pics"].is_dir);
        assert_eq!(child_names(&children, ""), vec!["pics"]);
        assert_eq!(child_names(&children, "pics"), vec!["a.png"]);
    }

    #[test]
    fn mixed_tree_lists_dirs_before_files_alphabetically() {
        let (_nodes, children) = build(vec![
            file_seed("zeta.txt", 1),
            file_seed("alpha.txt", 1),
            dir_seed("mid"),
            file_seed("mid/inner.txt", 1),
        ]);
        // Directories first (mid), then files alpha (alpha.txt, zeta.txt).
        assert_eq!(child_names(&children, ""), vec!["mid", "alpha.txt", "zeta.txt"]);
    }

    #[test]
    fn deeply_nested_single_file_creates_the_whole_chain() {
        let (nodes, _children) = build(vec![file_seed("x/y/z/w/deep.bin", 3)]);
        for dir in ["x", "x/y", "x/y/z", "x/y/z/w"] {
            assert!(nodes.get(dir).is_some_and(|n| n.is_dir), "{dir} should exist");
        }
        assert_eq!(nodes["x/y/z/w/deep.bin"].size, Some(3));
    }

    #[test]
    fn root_node_always_exists_and_is_a_directory() {
        let (nodes, _children) = build(vec![]);
        assert!(nodes[""].is_dir);
        assert_eq!(nodes[""].path, "");
    }

    #[test]
    fn leaf_and_parent_helpers() {
        assert_eq!(leaf_name("a/b/c.txt"), "c.txt");
        assert_eq!(leaf_name("top"), "top");
        assert_eq!(parent_path("a/b/c.txt"), "a/b");
        assert_eq!(parent_path("top"), "");
    }

    #[test]
    fn tree_building_fails_when_node_count_exceeds_the_cap() {
        // Six single-node files plus the root is 7 nodes; a cap of 5 must fail.
        let seeds: Vec<NodeSeed> = (0..6).map(|i| file_seed(&format!("f{i}.txt"), 1)).collect();
        let err = build_tree(seeds.clone(), 5).unwrap_err();
        assert!(matches!(err, ArchiveError::TooLarge(_)), "got {err:?}");
        // The same seeds fit comfortably under a generous cap.
        assert!(build_tree(seeds, 1_000).is_ok());
    }

    #[test]
    fn file_shadowing_a_directory_path_is_first_writer_wins_both_orders() {
        // File `foo` first, then `foo/bar`: `foo` stays a file; `foo/bar` can't
        // live under a file, so it's dropped (not left as an unreachable orphan).
        let (nodes, children) = build(vec![file_seed("foo", 1), file_seed("foo/bar", 2)]);
        assert!(!nodes["foo"].is_dir, "first writer (the file) keeps the path");
        assert!(
            !nodes.contains_key("foo/bar"),
            "the shadowed child is dropped, not orphaned"
        );
        assert_eq!(child_names(&children, ""), vec!["foo"]);

        // Reverse order: `foo/bar` first implies dir `foo`; the later file `foo`
        // yields to the directory (which has children).
        let (nodes, children) = build(vec![file_seed("foo/bar", 2), file_seed("foo", 1)]);
        assert!(nodes["foo"].is_dir, "first writer (the implied dir) keeps the path");
        assert!(nodes.contains_key("foo/bar"));
        assert_eq!(child_names(&children, "foo"), vec!["bar"]);
    }
}
