//! New-folder / new-file creation and the managed create mutations.
//!
//! The command layer (`commands/file_system/write_ops.rs`) is a thin
//! pass-through: it expands tilde, resolves the `volume_id`, runs
//! `create_directory_managed` / `create_file_managed` detached under its reply
//! deadline (`mutation_reply.rs`: `Done`, the typed `MutationError`, or
//! `StillRunning` with a settle event later). All the logic lives here per "smart
//! backend / thin frontend"; the backend never names the command layer or
//! `expand_tilde`.
//!
//! Create is a managed instant op: the mutation runs inside
//! `manager::run_instant`, so it registers a `Running` record + marks its volume
//! busy (eject guard) for its duration, yet still runs inline and returns the new
//! path to its caller. It does NOT reserve a lane or queue behind transfers (see
//! `manager::run_instant`). There's no inner timeout: the work runs to its own
//! end, and the `InstantTaskGuard` releases the busy set even on a panic.
//!
//! The instant listing update (`patch_listings_after_create` /
//! `should_emit_synthetic_diff`) lives here, co-located with the create op it
//! serves: it's the listing-cache half of "a new entry appeared", and keeping it
//! next to the create keeps the command a pure pass-through.

use std::path::{Path, PathBuf};

use cmdr_operation_log::types::OperationId;

use super::Operations;
use super::archive_edit::{self, ArchiveEditRequest};
use super::host::{EngineHost, InstantMutationFact, InstantOp, InstantTarget};
use super::look_alike::{NewEntry, place_new_entry};
use super::manager::{OperationDescriptor, OperationSummaryText};
use super::mutation_error::MutationError;
use super::routed_writes::{route_serving, routed_mutation_refusal};
use super::types::{ReadOnlySide, WriteOperationType};
use cmdr_archive::mutator::{AddEntry, AddSource, Changeset};
use cmdr_fs::log_private::LogPrivate;
use cmdr_fs::volume::{MutationEvent, Volume};

/// Whether a routed archive add creates a directory entry or an empty file.
enum ArchiveEntryKind {
    Dir,
    File,
}

/// Parent-aware "does this create target land at or inside a `.zip`?" — the
/// routing gate for `create_directory_managed` / `create_file_managed`. `None`
/// volume means the local `"root"` drive. Uses `path_crosses` (not
/// `path_is_inside`) because the parent can BE the `.zip` file itself (creating an
/// entry at the archive root).
async fn parent_crosses_archive_boundary(host: &EngineHost, volume_id: Option<&str>, parent_path: &str) -> bool {
    host.volumes()
        .path_crosses_archive_boundary(volume_id.unwrap_or("root"), Path::new(parent_path))
        .await
}

/// Refuses a new entry inside a repo's `.git/<category>/` trees: the portal
/// serves them read-only, and the drive underneath would get a real entry in the
/// repository's `.git` folder (`routed_writes.rs`). An in-zip parent never gets
/// here: the archive gate above routes it to the edit driver.
async fn refuse_routed_create(
    host: &EngineHost,
    volume_id: Option<&str>,
    parent_path: &str,
    name: &str,
) -> Option<MutationError> {
    let new_path = Path::new(parent_path).join(name);
    route_serving(host, volume_id.unwrap_or("root"), &new_path)
        .await
        .map(|_| routed_mutation_refusal(&new_path))
}

impl Operations {
    /// Creates a folder as a managed instant op and returns its new path.
    /// `parent_path` is already tilde-expanded by the command layer.
    ///
    /// Wraps `create_directory_core_on` in `manager::run_instant` (busy-marking the
    /// volume + registering a brief `Running` record) and emits the synthetic
    /// listing diff on success for local-FS-backed volumes.
    pub async fn create_directory_managed(
        &self,
        volume_id: Option<String>,
        parent_path: String,
        name: String,
        initiator: cmdr_operation_log::types::Initiator,
    ) -> Result<String, MutationError> {
        // Wrapped, not emitted per branch: the archive route is an early return and
        // the instant path has three `?`s after it, so any in-body emit would count a
        // biased subset. See `rename_managed`, which is wrapped for the same reason.
        let (result, target) = create_directory_managed_inner(self, volume_id, parent_path, name, initiator).await;
        let fact = InstantMutationFact::of(InstantOp::CreateFolder, initiator, target, &result);
        self.host().events().instant_mutation_settled(fact);
        result
    }

