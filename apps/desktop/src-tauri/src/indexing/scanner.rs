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
use crate::indexing::store::ScannedEntry;
use crate::indexing::writer::{IndexWriter, WriteMessage};

// ── Exclusion prefixes ──────────────────────────────────────────────

/// Absolute path prefixes to skip during scanning. See `docs/specs/drive-indexing/plan.md`.
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

/// `/System/` paths that are reachable via firmlinks (from `/usr/share/firmlinks`).
/// These are the ONLY `/System/` subdirectories we allow through the exclusion filter.
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

    /// Check whether cancellation has been requested.
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
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
    Cancelled,
}

impl std::fmt::Display for ScanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ScanError::Io(e) => write!(f, "I/O error: {e}"),
            ScanError::WriterSend(msg) => write!(f, "Writer send failed: {msg}"),
            ScanError::Cancelled => write!(f, "Scan cancelled"),
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
    let summary = run_scan(root, cancelled, &progress, writer, 2000, 0)?;

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
fn run_scan(
    root: &Path,
    cancelled: &AtomicBool,
    progress: &ScanProgress,
    writer: &IndexWriter,
    batch_size: usize,
    num_threads: usize,
) -> Result<ScanSummary, ScanError> {
    let start = Instant::now();
    let mut batch: Vec<ScannedEntry> = Vec::with_capacity(batch_size);
    let mut total_entries: u64 = 0;
    let mut total_dirs: u64 = 0;

    let root_str = root.to_string_lossy().to_string();
    let is_volume_root = root_str == "/";

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

        // For subtree scans, we still need to check exclusions on the iteration side
        // (process_read_dir handles it for children, but the walker might still yield
        // entries that got through before the callback ran)
        if !is_volume_root && should_exclude(&path_str) {
            continue;
        }

        // Normalize via firmlinks
        let normalized = firmlinks::normalize_path(&path_str);

        let is_dir = entry.file_type().is_dir();
        let is_symlink = entry.file_type().is_symlink();

        // Get metadata for size and modified time
        let (size, modified_at) = if is_dir || is_symlink {
            (None, entry_modified_at(&path))
        } else {
            let (sz, mtime) = entry_size_and_mtime(&path);
            (sz, mtime)
        };

        // Compute parent path (no trailing slash, consistent with store.rs conventions)
        let parent = compute_parent_path(&normalized);

        // Compute name
        let name = entry.file_name().to_string_lossy().to_string();

        let scanned = ScannedEntry {
            path: normalized,
            parent_path: parent,
            name,
            is_directory: is_dir,
            is_symlink,
            size,
            modified_at,
        };

        if is_dir {
            total_dirs += 1;
            progress.dirs_found.fetch_add(1, Ordering::Relaxed);
        }
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

    // Special handling for /System/: skip everything except firmlinked paths
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
fn compute_parent_path(path: &str) -> String {
    match path.rfind('/') {
        Some(0) => "/".to_string(),
        Some(pos) => path[..pos].to_string(),
        None => String::new(),
    }
}

/// Send a batch of entries to the writer and clear the batch buffer.
fn flush_batch(batch: &mut Vec<ScannedEntry>, writer: &IndexWriter) -> Result<(), ScanError> {
    if batch.is_empty() {
        return Ok(());
    }
    let entries = std::mem::take(batch);
    writer
        .send(WriteMessage::InsertEntries(entries))
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
    use crate::indexing::store::IndexStore;
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

    #[test]
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
    fn should_not_exclude_normal_paths() {
        assert!(!should_exclude("/Users/foo"));
        assert!(!should_exclude("/Users/foo/Documents"));
        assert!(!should_exclude("/Applications"));
        assert!(!should_exclude("/tmp"));
        assert!(!should_exclude("/opt/homebrew"));
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

        // Verify entries are in the DB
        let store = IndexStore::open(&db_path).unwrap();
        let root_str = scan_root.path().to_string_lossy().to_string();
        let children = store.list_entries_by_parent(&root_str).unwrap();
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
        let summary = scan_subtree(&subtree_root, &writer, &cancelled).unwrap();

        assert!(!summary.was_cancelled);
        // subdir contains: nested.txt, deep/, deep/leaf.txt
        assert_eq!(summary.total_entries, 3, "expected 3 entries under subdir");
        assert_eq!(summary.total_dirs, 1, "expected 1 directory (deep/)");

        // Wait for writer to process
        thread::sleep(Duration::from_millis(500));
        writer.shutdown();
        thread::sleep(Duration::from_millis(100));

        let store = IndexStore::open(&db_path).unwrap();
        let subdir_str = subtree_root.to_string_lossy().to_string();
        let children = store.list_entries_by_parent(&subdir_str).unwrap();
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
        let root_str = scan_root.path().to_string_lossy().to_string();
        let children = store.list_entries_by_parent(&root_str).unwrap();
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
        let root_str = scan_root.path().to_string_lossy().to_string();
        let children = store.list_entries_by_parent(&root_str).unwrap();

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
        assert!(exclusions.iter().any(|e| e.contains("System/Volumes/Data")));
    }
}
