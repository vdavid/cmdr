//! Tauri commands for write operations (create, copy, move, delete, trash) and scan preview.

use crate::file_system::write_operations::TrashRoutingAnswer;
use crate::file_system::write_operations::{
    ConflictId, ConflictResolution, ConflictResolutionOutcome, MUTATION_REPLY_DEADLINE, MutationError, MutationReply,
    MutationSettled, ScanPreviewRefusal, ScanPreviewStartResult, broadcast_settled,
    cancel_scan_preview as ops_cancel_scan_preview, create_directory_managed as ops_create_directory_managed,
    create_file_managed as ops_create_file_managed, get_scan_preview_totals as ops_get_scan_preview_totals,
    reply_within, resolve_write_conflict as ops_resolve_write_conflict, start_scan_preview as ops_start_scan_preview,
    trash_routing_for_selection as ops_trash_routing_for_selection,
};
use crate::file_system::{
    OperationEventSink, OperationSnapshot, PauseAllOutcome, PauseOutcome, ReadOnlySide, SortColumn, SortOrder,
    TauriEventSink, WriteOperationConfig, WriteOperationError, WriteOperationStartResult,
    cancel_operation as ops_cancel_operation, cancel_operations as ops_cancel_operations,
    cancel_write_operation as ops_cancel_write_operation, delete_files_start as ops_delete_files_start,
    dismiss_all_failed_operations as ops_dismiss_all_failed_operations,
    dismiss_failed_operation as ops_dismiss_failed_operation, list_operations as ops_list_operations,
    move_files_start as ops_move_files_start, pause_all as ops_pause_all, pause_operation as ops_pause_operation,
    resume_all as ops_resume_all, resume_operation as ops_resume_operation, trash_files_start as ops_trash_files_start,
};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::time::Duration;

use crate::deadline::{DeadlineError, blocking_with_timeout, timeout_detached_typed};
use crate::file_system::Volume;
use crate::file_system::volume::manager::get_volume_manager;
use crate::operation_log::types::Initiator;

use super::expand_tilde;
use crate::file_system::volume::manager::path_routes_over_its_parent;

/// Picks the source volume for a scan preview.
///
/// Routes a source a ROUTE serves — inside a `.zip`, or inside a repo's virtual
/// `.git` trees — to that read-only volume, so the preview scan reads the entries
/// the copy will actually read. Without it, such a source (whose display volume id
/// is the local parent drive, `"root"`) takes the local `std::fs` scan path,
/// finds 0 files because those paths have no inode, caches a 0-file preview, and
/// the copy reuses it: the extract-out "cancelled after 0 files" stall, and the
/// copy-out-of-a-snapshot dialog stuck forever on "Verifying before copy".
/// `path_routes_over_its_parent` is pure string work, so ordinary local/remote
/// scans pay nothing.
///
/// `None` means "scan the local filesystem directly" (the `std::fs` fast path);
/// `Some` means scan through the `Volume` trait. A non-local id no volume answers
/// for is refused: ❌ never fall back to `std::fs` for it, which walks an
/// `adb://…` path on the Mac and fails behind a Retry that can't help.
async fn scan_preview_source_volume(
    volume_id: &str,
    first_source: Option<&PathBuf>,
) -> Result<Option<Arc<dyn Volume>>, ScanPreviewRefusal> {
    // The `.zip` file itself is scanned as a plain file (one entry), never its
    // contents, which is why the gate asks about a path INSIDE a route rather
    // than about a `.zip` component.
    let is_routed_candidate = first_source.is_some_and(|first| path_routes_over_its_parent(first));
    let routed_source = if is_routed_candidate {
        // Bounded, because confirming a REMOTE archive boundary is real network
        // I/O and this runs BEFORE the preview exists — so the scan watchdog,
        // which bounds everything after, cannot cover it, and a wedged share
        // would hold the dialog on its spinner with no preview to give up on.
        // A deadline here degrades to the parent volume below, which reaches a
        // terminal outcome the ordinary way.
        let owned_id = volume_id.to_string();
        let owned_source = first_source.expect("candidate implies a source").clone();
        let resolved = timeout_detached_typed(
            Duration::from_secs(15),
            || DeadlineError::TimedOut,
            |detail| DeadlineError::Unexpected { detail },
            async move { Ok::<_, DeadlineError>(get_volume_manager().resolve(&owned_id, &owned_source).await) },
        )
        .await;
        // The route gates whether we actually got the routed volume: a mislabeled
        // `.zip`, a `.git` that isn't a repository, or a switched-off portal all
        // fall through to the parent, which the branches below handle.
        resolved.ok().and_then(|r| r.is_routed().then_some(r.volume).flatten())
    } else {
        None
    };
    if routed_source.is_some() {
        Ok(routed_source)
    } else if volume_id == "root" {
        Ok(None)
    } else {
        get_volume_manager()
            .get(volume_id)
            .map(Some)
            .ok_or_else(|| ScanPreviewRefusal::SourceNotConnected {
                volume_id: volume_id.to_string(),
            })
    }
}

