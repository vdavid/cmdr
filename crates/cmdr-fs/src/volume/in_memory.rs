//! In-memory file system volume for testing.
//!
//! Provides a fully in-memory file system that supports all Volume operations,
//! including create, delete, and list. Useful for unit and integration tests
//! without touching the real file system.

use super::{SmbConnectionState, SpaceInfo, VolumeError, VolumeReadStream};
// The `impl Volume` moved to `in_memory/volume_impl.rs`, but the builder docs below
// still link to the trait methods they steer.
#[cfg(doc)]
use super::Volume;
use crate::entry::FileEntry;
use crate::ignore_poison::IgnorePoison;
use crate::ignore_poison::RwLockIgnorePoison;
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::RwLock;

/// Entry in the in-memory file system.
struct InMemoryEntry {
    metadata: FileEntry,
    content: Option<Vec<u8>>,
}

/// An in-memory volume for testing without touching the real file system.
///
/// This implementation stores all entries in a HashMap, allowing full control
/// over the file system state for testing. It supports:
/// - Listing directories
/// - Getting single entry metadata
/// - Creating files and directories
/// - Deleting entries
/// - Stress testing with large file counts
pub struct InMemoryVolume {
    name: String,
    root: PathBuf,
    entries: RwLock<HashMap<PathBuf, InMemoryEntry>>,
    /// Configurable space info for testing. None means get_space_info returns NotSupported.
    space_info: Option<SpaceInfo>,
    /// Lane key the operation manager uses to (de)serialize this volume against
    /// others. `None` ⇒ fall back to the root lane (the trait default), so the
    /// ~169 existing `new(...)` sites are untouched. Manager tests set it via
    /// `with_lane_key` to force same-lane (serialize) vs different-lane
    /// (parallel) behavior.
    lane_key: Option<String>,
    /// What [`Volume::supports_local_fs_access`] reports. Default `false` (a real
    /// in-memory store is not on the local FS). Archive tests that want to model a
    /// LOCAL-backed parent (so `ArchiveVolume` takes its `LocalFileSource` fast
    /// path) set it `true` via [`with_local_fs_access`](Self::with_local_fs_access);
    /// remote-backed archive tests leave it `false`.
    local_fs_access: bool,
    /// Log of `read_range(offset, len)` calls, in order. Lets tests assert how
    /// many positioned reads a remote-archive flow issues (e.g. the
    /// central-directory tail-read strategy: one tail read, a second only if the
    /// directory exceeds the first window). See [`Self::read_range_log`].
    read_range_log: std::sync::Mutex<Vec<(u64, usize)>>,
    /// When `true`, [`Volume::read_range`] returns `NotSupported` (as a real
    /// backend without a positioned-read primitive does — `SmbVolume` before its
    /// smb2 primitive lands). Models the "refuse typed" remote-archive path.
    /// Default `false` (positioned reads work). Set via [`Self::with_read_range_unsupported`].
    read_range_unsupported: bool,
    /// When `true`, [`Volume::create_directory_errors_on_existing_dir`] reports
    /// `false`, modeling a backend that ALLOWS same-name sibling objects (MTP).
    /// The remote-archive-edit swap uses that flag to pick delete-then-rename over
    /// an atomic rename-overwrite. Default `false` (rejects collisions, like SMB /
    /// local / a plain in-memory store).
    sibling_duplicates_allowed: bool,
    /// When `true`, [`Volume::delete`] returns an `IoError` instead of removing the
    /// entry. Lets the remote-temp-reaper test prove a best-effort reap DELETE
    /// failure never fails the surrounding edit (the edit commits via a
    /// rename-overwrite swap, which doesn't call `delete`). Default `false`.
    delete_fails: bool,
    /// Paths whose [`Volume::is_directory`] and [`Volume::get_metadata`] fail with
    /// an `IoError` instead of answering, modeling a stat that couldn't complete
    /// (a dropped MTP session, a hung mount) rather than a path that isn't there.
    /// That distinction is the whole point: a `NotFound` is an answer, and code
    /// that turns an unanswered stat into a confident "not a directory" is what
    /// routes a folder into a destructive file-shaped branch. Set via
    /// [`Self::set_stat_failing`]. Empty by default.
    stat_failing: RwLock<HashSet<PathBuf>>,
    /// What [`Volume::smb_connection_state`] reports. `None` (the default) is a
    /// volume that isn't a share at all; `Some` lets a test drive a code path
    /// gated on a live smb2 session without a server.
    smb_connection_state: Option<SmbConnectionState>,
    /// Raw errno to inject on the next `list_directory` call. Cleared after use.
    #[cfg(feature = "playwright-e2e")]
    injected_error: std::sync::Mutex<Option<i32>>,
}

