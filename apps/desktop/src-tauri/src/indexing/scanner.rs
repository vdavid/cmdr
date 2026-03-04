//! Parallel directory walker for drive indexing.
//!
//! Uses `jwalk` for fast parallel directory traversal. Provides both full-volume scan
//! (`scan_volume`) and targeted subtree scan (`scan_subtree`). Discovered entries are
//! sent in batches to the [`IndexWriter`] for insertion into the SQLite index.
//!
//! Scan exclusions (macOS system directories, virtual filesystems) are filtered via
//! jwalk's `process_read_dir` callback so excluded subtrees are never descended into.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

use jwalk::WalkDir;

use crate::indexing::firmlinks;
use crate::indexing::store::{EntryRow, IndexStore, ScanContext};
use crate::indexing::writer::{IndexWriter, WriteMessage};

// ── Exclusion prefixes ──────────────────────────────────────────────

/// macOS: absolute path prefixes to skip during scanning.
#[cfg(target_os = "macos")]
const EXCLUDED_PREFIXES: &[&str] = &[
    "/System/Volumes/Data/",
    "/System/Volumes/VM/",
    "/System/Volumes/Preboot/",
    "/System/Volumes/Update/",
    "/System/Volumes/xarts/",
    "/System/Volumes/iSCPreboot/",
    "/System/Volumes/Hardware/",
    "/Volumes/", // Skip mounted volumes (network shares, external drives) -- index boot volume only
    "/private/var/",
    "/Library/Caches/",
    "/.Spotlight-V100/",
    "/.fseventsd/",
    "/dev/",
    "/proc/",
];

/// Linux: virtual filesystems and system directories to skip during scanning.
#[cfg(target_os = "linux")]
const EXCLUDED_PREFIXES: &[&str] = &[
    "/dev/",
    "/proc/",
    "/sys/",
    "/run/",
    "/snap/",
    "/lost+found/",
    "/mnt/",   // Skip manual mount points -- index the root filesystem only
    "/media/", // Skip removable media
    "/boot/",
    "/tmp/",
    "/var/tmp/",
    "/var/cache/",
    "/var/log/",
    "/var/run/",
];

/// Fallback exclusion prefixes for other platforms.
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
const EXCLUDED_PREFIXES: &[&str] = &["/dev/", "/proc/"];

/// macOS: `/System/` paths reachable via firmlinks (from `/usr/share/firmlinks`).
/// These are the ONLY `/System/` subdirectories we allow through the exclusion filter.
#[cfg(target_os = "macos")]
const FIRMLINKED_SYSTEM_PREFIXES: &[&str] = &[
    "/System/Library/Caches",
    "/System/Library/Assets",
    "/System/Library/PreinstalledAssets",
    "/System/Library/AssetsV2",
    "/System/Library/PreinstalledAssetsV2",
    "/System/Library/CoreServices/CoreTypes.bundle/Contents/Library",
    "/System/Library/Speech",
];

// ── Types ────────────────────────────────────────────────────────────

/// Configuration for a scan operation.
pub struct ScanConfig {
    /// Root path to scan from.
    pub root: PathBuf,
    /// Batch size for sending entries to the writer.
    pub batch_size: usize,
    /// Number of jwalk rayon threads (0 = auto-detect).
    pub num_threads: usize,
}

impl Default for ScanConfig {
    fn default() -> Self {
        Self {
            root: PathBuf::from("/"),
            batch_size: 2000,
            num_threads: 0,
        }
    }
}

/// Progress counters for an active scan. Atomically updated by the scan thread.
pub struct ScanProgress {
    pub entries_scanned: Arc<AtomicU64>,
    pub dirs_found: Arc<AtomicU64>,
}