/// Rejects a local write op that touches a path a ROUTE serves: inside an
/// archive, or inside a repo's virtual `.git` trees. Neither has a file on disk
/// for `std::fs` to act on, so the local fast path would grind to a `NotFound`
/// that reads like a broken app; this answers with the typed read-only refusal
/// the frontend already renders.
///
/// Both questions confirm rather than guess: the archive half stats the `.zip`
/// and sniffs its magic, and the git half is `false` whenever the portal is
/// switched off, so a real folder called `foo.zip` or a real `.git/branches/`
/// keeps taking ordinary local writes.
///
/// `side` says which half of the transfer these paths are, because the two are
/// different sentences: a move OFF a snapshot is refused by its SOURCE, and
/// telling that user to choose a different destination names the half that was
/// fine. Callers that check both halves ask twice, once per side.
///
/// A backend safety net behind the frontend's read-only capability gating. The
/// transfers that legitimately cross a route (extract-out, copy out of a
/// snapshot, writing into a zip) run through `copy_between_volumes`, which routes
/// to the read-only volume, ❌ never this fast path.
fn reject_if_routed_over_a_parent<'a>(
    paths: impl IntoIterator<Item = &'a PathBuf>,
    side: ReadOnlySide,
) -> Result<(), WriteOperationError> {
    for path in paths {
        // Only a path INSIDE an archive is read-only. The `.zip` file itself is a
        // regular file — copying/moving/deleting/trashing it must work.
        if cmdr_archive::path_is_inside_archive(path) || crate::file_system::git::wiring::portal_serves(path) {
            return Err(WriteOperationError::ReadOnlyDevice {
                path: path.to_string_lossy().into_owned(),
                device_name: None,
                side,
            });
        }
    }
    Ok(())
}

/// Creates a folder. Thin pass-through to the managed create op
/// (`write_operations::create`): expand tilde (root only), answer within
/// `MUTATION_REPLY_DEADLINE`, and ship the typed `MutationError` the frontend
/// renders its words from. A create still running at the deadline answers
/// `StillRunning` and reports its end on `mutation-settled`
/// (`write_operations/mutation_reply.rs`).
#[tauri::command]
#[specta::specta]
pub async fn create_directory(
    app: tauri::AppHandle,
    volume_id: Option<String>,
    parent_path: String,
    name: String,
    initiator: Option<Initiator>,
) -> Result<MutationReply, MutationError> {
    create_directory_replying(volume_id, parent_path, name, initiator, broadcast_settled(app)).await
}

/// [`create_directory`] with the settle delivery passed in.
async fn create_directory_replying(
    volume_id: Option<String>,
    parent_path: String,
    name: String,
    initiator: Option<Initiator>,
    on_settled: impl FnOnce(MutationSettled) + Send + 'static,
) -> Result<MutationReply, MutationError> {
    let expanded_parent = expand_parent(volume_id.as_deref(), &parent_path);
    reply_within(
        MUTATION_REPLY_DEADLINE,
        ops_create_directory_managed(volume_id, expanded_parent, name, initiator.unwrap_or(Initiator::User)),
        on_settled,
    )
    .await
}

