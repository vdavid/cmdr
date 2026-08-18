//! Selectors: how the agent proposes 60 000 ops without naming 60 000 paths.
//!
//! The agent may THINK in patterns ("every installer in Downloads you've already opened"),
//! but a proposal is a concrete list. A selector is that pattern in terms the drive index can
//! answer, and it is resolved to concrete ops ONCE, at creation. The pattern survives as
//! display text; ❌ it is never re-resolved at approval, because freezing is what makes "what
//! the user saw is what runs" true.
//!
//! Resolution reads the **drive index**, ❌ never a live filesystem walk (the no-live-FS rule
//! in `agent/tools/CLAUDE.md`): a tool handler that walked the filesystem would block on a
//! dead mount and would read ground the user never consented to index.

use serde::{Deserialize, Serialize};

use crate::location::Location;
use crate::search::matcher::{Candidate, CompiledQuery, Evaluator};
use crate::search::types::{PatternType, SearchQuery};

/// A pattern the agent proposes over, in terms the drive index can answer. Every predicate is
/// DETERMINISTIC: the user can check each one against the file itself, which is what makes a
/// selector reviewable rather than a claim to take on faith.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpSelector {
    /// The subtree to look in.
    pub root: Location,
    /// A glob over the file NAME (`*.dmg`). `None` matches every name.
    pub name_glob: Option<String>,
    pub min_size: Option<u64>,
    pub max_size: Option<u64>,
    /// Only files last modified strictly before this unix second ("older than 30 days").
    pub modified_before: Option<i64>,
    /// Only files last modified strictly after this unix second.
    pub modified_after: Option<i64>,
}

impl OpSelector {
    /// The pattern as display text, for the group's name: a path and a glob, no prose.
    ///
    /// Deliberately symbolic. The predicates (age, size) are rendered from the stored JSON by
    /// the review dialog, where they can be localized; a sentence built here would ship one
    /// language into the database.
    pub fn pattern_text(&self) -> String {
        let root = self.root.path.trim_end_matches('/');
        match &self.name_glob {
            Some(glob) => format!("{root}/{glob}"),
            None => format!("{root}/"),
        }
    }

    /// The selector as the JSON stored in `proposals.selector`.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// The predicates as a compiled query, so a selector and a search agree about what
    /// `*.dmg` means. ❌ Never re-derive glob translation or case folding here: that fork is
    /// how a selector and the search box would disagree about the same pattern.
    fn compile(&self) -> Result<CompiledQuery, SelectorRefusal> {
        let query = SearchQuery {
            name_pattern: self.name_glob.clone(),
            pattern_type: PatternType::Glob,
            min_size: self.min_size,
            max_size: self.max_size,
            modified_after: self.modified_after.map(|t| t.max(0) as u64),
            modified_before: self.modified_before.map(|t| t.max(0) as u64),
            is_directory: Some(false),
            include_paths: None,
            exclude_dir_names: None,
            include_path_ids: None,
            limit: u32::MAX,
            case_sensitive: None,
            sort_by: None,
            exclude_system_dirs: Some(false),
            count_only: false,
        };
        // `Arena { entries: 0 }` never refuses a broad selector: the scope is one subtree the
        // agent named, and "everything in this folder" is a legitimate proposal.
        CompiledQuery::compile(&query, Evaluator::Arena { entries: 0 })
            .map_err(|e| SelectorRefusal::BadPattern { reason: e.to_string() })
    }

    /// Whether one indexed entry satisfies the selector.
    fn matches(&self, compiled: &CompiledQuery, name: &str, size: Option<u64>, modified_at: Option<u64>) -> bool {
        compiled.matches(&Candidate {
            name,
            is_directory: false,
            size,
            modified_at,
        })
    }
}

/// One file a selector resolved to, with what the index knew about it at that moment. The
/// snapshot rides onto the op row so the executor can tell at apply time whether the file
/// still is what the user reviewed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexedFile {
    /// The absolute path, as an executor takes it.
    pub path: String,
    pub size: Option<u64>,
    /// Modification time, unix seconds.
    pub modified_at: Option<i64>,
    pub inode: Option<u64>,
}

