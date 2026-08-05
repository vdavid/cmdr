//! Everything that has to exist before a coverage frontier can be walked.
//!
//! A walk needs three things the index doesn't always have: a database with a
//! writer behind it, an epoch to stamp the directories it lists with, and an
//! `entries` row to resolve its root against. A volume nobody ever indexed has
//! none of them; a volume indexed yesterday can still be missing the last one,
//! because a folder created since its parent was listed has no row either. Both
//! land here, and the second is why this is not only a cold-drive concern.
//!
//! ❌ Nothing here lists a directory or claims coverage. It creates the rows a
//! walk needs to START, each at `listed_epoch = 0` — the walk earns the coverage,
//! and an ancestor that claimed a listing it never did would mark a whole tree
//! covered off the back of one walked folder.

use std::path::{Path, PathBuf};

use super::CoverContext;
use crate::indexing::metadata::extract_metadata;
use crate::indexing::store::{IndexStore, ROOT_ID, resolve_path};
use crate::indexing::writer::WriteMessage;

/// Why a frontier path can't be walked.
///
/// Each variant is a different thing being wrong with the ground, and the walk
/// treats them alike (the root stays frontier, the next search asks again); they
/// are separate so the log line says which, and so a future caller that wants to
/// act on one doesn't have to read a message to tell them apart.
#[derive(Debug)]
pub(super) enum NotWalkable {
    /// The path isn't on this volume at all, so this index has no place for it.
    OutsideVolume,
    /// A component isn't a readable directory on disk: it was deleted between the
    /// coverage answer and the walk, or it's a symlink (which the index stores but
    /// never descends into).
    NotADirectoryOnDisk(PathBuf),
    /// The index holds a FILE row where the chain needs a directory — a stale
    /// file→dir type change. Parenting rows under it would orphan them, so the
    /// walk declines and leaves the repair to the reconcile that heals type
    /// changes.
    FileRowInTheChain(PathBuf),
    /// The store wouldn't answer. Log-only.
    Store(String),
}

impl std::fmt::Display for NotWalkable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OutsideVolume => f.write_str("the path is outside this volume's index"),
            Self::NotADirectoryOnDisk(path) => write!(f, "{} isn't a readable directory", path.display()),
            Self::FileRowInTheChain(path) => write!(f, "the index holds {} as a file", path.display()),
            Self::Store(e) => write!(f, "the index store wouldn't answer: {e}"),
        }
    }
}

/// Make sure `root` has an `entries` row for the walk to start from,
/// materializing the chain from the volume root down to it.
///
/// The common case costs one lookup: a frontier node a coverage answer named by
/// descending into its parent's listing already has a row. The slow path runs
/// only for ground the index has never seen, and it goes through the volume's
/// one writer (never a direct insert), so the ids stay the writer's to allocate
/// and a row that already exists is upserted rather than duplicated.
pub(super) fn ensure_walkable(context: &CoverContext, root: &Path) -> Result<(), NotWalkable> {
    let absolute = context.space.absolute(&root.to_string_lossy());
    let index_relative = context
        .space
        .index_relative(&absolute)
        .ok_or(NotWalkable::OutsideVolume)?;

    let db_path = context.writer.db_path();
    let conn = IndexStore::open_read_connection(&db_path).map_err(|e| NotWalkable::Store(e.to_string()))?;
    if resolve_path(&conn, &index_relative)
        .map_err(|e| NotWalkable::Store(e.to_string()))?
        .is_some()
    {
        return Ok(());
    }

    // The absolute path is rebuilt alongside the index chain, so each row the
    // walk has to create carries the real directory's metadata rather than a
    // placeholder. It starts at the volume root, which is what `ROOT_ID` means:
    // `/` for the boot disk, the mount point for every other kind.
    let mut on_disk = PathBuf::from(context.space.volume_root_string());
    let mut parent_id = ROOT_ID;
    for component in index_relative.split('/').filter(|c| !c.is_empty()) {
        on_disk.push(component);
        parent_id = match IndexStore::resolve_component(&conn, parent_id, component)
            .map_err(|e| NotWalkable::Store(e.to_string()))?
        {
            Some(id) => {
                if !is_directory_row(&conn, id)? {
                    return Err(NotWalkable::FileRowInTheChain(on_disk));
                }
                id
            }
            None => create_directory_row(context, &conn, parent_id, component, &on_disk)?,
        };
    }
    Ok(())
}

/// Whether an existing row is a directory. A missing row here is a row deleted
/// between the resolve and this read, which reads the same way as a broken chain.
fn is_directory_row(conn: &rusqlite::Connection, id: i64) -> Result<bool, NotWalkable> {
    Ok(IndexStore::get_entry_by_id(conn, id)
        .map_err(|e| NotWalkable::Store(e.to_string()))?
        .is_some_and(|row| row.is_directory))
}

/// Add one directory of the chain, and hand back its id.
///
/// `UpsertEntryV2` rather than an insert, because the writer resolves it by
/// `(parent_id, name)`: a row that arrives from somewhere else in the meantime is
/// updated, never duplicated past the `(parent_id, name_folded)` uniqueness the
/// store depends on. The flush is what makes the new id readable, and the depth
/// of a path is what bounds how many of them one walk pays for.
fn create_directory_row(
    context: &CoverContext,
    conn: &rusqlite::Connection,
    parent_id: i64,
    name: &str,
    on_disk: &Path,
) -> Result<i64, NotWalkable> {
    let metadata =
        std::fs::symlink_metadata(on_disk).map_err(|_| NotWalkable::NotADirectoryOnDisk(on_disk.to_path_buf()))?;
    // A symlink reports `is_dir() == false` here, which is the answer we want: the
    // index stores symlinks without descending into them, so a walk rooted below
    // one would attribute another directory's contents to this path.
    if !metadata.is_dir() {
        return Err(NotWalkable::NotADirectoryOnDisk(on_disk.to_path_buf()));
    }
    let snapshot = extract_metadata(&metadata, true, false);
    context
        .writer
        .send(WriteMessage::UpsertEntryV2 {
            parent_id,
            name: name.to_string(),
            is_directory: true,
            is_symlink: false,
            logical_size: snapshot.logical_size,
            physical_size: snapshot.physical_size,
            modified_at: snapshot.modified_at,
            inode: context.space.trust_inode(snapshot.inode),
            nlink: snapshot.nlink,
        })
        .map_err(|e| NotWalkable::Store(e.to_string()))?;
    context
        .writer
        .flush_blocking()
        .map_err(|e| NotWalkable::Store(e.to_string()))?;
    IndexStore::resolve_component(conn, parent_id, name)
        .map_err(|e| NotWalkable::Store(e.to_string()))?
        .ok_or_else(|| NotWalkable::Store(format!("{} is still absent after its own upsert", on_disk.display())))
}
