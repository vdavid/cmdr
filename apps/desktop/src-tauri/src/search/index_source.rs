//! The desktop's source of search rows: a loaded volume's arena, answered from its
//! index database. The seam it fills is `source.rs`; the path work only the host can
//! do (`realpath`) is here too.

use std::collections::HashMap;

use cmdr_index::store::{self, IndexStore};

use super::index::SearchIndex;
use super::query::expand_tilde;
use super::source::{DirSizeSource, SearchRows, SourceError, SourceRead};
use super::volumes::LoadedVolume;

/// A loaded volume as a search source: its arena, its mount root, and SQL over its
/// read pool for the scope and the folder sizes.
pub(crate) struct IndexSource<'a>(pub(crate) &'a LoadedVolume);

impl SearchRows for IndexSource<'_> {
    fn arena(&self) -> &SearchIndex {
        &self.0.index
    }

    fn path_prefix(&self) -> &str {
        self.0.mount_root.as_deref().unwrap_or("")
    }

    fn resolve_scope(&self, index_paths: &[&str]) -> Result<Vec<Option<i64>>, SourceError> {
        let unreadable = |detail: String| SourceError {
            read: SourceRead::Scope,
            detail,
        };
        self.0
            .pool
            .with_conn(|conn| {
                index_paths
                    .iter()
                    .map(|path| store::resolve_path(conn, path).map_err(|e| unreadable(e.to_string())))
                    .collect::<Result<Vec<_>, _>>()
            })
            .map_err(unreadable)?
    }
}

impl DirSizeSource for IndexSource<'_> {
    fn in_range(&self, min: Option<u64>, max: Option<u64>) -> Result<HashMap<i64, u64>, SourceError> {
        let unreadable = |detail: String| SourceError {
            read: SourceRead::SizeRange,
            detail,
        };
        // `dir_stats` is deliberately unindexed on size (`IndexStore::dir_sizes_in_range`),
        // so this is a full scan, asked only when a query needs it.
        let rows = self
            .0
            .pool
            .with_conn(|conn| IndexStore::dir_sizes_in_range(conn, min, max))
            .map_err(unreadable)?
            .map_err(|e| unreadable(e.to_string()))?;
        Ok(rows.into_iter().collect())
    }

    fn of(&self, ids: &[i64]) -> Result<Vec<Option<u64>>, SourceError> {
        let unreadable = |detail: String| SourceError {
            read: SourceRead::SizeFill,
            detail,
        };
        let stats = self
            .0
            .pool
            .with_conn(|conn| IndexStore::get_dir_stats_batch_by_ids(conn, ids))
            .map_err(unreadable)?
            .map_err(|e| unreadable(e.to_string()))?;
        Ok(stats
            .into_iter()
            .map(|row| row.map(|stats| stats.recursive_logical_size))
            .collect())
    }
}

/// Canonicalize a scope include path (resolve symlinks) so it matches the index's
/// stored REAL paths, without wedging on a hung mount.
///
/// The scanner walks the real filesystem, so the index stores canonical paths (on
/// macOS `/tmp` is a symlink, recorded as `/private/tmp`), while panes and agents
/// report the symlinked form (`scope:/tmp/x`). A literal prefix match then resolves
/// nothing → silent empty results. We canonicalize each include path ONCE here (a
/// handful of paths, off the hot per-entry scan loop).
///
/// The tilde is expanded FIRST (`query::expand_tilde` says why that order is
/// load-bearing). Both halves of a live search take the scope through here.
///
/// `fs::canonicalize` issues `realpath`, which blocks indefinitely on a dead network
/// mount, so it runs on a detached worker thread under a 2 s deadline (the sync
/// analog of `blocking_with_timeout`; the callers are sync). On timeout, an error, or
/// a non-existent path we keep the literal: best effort, so an offline or unmounted
/// index's scope still gets its literal match.
pub(crate) fn canonicalize_scope_path(path: &str) -> String {
    canonicalize_scope_path_under(path, super::host_home_dir().as_deref())
}

/// [`canonicalize_scope_path`] with `~` meaning `home`.
fn canonicalize_scope_path_under(path: &str, home: Option<&str>) -> String {
    let path = expand_tilde(path, home).into_owned();
    let owned = path.clone();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(std::fs::canonicalize(&owned));
    });
    match rx.recv_timeout(std::time::Duration::from_secs(2)) {
        Ok(Ok(canonical)) => canonical.to_string_lossy().into_owned(),
        _ => path,
    }
}

/// A query's scope paths as the seam takes them: each as written, beside its
/// canonical form, with `~` meaning `home` (the search's own home folder, the same
/// one its result rows write as `~`).
pub(crate) fn scope_pairs(paths: &[String], home: Option<&str>) -> Vec<(String, String)> {
    paths
        .iter()
        .map(|path| (path.clone(), canonicalize_scope_path_under(path, home)))
        .collect()
}

#[cfg(test)]
mod tests;
