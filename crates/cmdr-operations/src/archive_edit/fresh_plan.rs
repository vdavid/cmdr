//! Fresh-ZIP source planning and source/destination identity checks.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, UNIX_EPOCH};

use super::super::OperationEventSink;
use super::super::state::WriteOperationState;
use super::super::transfer::volume_outage::OutageAnnounce;
use super::super::transfer_tree::TreeOrder;
use super::super::types::{
    ConflictResolution, WriteConflictEvent, WriteConflictResolvedEvent, WriteOperationError, WriteOperationType,
};
use super::edit_error::EditError;
use super::fresh_zip::{FreshZipEntry, FreshZipSource, RemoteZipFeeder, remote_source_bridge};
use super::outage_wait::read_through_outages;
use cmdr_fs::entry::FileEntry;
use cmdr_fs::volume::{EntryKind, Volume, VolumeError};
use tokio::runtime::Handle;

pub(super) struct RemoteFeed {
    pub(super) path: PathBuf,
    pub(super) entry_name: String,
    pub(super) feeder: RemoteZipFeeder,
}

/// The frozen plan of one compress: every entry, in the order the zip is
/// written in. It can be run more than once: the compress makes the archive
/// again after a dropped share (`fresh_compress.rs`), and each run gets its
/// own remote feeds ([`bridged`](Self::bridged)).
pub(super) struct FreshPlan {
    pub(super) source_volume: Arc<dyn Volume>,
    pub(super) entries: Vec<PlannedEntry>,
    pub(super) source_bytes: u64,
    pub(super) skipped: usize,
}

/// One entry of the plan: the producer's `FreshZipEntry`, with a source that
/// can be read again.
#[derive(Debug, Clone)]
pub(super) struct PlannedEntry {
    pub(super) name: String,
    pub(super) source: PlannedSource,
    pub(super) size: u64,
    pub(super) is_directory: bool,
    pub(super) modified: Option<std::time::SystemTime>,
    pub(super) unix_mode: Option<u32>,
}

/// Where a planned entry's bytes come from. Unlike the producer's
/// `FreshZipSource`, whose remote half is a live channel to one feed, a remote
/// source here is only its path on the plan's source volume.
#[derive(Debug, Clone)]
pub(super) enum PlannedSource {
    /// A local file, or for a directory entry the local folder whose xattrs
    /// its sidecar carries.
    Local(PathBuf),
    Bytes(Vec<u8>),
    /// A file on the plan's (remote) source volume.
    Remote(PathBuf),
}

impl FreshPlan {
    /// One producer run of the plan: its entries for `produce`, each remote one
    /// bridged to a fresh feed that reads it from the source volume.
    pub(super) fn bridged(&self) -> (Vec<FreshZipEntry>, Vec<RemoteFeed>) {
        let mut feeds = Vec::new();
        let entries = self
            .entries
            .iter()
            .map(|entry| FreshZipEntry {
                name: entry.name.clone(),
                source: match &entry.source {
                    PlannedSource::Local(path) => FreshZipSource::Local(path.clone()),
                    PlannedSource::Bytes(bytes) => FreshZipSource::Bytes(bytes.clone()),
                    PlannedSource::Remote(path) => {
                        let (feeder, remote) = remote_source_bridge();
                        feeds.push(RemoteFeed {
                            path: path.clone(),
                            entry_name: entry.name.clone(),
                            feeder,
                        });
                        FreshZipSource::Remote(remote)
                    }
                },
                size: entry.size,
                is_directory: entry.is_directory,
                modified: entry.modified,
                unix_mode: entry.unix_mode,
            })
            .collect();
        (entries, feeds)
    }
}

/// How far the planning walk has got: what it has found so far and the
/// directory it's listing. The walk has no denominator, so the op reports it
/// as the indeterminate `Scanning` phase.
#[derive(Debug, Clone, Copy)]
pub(super) struct PlanProgress<'a> {
    pub(super) files: usize,
    pub(super) dirs: usize,
    pub(super) bytes: u64,
    pub(super) current_dir: &'a Path,
}

