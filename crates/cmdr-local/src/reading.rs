//! The local reader: directory reading and `FileEntry` construction.
//!
//! Pure I/O functions that read from disk and build `FileEntry` objects. No Tauri
//! or caching dependencies: the name order comes from the caller's
//! `ListingHost::sort_listing`. `LocalPosixVolume`, the watcher, `tags.rs`, and
//! `create.rs` read through it.
//!
//! Two listers build the same entries: `getattrlistbulk` on APFS and HFS+
//! (`bulk`, macOS), and `read_dir` plus one `lstat` per entry everywhere else.
//! See `DETAILS.md` § "The local reader".

#[cfg(target_os = "macos")]
mod bulk;

use std::collections::HashSet;
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::Path;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use cmdr_fs::benchmark;
use cmdr_fs::entry::{FileEntry, get_group_name, get_owner_name, is_hidden_by_name};
use cmdr_fs::log_private::LogPrivate;
use cmdr_fs::volume::host::listings::ListingHost;
use cmdr_fs::volume::{ListingProgress, SupportedDates};

/// Whether a POSIX entry's `st_flags` carries `UF_HIDDEN`, what `chflags hidden`
/// sets. macOS only; the flag doesn't exist on Linux, so this is always `false`
/// there. Reads a field the `stat`/`lstat` call that produced `metadata` already
/// fetched, so checking it costs no extra syscall.
#[cfg(target_os = "macos")]
fn has_uf_hidden(metadata: &fs::Metadata) -> bool {
    use std::os::macos::fs::MetadataExt as _;
    metadata.st_flags() & libc::UF_HIDDEN != 0
}

#[cfg(target_os = "linux")]
fn has_uf_hidden(_metadata: &fs::Metadata) -> bool {
    false
}

/// Reads a volume root's `/.hidden` file (Finder's legacy per-volume hide list,
/// still honored today) into the set of names it hides in that one directory.
/// Empty when the file doesn't exist or fails to read, which is the common case:
/// most directories, and most volumes, have none.
///
/// ❗ Call this only for the directory a listing has established IS a volume
/// root (`abs_path == volume.root()`); `/.hidden` has no meaning partway down a
/// tree, and reading a file that isn't there on every listing would be wasted
/// I/O for no benefit.
fn read_dot_hidden_names(root_dir: &Path) -> HashSet<String> {
    match fs::read_to_string(root_dir.join(".hidden")) {
        Ok(contents) => contents
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect(),
        Err(_) => HashSet::new(),
    }
}

/// Every entry of the folder at `path`, in the order `listings` decides
/// (`ListingHost::sort_listing`). On APFS and HFS+ each carries its added date.
pub fn list_directory_core(path: &Path, listings: &dyn ListingHost) -> Result<Vec<FileEntry>, std::io::Error> {
    list_directory_core_impl(path, None, false, listings, Pick::ByFilesystem)
}

/// Like `list_directory_core`, but publishes a running count into `tally` as it stats.
///
/// The tally is the bridge across a `spawn_blocking` boundary: `Volume::list_directory`
/// takes a `Sync`-but-not-`Send` progress callback that can't move onto the blocking
/// thread, so the lister publishes here and the async side samples and forwards on its
/// own cadence. Publishing is unthrottled (a relaxed store is nothing next to the `stat`
/// it accompanies); deciding how often the user sees a number belongs to the sampler.
///
/// `is_volume_root` gates the `/.hidden` read (see `read_dot_hidden_names`):
/// `LocalPosixVolume::list_directory` is the only caller, and it passes
/// `abs_path == self.root()`, the one place Cmdr knows a directory IS a volume
/// root rather than merely a directory somewhere under one.
pub(crate) fn list_directory_core_with_tally(
    path: &Path,
    tally: &ListingTally,
    is_volume_root: bool,
    listings: &dyn ListingHost,
) -> Result<Vec<FileEntry>, std::io::Error> {
    list_directory_core_impl(path, Some(tally), is_volume_root, listings, Pick::ByFilesystem)
}