/// Creates an empty file. Same shape as [`create_directory`].
#[tauri::command]
#[specta::specta]
pub async fn create_file(
    app: tauri::AppHandle,
    volume_id: Option<String>,
    parent_path: String,
    name: String,
    initiator: Option<Initiator>,
) -> Result<MutationReply, MutationError> {
    create_file_replying(volume_id, parent_path, name, initiator, broadcast_settled(app)).await
}

/// [`create_file`] with the settle delivery passed in.
async fn create_file_replying(
    volume_id: Option<String>,
    parent_path: String,
    name: String,
    initiator: Option<Initiator>,
    on_settled: impl FnOnce(MutationSettled) + Send + 'static,
) -> Result<MutationReply, MutationError> {
    let expanded_parent = expand_parent(volume_id.as_deref(), &parent_path);
    reply_within(
        MUTATION_REPLY_DEADLINE,
        ops_create_file_managed(volume_id, expanded_parent, name, initiator.unwrap_or(Initiator::User)),
        on_settled,
    )
    .await
}

/// Expands tilde for local (`root`) parents only; volume paths are
/// volume-relative and must never be tilde-expanded.
fn expand_parent(volume_id: Option<&str>, parent_path: &str) -> String {
    if volume_id.unwrap_or("root") == "root" {
        expand_tilde(parent_path)
    } else {
        parent_path.to_string()
    }
}

// ============================================================================
// Write operations (copy, move, delete)
// ============================================================================

/// Turns a same-`root` move request into backend arguments: tilde-expanded
/// paths and the default config. A transfer that touches a routed namespace on
/// either end doesn't belong on the local fast path (a copy out of a zip or a
/// snapshot routes through `copy_between_volumes`; writing INTO either is
/// read-only), so it is refused here.
fn local_transfer_request(
    sources: &[String],
    destination: &str,
    config: Option<WriteOperationConfig>,
) -> Result<(Vec<PathBuf>, PathBuf, WriteOperationConfig), WriteOperationError> {
    let sources: Vec<PathBuf> = sources.iter().map(|s| PathBuf::from(expand_tilde(s))).collect();
    let destination = PathBuf::from(expand_tilde(destination));
    // Twice, so the refusal names the half that actually refused rather than
    // whichever path the loop happened to reach first.
    reject_if_routed_over_a_parent(sources.iter(), ReadOnlySide::Source)?;
    reject_if_routed_over_a_parent(std::iter::once(&destination), ReadOnlySide::Destination)?;
    Ok((sources, destination, config.unwrap_or_default()))
}

/// Uses rename() for same-filesystem (instant), copy+delete for cross-filesystem.
/// Emits write-progress, write-complete, write-error, write-cancelled.
#[tauri::command]
#[specta::specta]
pub async fn move_files(
    app: tauri::AppHandle,
    sources: Vec<String>,
    destination: String,
    config: Option<WriteOperationConfig>,
    initiator: Option<Initiator>,
) -> Result<WriteOperationStartResult, WriteOperationError> {
    let (sources, destination, config) = local_transfer_request(&sources, &destination, config)?;

    // Same-`root` local move (the FE uses `move_between_volumes` whenever the
    // source and destination volumes differ), so no ejectable volume here.
    let events: Arc<dyn OperationEventSink> = Arc::new(TauriEventSink::new(app));
    ops_move_files_start(
        events,
        sources,
        destination,
        config,
        vec![],
        None,
        initiator.unwrap_or(Initiator::User),
        // No source binding: the user picked these in the pane they are looking at.
        None,
        // No typed sides: this is the same-`root` path, where both ends are the
        // boot volume and no drive can leave under it.
        None,
    )
    .await
}

