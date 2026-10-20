//! Rename validation and the managed rename mutation.
//!
//! The command layer (`commands/rename.rs`) is a thin pass-through: it expands
//! tilde, resolves the `volume_id`, and calls into here wrapped in its IPC
//! timeout tiers (2 s validity/permission, 5 s rename). All the business logic
//! lives here per "smart backend / thin frontend".
//!
//! - **Validation** (`check_rename_permission_for_volume` here, and
//!   `check_rename_validity_impl` in `rename/validity.rs`) is the snappy,
//!   UNMANAGED path: read-only, runs per-keystroke / on-commit, never touches
//!   the operation manager.
//! - **The mutation** (`rename_managed`) is a managed instant op: it runs the
//!   actual rename inside `manager::run_instant`, so it registers a `Running`
//!   record + marks its volume busy (eject guard) for its sub-second duration,
//!   yet still runs inline and returns its `Result` to the caller. It does NOT
//!   reserve a lane or queue behind transfers (see `manager::run_instant`). An
//!   entry whose rename copies (`Volume::rename_work`) starts a background move
//!   instead (`start_rename_as_move`).

use std::path::{Path, PathBuf};

use super::archive_edit::{self, ArchiveEditRequest};
use super::host::{self, InstantMutationFact, InstantOp, InstantTarget};
use super::look_alike::{NewEntry, place_new_entry};
use super::manager::{OperationDescriptor, OperationSummaryText};
use super::mutation_error::MutationError;
use super::routed_writes::{route_serving, routed_mutation_refusal};
use super::types::{ReadOnlySide, WriteOperationType};
use cmdr_archive::mutator::Changeset;
use cmdr_fs::log_error::LogError;
use cmdr_fs::log_private::LogPrivate;
use cmdr_fs::volume::{RenameWork, Volume};
use cmdr_operation_log::types::{Initiator, OpKind, OperationId};

mod bulk;
mod validity;

pub use bulk::{BulkRenameRow, RenameStartError, RenamesStarted};
#[cfg(test)]
pub(crate) use validity::RenameByMove;
pub use validity::{RenameValidityResult, same_local_file};

impl super::Operations {
    /// Renames a file or directory as a managed instant op. When `force` is true,
    /// proceeds even if the destination exists.
    ///
    /// Runs inside `manager::run_instant`, so the volume is marked busy for the
    /// mutation's (sub-second) duration and the op shows briefly in the queue, while
    /// the rename still runs inline and its `Result` is returned to the caller. The
    /// closure announces BOTH halves of the rename as own writes (a must-know
    /// invariant: `to` so the rename-arrival event is suppressed, `from` so a
    /// Cmdr-initiated move OUT of Downloads is also suppressed) BEFORE the syscall,
    /// then notifies the listing cache.
    ///
    /// `from`/`to` are already tilde-expanded (root) or volume-relative (non-root)
    /// by the command layer. `volume_id` is `"root"` for the local filesystem.
    pub async fn rename_managed(
        &self,
        from: PathBuf,
        to: PathBuf,
        force: bool,
        volume_id: String,
        initiator: Initiator,
    ) -> Result<(), MutationError> {
        // Kept for the refusal line below: `rename_managed_inner` consumes both, and a
        // refusal nobody can place is a refusal nobody can diagnose.
        let attempted = from.clone();
        let attempted_volume = volume_id.clone();

        // The emit wraps the whole driver rather than sitting at its ends: the archive
        // route is an early return, so a per-branch emit would count the filesystem
        // renames and quietly miss every in-zip one.
        let (result, target) = rename_managed_inner(self, from, to, force, volume_id, initiator).await;
        if let Err(error) = &result {
            log_rename_refusal(RenameStage::Rename, &attempted, &attempted_volume, error);
        }
        let fact = InstantMutationFact::of(InstantOp::Rename, initiator, target, &result);
        self.host().events().instant_mutation_settled(fact);
        result
    }
}

/// Which of the two points a rename can be refused at.
///
/// Both reach the user as the same one-line message under the name field, so a
/// log line that doesn't say which one answered sends the next reader to the
/// wrong half of the flow.
#[derive(Clone, Copy)]
enum RenameStage {
    /// `check_rename_permission_for_volume`, before the editor opens.
    Preflight,
    /// `rename_managed`, the rename itself.
    Rename,
}