    /// Creates an empty file as a managed instant op and returns its new path.
    /// Same shape as [`Self::create_directory_managed`].
    pub async fn create_file_managed(
        &self,
        volume_id: Option<String>,
        parent_path: String,
        name: String,
        initiator: cmdr_operation_log::types::Initiator,
    ) -> Result<String, MutationError> {
        // Wrapped, not emitted per branch: the archive route is an early return and
        // the instant path has three `?`s after it, so any in-body emit would count a
        // biased subset. See `rename_managed`, which is wrapped for the same reason.
        let (result, target) = create_file_managed_inner(self, volume_id, parent_path, name, initiator).await;
        let fact = InstantMutationFact::of(InstantOp::CreateFile, initiator, target, &result);
        self.host().events().instant_mutation_settled(fact);
        result
    }
}

/// The create itself. Returns where the new entry landed alongside its result, so
/// the wrapper above can label the event without re-deriving the route.
async fn create_directory_managed_inner(
    engine: &Operations,
    volume_id: Option<String>,
    parent_path: String,
    name: String,
    initiator: cmdr_operation_log::types::Initiator,
) -> (Result<String, MutationError>, InstantTarget) {
    // A parent that crosses into a `.zip` means the new folder lands INSIDE the
    // archive: route to the managed archive-edit driver (an O(archive) rewrite
    // with a real progress bar), not the instant path. Returns the operation id
    // (the FE queue window shows it), not a filesystem path. Parent-aware detection
    // (not the `std::fs`-only sync predicate) so creating inside a REMOTE zip
    // (direct SMB / MTP) routes too.
    if parent_crosses_archive_boundary(engine.host(), volume_id.as_deref(), &parent_path).await {
        return (
            route_archive_create(engine, &parent_path, &name, ArchiveEntryKind::Dir, volume_id).await,
            InstantTarget::Archive,
        );
    }
    if let Some(refusal) = refuse_routed_create(engine.host(), volume_id.as_deref(), &parent_path, &name).await {
        return (Err(refusal), InstantTarget::Volume);
    }

    let volume_id_for_diff = volume_id.clone();
    // Journal under the REAL volume id (`"root"` for the local drive) — the
    // volume-aware open/finalize collapse to the local shape when the id is root.
    let journal_volume_id = volume_id
        .as_deref()
        .unwrap_or(cmdr_fs::volume::DEFAULT_VOLUME_ID)
        .to_string();
    let descriptor = instant_descriptor(WriteOperationType::CreateFolder, volume_id.as_deref(), &name);
    let op_id = descriptor.operation_id.clone();
    // The op's own scope on the engine, which journals it: an instant op has no
    // state to carry one.
    let scope = engine.op_scope();
    super::journal::open_volume_op(
        &scope,
        op_id.as_str(),
        cmdr_operation_log::types::OpKind::CreateFolder,
        initiator,
        &journal_volume_id,
        None,
        1,
    );

    let result = engine
        .manager()
        .run_instant(
            descriptor,
            create_directory_core_on(engine.host(), volume_id, &parent_path, &name),
        )
        .await;
    super::journal::journal_instant_create(
        &scope,
        cmdr_operation_log::types::OpKind::CreateFolder,
        cmdr_operation_log::types::EntryType::Dir,
        &journal_volume_id,
        result.as_ref().ok().map(|(new_path, _)| new_path.as_path()),
    );
    let (new_path, expanded_path) = match result {
        Ok(paths) => paths,
        // A create that never landed is still a create the user asked for, so it
        // leaves through the same exit and the wrapper labels it `failed`.
        Err(e) => return (Err(e), InstantTarget::Volume),
    };

    // Synthetic diff only works for volumes backed by the local filesystem.
    // Protocol-only volumes (MTP) handle UI updates through their own event systems.
    if should_emit_synthetic_diff(engine.host(), volume_id_for_diff.as_deref()) {
        patch_listings_after_create(
            engine.host(),
            volume_id_for_diff.as_deref(),
            &new_path,
            &PathBuf::from(&expanded_path),
        );
    }
    (Ok(new_path.to_string_lossy().to_string()), InstantTarget::Volume)
}