impl ScanProgress {
    fn new() -> Self {
        Self {
            entries_scanned: Arc::new(AtomicU64::new(0)),
            dirs_found: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Read current progress snapshot.
    pub fn snapshot(&self) -> (u64, u64) {
        (
            self.entries_scanned.load(Ordering::Relaxed),
            self.dirs_found.load(Ordering::Relaxed),
        )
    }
}

/// Handle returned by `scan_volume` for progress tracking and cancellation.
pub struct ScanHandle {
    pub progress: Arc<ScanProgress>,
    cancelled: Arc<AtomicBool>,
}

impl ScanHandle {
    /// Signal the scan to stop. Already-written data remains in the DB.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}

/// Summary returned when a scan completes (or is cancelled).
#[derive(Debug, Clone)]
pub struct ScanSummary {
    pub total_entries: u64,
    pub total_dirs: u64,
    pub duration_ms: u64,
    pub was_cancelled: bool,
}

/// Errors that can occur during scanning.
#[derive(Debug)]
pub enum ScanError {
    Io(std::io::Error),
    WriterSend(String),
}

impl std::fmt::Display for ScanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ScanError::Io(e) => write!(f, "I/O error: {e}"),
            ScanError::WriterSend(msg) => write!(f, "Writer send failed: {msg}"),
        }
    }
}

impl std::error::Error for ScanError {}

impl From<std::io::Error> for ScanError {
    fn from(err: std::io::Error) -> Self {
        ScanError::Io(err)
    }
}

// ── Public API ───────────────────────────────────────────────────────

/// Start a full-volume scan on a background thread.
///
/// Spawns a `std::thread` that walks the directory tree using jwalk, sends batches
/// of [`ScannedEntry`] to the writer, and triggers `ComputeAllAggregates` on completion.
///
/// Returns a [`ScanHandle`] for progress/cancellation and a [`std::thread::JoinHandle`]
/// for the scan result.
pub fn scan_volume(
    config: ScanConfig,
    writer: &IndexWriter,
) -> Result<(ScanHandle, std::thread::JoinHandle<Result<ScanSummary, ScanError>>), ScanError> {
    let progress = Arc::new(ScanProgress::new());
    let cancelled = Arc::new(AtomicBool::new(false));

    let handle = ScanHandle {
        progress: Arc::clone(&progress),
        cancelled: Arc::clone(&cancelled),
    };

    let writer = writer.clone();
    let thread_handle = std::thread::Builder::new()
        .name("index-scanner".into())
        .spawn(move || {
            let summary = run_scan(
                &config.root,
                &cancelled,
                &progress,
                &writer,
                config.batch_size,
                config.num_threads,
                true, // volume scan: root always maps to ROOT_ID
            );

            // Trigger full aggregation if scan completed without cancellation
            if let Ok(ref s) = summary
                && !s.was_cancelled
                && let Err(e) = writer.send(WriteMessage::ComputeAllAggregates)
            {
                log::warn!("Scanner: failed to send ComputeAllAggregates: {e}");
            }

            summary
        })
        .map_err(ScanError::Io)?;

    Ok((handle, thread_handle))
}

/// Synchronous subtree scan. Runs in the caller's thread.
///
/// Used by micro-scans and `MustScanSubDirs` handling. After scanning, sends
/// `ComputeSubtreeAggregates` to the writer.
pub fn scan_subtree(root: &Path, writer: &IndexWriter, cancelled: &AtomicBool) -> Result<ScanSummary, ScanError> {
    let progress = Arc::new(ScanProgress::new());
    let summary = run_scan(root, cancelled, &progress, writer, 2000, 0, false)?;

    if !summary.was_cancelled {
        let root_str = root.to_string_lossy().to_string();
        if let Err(e) = writer.send(WriteMessage::ComputeSubtreeAggregates { root: root_str }) {
            log::warn!("Scanner: failed to send ComputeSubtreeAggregates: {e}");
        }
    }

    Ok(summary)
}

// ── Core scan logic ──────────────────────────────────────────────────