/// Which lister a test or the listing benchmark forces, where production picks
/// by filesystem.
#[cfg(any(test, feature = "testing"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lister {
    /// `getattrlistbulk`, whatever the filesystem, with every runtime fallback
    /// production has (a doubtful stream still re-lists through `read_dir`).
    #[cfg(target_os = "macos")]
    Bulk,
    /// `read_dir` plus one `lstat` per entry, what a bulk stream falls back to.
    ReadDir,
}

/// Lists `path` through `lister`, as `LocalPosixVolume::list_directory` would
/// with its choice forced. The parity cells and `examples/listing_bench` run both
/// listers through this.
///
/// # Errors
///
/// What the forced lister's listing returns.
#[cfg(any(test, feature = "testing"))]
pub fn list_directory_with(
    path: &Path,
    is_volume_root: bool,
    listings: &dyn ListingHost,
    lister: Lister,
) -> Result<Vec<FileEntry>, std::io::Error> {
    list_directory_core_impl(
        path,
        Some(&ListingTally::default()),
        is_volume_root,
        listings,
        Pick::Forced(lister),
    )
}

/// The added date `get_single_entry` and the `read_dir` lister fill on APFS and
/// HFS+: one `getattrlist` without following a link. `None` elsewhere.
#[cfg(any(test, feature = "testing"))]
pub fn added_at_by_path(path: &Path) -> Option<u64> {
    #[cfg(target_os = "macos")]
    return bulk::added_at_by_path(path);
    #[cfg(target_os = "linux")]
    {
        let _ = path;
        None
    }
}

/// Who decides which lister runs.
#[derive(Clone, Copy)]
enum Pick {
    /// Production: bulk on APFS and HFS+, `read_dir` on everything else.
    ByFilesystem,
    /// A test or the benchmark.
    #[cfg(any(test, feature = "testing"))]
    Forced(Lister),
}

/// Whether the entries a listing builds carry the added date.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum AddedAt {
    /// One `getattrlist` per entry: the listed directory is bulk-eligible, so its
    /// entries carry an added date whichever lister ran.
    #[cfg(target_os = "macos")]
    Fetch,
    /// The filesystem doesn't record one.
    Skip,
}

/// A listing's running entry counts, readable from another thread while the stat loop
/// fills it. Sample it with `snapshot`.
// DEFAULT-OK: zero is where every tally starts, before the walk has stat'ed anything. It
// claims nothing about the disk; it's the count of what we've looked at so far.
#[derive(Default)]
pub(crate) struct ListingTally {
    files: AtomicUsize,
    dirs: AtomicUsize,
    bytes: AtomicU64,
    /// Runs on the lister's thread after each entry is counted, so a test can hold
    /// the read partway and see a tick land deterministically, whatever the
    /// machine's load ([`crate::LocalPosixVolume::with_listing_hook`]).
    #[cfg(any(test, feature = "testing"))]
    after_record: Option<ListingHook>,
}

/// See [`ListingTally`]'s `after_record`.
#[cfg(any(test, feature = "testing"))]
pub type ListingHook = std::sync::Arc<dyn Fn() + Send + Sync>;

impl ListingTally {
    /// A tally that calls `hook` after each entry it counts.
    #[cfg(any(test, feature = "testing"))]
    pub(crate) fn with_hook(hook: Option<ListingHook>) -> Self {
        Self {
            after_record: hook,
            ..Self::default()
        }
    }

    fn record(&self, entry: &FileEntry) {
        if entry.is_directory {
            self.dirs.fetch_add(1, Ordering::Relaxed);
        } else {
            self.files.fetch_add(1, Ordering::Relaxed);
            self.bytes.fetch_add(entry.size.unwrap_or(0), Ordering::Relaxed);
        }
        #[cfg(any(test, feature = "testing"))]
        if let Some(hook) = &self.after_record {
            hook();
        }
    }

    /// Back to zero, for a listing that discards what it counted and starts over
    /// (a bulk stream re-listing through `read_dir`), so the readout never
    /// counts an entry twice.
    #[cfg(target_os = "macos")]
    fn reset(&self) {
        self.files.store(0, Ordering::Relaxed);
        self.dirs.store(0, Ordering::Relaxed);
        self.bytes.store(0, Ordering::Relaxed);
    }

