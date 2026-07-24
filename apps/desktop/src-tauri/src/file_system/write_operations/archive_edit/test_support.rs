//! Shared helpers and common re-exports for the archive-edit test modules. Each
//! `*_tests.rs` file does `use super::*;` (the module's public routes) plus
//! `use super::test_support::*;` (these helpers and the common types), so the test
//! bodies read the same as when they lived in one file.

use uuid::Uuid;

// Common re-exports the test bodies reference by bare name (they also serve
// test_support's own helpers below).
pub(super) use std::io::{Read, Write};
pub(super) use std::path::{Path, PathBuf};
pub(super) use std::sync::Arc;
pub(super) use std::time::Duration;

pub(super) use super::super::OperationEventSink;
pub(super) use super::super::manager::OperationSummaryText;
pub(super) use super::super::types::{CollectorEventSink, ConflictResolution, WriteOperationError, WriteOperationType};
pub(super) use crate::file_system::get_volume_manager;
pub(super) use crate::file_system::volume::Volume;
pub(super) use crate::file_system::volume::backends::archive::mutator::Changeset;
pub(super) use crate::ignore_poison::IgnorePoison;
pub(super) use crate::test_support::wait_until_async;
pub(super) use zip::write::SimpleFileOptions;
pub(super) use zip::{ZipArchive, ZipWriter};

/// Builds a one-entry zip at `path`.
pub(super) fn write_simple_zip(path: &Path, entry: &str, content: &[u8]) {
    let file = std::fs::File::create(path).expect("create zip");
    let mut writer = ZipWriter::new(file);
    writer
        .start_file(entry, SimpleFileOptions::default())
        .expect("start entry");
    writer.write_all(content).expect("write entry");
    writer.finish().expect("finish zip");
}

/// Builds a multi-entry zip at `path`.
pub(super) fn write_multi_zip(path: &Path, entries: &[(&str, &[u8])]) {
    let file = std::fs::File::create(path).expect("create zip");
    let mut writer = ZipWriter::new(file);
    for (name, content) in entries {
        writer.start_file(*name, SimpleFileOptions::default()).expect("start");
        writer.write_all(content).expect("write");
    }
    writer.finish().expect("finish zip");
}

/// Reads one entry's decompressed bytes back, or `None` if absent.
pub(super) fn read_entry(path: &Path, name: &str) -> Option<Vec<u8>> {
    let file = std::fs::File::open(path).ok()?;
    let mut archive = ZipArchive::new(file).ok()?;
    let mut entry = archive.by_name(name).ok()?;
    let mut buf = Vec::new();
    entry.read_to_end(&mut buf).ok()?;
    Some(buf)
}

/// A unique id giving one archive-edit test op its OWN operation-manager lane,
/// instead of a lane shared with other concurrently-running tests. Used as a
/// LOCAL parent-drive id (an unregistered id resolves to a local in-place edit,
/// exactly like the production `"root"` disk, so its fallback lane is the id
/// itself), or as an explicit `with_lane_key` on a test volume.
///
/// This is for test ISOLATION and parallel speed, mirroring the manager's own
/// `tests.rs` ("unique operation ids + lane keys"): the operation manager is a
/// process-global singleton, so a shared global lane (like `"root"`, or an
/// `InMemoryVolume`'s default root-`/` lane) serializes otherwise-unrelated tests
/// and couples their timing. Keep new local tests on this, NOT `"root"`. It is
/// NOT the orphan-safety mechanism — that's the manager spawning admitted ops on
/// the app runtime (`lifecycle/manager.rs` § the admission pass), which holds regardless of
/// lanes.
pub(super) fn unique_lane_id() -> String {
    format!("test-lane-{}", Uuid::new_v4())
}

/// A real zip as in-memory bytes, for seeding a remote parent's store.
pub(super) fn zip_bytes(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut cursor = std::io::Cursor::new(Vec::new());
    {
        let mut writer = ZipWriter::new(&mut cursor);
        for (name, content) in entries {
            writer.start_file(*name, SimpleFileOptions::default()).expect("start");
            writer.write_all(content).expect("write");
        }
        writer.finish().expect("finish");
    }
    cursor.into_inner()
}

/// Registers a NON-local `InMemoryVolume` (the remote-parent stand-in) holding a
/// zip at `archive_path`, under a unique volume id. Unregister with
/// `get_volume_manager().unregister(&id)` when done.
pub(super) async fn register_remote_zip(
    archive_path: &Path,
    entries: &[(&str, &[u8])],
) -> (String, Arc<crate::file_system::volume::InMemoryVolume>) {
    use crate::file_system::volume::InMemoryVolume;
    let id = format!("remote-test-{}", Uuid::new_v4());
    // The lane key is the unique volume id so each remote-parent op gets its own
    // operation-manager lane. An `InMemoryVolume` otherwise defaults its lane to
    // its root `/`, shared across every instance — and a copy-into reserves the
    // parent's lane, so two such ops would serialize otherwise-unrelated tests
    // (see `unique_lane_id` for the isolation rationale).
    let parent = InMemoryVolume::new("Remote").with_lane_key(id.clone());
    if let Some(dir) = archive_path.parent() {
        parent.create_directory(dir).await.expect("seed parent dir");
    }
    parent
        .create_file(archive_path, &zip_bytes(entries))
        .await
        .expect("seed remote zip");
    let parent = Arc::new(parent);
    get_volume_manager().register(&id, Arc::clone(&parent) as Arc<dyn Volume>);
    (id, parent)
}

/// Registers a NON-local `InMemoryVolume` (a remote SOURCE stand-in — MTP / SMB)
/// pre-populated with `files` (each `(relative_path, bytes)`), creating every
/// parent directory the paths imply. Returns the volume id and the handle;
/// unregister with `get_volume_manager().unregister(&id)` when done.
pub(super) async fn register_remote_source(
    files: &[(&str, &[u8])],
) -> (String, Arc<crate::file_system::volume::InMemoryVolume>) {
    use crate::file_system::volume::InMemoryVolume;
    let source = InMemoryVolume::new("RemoteSource");
    for (rel, bytes) in files {
        let path = PathBuf::from("/").join(rel);
        if let Some(parent) = path.parent() {
            let mut acc = PathBuf::from("/");
            for comp in parent.strip_prefix("/").unwrap_or(parent).components() {
                acc = acc.join(comp);
                // Ignore "already exists" for shared ancestors across files.
                let _ = source.create_directory(&acc).await;
            }
        }
        source.create_file(&path, bytes).await.expect("seed remote source file");
    }
    let source = Arc::new(source);
    let id = format!("remote-source-{}", Uuid::new_v4());
    get_volume_manager().register(&id, Arc::clone(&source) as Arc<dyn Volume>);
    (id, source)
}

/// Streams the archive back out of a (remote) parent and returns one entry's bytes.
pub(super) async fn read_remote_entry(parent: &dyn Volume, archive_path: &Path, name: &str) -> Option<Vec<u8>> {
    let mut stream = parent.open_read_stream(archive_path).await.ok()?;
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next_chunk().await {
        bytes.extend_from_slice(&chunk.ok()?);
    }
    let mut archive = ZipArchive::new(std::io::Cursor::new(bytes)).ok()?;
    let mut entry = archive.by_name(name).ok()?;
    let mut buf = Vec::new();
    entry.read_to_end(&mut buf).ok()?;
    Some(buf)
}