pub(super) type PlanProgressObserver<'a> = &'a (dyn Fn(PlanProgress<'_>) + Sync);

#[derive(Clone, Copy)]
struct PlanConflictContext<'a> {
    events: &'a dyn OperationEventSink,
    operation_id: &'a str,
    archive_path: &'a Path,
}

impl<'a> PlanConflictContext<'a> {
    /// Where the walk announces a wait for a dropped share: its own progress,
    /// since no transfer probe speaks for a compress yet.
    fn announce(self) -> OutageAnnounce<'a> {
        OutageAnnounce {
            sink: self.events,
            operation_id: self.operation_id,
            operation_type: WriteOperationType::Compress,
        }
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "the planning seam carries the selection, its conflict policy and prompt context, and the walk's progress observer"
)]
pub(super) async fn plan_sources(
    source: &Arc<dyn Volume>,
    source_paths: &[PathBuf],
    conflict: ConflictResolution,
    state: &Arc<WriteOperationState>,
    events: &dyn OperationEventSink,
    operation_id: &str,
    archive_path: &Path,
    on_progress: PlanProgressObserver<'_>,
) -> Result<FreshPlan, EditError> {
    plan_sources_with_context(
        source,
        source_paths,
        conflict,
        state,
        Some(PlanConflictContext {
            events,
            operation_id,
            archive_path,
        }),
        Some(on_progress),
    )
    .await
}

/// Walks the selection into a frozen plan.
///
/// ❗ Only the SELECTED items are stat'ed; every child comes from its parent's
/// listing, which already carries its kind, size, mtime, and mode. A stat per
/// child was one network round trip per file on SMB and SFTP, and a whole
/// parent listing per file on MTP. Symlinks and special files are skipped from
/// the same listing facts (`is_skipped_kind`), so there's no local syscall per
/// entry either. Same shape as `cmdr_fs::volume::scan_walk`.
///
/// The plan is the order the zip is written in, so it's in TREE ORDER
/// (`transfer_tree/order.rs`): the selected items are settled first (each one's
/// name in the zip, and whether it's packed at all), then walked folders first,
/// by name, every level sorted the same way. The compress's progress rows then
/// start top to bottom.
async fn plan_sources_with_context(
    source: &Arc<dyn Volume>,
    source_paths: &[PathBuf],
    conflict: ConflictResolution,
    state: &Arc<WriteOperationState>,
    conflict_context: Option<PlanConflictContext<'_>>,
    on_progress: Option<PlanProgressObserver<'_>>,
) -> Result<FreshPlan, EditError> {
    let mut entries: Vec<PlannedEntry> = Vec::new();
    let mut source_bytes = 0u64;
    let announce = conflict_context.map(PlanConflictContext::announce);
    let (tops, mut skipped) = settle_tops(source, source_paths, conflict, state, conflict_context).await?;
    // Found-so-far tallies for the scan readout only.
    let (mut files, mut dirs) = (0usize, 0usize);
    let local_root = source.supports_local_fs_access().then(|| source.local_path()).flatten();
    let order = TreeOrder::active(state.scope().host());

    for (top, top_name, top_meta) in tops {
        let mut stack = vec![(top, top_name, top_meta)];
        while let Some((path, inner, meta)) = stack.pop() {
            if state.stop_or_park_async().await {
                return Err(EditError::Cancelled);
            }
            if is_skipped_kind(&meta) {
                skipped += 1;
                continue;
            }
            let modified = meta
                .modified_at
                .map(|seconds| UNIX_EPOCH + Duration::from_secs(seconds));
            let unix_mode = source
                .metadata_caps()
                .reports
                .contains(cmdr_fs::metadata::MetadataClasses::MODE)
                .then_some(meta.permissions)
                .filter(|mode| *mode != 0);
            if meta.is_directory {
                entries.push(PlannedEntry {
                    name: format!("{}/", inner.trim_end_matches('/')),
                    // A local folder is its entry's source only for its xattrs,
                    // which the producer writes as a sidecar; no bytes.
                    source: local_root.as_ref().map_or_else(
                        || PlannedSource::Bytes(Vec::new()),
                        |root| {
                            PlannedSource::Local(if path.is_absolute() {
                                path.clone()
                            } else {
                                root.join(&path)
                            })
                        },
                    ),
                    size: 0,
                    is_directory: true,
                    modified,
                    unix_mode,
                });
                // A listing the dropped share cut off runs again once it's
                // back; its children join the plan only after it answers.
                let mut children =
                    read_through_outages(state, source, &path, announce.as_ref(), volume_read_error, || {
                        source.list_directory(&path, None)
                    })
                    .await?;
                dirs += 1;
                report(on_progress, files, dirs, source_bytes, &path);
                order.sort(&mut children, |child| {
                    (child.is_directory && !child.is_symlink, child.name.as_str())
                });
                // Reversed onto the stack, so the first in tree order pops first.
                for child in children.into_iter().rev() {
                    let child_inner = format!("{inner}/{}", child.name);
                    stack.push((path.join(&child.name), child_inner, child));
                }
            } else {
                let size = meta.size.ok_or_else(|| {
                    EditError::Op(WriteOperationError::ReadError {
                        path: path.display().to_string(),
                        message: "the source did not report its size".into(),
                    })
                })?;
                source_bytes = source_bytes.saturating_add(size);
                let entry_source = if let Some(root) = &local_root {
                    PlannedSource::Local(if path.is_absolute() {
                        path.clone()
                    } else {
                        root.join(&path)
                    })
                } else {
                    PlannedSource::Remote(path.clone())
                };
                entries.push(PlannedEntry {
                    name: inner,
                    source: entry_source,
                    size,
                    is_directory: false,
                    modified,
                    unix_mode,
                });
                files += 1;
                report(on_progress, files, dirs, source_bytes, path.parent().unwrap_or(&path));
            }
        }
    }
    Ok(FreshPlan {
        source_volume: Arc::clone(source),
        entries,
        source_bytes,
        skipped,
    })
}

