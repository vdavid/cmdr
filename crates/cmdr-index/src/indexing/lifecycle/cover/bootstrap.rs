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

use super::{CoverContext, Ground};
use crate::indexing::host::volumes::MountFacts;
use crate::indexing::lifecycle::state::{self, Activation};
use crate::indexing::metadata::MetadataSnapshot;
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
    /// to index it against. Also what a platform with no external transport
    /// compiled in reports for every non-boot volume, which is the same thing from
    /// the caller's side: nothing here can reach it.
    NotMounted,
    /// The volume's own scan owns the writer right now. Nothing to do: that scan
    /// already covers everything a search would want walked, and a second writer
    /// on one database races the id counter.
    ScanInProgress,
    /// Standing the index up failed. Log-only.
    Failed(String),
}

impl std::fmt::Display for NoCoverContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotMounted => f.write_str("nothing is mounted under that id"),
            Self::ScanInProgress => f.write_str("the volume's own scan is running"),
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
///
/// ⚠️ Neither indexing switch is consulted here, and that's Decision 13: the
/// master switch and the sticky per-drive `user_disabled` veto both stop work the
/// app would do UNINVITED, and a search is the opposite of uninvited. The
/// `WriterOnly` start below is carved out of the master gate for the same reason
/// (`state::start_indexing_for`).
pub(crate) fn context_for_walk(volume_id: &str) -> Result<CoverContext, NoCoverContext> {
    if let Some(context) = state::cover_context_for(volume_id) {
        return Ok(context);
    }
    if state::is_active(volume_id) {
        // Registered but not handing a writer over: either its own scan is
        // mid-flight, or it's still initializing. Either way that scan covers
        // everything a search would have walked, and a second writer on one
        // database races the id counter.
        return Err(NoCoverContext::ScanInProgress);
    }

    let volume = walkable_volume(volume_id)?;
    log::info!(
        "Cover: '{volume_id}' has no index; standing one up at {} ({:?}) for the walk to fill in",
        volume.root.display(),
        volume.kind,
    );
    state::start_indexing_for(
        volume_id,
        volume.root,
        volume.kind,
        volume.inodes_trustworthy,
        Activation::WriterOnly,
    )
    .map_err(NoCoverContext::Failed)?;

    if volume_id != ROOT_VOLUME_ID {
        // A new external index database just came online, so cap accumulation the
        // same way turning indexing on for the drive would have. Never touches a
        // registered volume, and this one is registered now.
        crate::indexing::resources::retention::enforce_external_index_cap();
    }

    state::cover_context_for(volume_id).ok_or_else(|| {
        // The reservation was won by something else between the two calls, and
        // whatever won it is scanning the volume.
        NoCoverContext::Failed(format!("'{volume_id}' still has no writer after being started"))
    })
}

/// What the bootstrap needs to know about a volume before it can index it.
pub(super) struct WalkableVolume {
    root: PathBuf,
    pub(super) kind: IndexVolumeKind,
    inodes_trustworthy: bool,
}

