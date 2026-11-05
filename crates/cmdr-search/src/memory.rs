//! A search source over rows a caller hands in, with no database: the browser demo's
//! tree, or a test's corpus.
//!
//! **Ids follow ONE defined order**: [`ARENA_ROOT_ID`] for the root, then depth-first
//! (a folder's whole subtree before its next sibling), each folder's children by the
//! byte order of their names. Ids aren't only lookup keys: every sort breaks ties on
//! the entry id and each result row carries it, so two sources agree on an answer only
//! if they agree on ids. The desktop's goldens insert their corpus in this order, and
//! the app's `search/source_equivalence_test.rs` holds the two sources to the same
//! answers.
//!
//! **Scope lookups fold through the volume's [`NameRules`]**, the fold the desktop
//! index keys `name_folded` on under `NameRules::host()`. ❌ Never
//! `cmdr_fs::name_fold::normalize_for_comparison` here: its browser branch is the
//! identity, which would disagree with a host that presents as a Mac.

use std::collections::HashMap;

use super::arena::{ARENA_ROOT_ID, ArenaBuilder, SearchIndex};
use super::name_rules::{NameFold, NameRules};
use super::seam::{SearchRows, SourceError};

/// The parent id the root row carries: no row has it (the desktop index's
/// `ROOT_PARENT_ID`).
const ROOT_PARENT_ID: i64 = 0;

/// One entry handed to [`MemorySource::new`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryRow {
    /// The entry's path below the volume root, `/`-led (`/photos/2024/a.jpg`). The
    /// root itself is implied and never listed.
    pub path: String,
    /// Whether it's a folder.
    pub is_directory: bool,
    /// A file's own size in bytes; `None` when unknown. A folder's size comes from a
    /// [`crate::DirSizeSource`], never from here.
    pub size: Option<u64>,
    /// Last modified, in seconds since the Unix epoch; `None` when unknown.
    pub modified_at: Option<u64>,
}

/// Why rows couldn't become a source. Each names the row's path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemorySourceError {
    /// A path that isn't `/`-led, is the root itself, or has an empty component.
    MalformedPath(String),
    /// Two rows with the same path.
    DuplicatePath(String),
    /// A row whose parent folder isn't among the rows.
    MissingParent(String),
    /// A row whose parent is a file.
    ParentIsAFile(String),
}

impl std::fmt::Display for MemorySourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MalformedPath(path) => write!(f, "not a `/`-led path below the root: {path:?}"),
            Self::DuplicatePath(path) => write!(f, "listed twice: {path:?}"),
            Self::MissingParent(path) => write!(f, "its parent folder isn't listed: {path:?}"),
            Self::ParentIsAFile(path) => write!(f, "its parent is a file: {path:?}"),
        }
    }
}

/// A volume's rows held in memory: the arena, its mount prefix, and the
/// `(parent id, folded name) → id` map scope lookups walk.
#[derive(Debug)]
pub struct MemorySource {
    arena: SearchIndex,
    path_prefix: String,
    fold: NameFold,
    children: HashMap<(i64, String), i64>,
}