    /// Reads the counts as they stand right now. The three fields are loaded
    /// separately, so a snapshot taken mid-stat can be an entry out of step with
    /// itself; that's fine for a progress readout and it never over-counts.
    pub(crate) fn snapshot(&self) -> ListingProgress {
        ListingProgress {
            files: self.files.load(Ordering::Relaxed),
            dirs: self.dirs.load(Ordering::Relaxed),
            bytes: self.bytes.load(Ordering::Relaxed),
            ..ListingProgress::default()
        }
    }
}

fn list_directory_core_impl(
    path: &Path,
    tally: Option<&ListingTally>,
    is_volume_root: bool,
    listings: &dyn ListingHost,
    pick: Pick,
) -> Result<Vec<FileEntry>, std::io::Error> {
    benchmark::log_event("list_directory_core START");
    let overall_start = std::time::Instant::now();

    // Read once per listing, never per entry: at a volume root only (see this
    // function's `is_volume_root` doc on the caller side), `/.hidden` names the
    // few entries Finder hides that carry no dot and no `UF_HIDDEN` flag.
    let dot_hidden_names = if is_volume_root {
        read_dot_hidden_names(path)
    } else {
        HashSet::new()
    };

    benchmark::log_event("read_entries START");
    let read_start = std::time::Instant::now();
    let mut entries = read_entries(path, &dot_hidden_names, tally, pick)?;
    let read_time = read_start.elapsed();
    benchmark::log_event_value("read_entries END, entries", entries.len());

    // Sort: directories first, then files, both by name in the panes' order. One
    // seam call per directory, never per entry.
    benchmark::log_event("sort START");
    listings.sort_listing(&mut entries);
    benchmark::log_event("sort END");

    let total_time = overall_start.elapsed();
    log::debug!(
        "list_directory_core: path={}, entries={}, read={}ms, total={}ms",
        LogPrivate::dir_path(path),
        entries.len(),
        read_time.as_millis(),
        total_time.as_millis()
    );
    if entries.is_empty() {
        // 0 entries can mean: a genuinely empty dir, a permission/TCC restriction that
        // macOS surfaces as an empty enumeration (e.g., iCloud Drive without Full Disk
        // Access), or a cloud sync placeholder that hasn't materialized yet. Surface the
        // path's own metadata so error reports distinguish "we got told the dir has no
        // children" from "the path itself is broken".
        //
        // Debug, not warn: an empty folder is ordinary (every fresh copy destination is
        // one), and the file target logs Debug unconditionally, so a report bundle still
        // carries this line when the restricted-directory case is the one being chased.
        let metadata_status = match fs::metadata(path) {
            Ok(m) => {
                let kind = if m.is_dir() {
                    "dir"
                } else if m.is_file() {
                    "file"
                } else {
                    "other"
                };
                format!("kind={kind}, perms=0o{:o}, size={}", m.permissions().mode(), m.len())
            }
            Err(e) => format!("metadata failed: {e}"),
        };
        log::debug!(
            "list_directory_core: 0 entries at {} ({metadata_status}). Could be genuinely empty, permission/TCC restricted (Full Disk Access?), or a cloud-sync placeholder.",
            LogPrivate::dir_path(path),
        );
    }
    benchmark::log_event("list_directory_core END");

    Ok(entries)
}

/// Every entry of `path`, unsorted. macOS picks the lister (`bulk::read_entries`).
#[cfg(target_os = "macos")]
fn read_entries(
    path: &Path,
    dot_hidden: &HashSet<String>,
    tally: Option<&ListingTally>,
    pick: Pick,
) -> std::io::Result<Vec<FileEntry>> {
    bulk::read_entries(path, dot_hidden, tally, pick)
}

/// Every entry of `path`, unsorted. Linux has no `getattrlistbulk`, so always
/// `read_dir`, and no added date. Each entry keeps only the dates `path`'s
/// listing claims (`withhold_unclaimed_dates`): one more `stat`, of `path`.
#[cfg(target_os = "linux")]
fn read_entries(
    path: &Path,
    dot_hidden: &HashSet<String>,
    tally: Option<&ListingTally>,
    pick: Pick,
) -> std::io::Result<Vec<FileEntry>> {
    let _ = pick;
    let entries = read_dir_entries(path, dot_hidden, tally, AddedAt::Skip)?;
    let dates = supported_dates_of(path);
    Ok(entries
        .into_iter()
        .map(|entry| withhold_unclaimed_dates(entry, dates))
        .collect())
}