impl RenameStage {
    fn label(self) -> &'static str {
        match self {
            Self::Preflight => "rename pre-flight",
            Self::Rename => "rename",
        }
    }
}

/// Writes the one line that says why a rename didn't happen.
///
/// ❗ **A typed refusal is not a silent one.** Every word the user sees is
/// rendered on the frontend from the variant alone, and the inline editor shows
/// no technical-details disclosure, so without this the errno inside a
/// `Volume` refusal reached nobody: ERR-8RFN4 arrived as the sentence "The
/// volume couldn't finish that" over a log with nothing in it.
///
/// A name that's taken or empty is an ordinary answer to an ordinary typo, so it
/// stays at debug; the rest mean something went wrong and carry the detail
/// (errno included) that says what.
fn log_rename_refusal(stage: RenameStage, from: &Path, volume_id: &str, error: &MutationError) {
    let expected = matches!(
        error,
        MutationError::AlreadyExists { .. }
            | MutationError::NameEmpty
            | MutationError::NameHasDisallowedCharacter
            | MutationError::CantRenameVolumeRoot
    );
    let stage = stage.label();
    if expected {
        log::debug!(target: "volume", "{stage} refused on '{volume_id}': {error} ({})", LogPrivate::path(from), error = LogError(error));
    } else {
        log::warn!(target: "volume", "{stage} refused on '{volume_id}': {error} ({})", LogPrivate::path(from), error = LogError(error));
    }
}