/// The create itself. Returns where the new entry landed alongside its result, so
/// the wrapper above can label the event without re-deriving the route.
async fn create_file_managed_inner(
    engine: &Operations,
    volume_id: Option<String>,
    parent_path: String,
    name: String,
    initiator: cmdr_operation_log::types::Initiator,
) -> (Result<String, MutationError>, InstantTarget) {
    // See `create_directory_managed`: a `.zip`-crossing parent routes the new
    // (empty) file into the archive via the managed edit driver (parent-aware, so
    // a REMOTE zip routes too).
    if parent_crosses_archive_boundary(engine.host(), volume_id.as_deref(), &parent_path).await {
        return (
            route_archive_create(engine, &parent_path, &name, ArchiveEntryKind::File, volume_id).await,
            InstantTarget::Archive,
        );
    }
    if let Some(refusal) = refuse_routed_create(engine.host(), volume_id.as_deref(), &parent_path, &name).await {
        return (Err(refusal), InstantTarget::Volume);
    }

    let volume_id_for_diff = volume_id.clone();
    let journal_volume_id = volume_id
        .as_deref()
        .unwrap_or(cmdr_fs::volume::DEFAULT_VOLUME_ID)
        .to_string();
    let descriptor = instant_descriptor(WriteOperationType::CreateFile, volume_id.as_deref(), &name);
    let op_id = descriptor.operation_id.clone();
    // The op's own scope on the engine, which journals it: an instant op has no
    // state to carry one.
    let scope = engine.op_scope();
    super::journal::open_volume_op(
        &scope,
        op_id.as_str(),
        cmdr_operation_log::types::OpKind::CreateFile,
        initiator,
        &journal_volume_id,
        None,
        1,
    );

    let result = engine
        .manager()
        .run_instant(
            descriptor,
            create_file_core_on(engine.host(), volume_id, &parent_path, &name),
        )
        .await;
    super::journal::journal_instant_create(
        &scope,
        cmdr_operation_log::types::OpKind::CreateFile,
        cmdr_operation_log::types::EntryType::File,
        &journal_volume_id,
        result.as_ref().ok().map(|(new_path, _)| new_path.as_path()),
    );
    let (new_path, expanded_path) = match result {
        Ok(paths) => paths,
        Err(e) => return (Err(e), InstantTarget::Volume),
    };

    if should_emit_synthetic_diff(engine.host(), volume_id_for_diff.as_deref()) {
        patch_listings_after_create(
            engine.host(),
            volume_id_for_diff.as_deref(),
            &new_path,
            &PathBuf::from(&expanded_path),
        );
    }
    (Ok(new_path.to_string_lossy().to_string()), InstantTarget::Volume)
}

