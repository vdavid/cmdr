//! Fixtures the scanner's test modules share: a writer over a temp DB, a real
//! temp tree, and the mock reader that lets a test decide exactly which
//! directory reads succeed, fail, or cancel the walk.
//!
//! Split out so `tests.rs` (the walker's own behavior) and `convergence_tests.rs`
//! (what a cancelled walk leaves behind) build their trees the same way.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Condvar, Mutex};

use tokio_util::sync::CancellationToken;

use cmdr_fs::ignore_poison::IgnorePoison;

use super::walker::{InlineStat, RawDirEntry, RawFileType, ReadDirFn, ReadProgress};
use crate::indexing::store::{IndexStore, ROOT_ID};
use crate::indexing::writer::IndexWriter;

/// Create a temp dir for volume-scan tests. On Linux, `/tmp/` is in the exclusion list,
/// so we use the current directory to avoid false rejections.
pub(super) fn scan_test_tempdir() -> tempfile::TempDir {
    // Create in CWD instead of /tmp/ to avoid:
    // - Linux: /tmp/ is in EXCLUDED_PREFIXES
    // - macOS: /tmp is a symlink to /private/tmp, causing path mismatches with normalize_path() which
    //   resolves /tmp → /private/tmp
    tempfile::Builder::new()
        .prefix("cmdr-scan-test-")
        .tempdir_in(std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
        .expect("failed to create temp dir in cwd")
}

/// Create a temp directory with a known file tree and return the root path.
pub(super) fn create_test_tree(dir: &Path) {
    let sub = dir.join("subdir");
    std::fs::create_dir_all(&sub).unwrap();
    std::fs::write(dir.join("file1.txt"), "hello world").unwrap();
    std::fs::write(dir.join("file2.txt"), "more content here").unwrap();
    std::fs::write(sub.join("nested.txt"), "nested file").unwrap();
    std::fs::create_dir_all(sub.join("deep")).unwrap();
    std::fs::write(sub.join("deep").join("leaf.txt"), "leaf").unwrap();
}

pub(super) fn setup_writer() -> (IndexWriter, PathBuf, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    let db_path = dir.path().join("test-index.db");
    let _store = IndexStore::open(&db_path).expect("failed to open store");
    let writer = IndexWriter::spawn(&db_path, crate::NoopEventSink::shared()).expect("failed to spawn writer");
    (writer, db_path, dir)
}

/// Insert the full parent directory chain for a path into the DB so that
/// `ScanContext::new` can resolve the subtree root for subtree scans.
/// Also syncs the writer's shared `next_id` counter with the DB.
pub(super) fn ensure_path_in_db(db_path: &Path, path: &Path, writer: &IndexWriter) {
    let conn = IndexStore::open_write_connection(db_path).unwrap();
    let path_str = path.to_string_lossy();
    let components: Vec<&str> = path_str.split('/').filter(|c| !c.is_empty()).collect();
    let mut parent_id = ROOT_ID;
    for component in components {
        parent_id = match IndexStore::resolve_component(&conn, parent_id, component) {
            Ok(Some(id)) => id,
            _ => IndexStore::insert_entry_v2(&conn, parent_id, component, true, false, None, None, None, None).unwrap(),
        };
    }
    // Sync the writer's next_id counter with what we just inserted
    let db_next_id = IndexStore::get_next_id(&conn).unwrap();
    writer.next_id().fetch_max(db_next_id, Ordering::Relaxed);
}

// ── A tree that only exists in the reader ────────────────────────────

/// One child of a mock directory.
pub(super) struct MockChild {
    pub name: &'static str,
    pub file_type: RawFileType,
    pub size: u64,
}

/// A file child of the given size.
pub(super) fn file(name: &'static str, size: u64) -> MockChild {
    MockChild {
        name,
        file_type: RawFileType::File,
        size,
    }
}

/// A directory child.
pub(super) fn dir(name: &'static str) -> MockChild {
    MockChild {
        name,
        file_type: RawFileType::Dir,
        size: 0,
    }
}

/// A read the test holds open until it says otherwise.
///
/// What it buys is a walk that is genuinely still running while the test looks at
/// what it has emitted so far — the only honest way to ask "does a consumer see
/// rows before the walk ends?". A condvar rather than a sleep: the test decides
/// when the read completes, so nothing depends on how fast the machine is.
pub(super) struct ReadGate {
    open: Mutex<bool>,
    wake: Condvar,
}

impl ReadGate {
    pub(super) fn closed() -> Arc<Self> {
        Arc::new(Self {
            open: Mutex::new(false),
            wake: Condvar::new(),
        })
    }

    /// Let the parked read finish.
    pub(super) fn open(&self) {
        *self.open.lock_ignore_poison() = true;
        self.wake.notify_all();
    }

    fn wait(&self) {
        let mut open = self.open.lock_ignore_poison();
        while !*open {
            open = self.wake.wait(open).unwrap_or_else(|e| e.into_inner());
        }
    }
}

/// A directory tree that lives only in the reader, so a test can decide which
/// reads succeed and when the walk gets cancelled without racing a real
/// filesystem.
///
/// Every child carries an inline stat, so the rows the visitor writes have real
/// sizes and the aggregates a test asserts on are the ones a real walk produces.
#[derive(Default)]
pub(super) struct MockTree {
    dirs: HashMap<PathBuf, Vec<MockChild>>,
    /// Reading this path cancels the walk and reports a read error, so the
    /// directory stays honestly unlisted — what a user pressing Esc looks like
    /// from inside the walker.
    cancel_on_read: Option<PathBuf>,
    /// Reading these paths fails with permission denied.
    denied: Vec<PathBuf>,
    /// Reading this path parks until the test opens the gate.
    gated: Option<(PathBuf, Arc<ReadGate>)>,
}

impl MockTree {
    pub(super) fn new() -> Self {
        Self::default()
    }

    /// Declare one directory's children.
    pub(super) fn dir_at(mut self, path: impl Into<PathBuf>, children: Vec<MockChild>) -> Self {
        self.dirs.insert(path.into(), children);
        self
    }

    /// The read that stops the walk: it cancels `cancel` and then reports a read
    /// error, so nothing marks this directory listed.
    pub(super) fn cancel_when_reading(mut self, path: impl Into<PathBuf>) -> Self {
        self.cancel_on_read = Some(path.into());
        self
    }

    /// A directory the walk is allowed to find but not to read.
    #[allow(
        dead_code,
        reason = "the unreadable-marking anchors use it; kept beside its siblings"
    )]
    pub(super) fn denied_at(mut self, path: impl Into<PathBuf>) -> Self {
        self.denied.push(path.into());
        self
    }

    /// A directory whose read parks until `gate` opens, so the walk is provably
    /// still running while the test inspects what it has emitted.
    pub(super) fn gated_at(mut self, path: impl Into<PathBuf>, gate: &Arc<ReadGate>) -> Self {
        self.gated = Some((path.into(), Arc::clone(gate)));
        self
    }

    /// The reader `run_scan` drives, wired to `cancel`.
    pub(super) fn reader(self, cancel: &CancellationToken) -> ReadDirFn {
        let MockTree {
            dirs,
            cancel_on_read,
            denied,
            gated,
        } = self;
        let dirs = Arc::new(dirs);
        let cancel = cancel.clone();
        Arc::new(move |path: &Path, progress: &ReadProgress| {
            if cancel_on_read.as_deref() == Some(path) {
                cancel.cancel();
                return Err(std::io::Error::other("cancelled mid-walk (test)"));
            }
            if let Some((gated_path, gate)) = gated.as_ref()
                && gated_path == path
            {
                gate.wait();
            }
            if denied.iter().any(|d| d == path) {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "permission denied (test)",
                ));
            }
            match dirs.get(path) {
                Some(children) => Ok(children
                    .iter()
                    .map(|child| {
                        progress.record_entries(1);
                        RawDirEntry {
                            path: path.join(child.name),
                            file_type: child.file_type,
                            stat: Some(InlineStat {
                                logical_size: child.size,
                                physical_size: child.size,
                                modified_at: Some(1_700_000_000),
                                inode: 0,
                                nlink: 1,
                            }),
                        }
                    })
                    .collect()),
                None => Err(std::io::Error::new(std::io::ErrorKind::NotFound, "no mock dir")),
            }
        })
    }
}