/// The rename itself. Returns where it landed alongside its result so the wrapper
/// above can label the event without re-deriving the route.
async fn rename_managed_inner(
    engine: &super::Operations,
    from: PathBuf,
    to: PathBuf,
    force: bool,
    volume_id: String,
    initiator: Initiator,
) -> (Result<(), MutationError>, InstantTarget) {
    // Renaming a path INSIDE an archive is a zip mutation: route it to the
    // managed archive-edit driver. The `.zip` file itself is a regular file —
    // renaming it must work like any other file — so only a genuinely-inner path
    // routes here. Parent-aware detection (not the `std::fs`-only sync predicate)
    // so a rename inside a REMOTE zip (direct SMB / MTP) routes too. Every
    // `manager.get` below passes `from`: plain `get`, as this gate sent an in-zip
    // rename away.
    let host = engine.host();
    let manager = host.volumes();
    if manager.path_is_inside_archive(&volume_id, &from).await || manager.path_is_inside_archive(&volume_id, &to).await
    {
        return (
            route_archive_rename(engine, &from, &to, &volume_id).await,
            InstantTarget::Archive,
        );
    }
    // Inside a repo's `.git/<category>/` trees the portal serves the path read-only;
    // the drive underneath would rename a real entry in the repository's `.git`.
    for path in [&from, &to] {
        if route_serving(host, &volume_id, path).await.is_some() {
            return (Err(routed_mutation_refusal(path)), InstantTarget::Volume);
        }
    }

    let is_root = volume_id == "root";
    // The target is a NEW name: spelled the way the volume wants new names, and
    // taken when the folder holds it under another Unicode spelling (a byte-exact
    // share would rename a twin in beside it). Settled before the journal and the
    // descriptor, so both record where the entry really lands.
    let to = if is_root {
        to
    } else {
        match manager.get(&volume_id) {
            Some(volume) => match rename_target(host, volume.as_ref(), &volume_id, &from, to, force).await {
                Ok(target) => target,
                Err(refusal) => return (Err(refusal), InstantTarget::Volume),
            },
            // The closure below refuses an unregistered volume in its own words.
            None => to,
        }
    };
    // ❗ An entry whose rename isn't one call here (an object store's folder or
    // big file) never reaches `Volume::rename`: it starts a background move,
    // with progress and cancel, and this answers once that has started, the
    // way an in-zip rename does.
    if !is_root && let Some(volume) = manager.get(&volume_id) {
        match volume.rename_work(&from).await {
            Ok(RenameWork::OneCall) => {}
            Ok(RenameWork::CopyThenDelete) => {
                return (
                    start_rename_as_move(engine, &from, &to, force, &volume_id, initiator).await,
                    InstantTarget::Volume,
                );
            }
            Err(error) => {
                return (Err(MutationError::Volume { error }), InstantTarget::Volume);
            }
        }
    }
    let descriptor = rename_descriptor(&from, &to, &volume_id);
    // Journal the rename as a single-item op under the REAL volume id. Snapshot the
    // source kind BEFORE the closure moves `from`/`to` and the rename fires: local
    // reads size + mtime for free from `symlink_metadata`; a volume probes only the
    // kind (dir vs file) for the row's `entry_type` — one metadata call for a
    // single-item op, since size/mtime aren't cheaply available on MTP (recorded
    // `None`, acceptable for a record).
    let op_id = descriptor.operation_id.clone();
    // Cloned so the record after the op (which moves `volume_id` into the closure)
    // still has the id.
    let journal_volume_id = volume_id.clone();
    let local_meta = if is_root {
        std::fs::symlink_metadata(&from).ok()
    } else {
        None
    };
    let volume_is_dir = if is_root {
        false
    } else {
        match manager.get(&volume_id) {
            // Truthful-enough default, unlike the copy/move/delete probes: this
            // value only labels the journal row's entry type for the undo
            // history. It reaches no destructive branch, so a mislabeled undo
            // entry is the whole cost of getting it wrong.
            // allowed-probe-unwrap: labels an undo-history row only; reaches no destructive branch
            Some(v) => v.is_directory(&from).await.unwrap_or(false),
            None => false,
        }
    };
    // The op's own scope on the engine, which journals it: an instant op has no
    // state to carry one.
    let scope = engine.op_scope();
    super::journal::open_volume_op(
        &scope,
        op_id.as_str(),
        OpKind::Rename,
        initiator,
        &journal_volume_id,
        None,
        1,
    );
    let journal_snapshot = Some((from.clone(), to.clone(), local_meta, volume_is_dir));

    let result = engine
        .manager()
        .run_instant(descriptor, async move {
            // Register both halves of the rename with the downloads watcher's
            // ignore set BEFORE the syscall (no-ops outside ~/Downloads).
            host.announce(&from);
            host.announce(&to);

            if !is_root {
                // Volume-aware rename (MTP, SMB, and other non-local volumes).
                // The volume's `rename` calls `notify_mutation` internally, so
                // the listing cache updates automatically.
                // Plain `get`, not `resolve`: a rename INSIDE an archive is rejected
                // upstream, so this is a normal file (incl. the `.zip` itself), which
                // renames through its own volume, never the ArchiveVolume.
                let Some(volume) = host.volumes().get(&volume_id) else {
                    return Err(super::mutation_error::unregistered_volume_refusal(
                        host.volumes(),
                        volume_id.clone(),
                        &from,
                    )
                    .await);
                };
                // A taken name is reported by the NAME, matching the local branch and
                // the live validation the user just saw; everything else rides as the
                // volume's own typed answer.
                volume.rename(&from, &to, force).await.map_err(|e| match e {
                    cmdr_fs::volume::VolumeError::AlreadyExists(_) => {
                        MutationError::AlreadyExists { name: name_of(&to) }
                    }
                    other => MutationError::Volume { error: other },
                })
            } else {
                // Local filesystem rename on the blocking pool.
                let from_syscall = from.clone();
                let to_syscall = to.clone();
                host.runtime()
                    .spawn_blocking(move || {
                        // The kernel decides whether the name is free, in the same
                        // syscall that takes it. A stat first and a plain
                        // `std::fs::rename` after are two operations, and POSIX
                        // rename replaces its target without a word: a file created
                        // in between would be destroyed with no prompt and no way
                        // back. `rename_no_replace` refuses instead.
                        //
                        // `force` is the caller saying "replace what's there", and a
                        // self-rename (a case-only change on a case-insensitive
                        // filesystem folds onto one entry) has to be allowed to land
                        // on its own target.
                        let renamed = if force || from_syscall == to_syscall {
                            std::fs::rename(&from_syscall, &to_syscall)
                        } else {
                            super::overwrite::rename_no_replace(&from_syscall, &to_syscall)
                        };
                        // The two paths mean different things: `ENOENT` is the source
                        // that's gone, `EEXIST` the destination that isn't free. A
                        // taken name is reported BY NAME (the frontend words it in
                        // ten locales), matching the volume branch above.
                        renamed.map_err(|e| {
                            if e.kind() == std::io::ErrorKind::AlreadyExists {
                                return MutationError::AlreadyExists {
                                    name: name_of(&to_syscall),
                                };
                            }
                            MutationError::Volume {
                                error: cmdr_local::rename_volume_error(&e, &from_syscall, &to_syscall),
                            }
                        })
                    })
                    .await
                    .map_err(|e| MutationError::Unexpected {
                        detail: format!("the rename task didn't finish: {e}"),
                    })??;

                // Notify the listing cache about the rename (the volume path does
                // this itself via `notify_mutation`; the local path must do it
                // explicitly).
                notify_rename_in_listing(host, &volume_id, &from, &to).await;
                Ok(())
            }
        })
        .await;

    // Journal the rename as a single-item op: source → dest, one row (the whole
    // subtree moves by one rename). Rollbackable iff unchanged (rechecked at
    // rollback time).
    if let Some((from_path, to_path, meta, volume_is_dir)) = journal_snapshot {
        use cmdr_operation_log::types::{EntryType, ExecutionStatus, ItemOutcome, OpKind};
        match &result {
            Ok(()) => {
                let is_dir = meta.as_ref().map(|m| m.is_dir()).unwrap_or(volume_is_dir);
                let entry_type = if is_dir { EntryType::Dir } else { EntryType::File };
                super::journal::record_volume_leaf(
                    &scope,
                    entry_type,
                    &journal_volume_id,
                    &from_path,
                    Some((&journal_volume_id, &to_path)),
                    meta.as_ref().map(|m| m.len() as i64),
                    meta.as_ref().and_then(super::journal::mtime_secs),
                    false,
                    ItemOutcome::Done,
                );
                super::journal::finalize_op(&scope, OpKind::Rename, ExecutionStatus::Done);
            }
            Err(_) => super::journal::finalize_op(&scope, OpKind::Rename, ExecutionStatus::Failed),
        }
    }
    (result, InstantTarget::Volume)
}

