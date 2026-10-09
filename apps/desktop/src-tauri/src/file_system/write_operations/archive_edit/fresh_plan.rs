//! Fresh-ZIP source planning and source/destination identity checks.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, UNIX_EPOCH};

use super::super::OperationEventSink;
use super::super::state::WriteOperationState;
use super::super::types::{ConflictResolution, WriteConflictEvent, WriteConflictResolvedEvent, WriteOperationError};
use super::edit_error::EditError;
use super::fresh_zip::{FreshZipEntry, FreshZipSource, RemoteZipFeeder, remote_source_bridge};
use crate::file_system::listing::FileEntry;
use crate::file_system::volume::{EntryKind, Volume, VolumeError};

pub(super) struct RemoteFeed {
    pub(super) path: PathBuf,
    pub(super) entry_name: String,
    pub(super) feeder: RemoteZipFeeder,
}

pub(super) struct FreshPlan {
    pub(super) source_volume: Arc<dyn Volume>,
    pub(super) entries: Vec<FreshZipEntry>,
    pub(super) remote_feeds: Vec<RemoteFeed>,
    pub(super) source_bytes: u64,
    pub(super) skipped: usize,
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
async fn plan_sources_with_context(
    source: &Arc<dyn Volume>,
    source_paths: &[PathBuf],
    conflict: ConflictResolution,
    state: &Arc<WriteOperationState>,
    conflict_context: Option<PlanConflictContext<'_>>,
    on_progress: Option<PlanProgressObserver<'_>>,
) -> Result<FreshPlan, EditError> {
    let mut entries: Vec<FreshZipEntry> = Vec::new();
    let mut remote_feeds: Vec<RemoteFeed> = Vec::new();
    let mut source_bytes = 0u64;
    let mut skipped = 0usize;
    let mut top_names = HashMap::<String, PathBuf>::new();
    let mut duplicate_latch = None;
    // Found-so-far tallies for the scan readout only. A duplicate top name
    // resolved to Overwrite drops entries from the plan; these don't go back.
    let (mut files, mut dirs) = (0usize, 0usize);
    let local_root = source.supports_local_fs_access().then(|| source.local_path()).flatten();

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
                ConflictResolution::Overwrite => {
                    let prefix = format!("{top_name}/");
                    entries.retain(|entry| entry.name != top_name && !entry.name.starts_with(&prefix));
                    remote_feeds.retain(|feed| feed.entry_name != top_name && !feed.entry_name.starts_with(&prefix));
                    source_bytes = entries.iter().map(|entry| entry.size).sum();
                }
            }
        }
        top_names.insert(top_name.clone(), top.clone());
        // A selected link is skipped like a nested one. Asked with `entry_kind`
        // (an `lstat`), ❌ never read off `get_metadata`: SFTP's `stat` follows
        // the link and reports its target.
        let top_kind = source
            .entry_kind(top)
            .await
            .map_err(|error| volume_read_error(top, error))?;
        if top_kind == EntryKind::Symlink {
            skipped += 1;
            continue;
        }
        let top_meta = source
            .get_metadata(top)
            .await
            .map_err(|error| volume_read_error(top, error))?;
        let mut stack = vec![(top.clone(), top_name, top_meta)];
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
                .reports_posix_mode()
                .then_some(meta.permissions)
                .filter(|mode| *mode != 0);
            if meta.is_directory {
                entries.push(FreshZipEntry {
                    name: format!("{}/", inner.trim_end_matches('/')),
                    source: FreshZipSource::Bytes(Vec::new()),
                    size: 0,
                    is_directory: true,
                    modified,
                    unix_mode,
                });
                let children = source
                    .list_directory(&path, None)
                    .await
                    .map_err(|error| volume_read_error(&path, error))?;
                dirs += 1;
                report(on_progress, files, dirs, source_bytes, &path);
                for child in children.into_iter().rev() {
                    let child_inner = format!("{inner}/{}", child.name);
                    stack.push((path.join(&child.name), child_inner, child));
                }
            } else {
                let size = meta.size.ok_or_else(|| {
                    EditError::Op(WriteOperationError::ReadError {
                        path: path.display().to_string(),
                        message: "the source did not report its size".to_string(),
                    })
                })?;
                source_bytes = source_bytes.saturating_add(size);
                let entry_source = if let Some(root) = &local_root {
                    FreshZipSource::Local(if path.is_absolute() {
                        path.clone()
                    } else {
                        root.join(&path)
                    })
                } else {
                    let (feeder, remote) = remote_source_bridge();
                    remote_feeds.push(RemoteFeed {
                        path: path.clone(),
                        entry_name: inner.clone(),
                        feeder,
                    });
                    FreshZipSource::Remote(remote)
                };
                entries.push(FreshZipEntry {
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
        remote_feeds,
        source_bytes,
        skipped,
    })
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
    if let Some(observer) = on_progress {
        observer(PlanProgress {
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
        return tokio::task::spawn_blocking(move || {
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
            message: error.to_string(),
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
        message: error.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use std::future::Future;

    use super::*;
    use crate::file_system::volume::{InMemoryVolume, LocalPosixVolume};
    use crate::file_system::write_operations::transfer::volume::forward_volume_methods;

    #[tokio::test]
    async fn distinct_handles_for_one_remote_resource_still_reject_containment() {
        let lane = "same-remote-resource";
        let source: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("source").with_lane_key(lane));
        let destination: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("destination").with_lane_key(lane));
        source
            .create_directory(Path::new("/folder"))
            .await
            .expect("create source folder");

        assert!(matches!(
            validate_aliases(
                &source,
                &[PathBuf::from("/folder")],
                &destination,
                Path::new("/folder/archive.zip"),
            )
            .await,
            Err(WriteOperationError::DestinationInsideSource { .. })
        ));
    }

    /// A remote-shaped source that counts its single-entry stats, each one a
    /// network round trip on SMB and SFTP (and a whole parent listing on MTP).
    struct CountingStats {
        inner: InMemoryVolume,
        stats: std::sync::atomic::AtomicUsize,
    }

    impl Volume for CountingStats {
        forward_volume_methods!(inner => name, root, list_directory, exists, is_directory, get_space_info);

        fn as_any(&self) -> &dyn std::any::Any {
            self
        }

        fn get_metadata<'a>(
            &'a self,
            path: &'a Path,
        ) -> std::pin::Pin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
            self.stats.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.inner.get_metadata(path)
        }
    }

    #[tokio::test]
    async fn planning_a_remote_tree_stats_only_the_selected_items() {
        let inner = InMemoryVolume::new("remote");
        inner.create_directory(Path::new("/tree")).await.expect("mkdir");
        inner.create_directory(Path::new("/tree/sub")).await.expect("mkdir sub");
        for index in 0..20 {
            inner
                .create_file(&PathBuf::from(format!("/tree/sub/f{index}.txt")), b"x")
                .await
                .expect("seed file");
        }
        let source = Arc::new(CountingStats {
            inner,
            stats: Default::default(),
        });
        let as_volume: Arc<dyn Volume> = Arc::clone(&source) as Arc<dyn Volume>;
        let state = Arc::new(WriteOperationState::new(Duration::ZERO));

        let plan = plan_sources_with_context(
            &as_volume,
            &[PathBuf::from("/tree")],
            ConflictResolution::Stop,
            &state,
            None,
            None,
        )
        .await
        .unwrap_or_else(|_| panic!("plan"));

        assert_eq!(plan.entries.len(), 22, "two directories and 20 files");
        assert_eq!(plan.source_bytes, 20);
        // The selected item costs a kind check (`entry_kind`, here the trait
        // default over `get_metadata`) and a stat; its 21 descendants cost none.
        assert_eq!(
            source.stats.load(std::sync::atomic::Ordering::SeqCst),
            2,
            "children come from their parent's listing, never a stat each"
        );
    }

    /// A remote whose `get_metadata` FOLLOWS links, the way SFTP's `stat` does:
    /// `/link` answers as the folder it points at, and only `entry_kind` (an
    /// `lstat`) says it's a link.
    struct LinkFollowingStat {
        inner: InMemoryVolume,
    }

    impl LinkFollowingStat {
        fn target(path: &Path) -> PathBuf {
            match path.strip_prefix("/link") {
                Ok(rest) => Path::new("/target").join(rest),
                Err(_) => path.to_path_buf(),
            }
        }
    }

    impl Volume for LinkFollowingStat {
        forward_volume_methods!(inner => name, root, exists, is_directory, get_space_info);

        fn as_any(&self) -> &dyn std::any::Any {
            self
        }

        fn list_directory<'a>(
            &'a self,
            path: &'a Path,
            on_progress: Option<&'a (dyn Fn(crate::file_system::volume::ListingProgress) + Sync)>,
        ) -> std::pin::Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
            Box::pin(async move { self.inner.list_directory(&Self::target(path), on_progress).await })
        }

        fn get_metadata<'a>(
            &'a self,
            path: &'a Path,
        ) -> std::pin::Pin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
            Box::pin(async move { self.inner.get_metadata(&Self::target(path)).await })
        }

        fn entry_kind<'a>(
            &'a self,
            path: &'a Path,
        ) -> std::pin::Pin<Box<dyn Future<Output = Result<EntryKind, VolumeError>> + Send + 'a>> {
            Box::pin(async move {
                if path == Path::new("/link") {
                    Ok(EntryKind::Symlink)
                } else {
                    self.inner.entry_kind(path).await
                }
            })
        }
    }

    #[tokio::test]
    async fn a_selected_symlink_is_skipped_even_where_stat_follows_links() {
        // A link inside a selected folder is skipped off its listing entry; a
        // selected link has to be skipped too, or SFTP (whose `stat` follows)
        // packs the target that the same link would never pack one level down.
        let inner = InMemoryVolume::new("remote");
        inner.create_directory(Path::new("/target")).await.expect("mkdir");
        inner
            .create_file(Path::new("/target/inside.txt"), b"x")
            .await
            .expect("seed");
        let source: Arc<dyn Volume> = Arc::new(LinkFollowingStat { inner });
        let state = Arc::new(WriteOperationState::new(Duration::ZERO));

        let plan = plan_sources_with_context(
            &source,
            &[PathBuf::from("/link")],
            ConflictResolution::Stop,
            &state,
            None,
            None,
        )
        .await
        .unwrap_or_else(|_| panic!("plan"));

        assert!(plan.entries.is_empty(), "the link's target is never packed");
        assert_eq!(plan.skipped, 1);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn planning_skips_local_symlinks_and_special_files_without_a_stat_each() {
        let temp = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(temp.path().join("tree")).expect("mkdir");
        std::fs::write(temp.path().join("tree/real.txt"), b"real").expect("write");
        std::os::unix::fs::symlink("real.txt", temp.path().join("tree/link")).expect("symlink");
        let _socket =
            std::os::unix::net::UnixListener::bind(temp.path().join("tree/sock")).expect("bind a socket file");
        let source: Arc<dyn Volume> = Arc::new(LocalPosixVolume::local_folder("source", temp.path().to_path_buf()));
        let state = Arc::new(WriteOperationState::new(Duration::ZERO));

        let plan = plan_sources_with_context(
            &source,
            &[PathBuf::from("tree")],
            ConflictResolution::Stop,
            &state,
            None,
            None,
        )
        .await
        .unwrap_or_else(|_| panic!("plan"));

        assert_eq!(
            plan.entries.iter().map(|entry| entry.name.as_str()).collect::<Vec<_>>(),
            vec!["tree/", "tree/real.txt"]
        );
        assert_eq!(plan.skipped, 2, "the symlink and the socket are skipped, never lost");
    }

    #[tokio::test]
    async fn duplicate_top_names_honor_rename_and_skip_policies() {
        let temp = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(temp.path().join("a")).expect("create a");
        std::fs::create_dir_all(temp.path().join("b")).expect("create b");
        std::fs::write(temp.path().join("a/report.txt"), b"first").expect("write first");
        std::fs::write(temp.path().join("b/report.txt"), b"second").expect("write second");
        let source: Arc<dyn Volume> = Arc::new(LocalPosixVolume::local_folder("source", temp.path().to_path_buf()));
        let paths = [PathBuf::from("a/report.txt"), PathBuf::from("b/report.txt")];
        let state = Arc::new(WriteOperationState::new(Duration::ZERO));

        let renamed = plan_sources_with_context(&source, &paths, ConflictResolution::Rename, &state, None, None)
            .await
            .unwrap_or_else(|_| panic!("rename plan"));
        assert_eq!(
            renamed
                .entries
                .iter()
                .map(|entry| entry.name.as_str())
                .collect::<Vec<_>>(),
            vec!["report.txt", "report (1).txt"]
        );

        let skipped = plan_sources_with_context(&source, &paths, ConflictResolution::Skip, &state, None, None)
            .await
            .unwrap_or_else(|_| panic!("skip plan"));
        assert_eq!(skipped.entries.len(), 1);
        assert_eq!(skipped.skipped, 1);
    }
}