/// Walk a directory tree and send discovered entries in batches to the writer.
///
/// Uses a `ScanContext` to assign integer IDs and parent IDs to each entry.
/// The context maintains a `HashMap<PathBuf, i64>` mapping directory paths
/// to their assigned IDs. For each entry:
/// 1. Look up parent_id from the map using the entry's parent path
/// 2. Assign `id = next_id; next_id += 1`
/// 3. If directory: add `(full_path, id)` to the map
fn run_scan(
    root: &Path,
    cancelled: &AtomicBool,
    progress: &ScanProgress,
    writer: &IndexWriter,
    batch_size: usize,
    num_threads: usize,
    is_volume_root: bool,
) -> Result<ScanSummary, ScanError> {
    let start = Instant::now();
    let mut batch: Vec<EntryRow> = Vec::with_capacity(batch_size);
    let mut total_entries: u64 = 0;
    let mut total_dirs: u64 = 0;

    // Initialize the scan context: seed root mapping and get next_id from DB.
    // We need a temporary read connection to fetch next_id. The writer thread
    // owns the write connection, but we only need a read here.
    let mut scan_ctx = {
        let db_path = writer.db_path();
        let conn = IndexStore::open_write_connection(&db_path).map_err(|e| ScanError::WriterSend(e.to_string()))?;
        conn.busy_timeout(std::time::Duration::from_secs(5))
            .map_err(|e| ScanError::WriterSend(e.to_string()))?;
        ScanContext::new(&conn, root, is_volume_root).map_err(|e| ScanError::WriterSend(e.to_string()))?
    };

    let walker = build_walker(root, num_threads, is_volume_root);

    for entry_result in walker {
        if cancelled.load(Ordering::Relaxed) {
            // Flush remaining batch before returning
            flush_batch(&mut batch, writer)?;
            return Ok(ScanSummary {
                total_entries,
                total_dirs,
                duration_ms: start.elapsed().as_millis() as u64,
                was_cancelled: true,
            });
        }

        let entry = match entry_result {
            Ok(e) => e,
            Err(e) => {
                log::debug!("Scanner: skipping errored entry: {e}");
                continue;
            }
        };

        // Skip the root entry itself (depth 0) to avoid storing "/" as an entry
        if entry.depth == 0 {
            continue;
        }

        let path = entry.path();
        let path_str = path.to_string_lossy().to_string();

        // For volume-root scans, double-check exclusions on the iteration side
        // (process_read_dir callback prevents descent, but entries can leak through
        // before the rayon callback runs). Subtree scans skip this — the caller
        // explicitly chose the subtree, so global exclusions don't apply.
        if is_volume_root && should_exclude(&path_str) {
            continue;
        }

        // Normalize via firmlinks
        let normalized = firmlinks::normalize_path(&path_str);
        let normalized_path = PathBuf::from(&normalized);

        let is_dir = entry.file_type().is_dir();
        let is_symlink = entry.file_type().is_symlink();

        // Get metadata for size and modified time
        let (size, modified_at) = if is_dir || is_symlink {
            (None, entry_modified_at(&path))
        } else {
            let (sz, mtime) = entry_size_and_mtime(&path);
            (sz, mtime)
        };

        // Look up parent_id from the scan context
        let parent_path = normalized_path.parent().unwrap_or(root);
        let parent_id = match scan_ctx.lookup_parent(parent_path) {
            Some(pid) => pid,
            None => {
                // Parent not in map -- this can happen if the parent was excluded
                // or if jwalk delivered entries out of order. Skip.
                log::debug!("Scanner: parent not found in context for {normalized}, skipping");
                continue;
            }
        };

        // Assign a fresh ID
        let id = scan_ctx.alloc_id();

        // Compute name
        let name = entry.file_name().to_string_lossy().to_string();

        // If directory, register in the scan context so children can find their parent
        if is_dir {
            scan_ctx.register_dir(normalized_path, id);
            total_dirs += 1;
            progress.dirs_found.fetch_add(1, Ordering::Relaxed);
        }

        let scanned = EntryRow {
            id,
            parent_id,
            name,
            is_directory: is_dir,
            is_symlink,
            size,
            modified_at,
        };

        total_entries += 1;
        progress.entries_scanned.fetch_add(1, Ordering::Relaxed);

        batch.push(scanned);
        if batch.len() >= batch_size {
            flush_batch(&mut batch, writer)?;
        }
    }

    // Flush final batch
    flush_batch(&mut batch, writer)?;

    Ok(ScanSummary {
        total_entries,
        total_dirs,
        duration_ms: start.elapsed().as_millis() as u64,
        was_cancelled: false,
    })
}