impl InMemoryVolume {
    /// Creates a new empty in-memory volume.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            root: PathBuf::from("/"),
            entries: RwLock::new(HashMap::new()),
            space_info: None,
            lane_key: None,
            local_fs_access: false,
            read_range_log: std::sync::Mutex::new(Vec::new()),
            read_range_unsupported: false,
            sibling_duplicates_allowed: false,
            delete_fails: false,
            stat_failing: RwLock::new(HashSet::new()),
            smb_connection_state: None,
            #[cfg(feature = "playwright-e2e")]
            injected_error: std::sync::Mutex::new(None),
        }
    }

    /// Makes [`Volume::smb_connection_state`] report `state`, so this volume passes
    /// (or fails) a gate that requires a live smb2 session. Everything else stays
    /// in memory: nothing here talks to a server.
    pub fn with_smb_connection_state(mut self, state: SmbConnectionState) -> Self {
        self.smb_connection_state = Some(state);
        self
    }

    /// Roots this volume at `root` instead of `/`, so it can stand in for a drive
    /// mounted at a real-looking path (`/Volumes/X`, `/media/X`) in a test that
    /// exercises mount-root resolution.
    pub fn with_root(mut self, root: impl Into<PathBuf>) -> Self {
        self.root = root.into();
        self
    }

    /// Makes [`Volume::create_directory_errors_on_existing_dir`] report `false`,
    /// modeling a backend that allows same-name siblings (MTP). Used by the
    /// remote-archive-edit swap tests to exercise the delete-then-rename path.
    pub fn with_sibling_duplicates_allowed(mut self) -> Self {
        self.sibling_duplicates_allowed = true;
        self
    }

    /// Makes [`Volume::delete`] fail with an `IoError`, modeling a backend that
    /// can't remove an entry. Used by the remote-temp-reaper test to prove a reap
    /// delete failure never fails or blocks the edit.
    pub fn with_delete_failing(mut self) -> Self {
        self.delete_fails = true;
        self
    }

    /// Test helper: makes `is_directory` and `get_metadata` FAIL for `path`
    /// (typed `IoError`), rather than reporting it missing. The path keeps
    /// existing for everything else, so a test can put an unanswerable stat in
    /// front of code that has to decide what to do without one.
    pub fn set_stat_failing(&self, path: &Path) {
        let normalized = self.normalize(path);
        self.stat_failing.write_ignore_poison().insert(normalized);
    }

    /// Whether `path`'s stat is configured to fail.
    fn stat_fails_for(&self, normalized: &Path) -> bool {
        self.stat_failing.read_ignore_poison().contains(normalized)
    }

    /// Test helper: overwrites an existing entry's `modified_at` (unix seconds), so
    /// a test can age a file into the past (or clear its mtime). Panics if the path
    /// isn't present.
    pub fn set_modified_at(&self, path: &Path, modified_at: Option<u64>) {
        let normalized = self.normalize(path);
        self.entries
            .write_ignore_poison()
            .get_mut(&normalized)
            .expect("set_modified_at: entry must exist")
            .metadata
            .modified_at = modified_at;
    }

    /// Makes [`Volume::read_range`] return `NotSupported`, modeling a remote
    /// backend without a positioned-read primitive (`SmbVolume` before its smb2
    /// primitive lands). `get_metadata` still works, so `VolumeManager::resolve`
    /// exercises its "route on an unavailable primitive, refuse typed downstream"
    /// path.
    pub fn with_read_range_unsupported(mut self) -> Self {
        self.read_range_unsupported = true;
        self
    }

    /// Records a `read_range` call for the request-count assertions in the
    /// remote-archive source tests.
    fn record_read_range(&self, offset: u64, len: usize) {
        self.read_range_log.lock_ignore_poison().push((offset, len));
    }

    /// The `(offset, len)` of every `read_range` call so far, in order. Tests use
    /// it to pin the remote-archive byte source's request pattern.
    pub fn read_range_log(&self) -> Vec<(u64, usize)> {
        self.read_range_log.lock_ignore_poison().clone()
    }

    /// Sets the operation-manager lane key. Two `InMemoryVolume`s with the same
    /// key serialize (one lane); distinct keys run in parallel (disjoint
    /// lanes). Used by manager tests to drive the admission logic. Without it,
    /// volumes fall back to the root lane (the trait default).
    pub fn with_lane_key(mut self, key: impl Into<String>) -> Self {
        self.lane_key = Some(key.into());
        self
    }

    /// Makes this volume report `supports_local_fs_access() = true`, so an
    /// `ArchiveVolume` backed by it takes the LOCAL `LocalFileSource` path (the
    /// archive's `.zip` is assumed to be a real local file). Archive tests use it
    /// to model a local-backed parent; leave it off (default) to model a
    /// remote-backed one.
    pub fn with_local_fs_access(mut self) -> Self {
        self.local_fs_access = true;
        self
    }

    /// Sets configurable space info so get_space_info() works in tests.
    pub fn with_space_info(mut self, total_bytes: u64, available_bytes: u64) -> Self {
        self.space_info = Some(SpaceInfo {
            total_bytes,
            available_bytes,
            used_bytes: total_bytes.saturating_sub(available_bytes),
        });
        self
    }

    /// Overrides the `get_metadata` / listing size of an existing file so it
    /// DISAGREES with the file's real streamed byte count, modeling a remote
    /// source whose listed size lies (a stale or racy directory entry). The
    /// content is untouched — `open_read_stream` still yields the real bytes — so
    /// a transfer that plans against the real stream lands correct bytes. Test-only.
    pub fn set_reported_size(&self, path: &Path, reported_size: u64) {
        let normalized = self.normalize(path);
        let mut entries = self.entries.write_ignore_poison();
        if let Some(entry) = entries.get_mut(&normalized) {
            entry.metadata.size = Some(reported_size);
        }
    }

    /// Overrides the reported TYPE of an existing entry, so `is_directory`,
    /// `get_metadata`, and listings all report `is_directory` while the entry
    /// keeps holding whatever it really holds. That gap is the fault this whole
    /// area defends against: a directory answered as a file gets streamed as one
    /// and picks the destructive cleanup branch, and until now there was no way
    /// to express it in a test. Test-only.
    pub fn set_reported_type(&self, path: &Path, is_directory: bool) {
        let normalized = self.normalize(path);
        let mut entries = self.entries.write_ignore_poison();
        if let Some(entry) = entries.get_mut(&normalized) {
            entry.metadata.is_directory = is_directory;
        }
    }

    /// Creates an in-memory volume pre-populated with entries.
    pub fn with_entries(name: impl Into<String>, entries: Vec<FileEntry>) -> Self {
        let volume = Self::new(name);
        {
            let mut map = volume.entries.write_ignore_poison();
            for entry in entries {
                let path = PathBuf::from(&entry.path);
                map.insert(
                    path,
                    InMemoryEntry {
                        metadata: entry,
                        content: None,
                    },
                );
            }
        }
        volume
    }

    /// Creates an in-memory volume with N auto-generated files for stress testing.
    ///
    /// Generated entries:
    /// - Every 10th entry is a directory
    /// - Every 50th entry is a symlink
    /// - File sizes increase linearly
    pub fn with_file_count(name: impl Into<String>, count: usize) -> Self {
        let entries: Vec<FileEntry> = (0..count)
            .map(|i| {
                let is_dir = i % 10 == 0;
                let file_name = format!("file_{:06}.txt", i);
                FileEntry {
                    size: Some(1024 * (i as u64)),
                    modified_at: Some(1_640_000_000 + i as u64),
                    created_at: Some(1_639_000_000 + i as u64),
                    permissions: 0o644,
                    owner: "testuser".to_string(),
                    group: "staff".to_string(),
                    extended_metadata_loaded: true,
                    ..FileEntry::new(file_name.clone(), format!("/{}", file_name), is_dir, i % 50 == 0)
                }
            })
            .collect();
        Self::with_entries(name, entries)
    }

    /// Normalizes a path relative to the volume root.
    ///
    /// A SCHEME-shaped path (`mtp://device/1/DCIM`) counts as absolute even though
    /// `Path::is_absolute` says otherwise: that's the whole path vocabulary of an
    /// MTP volume, and rooting it under `/` would make every lookup miss while the
    /// entries it was built with keep their real keys.
    fn normalize(&self, path: &Path) -> PathBuf {
        if path.as_os_str().is_empty() || path == Path::new(".") {
            PathBuf::from("/")
        } else if path.is_absolute() || path.to_string_lossy().contains("://") {
            path.to_path_buf()
        } else {
            PathBuf::from("/").join(path)
        }
    }

    /// Gets the parent path of a given path.
    fn parent_of(path: &Path) -> PathBuf {
        path.parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("/"))
    }

    /// Gets current timestamp as seconds since Unix epoch.
    fn now_secs() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }
}

/// Chunk size for InMemoryReadStream (64 KB, small enough to test multi-chunk behavior
/// without needing large test data).
const IN_MEMORY_STREAM_CHUNK_SIZE: usize = 64 * 1024;

/// Streaming reader for in-memory files.
struct InMemoryReadStream {
    data: Vec<u8>,
    offset: usize,
}

impl VolumeReadStream for InMemoryReadStream {
    fn next_chunk(&mut self) -> Pin<Box<dyn Future<Output = Option<Result<Vec<u8>, VolumeError>>> + Send + '_>> {
        Box::pin(async move {
            if self.offset >= self.data.len() {
                return None;
            }
            let end = (self.offset + IN_MEMORY_STREAM_CHUNK_SIZE).min(self.data.len());
            let chunk = self.data[self.offset..end].to_vec();
            self.offset = end;
            Some(Ok(chunk))
        })
    }

    fn total_size(&self) -> u64 {
        self.data.len() as u64
    }

    fn bytes_read(&self) -> u64 {
        self.offset as u64
    }
}

/// The `Volume` impl, in its own file so neither half runs long.
mod volume_impl;