/// Recursively deletes files and directories. Same events as `move_files`.
/// When `volume_id` is provided and is not "root", routes through the Volume trait.
#[tauri::command]
#[specta::specta]
pub async fn delete_files(
    app: tauri::AppHandle,
    sources: Vec<String>,
    volume_id: Option<String>,
    config: Option<WriteOperationConfig>,
    initiator: Option<Initiator>,
) -> Result<WriteOperationStartResult, WriteOperationError> {
    let is_local = volume_id.as_deref().unwrap_or("root") == "root";
    let sources: Vec<PathBuf> = if is_local {
        sources.iter().map(|s| PathBuf::from(expand_tilde(s))).collect()
    } else {
        sources.iter().map(PathBuf::from).collect()
    };
    let config = config.unwrap_or_default();

    // Deleting an entry INSIDE an archive routes to the managed archive-edit
    // driver inside `delete_files_start` (a `{ delete }` changeset), so no
    // rejection here. The `.zip` file itself deletes on the normal path.
    let events: Arc<dyn OperationEventSink> = Arc::new(TauriEventSink::new(app));
    ops_delete_files_start(
        events,
        sources,
        config,
        volume_id,
        initiator.unwrap_or(Initiator::User),
        None,
    )
    .await
}

/// Moves files to macOS Trash. Same events as `move_files` but with `operationType: trash`.
#[tauri::command]
#[specta::specta]
pub async fn trash_files(
    app: tauri::AppHandle,
    sources: Vec<String>,
    item_sizes: Option<Vec<u64>>,
    config: Option<WriteOperationConfig>,
    initiator: Option<Initiator>,
) -> Result<WriteOperationStartResult, WriteOperationError> {
    let sources: Vec<PathBuf> = sources.iter().map(|s| PathBuf::from(expand_tilde(s))).collect();
    let config = config.unwrap_or_default();

    // Trashing an entry inside a routed namespace is a mutation of the thing that
    // holds it, so the refusing half is the source the user selected.
    reject_if_routed_over_a_parent(sources.iter(), ReadOnlySide::Source)?;

    let events: Arc<dyn OperationEventSink> = Arc::new(TauriEventSink::new(app));
    ops_trash_files_start(
        events,
        sources,
        item_sizes,
        config,
        initiator.unwrap_or(Initiator::User),
        // No source binding: the user picked these in the pane they are looking at.
        None,
    )
    .await
}

/// Paths come from the pane the user is looking at, so this is a read-tier
/// question. The work is a `canonicalize` plus an `lstat` per selected item; the
/// timeout only bites when one of them sits on a hung mount.
const TRASH_ROUTING_TIMEOUT: Duration = Duration::from_secs(2);