/// Classify a volume by how its ground gets read, or say why nothing can read it.
///
/// The boot disk is known without asking anyone. Everything else is the same
/// question the enable command answers, decided by the same typed facts (a phone's
/// volume-id vocabulary, a live smb2 session, a network filesystem) through the
/// same predicates — never a path substring.
///
/// ⚠️ The kind names the SCAN PATH, not the protocol: everything that isn't the
/// boot disk, a phone, or a plain local mount is `Smb`, which is what every
/// trait-scanned, mount-rooted, journal-less volume needs. An NFS or WebDAV mount
/// classified that way walks correctly over the `Volume` trait; classifying it as
/// local instead would point the guarded walker at syscalls that block for
/// minutes, and refusing it outright would make a search of it silently wrong.
pub(super) fn walkable_volume(volume_id: &str) -> Result<WalkableVolume, NoCoverContext> {
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
    let root = volume.root().to_path_buf();
    // Trait-scanned volumes store trait-provided values in the `inode` column (MTP
    // puts PTP object handles there) and never run the local inode-keyed rename
    // pre-pass, so their inode identity counts as trustworthy.
    let via_trait = |kind| WalkableVolume {
        root: root.clone(),
        kind,
        inodes_trustworthy: true,
    };

    // A phone, by the id vocabulary the whole app routes MTP with. Asked first
    // because its files exist only over PTP: there is no mount to probe, and
    // `mtp://…` is not a path any `statfs` can answer for.
    if cmdr_fs::volume::mtp_ids::is_mtp_volume_id(volume_id) {
        return Ok(via_trait(IndexVolumeKind::Mtp));
    }
    // Anything else with no local filesystem behind it is reachable only through
    // its `Volume`, which is exactly what the trait walk needs. A probe would have
    // nothing to probe.
    if !volume.supports_local_fs_access() {
        return Ok(via_trait(IndexVolumeKind::Smb));
    }
    let facts = probe_mount(&root);
    if !routes_to_local_external(volume.smb_connection_state().is_some(), facts.is_network) {
        return Ok(via_trait(IndexVolumeKind::Smb));
    }
    Ok(WalkableVolume {
        root,
        kind: IndexVolumeKind::LocalExternal,
        inodes_trustworthy: facts.inodes_trustworthy,
    })
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn classify_external(_volume_id: &str) -> Result<WalkableVolume, NoCoverContext> {
    // No external-drive transport is compiled here, so nothing registers a volume
    // under that id and the boot disk is the only one with anywhere to run.
    Err(NoCoverContext::NotMounted)
}

/// The mount's filesystem facts, under a hard deadline, on a thread of its own.
///
/// The probe is a `statfs`: microseconds on a local mount, and minutes on a
/// wedged network one. This runs on the thread a search is waiting on, so it
/// can't afford to wait — and a probe that won't answer IS the answer, because
/// [`MountFacts::UNPROBEABLE`] reads as network, which routes the volume to the
/// `Volume`-trait walk. That's the right walk for a mount whose `statfs` won't
/// return: every round trip is deadline-bounded there, where the local guarded
/// walker would issue syscalls that block for minutes. The probe thread is left to
/// finish on its own; it holds nothing but its own syscall.
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

/// What the bootstrap found at a frontier root, which is what decides whether
/// anyone else has already spoken for it.
///
/// A row the index already held is a row the arena answers from, so a search's
/// covered half has it. A row this walk had to create is one nothing but the walk
/// can report — and the walk lists a directory's CONTENTS, so without this the
/// frontier root would be the one entry under a scope that no half of a live
/// search ever emits.
pub(super) enum RootRow {
    /// The index already held it.
    Existing,
    /// This walk created it, carrying what a listing would show for it.
    Created(MetadataSnapshot),
}

/// Make sure `root` has an `entries` row for the walk to start from,
/// materializing the chain from the volume root down to it.
///
/// The common case costs one lookup: a frontier node a coverage answer named by
/// descending into its parent's listing already has a row. The slow path runs
/// only for ground the index has never seen, and it goes through the volume's
/// one writer (never a direct insert), so the ids stay the writer's to allocate
/// and a row that already exists is upserted rather than duplicated.
///
/// Only the root's own row is reported back. The ancestors above it are outside
/// whatever scope asked for this walk (the frontier is cut inside the scope, so
/// the shallowest node it can name is the scope root itself), and a caller that
/// reported them would answer with folders nobody searched.
pub(super) fn ensure_walkable(context: &CoverContext, ground: &Ground, root: &Path) -> Result<RootRow, NotWalkable> {
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
        return Ok(RootRow::Existing);
    }

    // The absolute path is rebuilt alongside the index chain, so each row the
    // walk has to create carries the real directory's metadata rather than a
    // placeholder. It starts at the volume root, which is what `ROOT_ID` means:
    // `/` for the boot disk, the mount point for every other kind.
    let mut on_disk = PathBuf::from(context.space.volume_root_string());
    let mut parent_id = ROOT_ID;
    let mut root_row = RootRow::Existing;
    let mut components = index_relative.split('/').filter(|c| !c.is_empty()).peekable();
    while let Some(component) = components.next() {
        on_disk.push(component);
        let is_root = components.peek().is_none();
        parent_id = match IndexStore::resolve_component(&conn, parent_id, component)
            .map_err(|e| NotWalkable::Store(e.to_string()))?
        {
            Some(id) => {
                if !is_directory_row(&conn, id)? {
                    return Err(NotWalkable::FileRowInTheChain(on_disk));
                }
                id
            }
            None => {
                let (id, snapshot) = create_directory_row(context, ground, &conn, parent_id, component, &on_disk)?;
                if is_root {
                    root_row = RootRow::Created(snapshot);
                }
                id
            }
        };
    }
    Ok(root_row)
}

/// Whether an existing row is a directory. A missing row here is a row deleted
/// between the resolve and this read, which reads the same way as a broken chain.
fn is_directory_row(conn: &rusqlite::Connection, id: i64) -> Result<bool, NotWalkable> {
    Ok(IndexStore::get_entry_by_id(conn, id)
        .map_err(|e| NotWalkable::Store(e.to_string()))?
        .is_some_and(|row| row.is_directory))
}

/// Add one directory of the chain, and hand back its id and what a listing of it
/// would have shown.
///
/// `UpsertEntryV2` rather than an insert, because the writer resolves it by
/// `(parent_id, name)`: a row that arrives from somewhere else in the meantime is
/// updated, never duplicated past the `(parent_id, name_folded)` uniqueness the
/// store depends on. The flush is what makes the new id readable, and the depth
/// of a path is what bounds how many of them one walk pays for.
fn create_directory_row(
    context: &CoverContext,
    ground: &Ground,
    conn: &rusqlite::Connection,
    parent_id: i64,
    name: &str,
    on_disk: &Path,
) -> Result<(i64, MetadataSnapshot), NotWalkable> {
    let snapshot = ground
        .stat_directory(on_disk)
        .ok_or_else(|| NotWalkable::NotADirectoryOnDisk(on_disk.to_path_buf()))?;
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
    let id = IndexStore::resolve_component(conn, parent_id, name)
        .map_err(|e| NotWalkable::Store(e.to_string()))?
        .ok_or_else(|| NotWalkable::Store(format!("{} is still absent after its own upsert", on_disk.display())))?;
    Ok((id, snapshot))
}