/// Routes an in-archive mkdir/mkfile to the managed archive-edit driver. Builds
/// a one-entry changeset (an explicit directory entry, or a zero-byte file),
/// starts the managed op, and returns its operation id.
///
/// Note the return-value shift versus the instant path: this yields the
/// OPERATION ID, not the new filesystem path (an archive-inner path isn't a real
/// FS path, and the edit is asynchronous). The FE distinguishes archive-target
/// creates and reads the id as an operation handle, not a cursor target.
async fn route_archive_create(
    engine: &Operations,
    parent_path: &str,
    name: &str,
    kind: ArchiveEntryKind,
    volume_id: Option<String>,
) -> Result<String, MutationError> {
    // Confirmation happened at the routing site (parent-aware
    // `path_crosses_archive_boundary`), so a pure string split suffices — and it
    // works for a REMOTE zip, where the `std::fs` confirm would wrongly fail.
    let (archive_path, inner_parent) = cmdr_fs::archive_format::archive_boundary_candidate(Path::new(parent_path))
        .ok_or(MutationError::ArchiveNotEditable)?;
    // Only zip archives are writable; tar and 7z are browse + extract only.
    archive_edit::ensure_zip_writable(&archive_path, ReadOnlySide::Destination)
        .map_err(|_| MutationError::ArchiveReadOnly)?;
    let inner_path = archive_edit::join_inner_path(&inner_parent, name);

    // Reject a duplicate up front with the same friendly message the real-FS
    // mkdir/mkfile paths use, so the FE shows the standard "already exists" copy
    // instead of the raw `zip` "Duplicate filename" the mutator would hit at
    // write time — and no temp is built for a doomed edit.
    let parent_volume_id = volume_id.as_deref().unwrap_or("root");
    if archive_edit::archive_inner_exists(engine.host(), parent_volume_id, &archive_path, &inner_path).await {
        return Err(MutationError::AlreadyExists { name: name.to_string() });
    }

    let changeset = match kind {
        ArchiveEntryKind::Dir => Changeset {
            mkdirs: vec![inner_path],
            ..Default::default()
        },
        ArchiveEntryKind::File => Changeset {
            adds: vec![AddEntry {
                inner_path,
                source: AddSource::Bytes(Vec::new()),
            }],
            ..Default::default()
        },
    };

    let events = engine
        .host()
        .operation_sink()
        .ok_or(MutationError::ArchiveEditNotReady)?;
    let request = ArchiveEditRequest {
        archive_path,
        parent_volume_id: volume_id.unwrap_or_else(|| "root".to_string()),
        changeset,
        summary: OperationSummaryText {
            source: Some(name.to_string()),
            destination: None,
            items: None,
        },
        skipped_count: 0,
        // No scan preview: nothing walked a tree to plan this edit.
        preview_id: None,
    };
    let started =
        engine
            .archive_edit_start(events, request, 200)
            .await
            .map_err(|e| MutationError::ArchiveEditCouldntStart {
                detail: format!("{e:?}"),
            })?;
    Ok(started.operation_id)
}