impl MemorySource {
    /// Build a source over `rows`, in any order, every folder listed before or after
    /// what it holds. `path_prefix` is the mount root a row's path is prefixed with in
    /// results (empty for a `/`-rooted volume); `rules` is the volume's name rules,
    /// whose fold keys scope lookups.
    pub fn new(rows: Vec<MemoryRow>, path_prefix: &str, rules: NameRules) -> Result<Self, MemorySourceError> {
        let mut by_path: HashMap<&str, usize> = HashMap::with_capacity(rows.len());
        let mut children: HashMap<&str, Vec<usize>> = HashMap::new();
        for (i, row) in rows.iter().enumerate() {
            let (parent, name) = split(&row.path).ok_or_else(|| MemorySourceError::MalformedPath(row.path.clone()))?;
            if by_path.insert(&row.path, i).is_some() {
                return Err(MemorySourceError::DuplicatePath(row.path.clone()));
            }
            debug_assert!(!name.is_empty());
            children.entry(parent).or_default().push(i);
        }
        for (parent, kids) in &mut children {
            if !parent.is_empty() {
                let orphan = || rows[kids[0]].path.clone();
                match by_path.get(parent) {
                    None => return Err(MemorySourceError::MissingParent(orphan())),
                    Some(&p) if !rows[p].is_directory => return Err(MemorySourceError::ParentIsAFile(orphan())),
                    Some(_) => {}
                }
            }
            kids.sort_unstable_by(|&a, &b| name_of(&rows[a].path).as_bytes().cmp(name_of(&rows[b].path).as_bytes()));
        }

        let name_bytes = rows.iter().map(|row| name_of(&row.path).len()).sum();
        let mut arena = ArenaBuilder::with_capacity(rows.len() + 1, name_bytes);
        arena.push(ARENA_ROOT_ID, ROOT_PARENT_ID, "", true, None, None);
        let mut lookup = HashMap::with_capacity(rows.len());
        let mut next_id = ARENA_ROOT_ID + 1;
        // Preorder with an explicit stack (a deep tree can't overflow it), siblings
        // pushed in reverse so the smallest name comes off first.
        let mut stack: Vec<(i64, usize)> = Vec::new();
        let push_children = |stack: &mut Vec<(i64, usize)>, parent_id: i64, parent: &str| {
            for &i in children.get(parent).into_iter().flatten().rev() {
                stack.push((parent_id, i));
            }
        };
        push_children(&mut stack, ARENA_ROOT_ID, "");
        while let Some((parent_id, i)) = stack.pop() {
            let row = &rows[i];
            let name = name_of(&row.path);
            let id = next_id;
            next_id += 1;
            arena.push(id, parent_id, name, row.is_directory, row.size, row.modified_at);
            // Two names one fold makes equal (`a` and `A` under the Mac rules): the
            // first in byte order answers, as one row of a case-insensitive drive would.
            lookup
                .entry((parent_id, rules.fold.apply(name).into_owned()))
                .or_insert(id);
            if row.is_directory {
                push_children(&mut stack, id, &row.path);
            }
        }

        Ok(Self {
            // A memory source is rebuilt, never caught up, so its generation never moves.
            arena: arena.finish(0),
            path_prefix: path_prefix.to_string(),
            fold: rules.fold,
            children: lookup,
        })
    }

    /// The id of the entry at `index_path` (`/photos/2024`, `/` for the root), looked
    /// up through the fold, or `None` when no row is there. What a host keys its own
    /// per-entry facts by (a folder-size fold) to answer a [`crate::DirSizeSource`].
    pub fn id_of(&self, index_path: &str) -> Option<i64> {
        let trimmed = index_path.strip_suffix('/').unwrap_or(index_path);
        let mut current = ARENA_ROOT_ID;
        for component in trimmed.strip_prefix('/').unwrap_or(trimmed).split('/') {
            if component.is_empty() {
                continue;
            }
            current = *self.children.get(&(current, self.fold.apply(component).into_owned()))?;
        }
        Some(current)
    }
}

impl SearchRows for MemorySource {
    fn arena(&self) -> &SearchIndex {
        &self.arena
    }

    fn path_prefix(&self) -> &str {
        &self.path_prefix
    }

    fn resolve_scope(&self, index_paths: &[&str]) -> Result<Vec<Option<i64>>, SourceError> {
        Ok(index_paths.iter().map(|path| self.id_of(path)).collect())
    }
}

/// `path` split into its parent's path (`""` for the root) and its name, or `None`
/// for a path that isn't a well-formed `/`-led one below the root.
fn split(path: &str) -> Option<(&str, &str)> {
    let rest = path.strip_prefix('/')?;
    if rest.is_empty() || rest.split('/').any(str::is_empty) {
        return None;
    }
    path.rsplit_once('/')
}

fn name_of(path: &str) -> &str {
    path.rsplit_once('/').map_or(path, |(_, name)| name)
}

#[cfg(test)]
mod tests;