/// Starts the background move a rename that copies runs as
/// (`routing::start_rename_by_move`): `from` moves into `to`'s folder under
/// `to`'s name. A confirmed replace (`force`) runs under Overwrite; anything
/// else asks on a clash, which only a race can bring, since the name was
/// checked free just above.
async fn start_rename_as_move(
    engine: &super::Operations,
    from: &Path,
    to: &Path,
    force: bool,
    volume_id: &str,
    initiator: Initiator,
) -> Result<(), MutationError> {
    let events = engine
        .host()
        .operation_sink()
        .ok_or_else(|| MutationError::Unexpected {
            detail: "the operation manager isn't ready to start a move".to_string(),
        })?;
    let parent = to.parent().ok_or(MutationError::CantRenameVolumeRoot)?;
    let config = super::types::VolumeCopyConfig {
        conflict_resolution: if force {
            super::types::ConflictResolution::Overwrite
        } else {
            super::types::ConflictResolution::Stop
        },
        ..super::types::VolumeCopyConfig::default()
    };
    engine
        .start_rename_by_move(
            events,
            volume_id.to_string(),
            vec![(from.to_path_buf(), name_of(to))],
            parent.display().to_string(),
            config,
            initiator,
            None,
        )
        .await
        .map(|started| {
            log::info!(
                target: "volume",
                "rename of {} runs as move {} on '{volume_id}'",
                LogPrivate::path(from),
                started.operation_id
            );
        })
        .map_err(|e| MutationError::Unexpected {
            detail: format!("the move a rename runs as couldn't start: {e:?}"),
        })
}