/// Build the jwalk walker with exclusion filtering in `process_read_dir`.
fn build_walker(root: &Path, num_threads: usize, is_volume_root: bool) -> WalkDir {
    let parallelism = if num_threads == 0 {
        jwalk::Parallelism::RayonNewPool(0)
    } else {
        jwalk::Parallelism::RayonNewPool(num_threads)
    };

    WalkDir::new(root)
        .skip_hidden(false)
        .follow_links(false)
        .sort(false)
        .parallelism(parallelism)
        .process_read_dir(move |_depth, _path, _read_dir_state, children| {
            if !is_volume_root {
                return;
            }
            // Filter out excluded directories to prevent descent into them
            children.retain(|entry_result| {
                if let Ok(entry) = entry_result {
                    let path = entry.path();
                    let path_str = path.to_string_lossy();
                    !should_exclude(&path_str)
                } else {
                    true // Keep errors so they can be logged in the main loop
                }
            });
        })
}

// ── Helpers ──────────────────────────────────────────────────────────

/// Check if a path should be excluded from scanning.
fn should_exclude(path_str: &str) -> bool {
    // Check explicit exclusion prefixes
    for prefix in EXCLUDED_PREFIXES {
        if path_str.starts_with(prefix) {
            return true;
        }
        // Also match exact prefix without trailing slash (for example, "/dev" matches "/dev/")
        let prefix_no_slash = prefix.trim_end_matches('/');
        if path_str == prefix_no_slash {
            return true;
        }
    }

    // macOS: special handling for /System/ -- skip everything except firmlinked paths
    #[cfg(target_os = "macos")]
    if path_str.starts_with("/System/") || path_str == "/System" {
        // Already covered by EXCLUDED_PREFIXES above for /System/Volumes/*
        // For remaining /System/ paths, allow only firmlinked ones
        for allowed in FIRMLINKED_SYSTEM_PREFIXES {
            if path_str.starts_with(allowed) {
                return false;
            }
        }
        return true;
    }

    false
}

/// Get physical file size (st_blocks * 512) and modified time for a file.
#[cfg(unix)]
fn entry_size_and_mtime(path: &Path) -> (Option<u64>, Option<u64>) {
    use std::os::unix::fs::MetadataExt;
    match std::fs::symlink_metadata(path) {
        Ok(meta) => {
            let blocks = meta.blocks();
            let physical_size = if blocks > 0 { blocks * 512 } else { meta.len() };
            let mtime = meta.mtime();
            let mtime_u64 = if mtime >= 0 { Some(mtime as u64) } else { None };
            (Some(physical_size), mtime_u64)
        }
        Err(_) => (None, None),
    }
}

#[cfg(not(unix))]
fn entry_size_and_mtime(path: &Path) -> (Option<u64>, Option<u64>) {
    match std::fs::symlink_metadata(path) {
        Ok(meta) => {
            let size = meta.len();
            let mtime = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs());
            (Some(size), mtime)
        }
        Err(_) => (None, None),
    }
}

/// Get modified time for a directory or symlink entry.
fn entry_modified_at(path: &Path) -> Option<u64> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        std::fs::symlink_metadata(path).ok().and_then(|meta| {
            let mtime = meta.mtime();
            if mtime >= 0 { Some(mtime as u64) } else { None }
        })
    }
    #[cfg(not(unix))]
    {
        std::fs::symlink_metadata(path)
            .ok()
            .and_then(|meta| meta.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
    }
}

/// Compute parent path from a normalized path. No trailing slashes.
///
/// Examples:
/// - `/Users/foo/bar.txt` -> `/Users/foo`
/// - `/Users` -> `/`
/// - `/` -> `` (empty, should not happen since we skip root)
#[cfg(test)]
fn compute_parent_path(path: &str) -> String {
    match path.rfind('/') {
        Some(0) => "/".to_string(),
        Some(pos) => path[..pos].to_string(),
        None => String::new(),
    }
}

/// Send a batch of entries to the writer and clear the batch buffer.
fn flush_batch(batch: &mut Vec<EntryRow>, writer: &IndexWriter) -> Result<(), ScanError> {
    if batch.is_empty() {
        return Ok(());
    }
    let entries = std::mem::take(batch);
    writer
        .send(WriteMessage::InsertEntriesV2(entries))
        .map_err(|e| ScanError::WriterSend(e.to_string()))
}