/// Why a selector produced nothing to propose. Typed, so a caller says "that drive isn't
/// indexed yet" rather than the flatly wrong "nothing matched".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectorRefusal {
    /// The volume has no live index, so the honest answer is "I can't see that drive", not an
    /// empty list.
    NotIndexed { volume_id: String },
    /// The root isn't on the volume the selector named.
    NotOnVolume { volume_id: String, path: String },
    /// The volume is indexed, but the root isn't in its index (a typo, a folder that's gone,
    /// or ground the index hasn't walked yet).
    RootNotFound { path: String },
    /// The name glob doesn't compile. Carries the regex crate's own explanation, for logging.
    BadPattern { reason: String },
    /// The index couldn't be read. Carries its own words, for logging only.
    IndexUnavailable { detail: String },
}

/// Where a selector resolves. A trait so the resolution logic is testable against a fixed set
/// of files, and so nothing here can reach for the filesystem by accident.
pub trait SelectorIndex {
    /// Every file under the selector's root that satisfies it, in a stable order.
    fn resolve(&self, selector: &OpSelector) -> Result<Vec<IndexedFile>, SelectorRefusal>;
}

/// The production resolver: the app's live drive index.
pub struct DriveIndex;

impl SelectorIndex for DriveIndex {
    fn resolve(&self, selector: &OpSelector) -> Result<Vec<IndexedFile>, SelectorRefusal> {
        let volume_id = selector.root.volume_id.as_str();
        let index = crate::index_host::index();
        let pool = index.read_pool(volume_id).ok_or_else(|| SelectorRefusal::NotIndexed {
            volume_id: volume_id.to_string(),
        })?;
        // A non-root volume's index stores paths relative to its mount root, so the root has
        // to be mapped in, and every resolved path mapped back out.
        let read_root =
            index
                .read_path(volume_id, &selector.root.path)
                .ok_or_else(|| SelectorRefusal::NotOnVolume {
                    volume_id: volume_id.to_string(),
                    path: selector.root.path.clone(),
                })?;
        let mount_root = crate::search::volumes::registry_mount_root(volume_id);
        let compiled = selector.compile()?;

        pool.with_conn(|conn| walk(conn, selector, &compiled, &read_root, mount_root.as_deref()))
            .map_err(|detail| SelectorRefusal::IndexUnavailable { detail })?
    }
}

/// Walk the root's subtree in the index, collecting what the selector matches.
///
/// Depth-first over one directory's children at a time, carrying each directory's path down
/// rather than reconstructing a path per file. Symlinks are skipped entirely: the index never
/// follows them, so their size and date describe the LINK, and a proposal built on those would
/// show the user facts about something other than the file they think they're deciding on.
fn walk(
    conn: &rusqlite::Connection,
    selector: &OpSelector,
    compiled: &CompiledQuery,
    read_root: &str,
    mount_root: Option<&str>,
) -> Result<Vec<IndexedFile>, SelectorRefusal> {
    use cmdr_index::store::{IndexStore, resolve_path};

    let root_id = resolve_path(conn, read_root)
        .map_err(|e| SelectorRefusal::IndexUnavailable { detail: e.to_string() })?
        .ok_or_else(|| SelectorRefusal::RootNotFound {
            path: selector.root.path.clone(),
        })?;

    let mut found = Vec::new();
    let mut stack = vec![(root_id, read_root.trim_end_matches('/').to_string())];
    while let Some((dir_id, dir_path)) = stack.pop() {
        let children = IndexStore::list_children_on(dir_id, conn)
            .map_err(|e| SelectorRefusal::IndexUnavailable { detail: e.to_string() })?;
        for row in children {
            if row.is_symlink {
                continue;
            }
            let path = format!("{dir_path}/{}", row.name);
            if row.is_directory {
                stack.push((row.id, path));
                continue;
            }
            if selector.matches(compiled, &row.name, row.logical_size, row.modified_at) {
                found.push(IndexedFile {
                    path: absolute_path(&path, mount_root),
                    size: row.logical_size,
                    modified_at: row.modified_at.map(|t| t as i64),
                    inode: row.inode,
                });
            }
        }
    }
    // A stable order, so the same selector over the same index freezes the same op sequence
    // twice running — which is what makes a resolution reviewable and a test deterministic.
    found.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(found)
}

/// Put a mount-relative index path back into the absolute form an executor takes.
fn absolute_path(index_path: &str, mount_root: Option<&str>) -> String {
    match mount_root {
        Some(root) => format!("{}/{}", root.trim_end_matches('/'), index_path.trim_start_matches('/')),
        None => index_path.to_string(),
    }
}