/// Where a volume rename of `from` to `to` really lands (`look_alike.rs`).
///
/// A look-alike of `to` refuses a plain rename as the taken name it is. A
/// rename the user confirmed (`force`) replaces the look-alike under ITS
/// spelling, so the folder ends with one entry. `from` itself as the look-alike
/// is a respell, which is free.
async fn rename_target(
    host: &host::EngineHost,
    volume: &dyn Volume,
    volume_id: &str,
    from: &Path,
    to: PathBuf,
    force: bool,
) -> Result<PathBuf, MutationError> {
    match place_new_entry(host.listings(), volume, volume_id, &to, Some(from)).await {
        Ok(NewEntry::Free(target)) => Ok(target),
        Ok(NewEntry::Taken(entry)) if force => Ok(to.with_file_name(&entry.name)),
        Ok(NewEntry::Taken(_) | NewEntry::Ambiguous) => Err(MutationError::AlreadyExists { name: name_of(&to) }),
        Err(error) => Err(MutationError::Volume { error }),
    }
}

/// Routes an in-archive rename to the managed archive-edit driver. Both `from`
/// and `to` must resolve to the SAME archive (a rename within the zip). A
/// cross-boundary rename (in↔out of the archive) is a move, not a rename, and is
/// refused here — the FE routes those through copy/move.
///
/// Returns `Ok(())` once the managed op has STARTED (it runs asynchronously and
/// emits `write-progress`/`write-complete`), unlike a plain rename which
/// completes inline. The op id rides on the `operations-changed` queue snapshot.
async fn route_archive_rename(
    engine: &super::Operations,
    from: &Path,
    to: &Path,
    volume_id: &str,
) -> Result<(), MutationError> {
    // Confirmation happened at the routing site (parent-aware `path_is_inside_archive`),
    // so a pure string split suffices — and it works for a REMOTE zip, where the
    // `std::fs` confirm would wrongly fail. A `to` with no archive component means a
    // rename OUT of the archive (a move), which is refused here.
    let (from_archive, from_inner) =
        cmdr_fs::archive_format::archive_boundary_candidate(from).ok_or(MutationError::ArchiveNotEditable)?;
    let (to_archive, to_inner) =
        cmdr_fs::archive_format::archive_boundary_candidate(to).ok_or(MutationError::RenameOutOfArchive)?;
    if from_archive != to_archive {
        return Err(MutationError::RenameAcrossArchives);
    }
    // Only zip archives are writable; tar and 7z are browse + extract only.
    archive_edit::ensure_zip_writable(&from_archive, ReadOnlySide::Destination)
        .map_err(|_| MutationError::ArchiveReadOnly)?;

    let from_inner = archive_edit::normalize_inner_path(&from_inner);
    let to_inner = archive_edit::normalize_inner_path(&to_inner);
    if from_inner.is_empty() || to_inner.is_empty() {
        return Err(MutationError::ArchiveNotEditable);
    }

    // Reject renaming onto an existing inner name up front with the same friendly
    // message the real-FS rename uses, so the FE shows the standard "already
    // exists" copy instead of the raw `zip` "Duplicate filename" the mutator would
    // hit at write time — and no temp is built. (A no-op rename to the same name
    // is left to proceed; the mutator handles it harmlessly.)
    if to_inner != from_inner
        && archive_edit::archive_inner_exists(engine.host(), volume_id, &from_archive, &to_inner).await
    {
        return Err(MutationError::AlreadyExists { name: leaf(&to_inner) });
    }

    let events = engine
        .host()
        .operation_sink()
        .ok_or(MutationError::ArchiveEditNotReady)?;
    let summary = OperationSummaryText {
        source: Some(leaf(&from_inner)),
        destination: Some(leaf(&to_inner)),
        items: None,
    };
    let request = ArchiveEditRequest {
        archive_path: from_archive,
        parent_volume_id: volume_id.to_string(),
        changeset: Changeset {
            renames: vec![(from_inner, to_inner)],
            ..Default::default()
        },
        summary,
        skipped_count: 0,
        // No scan preview: nothing walked a tree to plan this edit.
        preview_id: None,
    };
    engine
        .archive_edit_start(events, request, 200)
        .await
        .map_err(|e| MutationError::ArchiveEditCouldntStart {
            detail: format!("{e:?}"),
        })?;
    Ok(())
}