/// The `read_dir` lister: every name, then one `lstat` per entry. A name whose
/// `lstat` fails lists as the minimal row; an iteration error fails the whole
/// listing.
fn read_dir_entries(
    path: &Path,
    dot_hidden: &HashSet<String>,
    tally: Option<&ListingTally>,
    added: AddedAt,
) -> std::io::Result<Vec<FileEntry>> {
    let dir_entries: Vec<_> = fs::read_dir(path)?.collect();
    let mut entries = Vec::with_capacity(dir_entries.len());
    for entry in dir_entries {
        let entry = entry?;
        let entry_path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        let file_entry = entry_from_lstat(&entry_path, name.clone(), dot_hidden, added).unwrap_or_else(|| {
            // `d_type` still says whether it's a link, which picks the row's icon.
            let is_symlink = entry.file_type().is_ok_and(|ft| ft.is_symlink());
            minimal_row(&entry_path, name, is_symlink, dot_hidden)
        });
        if let Some(tally) = tally {
            tally.record(&file_entry);
        }
        entries.push(file_entry);
    }
    Ok(entries)
}

/// The entry at `path` from its own `lstat`, or `None` when that `lstat` fails
/// (the entry vanished, its folder isn't searchable, a sandbox said `EPERM`).
/// With `AddedAt::Fetch` it also carries the added date.
pub(crate) fn entry_from_lstat(
    path: &Path,
    name: String,
    dot_hidden: &HashSet<String>,
    added: AddedAt,
) -> Option<FileEntry> {
    let metadata = fs::symlink_metadata(path).ok()?;
    let entry = entry_from_metadata(path, name, &metadata, dot_hidden);
    match added {
        #[cfg(target_os = "macos")]
        AddedAt::Fetch => Some(FileEntry {
            added_at: bulk::added_at_by_path(path),
            ..entry
        }),
        AddedAt::Skip => Some(entry),
    }
}

/// The row for an entry nothing could be read about: its name, whether it's a
/// link (from the caller's `d_type` or record type: it picks the
/// `symlink-broken` icon), and nothing else. No size, no dates, no owner, never a
/// folder.
pub(crate) fn minimal_row(path: &Path, name: String, is_symlink: bool, dot_hidden: &HashSet<String>) -> FileEntry {
    FileEntry {
        icon_id: if is_symlink {
            "symlink-broken".to_string()
        } else {
            "file".to_string()
        },
        is_hidden: is_hidden_by_name(&name) || dot_hidden.contains(&name),
        ..FileEntry::new(name, path.to_string_lossy().into_owned(), false, is_symlink)
    }
}

/// Gets metadata for a single file or directory path.
///
/// Used when we need metadata for a single path rather than listing
/// a directory. Useful for symlink target resolution and volume implementations.
/// Builds the same entry a listing of its folder does (through
/// `entry_from_metadata`), added date included, except that it never applies a
/// volume root's `/.hidden`.
pub fn get_single_entry(path: &Path) -> Result<FileEntry, std::io::Error> {
    let metadata = fs::symlink_metadata(path)?;
    let name = path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string());
    let entry = entry_from_metadata(path, name, &metadata, &HashSet::new());
    #[cfg(target_os = "macos")]
    let entry = FileEntry {
        added_at: bulk::parent_is_bulk_eligible(path)
            .then(|| bulk::added_at_by_path(path))
            .flatten(),
        ..entry
    };
    // Linux: keep the birth time only where the parent's listing claims one, as
    // `read_entries` does, so a re-stat equals its listing entry. The parent's
    // `stat` runs only for an entry that has a birth time to lose.
    #[cfg(target_os = "linux")]
    let entry = if entry.created_at.is_some() {
        let parent = parent_dir(path);
        withhold_unclaimed_dates(entry, supported_dates_of(parent))
    } else {
        entry
    };
    Ok(entry)
}

