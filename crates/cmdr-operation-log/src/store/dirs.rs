//! Dir interning: every directory prefix stored once, forever, as a `dirs` row
//! (`DETAILS.md` § Schema), and the walk back to a full path.

use rusqlite::{Connection, OptionalExtension};

use super::{OperationLogStoreError, fold_name};

/// Intern a directory path for `volume_id`, returning its `dir_id`. Interns each
/// ancestor once (reused across operations, so a hot directory is stored once
/// forever), then returns the leaf's id. A volume root (empty path) interns a
/// single row with name `""` and NULL parent.
///
/// A remote volume's path is rooted at `scheme://authority` (`adb://serial/…`,
/// `sftp://test_user_1@nas:22/…`), and that root is ONE name in the chain, see
/// [`split_scheme_root`].
///
/// Runs on a write-capable connection — inside the writer thread's transaction
/// in production, or directly in tests. Idempotent: the same `(volume_id, path)`
/// always returns the same id.
pub(crate) fn intern_dir(conn: &Connection, volume_id: &str, path: &str) -> Result<i64, OperationLogStoreError> {
    // The volume root anchors the chain (name ""), so a file directly at the
    // volume root still has a dir to reference.
    let mut parent = intern_one(conn, volume_id, None, "")?;
    let (scheme_root, rest) = split_scheme_root(path);
    let names = scheme_root.into_iter().chain(rest.split('/').filter(|c| !c.is_empty()));
    for component in names {
        parent = intern_one(conn, volume_id, Some(parent), component)?;
    }
    Ok(parent)
}

/// Intern the folder named `name` inside the interned dir `parent_dir_id`, on the
/// parent's volume, returning its id: a DIR item row's own folder
/// (`operation_items.self_dir_id`). Interning the child of an already interned parent
/// keeps [`split_scheme_root`]'s semantics for free, since a remote root is already
/// one name up the chain. An empty name (a row naming the volume root itself) is the
/// parent.
pub(crate) fn intern_child_dir(
    conn: &Connection,
    parent_dir_id: i64,
    name: &str,
) -> Result<i64, OperationLogStoreError> {
    if name.is_empty() {
        return Ok(parent_dir_id);
    }
    let volume_id: String = conn
        .prepare_cached("SELECT volume_id FROM dirs WHERE dir_id = ?1")?
        .query_row(rusqlite::params![parent_dir_id], |row| row.get(0))?;
    intern_one(conn, &volume_id, Some(parent_dir_id), name)
}

/// The separator that marks a path as rooted at a URL-style volume root.
const SCHEME_SEPARATOR: &str = "://";

/// Splits `scheme://authority` off the head of a remote volume's path, returning
/// it and what follows. A path with no such head comes back whole.
///
/// The root has to stay one name: splitting it on `/` like the rest drops the
/// empty segment inside `://`, and [`reconstruct_dir_path`] then rebuilds
/// `adb://serial/x` as `/adb:/serial/x`, a path no volume answers to, which the
/// rollback would hand to one. No real folder name can contain `://` (a name
/// holds no `/`), so a name that does is always this root.
fn split_scheme_root(path: &str) -> (Option<&str>, &str) {
    let Some(scheme_len) = path.find(SCHEME_SEPARATOR) else {
        return (None, path);
    };
    let is_scheme = scheme_len > 0
        && path[..scheme_len]
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'-' | b'.'));
    if !is_scheme {
        return (None, path);
    }
    let authority_start = scheme_len + SCHEME_SEPARATOR.len();
    let root_len = path[authority_start..]
        .find('/')
        .map_or(path.len(), |slash| authority_start + slash);
    (Some(&path[..root_len]), &path[root_len..])
}

/// Intern one `(volume_id, parent, name)` dir row, returning its id. Insert-or-
/// ignore against the IFNULL identity index, then read the id back (reliable
/// whether or not the insert did anything). The identity is the EXACT name;
/// `name_folded` rides along as a plain column.
fn intern_one(
    conn: &Connection,
    volume_id: &str,
    parent: Option<i64>,
    name: &str,
) -> Result<i64, OperationLogStoreError> {
    conn.prepare_cached(
        "INSERT INTO dirs (volume_id, parent_dir_id, name, name_folded)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT (volume_id, IFNULL(parent_dir_id, 0), name) DO NOTHING",
    )?
    .execute(rusqlite::params![volume_id, parent, name, fold_name(name)])?;

    // `parent.unwrap_or(0)` mirrors the index's IFNULL(parent, 0): dir ids start
    // at 1, so 0 is a safe stand-in for a NULL (root) parent.
    let id = conn
        .prepare_cached(
            "SELECT dir_id FROM dirs
             WHERE volume_id = ?1 AND IFNULL(parent_dir_id, 0) = ?2 AND name = ?3",
        )?
        .query_row(rusqlite::params![volume_id, parent.unwrap_or(0), name], |row| {
            row.get::<_, i64>(0)
        })?;
    Ok(id)
}

/// The interned id of the directory `path` on `volume_id`, or `None` when no
/// operation ever recorded it. The read-only twin of [`intern_dir`]: the same
/// walk, by the same EXACT names, inserting nothing. Each step is one lookup on
/// the `dirs_identity` index.
pub(crate) fn find_dir(conn: &Connection, volume_id: &str, path: &str) -> Result<Option<i64>, OperationLogStoreError> {
    let mut stmt = conn.prepare_cached(
        "SELECT dir_id FROM dirs
         WHERE volume_id = ?1 AND IFNULL(parent_dir_id, 0) = ?2 AND name = ?3",
    )?;
    let mut step = |parent: i64, name: &str| {
        stmt.query_row(rusqlite::params![volume_id, parent, name], |row| row.get::<_, i64>(0))
            .optional()
    };
    // The volume root's row is named "" under no parent (0 stands in for NULL).
    let Some(mut dir) = step(0, "")? else {
        return Ok(None);
    };
    let (scheme_root, rest) = split_scheme_root(path);
    for name in scheme_root.into_iter().chain(rest.split('/').filter(|c| !c.is_empty())) {
        let Some(child) = step(dir, name)? else {
            return Ok(None);
        };
        dir = child;
    }
    Ok(Some(dir))
}

/// Reconstruct a dir's full path by walking `parent_dir_id` to the volume root.
/// The root (name `""`) contributes nothing, so a path renders as `/a/b/c`, and a
/// remote one led by its `scheme://authority` name as `scheme://authority/a/b`.
pub fn reconstruct_dir_path(conn: &Connection, dir_id: i64) -> Result<String, OperationLogStoreError> {
    let mut names = Vec::new();
    let mut current = Some(dir_id);
    let mut stmt = conn.prepare_cached("SELECT name, parent_dir_id FROM dirs WHERE dir_id = ?1")?;
    while let Some(id) = current {
        let (name, parent): (String, Option<i64>) =
            stmt.query_row(rusqlite::params![id], |row| Ok((row.get(0)?, row.get(1)?)))?;
        if !name.is_empty() {
            names.push(name);
        }
        current = parent;
    }
    names.reverse();
    let joined = names.join("/");
    if names.first().is_some_and(|root| root.contains(SCHEME_SEPARATOR)) {
        Ok(joined)
    } else {
        Ok(format!("/{joined}"))
    }
}