/// The final component of a path, for the "already taken" message the rename
/// editor shows inline (which quotes the NAME, never the whole path).
fn name_of(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}

/// The last `/`-separated component of an inner path (for the queue summary).
fn leaf(inner_path: &str) -> String {
    inner_path.rsplit('/').next().unwrap_or(inner_path).to_string()
}

/// Builds the instant-op descriptor for a rename: no lanes, a `from → to`
/// basename summary, and the volume marked busy for its duration for non-root
/// volumes. Root is never ejectable, so it marks nothing busy (no eject-menu
/// churn for local renames).
fn rename_descriptor(from: &Path, to: &Path, volume_id: &str) -> OperationDescriptor {
    fn name(p: &Path) -> String {
        p.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| p.to_string_lossy().into_owned())
    }
    let volume_ids = if volume_id == "root" {
        vec![]
    } else {
        vec![volume_id.to_string()]
    };
    OperationDescriptor {
        operation_id: OperationId::from(cmdr_operation_log::new_operation_id()),
        operation_type: WriteOperationType::Rename,
        lanes: vec![],
        volume_ids,
        summary: OperationSummaryText {
            source: Some(name(from)),
            destination: Some(name(to)),
            items: None,
        },
        // An instant metadata op has no partial state, and no cancel path that
        // could catch it mid-flight.
        rollback_support: crate::types::RollbackSupport::InstantChange,
        // No scan preview: nothing walked a tree to plan this op.
        preview_id: None,
        reverses: None,
    }
}

/// Notifies the listing cache about a rename via the volume's `notify_mutation`.
async fn notify_rename_in_listing(host: &host::EngineHost, volume_id: &str, from: &Path, to: &Path) {
    use cmdr_fs::volume::MutationEvent;

    // Plain `get`, not `resolve`: a rename INSIDE an archive is rejected upstream,
    // so this only ever runs for a normal file (incl. the `.zip` file itself),
    // which must notify through its own volume — never route to the ArchiveVolume.
    let volume = match host.volumes().get(volume_id) {
        Some(v) => v,
        None => return,
    };

    if let (Some(from_parent), Some(from_name), Some(to_name)) = (from.parent(), from.file_name(), to.file_name()) {
        if from.parent() == to.parent() {
            // Same-directory rename
            volume
                .notify_mutation(
                    volume_id,
                    from_parent,
                    MutationEvent::Renamed {
                        from: from_name.to_string_lossy().to_string(),
                        to: to_name.to_string_lossy().to_string(),
                    },
                )
                .await;
        } else {
            // Cross-directory move
            volume
                .notify_mutation(
                    volume_id,
                    from_parent,
                    MutationEvent::Deleted(from_name.to_string_lossy().to_string()),
                )
                .await;
            if let Some(to_parent) = to.parent() {
                volume
                    .notify_mutation(
                        volume_id,
                        to_parent,
                        MutationEvent::Created(to_name.to_string_lossy().to_string()),
                    )
                    .await;
            }
        }
    }
}

impl crate::Operations {
    /// The pre-flight the inline editor runs before it opens: may this path be
    /// renamed at all?
    ///
    /// **Whether it applies is the VOLUME's answer, ❌ never the id's spelling.**
    /// The check below is `lstat` + `access`, so it means something only for a path
    /// an OS syscall can open by itself. A volume that serves its own I/O spells its
    /// paths with a scheme (`sftp://root@host:22/srv/data/x.xml`), and handing one to
    /// `symlink_metadata` doesn't ask a question, it manufactures a `NotFound`:
    /// ERR-KVERS reached a user as "There's nothing at "sftp://…" any more" on every
    /// F2, because the rename itself was never reached. Skipping is safe on those
    /// volumes — `rename_managed` is the authority, and it refuses with the backend's
    /// own typed answer.
    ///
    /// An id that resolves to no volume skips too: it's an unmount race, and the
    /// rename behind it will say so properly.
    pub async fn check_rename_permission_for_volume(
        &self,
        path: PathBuf,
        volume_id: String,
    ) -> Result<(), MutationError> {
        if !preflight_reaches(self.host(), &volume_id) {
            log::debug!(
                target: "volume",
                "rename pre-flight skipped on '{volume_id}': the volume serves its own I/O ({})",
                LogPrivate::path(&path)
            );
            return Ok(());
        }

        let attempted = path.clone();
        let checked = self
            .host()
            .runtime()
            .spawn_blocking(move || check_rename_permission_sync(&path));
        let result = match checked.await {
            Ok(result) => result,
            Err(join_err) => Err(MutationError::Unexpected {
                detail: format!("the permission check didn't finish: {join_err}"),
            }),
        };
        if let Err(error) = &result {
            log_rename_refusal(RenameStage::Preflight, &attempted, &volume_id, error);
        }
        result
    }
}