/// The folder whose listing holds `path` (`path` itself at `/`).
#[cfg(target_os = "linux")]
fn parent_dir(path: &Path) -> &Path {
    match path.parent() {
        Some(parent) if parent.as_os_str().is_empty() => Path::new("."),
        Some(parent) => parent,
        None => path,
    }
}

/// Which dates a listing of the folder `dir` carries beyond the modification
/// date, by construction from what the listers fill: the added date iff the
/// folder takes the bulk lister (APFS and HFS+, whose `read_dir` fallback and
/// `get_single_entry` fill it too), the creation date iff `dir`'s own `stat`
/// carries a birth time (macOS: always, `st_birthtime`; Linux: `statx`
/// returned `STATX_BTIME`, which is when `Metadata::created` answers).
///
/// Asks `dir` itself, not the volume: the root volume lists folders on other
/// filesystems (an exFAT stick under `/Volumes`). Blocking (a `statfs` and a
/// `stat`), so run it on `spawn_blocking`.
pub(crate) fn supported_dates_of(dir: &Path) -> SupportedDates {
    SupportedDates {
        created: fs::metadata(dir).and_then(|metadata| metadata.created()).is_ok(),
        #[cfg(target_os = "macos")]
        added: bulk::dir_is_bulk_eligible(dir),
        #[cfg(target_os = "linux")]
        added: false,
    }
}

/// `entry` without the dates its folder's listing doesn't claim (`dates`, from
/// `supported_dates_of`), so a listing never carries a date its claim denies.
///
/// Only Linux needs it: there an entry's birth time is its OWN filesystem's
/// (`statx`), so an ext4 mount point inside an NFS folder carries one the NFS
/// listing doesn't claim. On macOS every `stat` has `st_birthtime`, and the
/// added date is fetched only where the folder is bulk-eligible, so both claims
/// hold by construction there.
#[cfg_attr(target_os = "macos", allow(dead_code, reason = "Linux-only; tested on every OS"))]
pub(crate) fn withhold_unclaimed_dates(entry: FileEntry, dates: SupportedDates) -> FileEntry {
    FileEntry {
        created_at: entry.created_at.filter(|_| dates.created),
        added_at: entry.added_at.filter(|_| dates.added),
        ..entry
    }
}

/// Builds an entry from its `lstat`: everything from the link itself, except
/// `is_directory`, which follows a link to its target (one extra `stat`).
///
/// `dot_hidden` is the listing's `/.hidden` set (empty outside a volume root;
/// see `read_dot_hidden_names`), asked once per entry here rather than re-read
/// per entry by the caller. The added date is the caller's to fill.
pub(crate) fn entry_from_metadata(
    path: &Path,
    name: String,
    metadata: &fs::Metadata,
    dot_hidden: &HashSet<String>,
) -> FileEntry {
    let is_symlink = metadata.file_type().is_symlink();
    let target_is_dir = is_symlink && fs::metadata(path).is_ok_and(|m| m.is_dir());
    let is_dir = metadata.is_dir() || target_is_dir;

    let modified = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs());

    let created = metadata
        .created()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs());

    let physical_size = if metadata.is_file() {
        Some(metadata.blocks() * 512)
    } else {
        None
    };

    // `inode` is populated from `st_ino` for hardlinked files only. Hardlink
    // dedup in write-op walkers uses this to spot multiple paths pointing at the
    // same inode. Directories don't share inodes the way files do, and a
    // single-link file has nothing to dedupe, so leaving them `None` keeps the
    // dedup decision file-shaped.
    let inode = if metadata.is_file() && metadata.nlink() > 1 {
        Some(metadata.ino())
    } else {
        None
    };

    FileEntry {
        size: if metadata.is_file() { Some(metadata.len()) } else { None },
        physical_size,
        inode,
        modified_at: modified,
        created_at: created,
        permissions: metadata.permissions().mode(),
        owner: get_owner_name(metadata.uid()),
        group: get_group_name(metadata.gid()),
        is_hidden: is_hidden_by_name(&name) || has_uf_hidden(metadata) || dot_hidden.contains(&name),
        ..FileEntry::new(name, path.to_string_lossy().to_string(), is_dir, is_symlink)
    }
}