/// Answers whether an F8 over `sources` has to run as a permanent delete because
/// the selection holds online-only content in a cloud-storage folder, which a
/// trash would download before it could move.
///
/// Asked before the confirmation dialog opens, so the dialog can say why it's
/// asking about a delete. A timeout degrades to `Trash`, which is today's
/// behavior: the attempt goes to the OS and a refusal speaks for itself. The
/// answer's `folder_may_hold_online_only` says the dialog's own scan walk still
/// has to finish the question; see `delete/cloud_trash.rs`.
#[tauri::command]
#[specta::specta]
pub async fn trash_routing_for_paths(sources: Vec<String>) -> TrashRoutingAnswer {
    let sources: Vec<PathBuf> = sources.iter().map(|s| PathBuf::from(expand_tilde(s))).collect();
    blocking_with_timeout(TRASH_ROUTING_TIMEOUT, TrashRoutingAnswer::TRASH, move || {
        ops_trash_routing_for_selection(&sources)
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub fn cancel_write_operation(operation_id: String, rollback: bool) {
    ops_cancel_write_operation(&operation_id, rollback);
}

// ============================================================================
// Scan preview (for Copy dialog live stats)
// ============================================================================

/// Scans source files for Copy dialog stats. Results are cached for reuse by the actual copy.
/// Emits scan-preview-progress, scan-preview-complete, scan-preview-error, scan-preview-cancelled.
///
/// When `source_volume_id` is provided and is not "root", the scan uses the Volume trait
/// (enabling MTP and other non-local volumes). Otherwise, uses `std::fs` for local scanning.
#[tauri::command]
#[specta::specta]
pub async fn start_scan_preview(
    app: tauri::AppHandle,
    sources: Vec<String>,
    source_volume_id: Option<String>,
    sort_column: SortColumn,
    sort_order: SortOrder,
    progress_interval_ms: Option<u64>,
    // Compress-mode scans set this so the local walk samples a compressed-size
    // estimate. Ignored for remote sources (never sampled). `None` == false.
    sample_for_estimate: Option<bool>,
) -> Result<ScanPreviewStartResult, ScanPreviewRefusal> {
    let volume_id = source_volume_id.unwrap_or_else(|| "root".to_string());
    let is_local = volume_id == "root";

    // Only expand tilde for local paths
    let sources: Vec<PathBuf> = if is_local {
        sources.iter().map(|s| PathBuf::from(expand_tilde(s))).collect()
    } else {
        sources.iter().map(PathBuf::from).collect()
    };

    let source_volume = scan_preview_source_volume(&volume_id, sources.first()).await?;

    let progress_interval = progress_interval_ms.unwrap_or(500);
    Ok(ops_start_scan_preview(
        app,
        sources,
        source_volume,
        volume_id,
        sort_column,
        sort_order,
        progress_interval,
        sample_for_estimate.unwrap_or(false),
    ))
}

#[tauri::command]
#[specta::specta]
pub fn cancel_scan_preview(preview_id: String) {
    ops_cancel_scan_preview(&preview_id);
}

/// Returns the cached totals from a completed scan preview, or `null` while the
/// scan is still running / cancelled / errored. The FE uses the presence of a
/// value both as a "scan done" signal and to repopulate display state when its
/// listeners missed the events (a watcher-backed oracle can finish before the
/// FE finishes the `startScanPreview()` IPC round-trip).
#[tauri::command]
#[specta::specta]
pub fn check_scan_preview_status(
    preview_id: String,
) -> Option<crate::file_system::write_operations::ScanPreviewTotals> {
    ops_get_scan_preview_totals(&preview_id)
}

/// In Stop mode, the operation pauses on conflict and waits for this call to
/// proceed. The `write-conflict` event reaches every webview, so several
/// surfaces can be showing the same prompt; the returned outcome tells this
/// caller whether ITS answer is the one the operation acted on, so a surface
/// that lost the race takes its prompt down instead of hanging on it.
///
/// `conflict_id` names the clash being answered (it arrives on the event). An
/// answer for a clash the operation has already moved past is refused as
/// `stale_answer` rather than applied to whatever it is parked on now.
#[tauri::command]
#[specta::specta]
pub fn resolve_write_conflict(
    operation_id: String,
    conflict_id: ConflictId,
    resolution: ConflictResolution,
    apply_to_all: bool,
) -> ConflictResolutionOutcome {
    ops_resolve_write_conflict(&operation_id, conflict_id, resolution, apply_to_all)
}

// ============================================================================
// Operation manager (queue + lifecycle)
// ============================================================================

/// Returns the thin operation registry snapshot (membership + lifecycle
/// status) for the queue window. Live per-row progress comes from the separate
/// `write-progress` stream; this snapshot stays thin.
#[tauri::command]
#[specta::specta]
pub fn list_operations() -> Vec<OperationSnapshot> {
    ops_list_operations()
}

/// Cancels one operation, keeping already-copied files. A Queued op is dropped
/// without ever spawning; a Running/Paused op routes through the existing
/// keep-partials cancel path.
#[tauri::command]
#[specta::specta]
pub fn cancel_operation(operation_id: String) {
    ops_cancel_operation(&operation_id);
}

/// Cancels several operations (keep-partials each). Backs the queue window's
/// "Cancel selected".
#[tauri::command]
#[specta::specta]
pub fn cancel_operations(operation_ids: Vec<String>) {
    ops_cancel_operations(&operation_ids);
}

/// Pauses one Running operation. It parks at the next between-files boundary and
/// its lifecycle status flips to `paused` in `operations-changed`. A paused op
/// keeps holding its lane slots. Pausing a Queued/Done op is a no-op.
///
/// Returns what actually happened, so a caller never has to assume it worked.
#[tauri::command]
#[specta::specta]
pub fn pause_operation(operation_id: String) -> PauseOutcome {
    ops_pause_operation(&operation_id)
}

/// Resumes one paused operation: it continues from where it parked and its
/// status flips back to `running`. Resuming a non-paused op is a no-op.
///
/// Returns what actually happened, like [`pause_operation`].
#[tauri::command]
#[specta::specta]
pub fn resume_operation(operation_id: String) -> PauseOutcome {
    ops_resume_operation(&operation_id)
}

/// Pauses every currently-running operation. Backs the queue window's global
/// "Pause all".
///
/// Returns the per-outcome counts for the whole sweep, so a caller never has to
/// assume it worked. An empty running set and three parked copies are different
/// answers.
#[tauri::command]
#[specta::specta]
pub fn pause_all() -> PauseAllOutcome {
    ops_pause_all()
}

/// Resumes every currently-paused operation. Backs "Resume all".
///
/// Returns the sweep's counts, like [`pause_all`].
#[tauri::command]
#[specta::specta]
pub fn resume_all() -> PauseAllOutcome {
    ops_resume_all()
}

/// Drops one retained failure from the snapshot (the queue row's Dismiss, and
/// the foreground error dialog's close path). Dismissal is always explicit: a
/// failure never expires on its own.
#[tauri::command]
#[specta::specta]
pub fn dismiss_failed_operation(operation_id: String) {
    ops_dismiss_failed_operation(&operation_id);
}

/// Drops every retained failure. Backs the queue window's "Dismiss all".
#[tauri::command]
#[specta::specta]
pub fn dismiss_all_failed_operations() {
    ops_dismiss_all_failed_operations();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reject_if_routed_over_a_parent_flags_a_path_inside_a_zip() {
        let dir = tempfile::tempdir().expect("tempdir");
        let zip = dir.path().join("bundle.zip");
        // A zip start-of-file signature is enough for the boundary magic check.
        std::fs::write(&zip, b"PK\x03\x04rest").expect("write zip magic");

        // A path INSIDE the archive is refused with a typed read-only error...
        let inner = zip.join("inner.txt");
        let err = reject_if_routed_over_a_parent(std::iter::once(&inner), ReadOnlySide::Source)
            .expect_err("archive-inner path must be refused");
        assert!(
            matches!(err, WriteOperationError::ReadOnlyDevice { .. }),
            "expected ReadOnlyDevice, got {err:?}"
        );

        // ...while a plain local sibling passes (proves the guard, not a blanket reject).
        let plain = dir.path().join("plain.txt");
        assert!(reject_if_routed_over_a_parent(std::iter::once(&plain), ReadOnlySide::Source).is_ok());

        // ...AND the `.zip` FILE ITSELF passes: copying/moving/deleting/trashing a
        // zip file is a normal file op, not a write INSIDE the archive.
        assert!(
            reject_if_routed_over_a_parent(std::iter::once(&zip), ReadOnlySide::Source).is_ok(),
            "the .zip file itself must not be refused"
        );
    }

    /// The git half of the same guard, and the toggle that switches it off: with
    /// the portal on, `.git/branches/…` has no file for `std::fs` to move; with it
    /// off, the same path is whatever is on disk and stays an ordinary local write.
    #[test]
    fn reject_if_routed_over_a_parent_flags_a_snapshot_path_only_while_the_portal_is_on() {
        use crate::file_system::git;
        use cmdr_git::test_fixtures::{Fixture, cleanup, temp_dir};

        let dir = temp_dir("write_ops_guard", "snapshot");
        let mut fixture = Fixture::init(dir.clone());
        fixture.commit_file("README.md", b"hello\n", "initial");
        let snapshot = dir.join(".git/branches/main/README.md");

        git::wiring::set_virtual_portal_enabled(true);
        let err = reject_if_routed_over_a_parent(std::iter::once(&snapshot), ReadOnlySide::Source)
            .expect_err("a snapshot path must be refused");
        assert!(
            matches!(err, WriteOperationError::ReadOnlyDevice { .. }),
            "expected ReadOnlyDevice, got {err:?}"
        );

        // The real files under `.git/` are the parent volume's, always writable.
        assert!(
            reject_if_routed_over_a_parent(std::iter::once(&dir.join(".git/config")), ReadOnlySide::Source).is_ok()
        );

        git::wiring::set_virtual_portal_enabled(false);
        assert!(
            reject_if_routed_over_a_parent(std::iter::once(&snapshot), ReadOnlySide::Source).is_ok(),
            "with the portal off there is no route to protect"
        );
        git::wiring::set_virtual_portal_enabled(true);

        cleanup(&dir);
    }

    #[tokio::test]
    async fn scan_preview_routes_an_archive_source_to_the_archive_volume() {
        use crate::file_system::volume::InMemoryVolume;
        use crate::file_system::volume::manager::test_support::TestVolumeRegistration;

        let dir = tempfile::tempdir().expect("tempdir");
        let zip = dir.path().join("bundle.zip");
        std::fs::write(&zip, b"PK\x03\x04rest").expect("write zip magic");

        // resolve needs the parent drive registered to build the ArchiveVolume.
        // The `.zip` is a real temp file, so the parent is LOCAL (std::fs confirm).
        // The guard puts the previous `"root"` back on drop: under plain
        // `cargo test` this global is shared, and a leftover `InMemoryVolume`
        // here fails every real-FS create/paste test that resolves `None` to it.
        let _root =
            TestVolumeRegistration::install("root", Arc::new(InMemoryVolume::new("Root").with_local_fs_access()));

        // An archive-inner source resolves to the ArchiveVolume (its root() is the
        // `.zip`), so the preview scans INSIDE the zip instead of via `std::fs`
        // (which would find 0 files and stall extract-out).
        let inner = zip.join("inner.txt");
        let source = scan_preview_source_volume("root", Some(&inner))
            .await
            .expect("an archive source is never refused")
            .expect("archive source volume");
        assert_eq!(source.root(), zip);

        // A plain local source stays `None` — the `std::fs` fast path.
        let plain = dir.path().join("plain.txt");
        assert!(matches!(
            scan_preview_source_volume("root", Some(&plain)).await,
            Ok(None)
        ));
    }

    /// A share that holds new folders and files for `delay`, with `/docs/taken`
    /// already there, registered under `volume_id`.
    async fn register_slowly_creating_volume(volume_id: &str, delay: Duration) {
        use crate::file_system::volume::InMemoryVolume;
        use crate::test_support::SlowVolume;
        use std::path::Path;

        let inner = InMemoryVolume::new("Slow NAS");
        inner.create_directory(Path::new("/docs")).await.unwrap();
        inner.create_directory(Path::new("/docs/taken")).await.unwrap();
        get_volume_manager().register_if_absent(volume_id, Arc::new(SlowVolume::creating_slowly(inner, delay)));
    }

    /// An `on_settled` that hands the event to the test, and its receiver.
    fn settle_channel() -> (
        impl FnOnce(MutationSettled) + Send + 'static,
        tokio::sync::oneshot::Receiver<MutationSettled>,
    ) {
        let (tx, rx) = tokio::sync::oneshot::channel();
        (
            move |settled| {
                let _ = tx.send(settled);
            },
            rx,
        )
    }

    /// ERR-AREUV: a new folder on a busy share took 7–12 s, the dialog said it
    /// timed out, and then the folder appeared. Past the deadline the reply is
    /// "still running" and the settle says it landed.
    #[tokio::test(start_paused = true)]
    async fn a_slow_new_folder_replies_still_running_then_settles_landed() {
        use crate::file_system::write_operations::MutationSettledOutcome;

        let volume_id = "smb-slow-mkdir-test";
        register_slowly_creating_volume(volume_id, Duration::from_secs(7)).await;
        let (on_settled, rx) = settle_channel();

        let reply = create_directory_replying(
            Some(volume_id.to_string()),
            "/docs".to_string(),
            "photos".to_string(),
            None,
            on_settled,
        )
        .await;

        let Ok(MutationReply::StillRunning { pending_id }) = reply else {
            panic!("a 7 s create is still running at the deadline, not refused: {reply:?}");
        };
        let settled = rx.await.expect("the create settles");
        assert_eq!(settled.pending_id, pending_id);
        assert!(matches!(settled.outcome, MutationSettledOutcome::Landed), "{settled:?}");
        let volume = get_volume_manager().get(volume_id).expect("registered");
        assert!(volume.exists(std::path::Path::new("/docs/photos")).await);
    }

    /// The other end a slow create can reach: the volume refuses once it answers,
    /// and the settle carries that refusal typed, as an in-time reply would.
    #[tokio::test(start_paused = true)]
    async fn a_slow_new_file_the_volume_refuses_settles_with_the_typed_reason() {
        use crate::file_system::write_operations::MutationSettledOutcome;

        let volume_id = "smb-slow-mkfile-refused-test";
        register_slowly_creating_volume(volume_id, Duration::from_secs(7)).await;
        let (on_settled, rx) = settle_channel();

        let reply = create_file_replying(
            Some(volume_id.to_string()),
            "/docs".to_string(),
            "taken".to_string(),
            None,
            on_settled,
        )
        .await;

        assert!(matches!(reply, Ok(MutationReply::StillRunning { .. })), "{reply:?}");
        let settled = rx.await.expect("the create settles");
        assert!(
            matches!(
                &settled.outcome,
                MutationSettledOutcome::Refused {
                    error: MutationError::AlreadyExists { name }
                } if name == "taken"
            ),
            "{settled:?}"
        );
    }

    /// A healthy create answers inside the deadline, and nothing settles later.
    #[tokio::test(start_paused = true)]
    async fn a_quick_new_folder_replies_done() {
        let volume_id = "smb-quick-mkdir-test";
        register_slowly_creating_volume(volume_id, Duration::from_millis(200)).await;
        let (on_settled, rx) = settle_channel();

        let reply = create_directory_replying(
            Some(volume_id.to_string()),
            "/docs".to_string(),
            "quick".to_string(),
            None,
            on_settled,
        )
        .await;

        assert_eq!(reply.ok(), Some(MutationReply::Done));
        assert!(rx.await.is_err(), "an in-time reply never settles");
    }

    /// A phone unplugged under a search-results pane leaves an id no volume
    /// answers for. The preview refuses it with a typed reason, ❌ never falls
    /// back to walking `adb://…` on the Mac: that walk can only fail, and the
    /// dialog would offer a Retry that never works.
    #[tokio::test]
    async fn scan_preview_refuses_a_source_volume_nothing_answers_for() {
        let source = PathBuf::from("/sdcard/DCIM/Camera/a.jpg");
        let refused = scan_preview_source_volume("an-unplugged-phone", Some(&source)).await;
        assert!(
            matches!(&refused, Err(ScanPreviewRefusal::SourceNotConnected { volume_id }) if volume_id == "an-unplugged-phone"),
            "an unknown source volume is refused, not walked locally; got {:?}",
            refused.as_ref().map(Option::is_some)
        );
    }
}
