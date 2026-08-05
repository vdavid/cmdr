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
use crate::indexing::host::volumes::MountFacts;
use crate::indexing::lifecycle::state::{self, Activation};
use crate::indexing::metadata::extract_metadata;
use crate::indexing::store::{IndexStore, ROOT_ID, resolve_path};
use crate::indexing::volume::{IndexVolumeKind, ROOT_VOLUME_ID};
use crate::indexing::writer::WriteMessage;

/// Why a volume can't be walked at all.
///
/// Separate from [`NotWalkable`], which is about one path: these are about the
/// volume, and the walk never starts.
#[derive(Debug)]
pub(crate) enum NoCoverContext {
    /// Nothing is mounted under this id, so there's no drive to walk and no root
    /// to index it against.
    NotMounted,
    /// A share or a phone. Its scoped walk is the `Volume`-trait one M3d builds;
    /// pointing the LOCAL guarded walker at a network mount would traverse a
    /// share over syscalls that block for minutes.
    NotLocallyWalkable,
    /// The volume's own scan owns the writer right now. Nothing to do: that scan
    /// already covers everything a search would want walked, and a second writer
    /// on one database races the id counter.
    ScanInProgress,
    /// Drive indexing is off in settings, and the master switch is a hard gate
    /// over every start.
    ///
    /// Decision 13 carves a user-initiated read out of it — searching is not
    /// background work — but that carve-out is M3c's, together with the four docs
    /// that state the invariant. Until then "nothing indexes, anywhere" stays
    /// true, and this is the one place the walk honors it.
    MasterSwitchOff,
    /// Standing the index up failed. Log-only.
    Failed(String),
}

impl std::fmt::Display for NoCoverContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotMounted => f.write_str("nothing is mounted under that id"),
            Self::NotLocallyWalkable => f.write_str("a share or a phone can't be walked locally"),
            Self::ScanInProgress => f.write_str("the volume's own scan is running"),
            Self::MasterSwitchOff => f.write_str("drive indexing is off in settings"),
            Self::Failed(e) => write!(f, "the index wouldn't start: {e}"),
        }
    }
}

/// The context a walk on `volume_id` runs in, standing the index up first when
/// the volume has none.
///
/// A volume that's already indexing hands its RUNNING writer over untouched —
/// one writer per database, always. A volume with no index gets one built for
/// exactly this: a database, an epoch, the read handles, and a writer, with no
/// scan and no watcher behind them, because the walk writes its own rows and a
/// full scan of the drive is not what someone searching one folder asked for.
pub(crate) fn context_for_walk(volume_id: &str) -> Result<CoverContext, NoCoverContext> {
    if let Some(context) = state::cover_context_for(volume_id) {
        return Ok(context);
    }
    if state::is_active(volume_id) {
        // Registered but not `Running`: its own scan is mid-flight.
        return Err(NoCoverContext::ScanInProgress);
    }

    if !crate::indexing::lifecycle::master::master_enabled() {
        return Err(NoCoverContext::MasterSwitchOff);
    }

    let volume = locally_walkable_volume(volume_id)?;
    log::info!(
        "Cover: '{volume_id}' has no index; standing one up at {} for the walk to fill in",
        volume.root.display()
    );
    state::start_indexing_for(
        volume_id,
        volume.root,
        volume.kind,
        volume.inodes_trustworthy,
        Activation::WriterOnly,
    )
    .map_err(NoCoverContext::Failed)?;

    if volume.kind == IndexVolumeKind::LocalExternal {
        // A new external index database just came online, so cap accumulation the
        // same way turning indexing on for the drive would have. Never touches a
        // registered volume, and this one is registered now.
        crate::indexing::resources::retention::enforce_external_index_cap();
    }

    state::cover_context_for(volume_id).ok_or_else(|| {
        // The reservation was won by something else between the two calls, or the
        // start no-op'd against the master switch.
        NoCoverContext::Failed(format!("'{volume_id}' still has no writer after being started"))
    })
}