/// The selected items the plan packs, in tree order, each with its name in the
/// zip and its own stat; plus how many were left out. A duplicate top name is
/// settled here, in selection order, by the conflict policy (Stop asks): Skip
/// leaves the later one out, Rename gives it a free name, and Overwrite drops
/// the earlier one. A selected link is left out like a nested one.
async fn settle_tops(
    source: &Arc<dyn Volume>,
    source_paths: &[PathBuf],
    conflict: ConflictResolution,
    state: &Arc<WriteOperationState>,
    conflict_context: Option<PlanConflictContext<'_>>,
) -> Result<(Vec<(PathBuf, String, FileEntry)>, usize), EditError> {
    let mut tops: Vec<(PathBuf, String, FileEntry)> = Vec::new();
    let mut skipped = 0usize;
    let announce = conflict_context.map(PlanConflictContext::announce);
    let mut top_names = HashMap::<String, PathBuf>::new();
    let mut duplicate_latch = None;
    for top in source_paths {
        if state.stop_or_park_async().await {
            return Err(EditError::Cancelled);
        }
        let Some(mut top_name) = top.file_name().map(|name| name.to_string_lossy().into_owned()) else {
            skipped += 1;
            continue;
        };
        if let Some(first) = top_names.get(&top_name).cloned() {
            let resolution = if let Some(latched) = duplicate_latch {
                latched
            } else if conflict == ConflictResolution::Stop {
                let Some(context) = conflict_context else {
                    return Err(EditError::Op(WriteOperationError::DuplicateSourceNames {
                        name: top_name,
                        first: first.display().to_string(),
                        second: top.display().to_string(),
                    }));
                };
                let response = prompt_duplicate_source(source, top, &first, &top_name, state, context).await?;
                if response.apply_to_all {
                    duplicate_latch = Some(response.resolution);
                }
                response.resolution
            } else {
                conflict
            };
            let resolution = reduce_duplicate_conditional(source, top, &first, resolution).await;
            match resolution {
                ConflictResolution::Skip
                | ConflictResolution::OverwriteSmaller
                | ConflictResolution::OverwriteOlder => {
                    skipped += 1;
                    continue;
                }
                ConflictResolution::Stop => {
                    return Err(EditError::Op(WriteOperationError::DuplicateSourceNames {
                        name: top_name,
                        first: first.display().to_string(),
                        second: top.display().to_string(),
                    }));
                }
                ConflictResolution::Rename => top_name = unique_top_name(&top_name, &top_names),
                // Nothing is walked yet, so the earlier item just leaves.
                ConflictResolution::Overwrite => tops.retain(|(_, name, _)| *name != top_name),
            }
        }
        // The root `__MACOSX` is where a zip's sidecars live, hidden from every
        // listing, so an item named that would read back as nothing (and fail
        // the stage's validation). Skipped and counted, like a symlink.
        if cmdr_archive::mutator::is_in_sidecar_folder(&top_name) {
            skipped += 1;
            continue;
        }
        top_names.insert(top_name.clone(), top.clone());
        // A selected link is skipped like a nested one. Asked with `entry_kind`
        // (an `lstat`), ❌ never read off `get_metadata`: SFTP's `stat` follows
        // the link and reports its target.
        let top_kind = read_through_outages(state, source, top, announce.as_ref(), volume_read_error, || {
            source.entry_kind(top)
        })
        .await?;
        if top_kind == EntryKind::Symlink {
            skipped += 1;
            continue;
        }
        let top_meta = read_through_outages(state, source, top, announce.as_ref(), volume_read_error, || {
            source.get_metadata(top)
        })
        .await?;
        tops.push((top.clone(), top_name, top_meta));
    }
    TreeOrder::active(state.scope().host()).sort(&mut tops, |(_, name, meta)| (meta.is_directory, name.as_str()));
    Ok((tops, skipped))
}

