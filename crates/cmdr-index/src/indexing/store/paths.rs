//! Turning a path into an entry id and an entry id back into a path, over the
//! integer-keyed parent-child tree.
//!
//! Every walk here goes component by component through the `platform_case`
//! collation, so the answers match what the filesystem itself would call the
//! same name.

use rusqlite::{Connection, params};
use std::path::Path;

use super::schema::{ROOT_ID, ROOT_PARENT_ID, ensure_root_sentinel};
use super::{IndexStore, IndexStoreError};

/// Resolve the entry id to use as a scan's root, seeding the `ROOT` sentinel for
/// a volume-root scan.
///
/// For a volume-root scan the root is always `ROOT_ID` (the sentinel is created
/// if absent). For a subtree scan the root's actual entry id is resolved from the
/// DB, erroring if it isn't indexed yet (for example a subtree scan racing an
/// ongoing full scan; the full scan will cover it).
///
/// Shared by both scanners. The network (SMB/MTP) `network_scanner` wraps it in a
/// [`ScanContext`](super::ScanContext) path→id map (its serial BFS resolves parents
/// by path); the local scanner carries `parent_id` through its parallel walk and
/// needs only the root id from here.
pub fn resolve_scan_root(conn: &Connection, root: &Path, is_volume_root: bool) -> Result<i64, IndexStoreError> {
    if is_volume_root {
        // Only volume-root scans create the sentinel; subtree scans run after the
        // full scan already inserted it, and their connection may be read-only or
        // contending with the writer thread's write lock.
        ensure_root_sentinel(conn)?;
        return Ok(ROOT_ID);
    }

    let root_str = root.to_string_lossy();
    if let Some(id) = resolve_path(conn, &root_str)? {
        return Ok(id);
    }

    // Diagnose which component is missing by walking the path.
    let stripped = root_str.strip_prefix('/').unwrap_or(&root_str);
    let mut current_id = ROOT_ID;
    for component in stripped.split('/') {
        if component.is_empty() {
            continue;
        }
        match IndexStore::resolve_component(conn, current_id, component) {
            Ok(Some(id)) => current_id = id,
            Ok(None) => {
                log::debug!(
                    "resolve_scan_root: resolve_path({root_str}) failed at component \"{component}\" (parent_id={current_id})"
                );
                break;
            }
            Err(e) => {
                log::debug!(
                    "resolve_scan_root: resolve_path({root_str}) errored at component \"{component}\" (parent_id={current_id}): {e}"
                );
                break;
            }
        }
    }
    Err(IndexStoreError::Sqlite(rusqlite::Error::QueryReturnedNoRows))
}

/// Reconstruct the full path for an entry by walking up the parent chain.
///
/// Returns `/` for the root sentinel itself, and `/component/component/...`
/// for all other entries.
pub(super) fn reconstruct_path(conn: &Connection, entry_id: i64) -> Result<String, IndexStoreError> {
    if entry_id == ROOT_ID {
        return Ok("/".to_string());
    }

    let mut components = Vec::new();
    let mut current_id = entry_id;

    loop {
        if current_id == ROOT_ID || current_id == ROOT_PARENT_ID {
            break;
        }
        let mut stmt = conn.prepare_cached("SELECT parent_id, name FROM entries WHERE id = ?1")?;
        let (parent_id, name): (i64, String) =
            stmt.query_row(params![current_id], |row| Ok((row.get(0)?, row.get(1)?)))?;
        components.push(name);
        current_id = parent_id;
    }

    components.reverse();
    Ok(format!("/{}", components.join("/")))
}

/// Resolve a path string to an entry ID by walking component-by-component from
/// the index root (`ROOT_ID`).
///
/// Returns `None` if any component along the path doesn't exist. The path must be
/// absolute (starting with `/`). For a `root` (local-disk) index `ROOT_ID` is `/`,
/// so an absolute filesystem path resolves directly. For a network/MTP index
/// `ROOT_ID` is the volume root, so a mount-absolute path must be mapped into the
/// volume's index path space first (see [`crate::indexing::paths::routing::index_read_path`]).
pub fn resolve_path(conn: &Connection, path: &str) -> Result<Option<i64>, IndexStoreError> {
    resolve_path_under(conn, ROOT_ID, path)
}

/// Resolve a path RELATIVE to a given root entry id by walking
/// component-by-component from `root_id`.
///
/// Returns `None` if any component doesn't exist under that root. A leading `/`
/// on `relative_path` is treated as relative to `root_id` (NOT the index root),
/// and an empty path (`""` or `"/"`) resolves to `root_id` itself.
///
/// This is the root-relative generalization of [`resolve_path`] (which is just
/// `resolve_path_under(conn, ROOT_ID, path)`). It exists because a network/MTP
/// index is rooted at the VOLUME root rather than `/`: once a mount-absolute hot
/// path has had its volume-root prefix stripped to a relative remainder, this
/// walks that remainder from the index's `ROOT_ID`.
pub(crate) fn resolve_path_under(
    conn: &Connection,
    root_id: i64,
    relative_path: &str,
) -> Result<Option<i64>, IndexStoreError> {
    let trimmed = relative_path.strip_suffix('/').unwrap_or(relative_path);

    let mut current_id = root_id;
    for component in trimmed.strip_prefix('/').unwrap_or(trimmed).split('/') {
        if component.is_empty() {
            continue;
        }
        match IndexStore::resolve_component(conn, current_id, component)? {
            Some(id) => current_id = id,
            None => return Ok(None),
        }
    }
    Ok(Some(current_id))
}

/// Reconstruct a path from an in-memory map of `id -> (parent_id, name)`.
/// More efficient than DB queries when reconstructing many paths.
#[cfg(test)]
pub(super) fn reconstruct_path_from_map(entry_id: i64, map: &std::collections::HashMap<i64, (i64, &str)>) -> String {
    if entry_id == ROOT_ID {
        return "/".to_string();
    }

    let mut components = Vec::new();
    let mut current_id = entry_id;

    loop {
        if current_id == ROOT_ID || current_id == ROOT_PARENT_ID {
            break;
        }
        match map.get(&current_id) {
            Some((parent_id, name)) => {
                components.push(*name);
                current_id = *parent_id;
            }
            None => break,
        }
    }

    components.reverse();
    format!("/{}", components.join("/"))
}