/// What the bootstrap needs to know about a volume before it can index it.
struct WalkableVolume {
    root: PathBuf,
    kind: IndexVolumeKind,
    inodes_trustworthy: bool,
}

/// Classify a volume the LOCAL guarded walker may walk, or say why it can't.
///
/// The boot disk is known without asking anyone. Everything else is the same
/// question `local_external::classify` answers at the enable command, decided by
/// the same typed facts (a live smb2 session, a network filesystem) through the
/// same predicate — never a volume-id or path substring
/// (`.claude/rules/no-string-matching.md`).
fn locally_walkable_volume(volume_id: &str) -> Result<WalkableVolume, NoCoverContext> {
    if volume_id == ROOT_VOLUME_ID {
        // The boot disk is APFS, so its inodes are trustworthy.
        return Ok(WalkableVolume {
            root: PathBuf::from("/"),
            kind: IndexVolumeKind::Local,
            inodes_trustworthy: true,
        });
    }
    classify_external(volume_id)
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn classify_external(volume_id: &str) -> Result<WalkableVolume, NoCoverContext> {
    use crate::indexing::transports::local_external::index::routes_to_local_external;

    let volumes = crate::indexing::host::volumes::current();
    let volume = volumes.get(volume_id).ok_or(NoCoverContext::NotMounted)?;
    // A phone's files exist only over PTP, so there is no path to walk at all.
    if !volume.supports_local_fs_access() {
        return Err(NoCoverContext::NotLocallyWalkable);
    }
    let root = volume.root().to_path_buf();
    let facts = probe_mount(&root);
    if !routes_to_local_external(volume.smb_connection_state().is_some(), facts.is_network) {
        return Err(NoCoverContext::NotLocallyWalkable);
    }
    Ok(WalkableVolume {
        root,
        kind: IndexVolumeKind::LocalExternal,
        inodes_trustworthy: facts.inodes_trustworthy,
    })
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn classify_external(_volume_id: &str) -> Result<WalkableVolume, NoCoverContext> {
    // No external-drive transport is compiled here, so the boot disk is the only
    // volume with anywhere to run.
    Err(NoCoverContext::NotLocallyWalkable)
}

/// The mount's filesystem facts, under a hard deadline, on a thread of its own.
///
/// The probe is a `statfs`: microseconds on a local mount, and minutes on a
/// wedged network one. This runs on the thread a search is waiting on, so it
/// can't afford to wait — and a probe that won't answer IS the answer, because
/// [`MountFacts::UNPROBEABLE`] reads as network and a network volume is refused
/// here anyway. The probe thread is left to finish on its own; it holds nothing
/// but its own syscall.
///
/// A dedicated thread rather than the async timeout `local_external::classify`
/// uses, because this path has no runtime to borrow and must not become async
/// for one syscall.
fn probe_mount(root: &Path) -> MountFacts {
    let (answer, wait) = std::sync::mpsc::channel();
    let probe_root = root.to_path_buf();
    let spawned = std::thread::Builder::new()
        .name("index-mount-probe".into())
        .spawn(move || {
            let _ = answer.send(crate::indexing::host::volumes::current().mount_facts(&probe_root));
        });
    if spawned.is_err() {
        return MountFacts::UNPROBEABLE;
    }
    wait.recv_timeout(MOUNT_PROBE_TIMEOUT)
        .unwrap_or(MountFacts::UNPROBEABLE)
}

/// How long the mount probe may take before the volume counts as unprobeable.
/// The same deadline the enable command's classification uses, for the same
/// reason.
#[cfg(any(target_os = "macos", target_os = "linux"))]
const MOUNT_PROBE_TIMEOUT: std::time::Duration = crate::indexing::transports::local_external::index::FS_PROBE_TIMEOUT;
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
const MOUNT_PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

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