/// A symlink (never followed: its target may be an ancestor, or elsewhere in the
/// selection) or a special file (fifo, socket, device) that a ZIP can't hold.
/// Local and ADB listings carry the full `st_mode` in `permissions`, and SFTP
/// its file-type bits alone (`cmdr-sftp`'s `mapping.rs`), so the type answers
/// without opening anything. ❗ That matters: a read of a FIFO blocks, and on
/// SFTP it blocks the one `sftp-server` every operation shares. SMB and MTP have
/// no special files and report no type bits, which never reads as special.
fn is_skipped_kind(entry: &FileEntry) -> bool {
    const S_IFMT: u32 = 0o170_000;
    const REPRESENTABLE: [u32; 3] = [0o100_000, 0o040_000, 0o120_000]; // file, directory, symlink
    let kind = entry.permissions & S_IFMT;
    entry.is_symlink || (kind != 0 && !REPRESENTABLE.contains(&kind))
}

fn report(on_progress: Option<PlanProgressObserver<'_>>, files: usize, dirs: usize, bytes: u64, current_dir: &Path) {
    if let Some(report_to) = on_progress {
        report_to(PlanProgress {
            files,
            dirs,
            bytes,
            current_dir,
        });
    }
}

async fn prompt_duplicate_source(
    source: &Arc<dyn Volume>,
    incoming: &Path,
    existing: &Path,
    name: &str,
    state: &Arc<WriteOperationState>,
    context: PlanConflictContext<'_>,
) -> Result<super::super::state::ConflictResolutionResponse, EditError> {
    let incoming_meta = source.get_metadata(incoming).await.ok();
    let existing_meta = source.get_metadata(existing).await.ok();
    let source_size = incoming_meta.as_ref().and_then(|meta| meta.size);
    let destination_size = existing_meta.as_ref().and_then(|meta| meta.size);
    let source_modified = incoming_meta
        .as_ref()
        .and_then(|meta| meta.modified_at)
        .and_then(|value| i64::try_from(value).ok());
    let destination_modified = existing_meta
        .as_ref()
        .and_then(|meta| meta.modified_at)
        .and_then(|value| i64::try_from(value).ok());
    let (tx, rx) = tokio::sync::oneshot::channel();
    let event = state.conflict_slot.arm(tx, |conflict_id| WriteConflictEvent {
        operation_id: context.operation_id.to_string(),
        conflict_id,
        source_path: incoming.display().to_string(),
        destination_path: context.archive_path.join(name).display().to_string(),
        source_size,
        destination_size,
        source_modified,
        destination_modified,
        destination_is_newer: matches!((source_modified, destination_modified), (Some(source), Some(dest)) if dest > source),
        size_difference: match (destination_size, source_size) {
            (Some(dest), Some(source)) => i64::try_from(dest)
                .ok()
                .zip(i64::try_from(source).ok())
                .map(|(dest, source)| dest - source),
            _ => None,
        },
        source_is_directory: incoming_meta.as_ref().is_some_and(|meta| meta.is_directory),
        destination_is_directory: existing_meta.as_ref().is_some_and(|meta| meta.is_directory),
        destination_is_look_alike: false,
    });
    let conflict_id = event.conflict_id;
    state.announce_human_wait(context.events);
    context.events.emit_conflict(event);
    let response = rx.await.map_err(|_| EditError::Cancelled)?;
    state.announce_human_wait(context.events);
    context.events.emit_conflict_resolved(WriteConflictResolvedEvent {
        operation_id: context.operation_id.to_string(),
        conflict_id,
    });
    Ok(response)
}

