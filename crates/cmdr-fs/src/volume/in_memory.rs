//! In-memory file system volume for testing.
//!
//! Provides a fully in-memory file system that supports all Volume operations,
//! including create, delete, and list. Useful for unit and integration tests
//! without touching the real file system.
//!
//! This module is what the double can be TOLD to be: the store, the knobs, and
//! the lies it tells on request. How it then behaves as a [`Volume`] is
//! `volume_impl.rs`.

#[cfg(doc)]
use super::Volume; // the knobs' docs link the trait methods each one bends
use super::{BackendKind, ConnectionState, IndexWalk, SpaceInfo, VolumeError};
use crate::entry::FileEntry;
use crate::ignore_poison::IgnorePoison;
use crate::ignore_poison::RwLockIgnorePoison;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::RwLock;

mod volume_impl;

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
    /// Per-subtree space for [`Volume::get_space_info_at`], modeling a volume
    /// that spans several filesystems. The deepest matching prefix answers;
    /// a path under none of them falls back to `space_info`.
    space_info_under: Vec<(PathBuf, SpaceInfo)>,
    /// Per-subtree answers for [`Volume::write_access_at`], set by
    /// [`with_write_access_under`](Self::with_write_access_under). The deepest
    /// matching prefix answers; a path under none of them is `Unknown`.
    write_access_under: Vec<(PathBuf, super::WriteAccess)>,
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
    /// What [`Volume::routes_over_a_parent`] reports. Default `false` (a mount of
    /// its own). Set it `true` via [`routing_over_a_parent`](Self::routing_over_a_parent)
    /// to stand in for a routed backend without naming a concrete one.
    routes_over_a_parent: bool,
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
    /// How long each read chunk takes to arrive. `None` (the default) means an
    /// in-memory read completes without ever yielding, which is right for the
    /// hundreds of tests that only care about the bytes — and useless for a test
    /// that has to CATCH a transfer mid-file, because the whole file lands inside
    /// one poll. Set via [`Self::with_read_chunk_delay`] to model a stream that
    /// arrives over time, so a cancel or a pause has somewhere to land.
    read_chunk_delay: Option<std::time::Duration>,
    /// When set, [`Volume::rename`] returns this error instead of moving the entry,
    /// and the store is left exactly as it was. The variant matters: a caller that
    /// clears the destination on ANY rename failure deletes the user's file over a
    /// transient blip, so the tests that pin "clear the way only on
    /// `AlreadyExists`" need a rename that fails some OTHER way. Default `None`.
    /// Set via [`Self::with_rename_failing`].
    rename_failure: Option<VolumeError>,
    rename_to_failing: RwLock<HashSet<PathBuf>>, // [`Self::set_rename_to_failing`]
    /// When `true`, [`Volume::create_directory`] returns `NotFound` for the path it
    /// was handed instead of creating it. Models a backend that can't ADDRESS the
    /// destination at all (a share answering `NotFound` for a path outside its
    /// mount), which is the one shape that makes a destination failure look
    /// exactly like a missing source. Default `false`. Set via
    /// [`Self::with_create_directory_not_found`].
    create_directory_not_found: bool,
    /// What [`Volume::composes_new_names`] reports. Default `false`; set via
    /// [`Self::with_composed_new_names`] to stand in for a share.
    composes_new_names: bool,
    /// What [`Volume::publishes_writes_whole`] reports, and whether
    /// `write_from_stream` honours [`WriteMode`](super::WriteMode) the way an
    /// object store does (`CreateOrReplace` replaces at the end, the old bytes
    /// readable until then). Default `false`, where a write onto an existing
    /// name refuses whatever the mode. Set via [`Self::with_whole_publish`].
    publishes_writes_whole: bool,
    /// Whether this volume renames like an object store: [`Volume::rename_work`]
    /// answers `CopyThenDelete` for every entry, and [`Volume::rename`] refuses
    /// with `NotSupported`, so a caller that forgot to ask gets caught. Default
    /// `false`. Set via [`Self::with_renames_by_copy`].
    renames_by_copy: bool,
    /// Every batch [`Volume::delete_files`] was handed, in order, so a test can
    /// see the sweep batched.
    delete_batches: std::sync::Mutex<Vec<usize>>,
    /// Paths whose [`Volume::is_directory`] and [`Volume::get_metadata`] fail with
    /// an `IoError` instead of answering, modeling a stat that couldn't complete
    /// (a dropped MTP session, a hung mount) rather than a path that isn't there.
    /// That distinction is the whole point: a `NotFound` is an answer, and code
    /// that turns an unanswered stat into a confident "not a directory" is what
    /// routes a folder into a destructive file-shaped branch. Set via
    /// [`Self::set_stat_failing`]. Empty by default.
    stat_failing: RwLock<HashSet<PathBuf>>,
    /// What [`Volume::connection_state`] reports. `None` (the default) is a
    /// volume with no session at all; `Some` lets a test drive a code path gated
    /// on a live remote session without a server.
    connection_state: Option<ConnectionState>,
    /// What [`Volume::backend_kind`] reports. [`BackendKind::Local`] by default,
    /// so a double only names a transport when the code under test asks about
    /// one.
    backend_kind: BackendKind,
    /// [`Volume::index_walk`] answers set by
    /// [`with_index_walk`](Self::with_index_walk), by directory path. Any other
    /// directory keeps the trait's default.
    index_walk_overrides: Vec<(PathBuf, IndexWalk)>,
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
            space_info_under: Vec::new(),
            write_access_under: Vec::new(),
            lane_key: None,
            local_fs_access: false,
            routes_over_a_parent: false,
            read_range_log: std::sync::Mutex::new(Vec::new()),
            read_range_unsupported: false,
            sibling_duplicates_allowed: false,
            delete_fails: false,
            read_chunk_delay: None,
            rename_failure: None,
            rename_to_failing: RwLock::new(HashSet::new()),
            create_directory_not_found: false,
            composes_new_names: false,
            publishes_writes_whole: false,
            renames_by_copy: false,
            delete_batches: std::sync::Mutex::new(Vec::new()),
            stat_failing: RwLock::new(HashSet::new()),
            connection_state: None,
            backend_kind: BackendKind::Local,
            index_walk_overrides: Vec::new(),
            #[cfg(feature = "playwright-e2e")]
            injected_error: std::sync::Mutex::new(None),
        }
    }

    /// Makes [`Volume::connection_state`] report `state`, so this volume passes
    /// (or fails) a gate that requires a live session. Everything else stays in
    /// memory: nothing here talks to a server.
    pub fn with_connection_state(mut self, state: ConnectionState) -> Self {
        self.connection_state = Some(state);
        self
    }

    /// Makes [`Volume::backend_kind`] report `kind`, so this volume stands in for
    /// a share, a server, or a phone at a gate that dispatches on the transport.
    pub fn with_backend_kind(mut self, kind: BackendKind) -> Self {
        self.backend_kind = kind;
        self
    }

    /// Makes [`Volume::index_walk`] answer `walk` for the directory at `dir`,
    /// so this volume stands in for a backend that keeps a tree out of its index
    /// walks (a phone's `/proc`) or names the link that is its storage (a phone's
    /// `/sdcard`). Every other directory keeps the trait's default answer.
    pub fn with_index_walk(mut self, dir: impl Into<PathBuf>, walk: IndexWalk) -> Self {
        self.index_walk_overrides.push((dir.into(), walk));
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

    /// Makes every read chunk take `delay` to arrive, so a reader spends real time
    /// inside one file. That's what gives a test a window to cancel or pause a
    /// transfer MID-FILE, instead of racing a read that finishes in one poll.
    pub fn with_read_chunk_delay(mut self, delay: std::time::Duration) -> Self {
        self.read_chunk_delay = Some(delay);
        self
    }

    /// Makes [`Volume::rename`] fail with `error` and change nothing, modeling a
    /// rename that couldn't complete for a reason OTHER than a live destination:
    /// a dropped session, a server that refused, a backend with no rename at all.
    ///
    /// That distinction is the whole point. `AlreadyExists` means "something is
    /// in the way", and a caller may clear it; every other failure means the
    /// destination is none of our business, and clearing it destroys a file that
    /// was never ours to touch.
    pub fn with_rename_failing(mut self, error: VolumeError) -> Self {
        self.rename_failure = Some(error);
        self
    }

    /// Makes [`Volume::create_directory`] fail with `NotFound`, modeling a backend
    /// that can't address the path at all rather than one that tried and couldn't
    /// write. A share answers this way for a path outside its mount, and it's the
    /// case that decides whether a DESTINATION failure gets reported as a missing
    /// SOURCE.
    pub fn with_create_directory_not_found(mut self) -> Self {
        self.create_directory_not_found = true;
        self
    }

    /// Makes [`Volume::composes_new_names`] report `true`, standing in for a share
    /// whose new names go out composed. The store itself stays byte-exact, like
    /// the server behind one.
    pub fn with_composed_new_names(mut self) -> Self {
        self.composes_new_names = true;
        self
    }

    /// Makes this volume an object store's double: [`Volume::publishes_writes_whole`]
    /// answers `true`, and `write_from_stream` lands its bytes only once the
    /// stream ends, replacing an existing file under `CreateOrReplace` and
    /// refusing one under `CreateNew`. A write that fails or is cancelled leaves
    /// the name exactly as it was.
    pub fn with_whole_publish(mut self) -> Self {
        self.publishes_writes_whole = true;
        self
    }

    /// Makes this volume rename like an object store: every entry's
    /// [`Volume::rename_work`] is `CopyThenDelete`, and `rename` itself refuses
    /// with `NotSupported`.
    pub fn with_renames_by_copy(mut self) -> Self {
        self.renames_by_copy = true;
        self
    }

    /// How many paths each [`Volume::delete_files`] call carried, in order.
    pub fn delete_batches(&self) -> Vec<usize> {
        self.delete_batches.lock_ignore_poison().clone()
    }

    /// Test helper: fails any `rename` whose DESTINATION is `to`, AFTER the
    /// occupancy check — a destination that refuses a rename onto a name that IS
    /// free, so a caller which clears the way and retries still can't land. The
    /// per-path twin of [`Self::with_rename_failing`], which refuses every name
    /// and so can't exercise a caller reaching for a SECOND one.
    pub fn set_rename_to_failing(&self, to: &Path) {
        let normalized = self.normalize(to);
        self.rename_to_failing.write_ignore_poison().insert(normalized);
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

    /// The one stat body behind `get_metadata` and `is_directory`: answers `f`
    /// over the entry's metadata, `NotFound` for a missing path, and the typed
    /// `IoError` for a path under [`Self::set_stat_failing`].
    fn stat_with<T>(&self, path: &Path, f: impl FnOnce(&FileEntry) -> T) -> Result<T, VolumeError> {
        let entries = self.entries.read().map_err(|_| VolumeError::IoError {
            message: "Lock poisoned".into(),
            raw_os_error: None,
        })?;

        let normalized = self.normalize(path);
        if self.stat_fails_for(&normalized) {
            return Err(VolumeError::IoError {
                message: format!("Stat unavailable for {}", normalized.display()),
                raw_os_error: None,
            });
        }

        entries
            .get(&normalized)
            .map(|e| f(&e.metadata))
            .ok_or_else(|| VolumeError::NotFound(normalized.display().to_string()))
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

    /// Makes this volume report `routes_over_a_parent() = true`, standing in for
    /// a read-only volume a route minted over some other volume's storage.
    ///
    /// The point of the stub is that it names NO concrete backend: a host's
    /// "a routed volume is not a mount" rule has to hold for the next routed
    /// backend too, and a cell written against `ArchiveVolume` or
    /// `GitPortalVolume` can't say that.
    pub fn routing_over_a_parent(mut self) -> Self {
        self.routes_over_a_parent = true;
        self
    }

    /// Sets configurable BOUNDED space info so get_space_info() works in tests.
    pub fn with_space_info(mut self, total_bytes: u64, available_bytes: u64) -> Self {
        self.space_info = Some(SpaceInfo::bounded(total_bytes, available_bytes));
        self
    }

    /// Sets configurable UNBOUNDED space info: storage with no ceiling, where
    /// only what's stored is known (an unlimited Nextcloud account). Lets a test
    /// drive the pre-flight's "can't tell, go ahead" path with a volume that
    /// nonetheless answers.
    pub fn with_unbounded_space_info(mut self, used_bytes: u64) -> Self {
        self.space_info = Some(SpaceInfo::Unbounded { used_bytes });
        self
    }

    /// Makes [`Volume::get_space_info_at`] answer a BOUNDED figure for `under`
    /// and everything below it, modeling a second filesystem mounted there (a
    /// phone's shared storage beside its read-only system image). The volume's
    /// own figure stays whatever [`with_space_info`](Self::with_space_info) set.
    pub fn with_space_info_under(mut self, under: impl Into<PathBuf>, total_bytes: u64, available_bytes: u64) -> Self {
        self.space_info_under
            .push((under.into(), SpaceInfo::bounded(total_bytes, available_bytes)));
        self
    }

    /// Makes [`Volume::write_access_at`] answer `access` for `under` and everything
    /// below it, modeling a read-only system image or a folder this user can't
    /// write into. Every other path answers `Unknown`, as a backend with no way to
    /// ask does.
    pub fn with_write_access_under(mut self, under: impl Into<PathBuf>, access: super::WriteAccess) -> Self {
        self.write_access_under.push((under.into(), access));
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

    /// Overrides the NAME an existing entry is listed under, while it stays
    /// stored (and readable) at its real path: a hostile server or device that
    /// lists `../x` or `/x` for a file. Whatever joins a listed name onto a
    /// destination path has to refuse it rather than write outside the folder it
    /// was given (`ChildName`). Test-only.
    pub fn set_reported_name(&self, path: &Path, reported_name: &str) {
        let normalized = self.normalize(path);
        let mut entries = self.entries.write_ignore_poison();
        if let Some(entry) = entries.get_mut(&normalized) {
            entry.metadata.name = reported_name.to_string();
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

    /// Puts `data` at `path` in one step, replacing a FILE that's there: the
    /// landing of a [`with_whole_publish`](Self::with_whole_publish) write.
    /// A folder at the name is refused, as an object store's would be.
    fn replace_whole(&self, path: &Path, data: Vec<u8>) -> Result<(), VolumeError> {
        let normalized = self.normalize(path);
        let mut entries = self.entries.write_ignore_poison();
        if entries
            .get(&normalized)
            .is_some_and(|entry| entry.metadata.is_directory)
        {
            return Err(VolumeError::IsADirectory(normalized.display().to_string()));
        }
        Self::insert_file(&mut entries, normalized, data);
        Ok(())
    }

    /// Puts a fresh file holding `data` at `normalized`, dated now, replacing
    /// whatever entry is there. The caller has already decided it may.
    fn insert_file(entries: &mut HashMap<PathBuf, InMemoryEntry>, normalized: PathBuf, data: Vec<u8>) {
        let name = normalized
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let metadata = FileEntry {
            size: Some(data.len() as u64),
            modified_at: Some(Self::now_secs()),
            created_at: Some(Self::now_secs()),
            permissions: 0o644,
            owner: "testuser".to_string(),
            group: "staff".to_string(),
            extended_metadata_loaded: true,
            ..FileEntry::new(name, normalized.display().to_string(), false, false)
        };
        Self::touch_parent_of(entries, &normalized);
        entries.insert(
            normalized,
            InMemoryEntry {
                metadata,
                content: Some(data),
            },
        );
    }

    /// Moves a folder's date to now when an entry lands in it or leaves it, as on
    /// every real store, so an engine test can tell a folder dated after its
    /// contents landed from one dated before them.
    fn touch_parent_of(entries: &mut HashMap<PathBuf, InMemoryEntry>, child: &Path) {
        if let Some(parent) = child.parent().and_then(|parent| entries.get_mut(parent))
            && parent.metadata.is_directory
        {
            parent.metadata.modified_at = Some(Self::now_secs());
        }
    }

    /// Gets current timestamp as seconds since Unix epoch.
    fn now_secs() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }
}