/// Build the default exclusion list. Public for tests and future configurability.
pub fn default_exclusions() -> Vec<String> {
    EXCLUDED_PREFIXES.iter().map(|s| (*s).to_string()).collect()
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indexing::store::{self, IndexStore, ROOT_ID};
    use crate::indexing::writer::IndexWriter;
    use std::fs;
    use std::thread;
    use std::time::Duration;

    /// Create a temp directory with a known file tree and return the root path.
    fn create_test_tree(dir: &Path) {
        let sub = dir.join("subdir");
        fs::create_dir_all(&sub).unwrap();
        fs::write(dir.join("file1.txt"), "hello world").unwrap();
        fs::write(dir.join("file2.txt"), "more content here").unwrap();
        fs::write(sub.join("nested.txt"), "nested file").unwrap();
        fs::create_dir_all(sub.join("deep")).unwrap();
        fs::write(sub.join("deep").join("leaf.txt"), "leaf").unwrap();
    }

    fn setup_writer() -> (IndexWriter, PathBuf, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let db_path = dir.path().join("test-index.db");
        let _store = IndexStore::open(&db_path).expect("failed to open store");
        let writer = IndexWriter::spawn(&db_path).expect("failed to spawn writer");
        (writer, db_path, dir)
    }

    /// Insert the full parent directory chain for a path into the DB so that
    /// `ScanContext::new` can resolve the subtree root for subtree scans.
    fn ensure_path_in_db(db_path: &Path, path: &Path) {
        let conn = IndexStore::open_write_connection(db_path).unwrap();
        let path_str = path.to_string_lossy();
        let components: Vec<&str> = path_str.split('/').filter(|c| !c.is_empty()).collect();
        let mut parent_id = ROOT_ID;
        for component in components {
            parent_id = match IndexStore::resolve_component(&conn, parent_id, component) {
                Ok(Some(id)) => id,
                _ => IndexStore::insert_entry_v2(&conn, parent_id, component, true, false, None, None).unwrap(),
            };
        }
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn should_exclude_system_volumes() {
        assert!(should_exclude("/System/Volumes/Data/"));
        assert!(should_exclude("/System/Volumes/Data/Users/foo"));
        assert!(should_exclude("/System/Volumes/VM/"));
        assert!(should_exclude("/System/Volumes/Preboot/"));
        assert!(should_exclude("/dev"));
        assert!(should_exclude("/dev/null"));
        assert!(should_exclude("/proc"));
        assert!(should_exclude("/private/var/"));
        assert!(should_exclude("/private/var/folders/xx"));
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn should_exclude_system_except_firmlinked() {
        // Generic /System/ paths should be excluded
        assert!(should_exclude("/System/foo"));
        assert!(should_exclude("/System/Library/Frameworks"));
        assert!(should_exclude("/System"));

        // Firmlinked /System/ paths should NOT be excluded
        assert!(!should_exclude("/System/Library/Caches"));
        assert!(!should_exclude("/System/Library/Caches/com.apple.something"));
        assert!(!should_exclude("/System/Library/Assets"));
        assert!(!should_exclude("/System/Library/Speech"));
        assert!(!should_exclude("/System/Library/Speech/Voices"));
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn should_not_exclude_normal_paths() {
        assert!(!should_exclude("/Users/foo"));
        assert!(!should_exclude("/Users/foo/Documents"));
        assert!(!should_exclude("/Applications"));
        assert!(!should_exclude("/tmp"));
        assert!(!should_exclude("/opt/homebrew"));
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn should_exclude_linux_virtual_filesystems() {
        assert!(should_exclude("/dev"));
        assert!(should_exclude("/dev/null"));
        assert!(should_exclude("/proc"));
        assert!(should_exclude("/proc/1/status"));
        assert!(should_exclude("/sys"));
        assert!(should_exclude("/sys/class/block"));
        assert!(should_exclude("/run"));
        assert!(should_exclude("/run/user/1000"));
        assert!(should_exclude("/snap"));
        assert!(should_exclude("/mnt"));
        assert!(should_exclude("/media"));
        assert!(should_exclude("/boot"));
        assert!(should_exclude("/tmp"));
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn should_not_exclude_linux_normal_paths() {
        assert!(!should_exclude("/home/user"));
        assert!(!should_exclude("/home/user/Documents"));
        assert!(!should_exclude("/usr/local/bin"));
        assert!(!should_exclude("/opt/app"));
        assert!(!should_exclude("/etc/config"));
        assert!(!should_exclude("/var/lib"));
    }

    #[test]
    fn compute_parent_path_cases() {
        assert_eq!(compute_parent_path("/Users/foo/bar.txt"), "/Users/foo");
        assert_eq!(compute_parent_path("/Users"), "/");
        assert_eq!(compute_parent_path("/a/b/c"), "/a/b");
        // "/" returns "/" (top-level slash-only path). In practice, root is skipped (depth 0).
        assert_eq!(compute_parent_path("/"), "/");
    }

    #[test]
    fn scan_temp_directory_tree() {
        let scan_root = tempfile::tempdir().expect("scan root");
        create_test_tree(scan_root.path());

        let (writer, db_path, _db_dir) = setup_writer();

        let config = ScanConfig {
            root: scan_root.path().to_path_buf(),
            batch_size: 100,
            num_threads: 1,
        };

        let (handle, join_handle) = scan_volume(config, &writer).unwrap();
        let summary = join_handle.join().expect("scan thread panicked").unwrap();

        assert!(!summary.was_cancelled);
        // We created: subdir/, file1.txt, file2.txt, subdir/nested.txt, subdir/deep/, subdir/deep/leaf.txt
        assert_eq!(summary.total_entries, 6, "expected 6 entries (2 dirs + 4 files)");
        assert_eq!(summary.total_dirs, 2, "expected 2 directories");
        assert!(summary.duration_ms < 10_000, "scan should complete quickly");

        // Verify progress matches summary
        let (entries, dirs) = handle.progress.snapshot();
        assert_eq!(entries, summary.total_entries);
        assert_eq!(dirs, summary.total_dirs);

        // Wait for writer to process all messages + aggregation
        thread::sleep(Duration::from_millis(500));
        writer.shutdown();
        thread::sleep(Duration::from_millis(100));

        // Verify entries are in the DB using integer-keyed API.
        // The scanner maps the scan root to ROOT_ID, so children are under ROOT_ID.
        let store = IndexStore::open(&db_path).unwrap();
        let children = store.list_children(ROOT_ID).unwrap();
        assert_eq!(
            children.len(),
            3,
            "root should have 3 children: subdir, file1.txt, file2.txt"
        );

        // Verify a file has a non-zero physical size
        let file1 = children.iter().find(|e| e.name == "file1.txt").unwrap();
        assert!(!file1.is_directory);
        assert!(file1.size.unwrap_or(0) > 0, "file should have nonzero physical size");
    }

    #[test]
    fn scan_subtree_only() {
        let scan_root = tempfile::tempdir().expect("scan root");
        create_test_tree(scan_root.path());

        let (writer, db_path, _db_dir) = setup_writer();
        let cancelled = AtomicBool::new(false);

        let subtree_root = scan_root.path().join("subdir");

        // Pre-insert the subtree root's parent chain so ScanContext can resolve it
        ensure_path_in_db(&db_path, &subtree_root);

        let summary = scan_subtree(&subtree_root, &writer, &cancelled).unwrap();

        assert!(!summary.was_cancelled);
        // subdir contains: nested.txt, deep/, deep/leaf.txt
        assert_eq!(summary.total_entries, 3, "expected 3 entries under subdir");
        assert_eq!(summary.total_dirs, 1, "expected 1 directory (deep/)");

        // Wait for writer to process
        thread::sleep(Duration::from_millis(500));
        writer.shutdown();
        thread::sleep(Duration::from_millis(100));

        // The subtree scan resolves the actual entry ID for the subtree root.
        // Children should be listed under that ID, not ROOT_ID.
        let store = IndexStore::open(&db_path).unwrap();
        let conn = store.read_conn();
        let subtree_id = store::resolve_path(conn, &subtree_root.to_string_lossy())
            .unwrap()
            .expect("subtree root should be in DB");
        let children = store.list_children(subtree_id).unwrap();
        assert_eq!(children.len(), 2, "subdir should have 2 children: nested.txt, deep");
    }

    #[test]
    fn scan_cancellation() {
        let scan_root = tempfile::tempdir().expect("scan root");
        create_test_tree(scan_root.path());

        let (writer, _db_path, _db_dir) = setup_writer();

        let config = ScanConfig {
            root: scan_root.path().to_path_buf(),
            batch_size: 1, // Tiny batch so we check cancellation frequently
            num_threads: 1,
        };

        let (handle, join_handle) = scan_volume(config, &writer).unwrap();
        // Cancel immediately
        handle.cancel();

        let summary = join_handle.join().expect("scan thread panicked").unwrap();
        assert!(summary.was_cancelled);

        writer.shutdown();
    }

    #[test]
    fn scan_empty_directory() {
        let scan_root = tempfile::tempdir().expect("scan root");
        let (writer, _db_path, _db_dir) = setup_writer();

        let config = ScanConfig {
            root: scan_root.path().to_path_buf(),
            batch_size: 100,
            num_threads: 1,
        };

        let (_handle, join_handle) = scan_volume(config, &writer).unwrap();
        let summary = join_handle.join().expect("scan thread panicked").unwrap();

        assert!(!summary.was_cancelled);
        assert_eq!(summary.total_entries, 0);
        assert_eq!(summary.total_dirs, 0);

        writer.shutdown();
    }

    #[test]
    #[cfg(unix)]
    fn physical_size_is_captured() {
        let scan_root = tempfile::tempdir().expect("scan root");
        // Write a file with known content
        let content = vec![0u8; 8192]; // 8KB, should allocate at least one block
        fs::write(scan_root.path().join("sized.bin"), &content).unwrap();

        let (writer, db_path, _db_dir) = setup_writer();

        let config = ScanConfig {
            root: scan_root.path().to_path_buf(),
            batch_size: 100,
            num_threads: 1,
        };

        let (_handle, join_handle) = scan_volume(config, &writer).unwrap();
        let _summary = join_handle.join().expect("scan thread panicked").unwrap();

        thread::sleep(Duration::from_millis(300));
        writer.shutdown();
        thread::sleep(Duration::from_millis(100));

        let store = IndexStore::open(&db_path).unwrap();
        let children = store.list_children(ROOT_ID).unwrap();
        let sized = children.iter().find(|e| e.name == "sized.bin").unwrap();

        // Physical size should be >= logical size (and a multiple of 512)
        let phys = sized.size.unwrap();
        assert!(phys >= 8192, "physical size ({phys}) should be >= logical size (8192)");
        assert_eq!(phys % 512, 0, "physical size should be a multiple of 512");
    }

    #[test]
    fn scan_handles_symlinks() {
        let scan_root = tempfile::tempdir().expect("scan root");
        fs::write(scan_root.path().join("real.txt"), "real content").unwrap();

        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(scan_root.path().join("real.txt"), scan_root.path().join("link.txt")).unwrap();
        }

        let (writer, db_path, _db_dir) = setup_writer();

        let config = ScanConfig {
            root: scan_root.path().to_path_buf(),
            batch_size: 100,
            num_threads: 1,
        };

        let (_handle, join_handle) = scan_volume(config, &writer).unwrap();
        let _summary = join_handle.join().expect("scan thread panicked").unwrap();

        thread::sleep(Duration::from_millis(300));
        writer.shutdown();
        thread::sleep(Duration::from_millis(100));

        let store = IndexStore::open(&db_path).unwrap();
        let children = store.list_children(ROOT_ID).unwrap();

        #[cfg(unix)]
        {
            assert_eq!(children.len(), 2);
            let link = children.iter().find(|e| e.name == "link.txt").unwrap();
            assert!(link.is_symlink, "symlink should be marked as symlink");
            assert!(!link.is_directory);
        }
    }

    #[test]
    fn default_exclusions_populated() {
        let exclusions = default_exclusions();
        assert!(!exclusions.is_empty());
        #[cfg(target_os = "macos")]
        assert!(exclusions.iter().any(|e| e.contains("System/Volumes/Data")));
        #[cfg(target_os = "linux")]
        assert!(exclusions.iter().any(|e| e.contains("/proc")));
    }

    #[test]
    fn scan_assigns_integer_ids() {
        // Verify that the scanner correctly assigns integer IDs and parent IDs
        let scan_root = tempfile::tempdir().expect("scan root");
        create_test_tree(scan_root.path());

        let (writer, db_path, _db_dir) = setup_writer();

        let config = ScanConfig {
            root: scan_root.path().to_path_buf(),
            batch_size: 100,
            num_threads: 1,
        };

        let (_handle, join_handle) = scan_volume(config, &writer).unwrap();
        let _summary = join_handle.join().expect("scan thread panicked").unwrap();

        thread::sleep(Duration::from_millis(500));
        writer.shutdown();
        thread::sleep(Duration::from_millis(100));

        let store = IndexStore::open(&db_path).unwrap();

        // All top-level entries should have parent_id = ROOT_ID
        let top_children = store.list_children(ROOT_ID).unwrap();
        assert_eq!(top_children.len(), 3); // subdir, file1.txt, file2.txt

        for child in &top_children {
            assert_eq!(child.parent_id, ROOT_ID);
            assert!(child.id > ROOT_ID, "all IDs should be > ROOT_ID");
        }

        // Find the subdir entry and check its children
        let subdir = top_children.iter().find(|e| e.name == "subdir").unwrap();
        assert!(subdir.is_directory);
        let subdir_children = store.list_children(subdir.id).unwrap();
        assert_eq!(subdir_children.len(), 2); // nested.txt, deep

        for child in &subdir_children {
            assert_eq!(child.parent_id, subdir.id, "children should reference parent's ID");
        }

        // Find the deep directory and check its children
        let deep = subdir_children.iter().find(|e| e.name == "deep").unwrap();
        assert!(deep.is_directory);
        let deep_children = store.list_children(deep.id).unwrap();
        assert_eq!(deep_children.len(), 1); // leaf.txt
        assert_eq!(deep_children[0].name, "leaf.txt");
        assert_eq!(deep_children[0].parent_id, deep.id);
    }

    #[test]
    fn scan_context_id_allocation() {
        // Verify ScanContext properly assigns monotonically increasing IDs
        let dir = tempfile::tempdir().expect("temp dir");
        let db_path = dir.path().join("test-ctx.db");
        let _store = IndexStore::open(&db_path).expect("open store");
        let conn = IndexStore::open_write_connection(&db_path).unwrap();

        let root_path = Path::new("/test/root");
        let mut ctx = ScanContext::new(&conn, root_path, true).unwrap();

        // Root sentinel exists (id=1), so next_id should be >= 2
        assert!(ctx.next_id >= 2);

        let id1 = ctx.alloc_id();
        let id2 = ctx.alloc_id();
        let id3 = ctx.alloc_id();
        assert_eq!(id2, id1 + 1);
        assert_eq!(id3, id2 + 1);

        // Volume root → maps to ROOT_ID
        assert_eq!(ctx.lookup_parent(root_path), Some(ROOT_ID));

        // Register a directory and look it up
        let dir_path = PathBuf::from("/test/root/mydir");
        ctx.register_dir(dir_path.clone(), id1);
        assert_eq!(ctx.lookup_parent(&dir_path), Some(id1));

        // Unknown path returns None
        assert_eq!(ctx.lookup_parent(Path::new("/unknown")), None);
    }

    #[test]
    fn scan_context_subtree_resolves_actual_id() {
        // When the subtree root exists in the DB, ScanContext should use its actual ID
        let dir = tempfile::tempdir().expect("temp dir");
        let db_path = dir.path().join("test-ctx-subtree.db");
        let _store = IndexStore::open(&db_path).expect("open store");
        let conn = IndexStore::open_write_connection(&db_path).unwrap();

        // Insert a directory chain: ROOT → Volumes → "NO NAME"
        let volumes_id = IndexStore::insert_entry_v2(&conn, ROOT_ID, "Volumes", true, false, None, None).unwrap();
        let noname_id = IndexStore::insert_entry_v2(&conn, volumes_id, "NO NAME", true, false, None, None).unwrap();
        assert_ne!(noname_id, ROOT_ID);

        // Create ScanContext for the subtree root
        let subtree_root = Path::new("/Volumes/NO NAME");
        let ctx = ScanContext::new(&conn, subtree_root, false).unwrap();

        // Should resolve to the actual entry ID, NOT ROOT_ID
        assert_eq!(ctx.lookup_parent(subtree_root), Some(noname_id));
    }
}