async fn reduce_duplicate_conditional(
    source: &Arc<dyn Volume>,
    incoming: &Path,
    existing: &Path,
    resolution: ConflictResolution,
) -> ConflictResolution {
    let (Ok(incoming), Ok(existing)) = (source.get_metadata(incoming).await, source.get_metadata(existing).await)
    else {
        return match resolution {
            ConflictResolution::OverwriteSmaller | ConflictResolution::OverwriteOlder => ConflictResolution::Skip,
            other => other,
        };
    };
    match resolution {
        ConflictResolution::OverwriteSmaller => match (incoming.size, existing.size) {
            (Some(incoming), Some(existing)) if existing < incoming => ConflictResolution::Overwrite,
            _ => ConflictResolution::Skip,
        },
        ConflictResolution::OverwriteOlder => match (incoming.modified_at, existing.modified_at) {
            (Some(incoming), Some(existing)) if existing < incoming => ConflictResolution::Overwrite,
            _ => ConflictResolution::Skip,
        },
        other => other,
    }
}

fn unique_top_name(name: &str, planned: &HashMap<String, PathBuf>) -> String {
    let path = Path::new(name);
    let stem = path.file_stem().and_then(|part| part.to_str()).unwrap_or(name);
    let extension = path
        .extension()
        .and_then(|part| part.to_str())
        .map(|part| format!(".{part}"))
        .unwrap_or_default();
    for suffix in 1..=9999 {
        let candidate = format!("{stem} ({suffix}){extension}");
        if !planned.contains_key(&candidate) {
            return candidate;
        }
    }
    format!("{stem} ({}){extension}", uuid::Uuid::new_v4())
}

pub(super) async fn validate_aliases(
    runtime: &Handle,
    source: &Arc<dyn Volume>,
    source_paths: &[PathBuf],
    dest: &Arc<dyn Volume>,
    archive_path: &Path,
) -> Result<(), WriteOperationError> {
    let source_local = source.supports_local_fs_access().then(|| source.local_path()).flatten();
    let dest_local = dest.supports_local_fs_access().then(|| dest.local_path()).flatten();
    if let (Some(root), Some(dest_root)) = (source_local, dest_local) {
        let absolute_sources = source_paths
            .iter()
            .map(|path| {
                if path.is_absolute() {
                    path.clone()
                } else {
                    root.join(path)
                }
            })
            .collect::<Vec<_>>();
        let absolute_dest = if archive_path.is_absolute() {
            archive_path.to_path_buf()
        } else {
            dest_root.join(archive_path)
        };
        return runtime
            .spawn_blocking(move || {
                super::super::validation::validate_destination_not_inside_source(&absolute_sources, &absolute_dest)?;
                for source_path in &absolute_sources {
                    if super::super::validation::is_same_file(source_path, &absolute_dest) {
                        return Err(WriteOperationError::DestinationInsideSource {
                            source: source_path.display().to_string(),
                            destination: absolute_dest.display().to_string(),
                        });
                    }
                }
                Ok(())
            })
            .await
            .map_err(|error| WriteOperationError::IoError {
                path: archive_path.display().to_string(),
                message: error.to_string().into(),
            })?;
    } else if Arc::ptr_eq(source, dest) || source.lane_key() == dest.lane_key() {
        let archive_path = cmdr_fs::volume::remote_paths::normalize_remote_path(archive_path);
        for source_path in source_paths {
            let source_path = cmdr_fs::volume::remote_paths::normalize_remote_path(source_path);
            let meta = source.get_metadata(&source_path).await.ok();
            if source_path == archive_path
                || meta.is_some_and(|entry| entry.is_directory && archive_path.starts_with(&source_path))
            {
                return Err(WriteOperationError::DestinationInsideSource {
                    source: source_path.display().to_string(),
                    destination: archive_path.display().to_string(),
                });
            }
        }
    }
    Ok(())
}

fn volume_read_error(path: &Path, error: VolumeError) -> EditError {
    EditError::Op(WriteOperationError::ReadError {
        path: path.display().to_string(),
        message: cmdr_fs::error_detail::ErrorDetail::of(error),
    })
}

#[cfg(test)]
#[path = "fresh_plan_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "fresh_plan_outage_tests.rs"]
mod outage_tests;