/// Whether a local `lstat` / `access` can answer for a path on this volume.
fn preflight_reaches(host: &host::EngineHost, volume_id: &str) -> bool {
    if volume_id == "root" {
        return true;
    }
    host.volumes()
        .get(volume_id)
        .is_some_and(|volume| volume.supports_local_fs_access())
}

/// Synchronous permission check: file exists, parent writable, and (macOS) not
/// immutable / SIP-protected. Runs on the blocking pool, behind the volume gate
/// in `Operations::check_rename_permission_for_volume` — ❌ never on a path off
/// a volume that serves its own I/O.
fn check_rename_permission_sync(path: &Path) -> Result<(), MutationError> {
    // Check that the file itself exists
    if std::fs::symlink_metadata(path).is_err() {
        return Err(MutationError::NotFound {
            path: path.display().to_string(),
        });
    }

    // Check parent directory is writable
    let parent = path.parent().ok_or(MutationError::CantRenameVolumeRoot)?;
    check_dir_writable(parent)?;

    // Check macOS-specific flags (immutable, SIP, locks)
    #[cfg(target_os = "macos")]
    check_macos_flags(path)?;

    Ok(())
}

/// Checks if a directory is writable using access(W_OK).
#[cfg(unix)]
fn check_dir_writable(dir: &Path) -> Result<(), MutationError> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let c_path = CString::new(dir.as_os_str().as_bytes()).map_err(|_| MutationError::NameHasDisallowedCharacter)?;
    // SAFETY: c_path is a valid null-terminated C string
    let result = unsafe { libc::access(c_path.as_ptr(), libc::W_OK) };
    if result != 0 {
        return Err(MutationError::ParentNotWritable {
            path: dir.display().to_string(),
        });
    }
    Ok(())
}

#[cfg(not(unix))]
fn check_dir_writable(_dir: &Path) -> Result<(), MutationError> {
    Ok(())
}

/// Checks macOS-specific immutable/SIP/lock flags.
#[cfg(target_os = "macos")]
fn check_macos_flags(path: &Path) -> Result<(), MutationError> {
    use std::ffi::CString;
    use std::mem::MaybeUninit;
    use std::os::unix::ffi::OsStrExt;

    let c_path = CString::new(path.as_os_str().as_bytes()).map_err(|_| MutationError::NameHasDisallowedCharacter)?;

    let mut stat = MaybeUninit::<libc::stat>::uninit();
    // SAFETY: c_path is valid, stat is a valid pointer
    let result = unsafe { libc::lstat(c_path.as_ptr(), stat.as_mut_ptr()) };
    if result != 0 {
        // Can't stat; file may have been deleted, let the rename itself fail with a clear error
        return Ok(());
    }

    // SAFETY: lstat succeeded
    let stat = unsafe { stat.assume_init() };

    // UF_IMMUTABLE (user immutable / "uchg" flag)
    const UF_IMMUTABLE: u32 = 0x00000002;
    // SF_IMMUTABLE (system immutable, set by SIP)
    const SF_IMMUTABLE: u32 = 0x00020000;

    if (stat.st_flags & UF_IMMUTABLE) != 0 {
        return Err(MutationError::FileLocked {
            path: path.display().to_string(),
        });
    }
    if (stat.st_flags & SF_IMMUTABLE) != 0 {
        return Err(MutationError::SipProtected {
            path: path.display().to_string(),
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests;
