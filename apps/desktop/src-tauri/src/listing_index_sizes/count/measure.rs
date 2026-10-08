//! Measuring one folder: its file count, folder count, bytes, and on-disk bytes,
//! with running totals a ticker can read while it walks.
//!
//! On a volume backed by a local path (the Mac's disks, and mounted shares under
//! `/Volumes`) the walk is ours: `walkdir` on a blocking thread, a stop check per
//! entry, the totals bumped per entry, and an unreadable subfolder SKIPPED and
//! counted rather than ending the walk, so `~/Library` gets a lower bound instead
//! of nothing. ❌ Not the copy scan there: it reports only when the folder is done,
//! and a copy rightly refuses to start on a folder it can't read, while a size
//! wants everything it can see. Elsewhere (SFTP, WebDAV, S3, archives) the copy
//! scan the transfer dialog uses, through `ScanBoundary`, which bumps the totals
//! as it goes.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use cmdr_fs::volume::{ListingProgress, ScanBoundary, ScanStop, VolumeError};
use walkdir::WalkDir;

use super::jobs::Job;
use crate::file_system::volume::Volume;

/// Running totals: the walk adds, the ticker reads.
#[derive(Default)]
pub(super) struct Live {
    files: AtomicUsize,
    dirs: AtomicUsize,
    bytes: AtomicU64,
}

impl Live {
    pub(super) fn snapshot(&self) -> ListingProgress {
        ListingProgress {
            files: self.files.load(Ordering::Relaxed),
            dirs: self.dirs.load(Ordering::Relaxed),
            bytes: self.bytes.load(Ordering::Relaxed),
        }
    }

    fn set(&self, progress: ListingProgress) {
        self.files.store(progress.files, Ordering::Relaxed);
        self.dirs.store(progress.dirs, Ordering::Relaxed);
        self.bytes.store(progress.bytes, Ordering::Relaxed);
    }
}

/// What walking a folder found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Measured {
    pub progress: ListingProgress,
    /// On-disk bytes, each inode once (`du`).
    pub physical: u64,
    /// Subfolders and files it couldn't read: when nonzero, the size is a lower bound.
    pub skipped: usize,
}

/// Why a walk gave no size.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum MeasureError {
    /// The job was stopped.
    Stopped,
    /// The folder itself couldn't be read (permissions, gone, the share dropped).
    Unreadable(String),
}

/// Owns backend I/O independently of the UI wait. Dropping this handle detaches
/// the worker, never cancels an in-flight protocol operation or local syscall.
/// The worker holds no listing or sink and only writes its private live totals.
pub(super) fn start(
    volume: Arc<dyn Volume>,
    folder: String,
    job: Arc<Job>,
    live: Arc<Live>,
) -> tokio::task::JoinHandle<Result<Measured, MeasureError>> {
    match volume.local_path() {
        Some(root) => {
            let path = cmdr_fs::volume::root_anchored(&root, Path::new(&folder));
            // No async task retained waiting for an uninterruptible syscall.
            tokio::task::spawn_blocking(move || walk_local(&path, &job, &live))
        }
        None => tokio::spawn(async move { measure_through_volume(&volume, &folder, &job, &live).await }),
    }
}

async fn measure_through_volume(
    volume: &Arc<dyn Volume>,
    folder: &str,
    job: &Arc<Job>,
    live: &Arc<Live>,
) -> Result<Measured, MeasureError> {
    if job.is_cancelled() {
        return Err(MeasureError::Stopped);
    }
    let on_progress = |progress: ListingProgress| live.set(progress);
    let boundary = ScanBoundary::new(Some(&on_progress)).stopping_at(ScanStop::new(Arc::clone(job) as _));
    match volume
        .scan_for_copy_batch_with_boundary(&[PathBuf::from(folder)], &boundary)
        .await
    {
        Ok(_) if job.is_cancelled() => Err(MeasureError::Stopped),
        Ok(scan) => Ok(Measured {
            progress: ListingProgress {
                files: scan.aggregate.file_count,
                dirs: scan.aggregate.dir_count,
                bytes: scan.aggregate.total_bytes,
            },
            physical: scan.aggregate.dedup_bytes,
            skipped: 0,
        }),
        Err(VolumeError::Cancelled(_)) => Err(MeasureError::Stopped),
        Err(err) => Err(MeasureError::Unreadable(err.to_string())),
    }
}

/// The local walk. Runs on a blocking thread.
pub(super) fn walk_local(root: &Path, job: &Job, live: &Live) -> Result<Measured, MeasureError> {
    use std::os::unix::fs::MetadataExt;

    if job.is_cancelled() {
        return Err(MeasureError::Stopped);
    }
    let mut physical = 0u64;
    let mut skipped = 0usize;
    // Hard links count once on disk; `nlink == 1` (nearly every file) skips the set.
    let mut seen_inodes: HashSet<u64> = HashSet::new();
    for entry in WalkDir::new(root) {
        if job.is_cancelled() {
            return Err(MeasureError::Stopped);
        }
        let entry = match entry {
            Ok(entry) => entry,
            Err(err) if err.depth() == 0 => return Err(MeasureError::Unreadable(err.to_string())),
            Err(_) => {
                skipped += 1;
                continue;
            }
        };
        if entry.depth() == 0 {
            continue;
        }
        let file_type = entry.file_type();
        if file_type.is_dir() {
            live.dirs.fetch_add(1, Ordering::Relaxed);
        } else if file_type.is_file() {
            match entry.metadata() {
                Ok(meta) => {
                    live.files.fetch_add(1, Ordering::Relaxed);
                    live.bytes.fetch_add(meta.len(), Ordering::Relaxed);
                    if meta.nlink() <= 1 || seen_inodes.insert(meta.ino()) {
                        physical += meta.len();
                    }
                }
                Err(_) => skipped += 1,
            }
        }
    }
    Ok(Measured {
        progress: live.snapshot(),
        physical,
        skipped,
    })
}
