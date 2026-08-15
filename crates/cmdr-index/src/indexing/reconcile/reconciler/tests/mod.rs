//! Reconciler tests, split by theme. This module holds what every theme needs:
//! the event builders, the FSEvents flag sets, and the writer/DB fixtures
//! (`setup_test_writer` and friends, which `rescan/settle.rs` also imports).
//! The themes are the sibling modules below.

use super::*;
use crate::indexing::store::{IndexStore, ROOT_ID};
use crate::indexing::stress_test_helpers::{TestInstanceGuard, check_db_consistency};
use crate::indexing::volume::IndexVolumeKind;
use crate::indexing::watch::watcher::FsEventFlags;
use std::os::unix::ffi::OsStrExt;
use std::time::Duration;

mod bulk_window;
mod directory_read;
mod hardlinks;
mod live_events;
mod must_scan_routing;
mod replay;
mod subtree;

fn make_event(path: &str, event_id: u64, flags: FsEventFlags) -> FsChangeEvent {
    FsChangeEvent {
        path: path.to_string(),
        event_id,
        flags,
    }
}

fn created_file_flags() -> FsEventFlags {
    FsEventFlags {
        item_created: true,
        item_is_file: true,
        ..Default::default()
    }
}

fn removed_file_flags() -> FsEventFlags {
    FsEventFlags {
        item_removed: true,
        item_is_file: true,
        ..Default::default()
    }
}

fn modified_file_flags() -> FsEventFlags {
    FsEventFlags {
        item_modified: true,
        item_is_file: true,
        ..Default::default()
    }
}

fn created_dir_flags() -> FsEventFlags {
    FsEventFlags {
        item_created: true,
        item_is_dir: true,
        ..Default::default()
    }
}

fn removed_dir_flags() -> FsEventFlags {
    FsEventFlags {
        item_removed: true,
        item_is_dir: true,
        ..Default::default()
    }
}

fn history_done_flags() -> FsEventFlags {
    FsEventFlags {
        history_done: true,
        ..Default::default()
    }
}

// ── Test helpers ─────────────────────────────────────────────────

/// Set up a writer and a read connection for tests.
pub(super) fn setup_test_writer() -> (IndexWriter, tempfile::TempDir, Connection) {
    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("test-reconciler.db");
    let _store = IndexStore::open(&db_path).expect("open store");
    let writer = IndexWriter::spawn(&db_path, crate::NoopEventSink::shared()).expect("spawn writer");
    let conn = IndexStore::open_write_connection(&db_path).expect("open WAL conn for reads");
    (writer, dir, conn)
}

/// Set up a NON-root writer + a private per-volume `IndexInstance` for the
/// hourglass-hold routing tests, so the reconciler's `hold_rescan` routes to a
/// PRIVATE tracker (`get_pending_sizes_for(volume_id)`) immune to foreign root
/// writers clearing the process-global root `PENDING_SIZES` mid-assertion (the
/// isolation flake). Pair with `EventReconciler::new_for(volume_id,
/// IndexPathSpace::root())`: the ROOT path space keeps `is_boot_disk()` true, so
/// the shallow once-a-day sweep-window semantics are unchanged; only the volume
/// id is private. The writer is spawned NON-root for the same id so ITS
/// end-of-drain clear also targets the private tracker, never the shared global.
fn setup_private_writer(volume_id: &str) -> (IndexWriter, tempfile::TempDir, Connection, TestInstanceGuard) {
    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("test-reconciler.db");
    let _store = IndexStore::open(&db_path).expect("open store");
    let writer = IndexWriter::spawn_for(&db_path, crate::NoopEventSink::shared(), false, volume_id.to_string())
        .expect("spawn writer");
    let conn = IndexStore::open_write_connection(&db_path).expect("open WAL conn for reads");
    let instance = TestInstanceGuard::register(volume_id, &db_path, IndexVolumeKind::Smb);
    (writer, dir, conn, instance)
}

/// Ensure all components of an absolute path exist in the DB as directory entries.
///
/// Walks from root downward, inserting each missing component. This simulates
/// what the full scan does in production: all directories are indexed before
/// live events arrive. Also syncs the writer's shared `next_id` counter.
pub(super) fn ensure_path_in_db(db_path: &Path, abs_path: &str, writer: &IndexWriter) {
    let conn = IndexStore::open_write_connection(db_path).unwrap();
    let components: Vec<&str> = abs_path
        .strip_prefix('/')
        .unwrap_or(abs_path)
        .split('/')
        .filter(|c| !c.is_empty())
        .collect();

    let mut current_id = ROOT_ID;
    for component in components {
        match IndexStore::resolve_component(&conn, current_id, component).unwrap() {
            Some(id) => current_id = id,
            None => {
                current_id =
                    IndexStore::insert_entry_v2(&conn, current_id, component, true, false, None, None, None, None)
                        .unwrap();
            }
        }
    }
    // Sync the writer's next_id counter with what we just inserted
    let db_next_id = IndexStore::get_next_id(&conn).unwrap();
    writer.next_id().fetch_max(db_next_id, Ordering::Relaxed);
}

/// Create a temp directory outside indexing-excluded paths.
/// On Linux, `/tmp/` is excluded from indexing; use the current directory instead.
pub(super) fn non_excluded_tempdir() -> tempfile::TempDir {
    // Create in CWD instead of /tmp/ to avoid:
    // - Linux: /tmp/ is in EXCLUDED_PREFIXES
    // - macOS: /tmp is a symlink to /private/tmp, causing path mismatches with normalize_path() which
    //   resolves /tmp → /private/tmp
    tempfile::Builder::new()
        .prefix("cmdr_test_")
        .tempdir_in(std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
        .expect("tempdir in cwd")
}