/// Builds the instant-op descriptor: no lanes, the new entry's name as the
/// summary, and the volume marked busy for non-root volumes (`root` is never
/// ejectable, so it stays out of the busy set — no eject-menu churn for local
/// creates).
///
/// `pub(super)` so the sibling numbered create (`create_numbered.rs`) routes through the same
/// managed `CreateFile` instant op (and journals for free).
pub(super) fn instant_descriptor(
    op_type: WriteOperationType,
    volume_id: Option<&str>,
    name: &str,
) -> OperationDescriptor {
    let volume_ids = match volume_id {
        None | Some("root") => vec![],
        Some(id) => vec![id.to_string()],
    };
    OperationDescriptor {
        operation_id: OperationId::from(cmdr_operation_log::new_operation_id()),
        operation_type: op_type,
        lanes: vec![],
        volume_ids,
        summary: OperationSummaryText {
            source: Some(name.to_string()),
            destination: None,
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

/// Core mkdir logic: validates the name, resolves the volume, registers the
/// downloads-watcher ignore, and issues the `create_directory` syscall. Returns
/// `(new_path, parent_path)` (the parent is the already-expanded path passed in).
/// Kept separate from the managed wrapper so it's testable without the operation
/// manager, against `host`'s volumes and own-write seam. `parent_path` must
/// already be tilde-expanded by the caller.
pub async fn create_directory_core_on(
    host: &EngineHost,
    volume_id: Option<String>,
    parent_path: &str,
    name: &str,
) -> Result<(PathBuf, String), MutationError> {
    if name.is_empty() {
        return Err(MutationError::NameEmpty);
    }
    if name.contains('/') || name.contains('\0') {
        return Err(MutationError::NameHasDisallowedCharacter);
    }

    // Defensive fallback: the managed wrapper (`create_directory_managed`) routes
    // an archive-crossing parent to `route_archive_create` BEFORE reaching core,
    // so in production this branch never fires. It stays as a guard for a direct
    // `*_core` caller (tests): core's plain-FS create can't reach inside a `.zip`,
    // so it refuses rather than attempting a bogus filesystem write. Uses
    // `path_crosses` (NOT `path_is_inside`) so a `.zip` file AS the parent (a child
    // at the archive root) is caught too.
    if cmdr_archive::path_crosses_archive_boundary(Path::new(parent_path)) {
        return Err(MutationError::ArchiveNotEditable);
    }

    let volume_id = volume_id.unwrap_or_else(|| "root".to_string());
    let expanded_path = parent_path.to_string();

    // Try to use Volume abstraction. Plain `get`: the archive gates above sent an
    // in-zip parent to the archive-edit driver.
    if let Some(volume) = host.volumes().get(&volume_id) {
        let new_path = new_entry_path(host, volume.as_ref(), &volume_id, &expanded_path, name).await?;

        // Register the new directory path with the downloads watcher's
        // ignore set; no-ops for paths outside ~/Downloads.
        host.announce(&new_path);

        volume
            .create_directory(&new_path)
            .await
            .map_err(|e| named_conflict_or_volume(e, name))?;

        return Ok((new_path, expanded_path));
    }

    // "root" and every mounted volume is always registered in `VolumeManager`,
    // so reaching here means a phone or saved server nobody connected yet, or a
    // volume unregistered out from under us (an unmount race). Error out instead
    // of falling back to an untimed synchronous `std::fs::create_dir` on the async
    // executor, which would violate this module's "every FS-touching command is
    // timed" contract on a hung mount.
    Err(super::mutation_error::unregistered_volume_refusal(host.volumes(), volume_id, Path::new(parent_path)).await)
}

/// Where the new folder or file `name` goes in `parent`: spelled the way the
/// volume wants new names, and refused as taken when the folder already holds
/// the name under another Unicode spelling, which a byte-exact share would
/// otherwise create a twin beside (`look_alike.rs`). A pane showing `parent`
/// answers that from its listing, so the person isn't waiting on a re-read.
async fn new_entry_path(
    host: &EngineHost,
    volume: &dyn Volume,
    volume_id: &str,
    parent: &str,
    name: &str,
) -> Result<PathBuf, MutationError> {
    match place_new_entry(
        host.listings(),
        volume,
        volume_id,
        &PathBuf::from(parent).join(name),
        None,
    )
    .await
    {
        Ok(NewEntry::Free(path)) => Ok(path),
        Ok(NewEntry::Taken(_) | NewEntry::Ambiguous) => Err(MutationError::AlreadyExists { name: name.to_string() }),
        Err(error) => Err(MutationError::Volume { error }),
    }
}

/// Reports a taken name by the NAME the user typed, and everything else by the
/// volume's own typed answer.
///
/// The name matters because the New Folder / New File dialog quotes it back
/// inline, right under the field the user is still standing in; the volume's
/// `AlreadyExists` carries the full path, which reads as a different thing
/// entirely in a one-line inline message.
fn named_conflict_or_volume(e: cmdr_fs::volume::VolumeError, name: &str) -> MutationError {
    match e {
        cmdr_fs::volume::VolumeError::AlreadyExists(_) => MutationError::AlreadyExists { name: name.to_string() },
        other => MutationError::Volume { error: other },
    }
}

/// Core file-creation logic, against `host`'s volumes and own-write seam. Same
/// shape as [`create_directory_core_on`].
pub(crate) async fn create_file_core_on(
    host: &EngineHost,
    volume_id: Option<String>,
    parent_path: &str,
    name: &str,
) -> Result<(PathBuf, String), MutationError> {
    if name.is_empty() {
        return Err(MutationError::NameEmpty);
    }
    if name.contains('/') || name.contains('\0') {
        return Err(MutationError::NameHasDisallowedCharacter);
    }

    // Defensive fallback, same as `create_directory_core_on`: the managed wrapper
    // (`create_file_managed`) routes an archive-crossing parent to
    // `route_archive_create` before reaching core, so this branch is unreachable
    // in production. It guards a direct `*_core` caller (tests) from a bogus
    // plain-FS write into a `.zip`. `path_crosses` also catches a `.zip`-file parent.
    if cmdr_archive::path_crosses_archive_boundary(Path::new(parent_path)) {
        return Err(MutationError::ArchiveNotEditable);
    }

    let volume_id = volume_id.unwrap_or_else(|| "root".to_string());
    let expanded_path = parent_path.to_string();

    // Try to use Volume abstraction. Plain `get`: the archive gates above sent an
    // in-zip parent to the archive-edit driver.
    if let Some(volume) = host.volumes().get(&volume_id) {
        let new_path = new_entry_path(host, volume.as_ref(), &volume_id, &expanded_path, name).await?;

        // Register the new file path with the downloads watcher's ignore
        // set; no-ops for paths outside ~/Downloads.
        host.announce(&new_path);

        volume
            .create_file(&new_path, b"")
            .await
            .map_err(|e| named_conflict_or_volume(e, name))?;

        return Ok((new_path, expanded_path));
    }

    // See `create_directory_core_on`: an unregistered volume is one nobody connected
    // or an unmount race; error out instead of an untimed
    // `std::fs::File::create_new` fallback.
    Err(super::mutation_error::unregistered_volume_refusal(host.volumes(), volume_id, Path::new(parent_path)).await)
}

/// Returns true if a synthetic entry diff should be emitted for this volume.
/// Protocol-only volumes (like MTP) don't support `std::fs` access, so synthetic
/// diffs would fail. These volumes handle UI updates through their own event systems.
///
/// `pub(super)` so the sibling numbered create (`create_numbered.rs`) reuses the same
/// listing-cache update the create op emits (cursor-land parity with mkfile).
pub(super) fn should_emit_synthetic_diff(host: &EngineHost, volume_id: Option<&str>) -> bool {
    match volume_id {
        None => true, // No volume_id means local filesystem
        Some(id) => host.volumes().get(id).is_none_or(|v| v.supports_local_fs_access()),
    }
}

/// Lands a newly created entry in the panes on its own volume, the way every
/// other local mutation does: the local backend's mutation patch stats it and
/// hands an `Added` to the composer (index, badges, then panes). A pane that
/// already holds the path (the watcher got there first) gets the fresher stat as
/// an upsert, tags carried forward.
///
/// `new_path`'s own file name, never the name the user typed: the paste retry
/// loop may have picked `pasted (N)`. `None` is the local filesystem (`root`);
/// this only runs for local-FS volumes (`should_emit_synthetic_diff`).
///
/// `pub(super)` so the sibling numbered create (`create_numbered.rs`) reuses it (see
/// `should_emit_synthetic_diff`).
pub(super) fn patch_listings_after_create(
    host: &EngineHost,
    volume_id: Option<&str>,
    new_path: &Path,
    parent_path: &Path,
) {
    let Some(name) = new_path.file_name() else {
        log::warn!("Created entry has no file name: {}", LogPrivate::path(new_path));
        return;
    };
    cmdr_local::mutation::patch_listing_after_local_mutation(
        host.listings(),
        volume_id.unwrap_or(cmdr_fs::volume::DEFAULT_VOLUME_ID),
        parent_path,
        MutationEvent::Created(name.to_string_lossy().into_owned()),
    );
}

#[cfg(test)]
mod tests;
