//! The landing paths the plain copy and move cells above don't walk: an extract
//! out of a zip and out of a compressed tar, an overwrite and a folder merge
//! through the volume engine, a merge through the local copy, a run stopped after
//! its first item, and a destination whose node read fails.
//!
//! On each, a landed file's row records the node that's there once the run ends
//! (or none), a folder that was already there claims no node, and the run ends
//! the way it would with nothing journaled: reading the node never fails or
//! redirects an operation.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use super::*;
use crate::event_sinks::{OperationEventSink, SinkDecorator};
use crate::forward_volume_methods;
use crate::state::OperationIntent;
use crate::types::{WriteOperationError, WriteSourceItemDoneEvent};

use cmdr_archive::{ArchiveFormat, ArchiveVolume, TarCodec};
use cmdr_fs::volume::host::VolumeHost;
use cmdr_fs::volume::{MountClass, VolumeError};

// ── Extract ──────────────────────────────────────────────────────────────────

/// An `ArchiveVolume` over the archive at `path`, read through a local-backed
/// parent the way the app opens one on a local disk.
fn archive_at(path: &Path, format: ArchiveFormat) -> Arc<dyn Volume> {
    let parent: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Parent").with_local_fs_access());
    Arc::new(ArchiveVolume::new(
        parent,
        path.to_path_buf(),
        format,
        VolumeHost::detached(),
    ))
}

fn write_zip(path: &Path, files: &[(&str, &[u8])]) {
    use std::io::Write;
    let mut writer = zip::ZipWriter::new(std::fs::File::create(path).expect("create zip"));
    let options = zip::write::SimpleFileOptions::default();
    for (name, bytes) in files {
        writer.start_file(*name, options).expect("start entry");
        writer.write_all(bytes).expect("write entry");
    }
    writer.finish().expect("finish zip");
}

fn write_tar_gz(path: &Path, files: &[(&str, &[u8])]) {
    use std::io::Write;
    let mut builder = tar::Builder::new(Vec::new());
    for (name, bytes) in files {
        let mut header = tar::Header::new_ustar();
        header.set_size(bytes.len() as u64);
        header.set_mode(0o644);
        header.set_mtime(OLD as u64);
        header.set_entry_type(tar::EntryType::Regular);
        builder.append_data(&mut header, name, *bytes).expect("append entry");
    }
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    gz.write_all(&builder.into_inner().expect("finish tar")).expect("gzip");
    std::fs::write(path, gz.finish().expect("finish gzip")).expect("write archive");
}

/// Each landed file's row names its node, and its source is the entry's path
/// inside the archive: the shape a reader tells an extract from a copy by.
fn assert_extracted(log: &TestOperationLog, archive: &Path, landed: &[PathBuf]) {
    let units = units(log, "op");
    for path in landed {
        let unit = unit_landing_at(&units, path);
        assert_eq!(unit.dest_identity, expected_at(path), "{}", path.display());
        assert!(
            unit.source_path.starts_with(archive) && unit.source_path != archive,
            "the row's source is inside the archive: {}",
            unit.source_path.display()
        );
    }
}

/// A zip reads at random, so its file and folder go through the per-entry walk.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_extract_out_of_a_zip_onto_a_local_disk_records_each_landed_node() {
    let (engine, log) = journaled();
    let work = tempfile::tempdir().expect("work");
    let zip = work.path().join("bundle.zip");
    write_zip(&zip, &[("readme.txt", b"hello"), ("docs/a.txt", b"aaa")]);
    let out = work.path().join("out");
    std::fs::create_dir_all(&out).expect("mk out");

    volume_transfer(
        &engine,
        "op",
        OpKind::Copy,
        archive_at(&zip, ArchiveFormat::Zip),
        &[zip.join("readme.txt"), zip.join("docs")],
        local_disk(&out),
        &out,
    )
    .await;

    assert_eq!(std::fs::read(out.join("docs/a.txt")).expect("extracted"), b"aaa");
    assert_extracted(&log, &zip, &[out.join("readme.txt"), out.join("docs/a.txt")]);
}

/// A compressed tar reads front to back, so a folder in it lands through the
/// one-pass extractor, a body of its own.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_one_pass_extract_out_of_a_compressed_tar_records_each_landed_node() {
    let (engine, log) = journaled();
    let work = tempfile::tempdir().expect("work");
    let archive = work.path().join("bundle.tar.gz");
    write_tar_gz(&archive, &[("docs/a.txt", b"alpha"), ("docs/sub/b.txt", b"bravo")]);
    let out = work.path().join("out");
    std::fs::create_dir_all(&out).expect("mk out");
    let source = archive_at(&archive, ArchiveFormat::Tar(TarCodec::Gzip));
    assert!(
        source.extraction_is_sequential(&archive.join("docs")),
        "the cell walks the one-pass body"
    );

    volume_transfer(
        &engine,
        "op",
        OpKind::Copy,
        source,
        &[archive.join("docs")],
        local_disk(&out),
        &out,
    )
    .await;

    assert_eq!(std::fs::read(out.join("docs/sub/b.txt")).expect("extracted"), b"bravo");
    assert_extracted(&log, &archive, &[out.join("docs/a.txt"), out.join("docs/sub/b.txt")]);
}

// ── Overwrite and merge ──────────────────────────────────────────────────────

/// The volume engine replaces a file on a local disk through a temp and a rename,
/// so the row names the new node, never the one it replaced.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_volume_copy_overwriting_a_file_on_a_local_disk_records_the_new_node() {
    let (engine, log) = journaled();
    let work = tempfile::tempdir().expect("work");
    write_old(&work.path().join("notes.txt"), b"old notes");
    let replaced = node_at(&work.path().join("notes.txt"));
    let share = share_with_an_album().await;

    volume_transfer_with(
        &engine,
        "op",
        OpKind::Copy,
        (share as Arc<dyn Volume>, &[PathBuf::from("/notes.txt")]),
        (local_disk(work.path()), work.path()),
        &VolumeCopyConfig {
            conflict_resolution: ConflictResolution::Overwrite,
            ..VolumeCopyConfig::default()
        },
    )
    .await;

    let landed = work.path().join("notes.txt");
    assert_eq!(std::fs::read(&landed).expect("read"), b"notes");
    assert_ne!(node_at(&landed), replaced, "the overwrite put a new node there");
    assert_eq!(
        unit_landing_at(&units(&log, "op"), &landed).dest_identity,
        expected_at(&landed)
    );
}

/// What a merge into an existing folder must show: each file it landed names its
/// node, the file that was there has no row, and the folder claims no node.
fn assert_merged(log: &TestOperationLog, folder: &Path) {
    let units = units(log, "op");
    let landed = folder.join("a.jpg");
    assert_eq!(unit_landing_at(&units, &landed).dest_identity, expected_at(&landed));
    let kept = folder.join("kept.jpg");
    assert_eq!(std::fs::read(&kept).expect("kept"), b"kept");
    assert!(
        units
            .iter()
            .all(|unit| unit.dest_path.as_deref() != Some(kept.as_path())),
        "a file the merge never touched has no row"
    );
    assert!(
        units
            .iter()
            .filter(|unit| unit.dest_path.as_deref() == Some(folder))
            .all(|unit| unit.dest_identity.is_none()),
        "the folder that was there claims no node: {units:?}"
    );
}

#[test]
fn a_local_copy_merged_into_an_existing_folder_records_only_what_it_landed() {
    let (engine, log) = journaled();
    let work = tempfile::tempdir().expect("work");
    write_old(&work.path().join("album/a.jpg"), b"aaa");
    let dst = work.path().join("dst");
    write_old(&dst.join("album/kept.jpg"), b"kept");

    local_copy(
        &engine,
        "op",
        &work.path().join("album"),
        &dst,
        &WriteOperationConfig::default(),
    );

    assert_merged(&log, &dst.join("album"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_volume_copy_merged_into_an_existing_folder_records_only_what_it_landed() {
    let (engine, log) = journaled();
    let work = tempfile::tempdir().expect("work");
    write_old(&work.path().join("album/kept.jpg"), b"kept");
    let share = share_with_an_album().await;

    volume_transfer(
        &engine,
        "op",
        OpKind::Copy,
        share as Arc<dyn Volume>,
        &[PathBuf::from("/album")],
        local_disk(work.path()),
        work.path(),
    )
    .await;

    assert_merged(&log, &work.path().join("album"));
}

// ── Stopped ──────────────────────────────────────────────────────────────────

/// A sink that stops its operation the moment its first source item is done.
struct StopAfterFirstItem {
    inner: CollectorEventSink,
    state: Arc<WriteOperationState>,
    fired: AtomicBool,
}

impl SinkDecorator for StopAfterFirstItem {
    fn inner(&self) -> &dyn OperationEventSink {
        &self.inner
    }

    fn emit_source_item_done(&self, event: WriteSourceItemDoneEvent) {
        self.inner.emit_source_item_done(event);
        if !self.fired.swap(true, Ordering::SeqCst) {
            self.state.intent().set_for_test(OperationIntent::Stopped);
        }
    }
}

/// Copies two files, stopping after the first, on `engine`. Returns whether the
/// run ended cancelled and the names now in the destination.
fn copy_stopped_after_one(engine: &TestOperations, work: &Path) -> (bool, Vec<String>) {
    let sources = [work.join("src/a.txt"), work.join("src/b.txt")];
    for source in &sources {
        write_old(source, b"bytes");
    }
    let dst = work.join("dst");
    std::fs::create_dir_all(&dst).expect("mk dst");
    let st = state(engine);
    journal::open_local_op(st.scope(), "op", OpKind::Copy, Initiator::User, 0, Some("root"));
    let sink = StopAfterFirstItem {
        inner: CollectorEventSink::new(),
        state: Arc::clone(&st),
        fired: AtomicBool::new(false),
    };
    let result = copy_files_with_progress_inner(&sink, "op", &st, &sources, &dst, &WriteOperationConfig::default());
    let cancelled = matches!(result, Err(WriteOperationError::Cancelled { .. }));
    journal::finalize_op(st.scope(), OpKind::Copy, ExecutionStatus::Canceled);
    let mut names: Vec<String> = std::fs::read_dir(&dst)
        .expect("list dst")
        .map(|entry| entry.expect("entry").file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    (cancelled, names)
}

/// A file that landed before the stop names its node, and the stop ends the run
/// exactly as it does with nothing journaled.
#[test]
fn a_copy_stopped_after_its_first_file_records_that_files_node_and_stops_the_same() {
    let (engine, log) = journaled();
    let work = tempfile::tempdir().expect("work");
    let journaled_run = copy_stopped_after_one(&engine, work.path());

    let bare = tempfile::tempdir().expect("bare");
    let bare_run = copy_stopped_after_one(&TestOperations::new(), bare.path());

    assert_eq!(journaled_run, bare_run, "the journal changes nothing about the run");
    assert_eq!(
        journaled_run,
        (true, vec!["a.txt".to_string()]),
        "stopped after one file"
    );
    let landed = work.path().join("dst/a.txt");
    assert_eq!(
        unit_landing_at(&units(&log, "op"), &landed).dest_identity,
        expected_at(&landed)
    );
}

// ── A node read that fails ───────────────────────────────────────────────────

/// A local disk whose node read always fails, as an `lstat` racing an unmount would.
struct NodeReadFails {
    inner: Arc<dyn Volume>,
}

impl Volume for NodeReadFails {
    forward_volume_methods!(inner =>
        name, root, list_directory, get_metadata, exists, is_directory, create_file, create_directory,
        create_directory_all, delete, rename, get_space_info, supports_streaming, supports_export,
        operations_are_local, max_concurrent_ops, scan_for_copy, scan_for_conflicts, open_read_stream,
        write_from_stream, supports_unknown_length_writes, write_is_single_shot,
        create_directory_errors_on_existing_dir, apply_metadata, metadata_caps,
    );

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn mount_class(&self) -> Option<MountClass> {
        Some(MountClass::LocalDisk)
    }

    fn node_identity<'a>(
        &'a self,
        path: &'a Path,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<Option<NodeIdentity>, VolumeError>> + Send + 'a>> {
        Box::pin(async move { Err(VolumeError::NotFound(path.display().to_string())) })
    }
}

/// The copy lands every file and finishes clean; its rows record no node and
/// keep the checks a row from before identities gets.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_node_read_that_fails_records_none_and_leaves_the_copy_whole() {
    let (engine, log) = journaled();
    let work = tempfile::tempdir().expect("work");
    let share = share_with_an_album().await;
    let dest: Arc<dyn Volume> = Arc::new(NodeReadFails {
        inner: local_disk(work.path()),
    });

    volume_transfer(
        &engine,
        "op",
        OpKind::Copy,
        share as Arc<dyn Volume>,
        &[PathBuf::from("/album"), PathBuf::from("/notes.txt")],
        dest,
        work.path(),
    )
    .await;

    assert_eq!(std::fs::read(work.path().join("album/a.jpg")).expect("landed"), b"aaa");
    assert_eq!(std::fs::read(work.path().join("notes.txt")).expect("landed"), b"notes");
    let units = units(&log, "op");
    for landed in [work.path().join("album/a.jpg"), work.path().join("notes.txt")] {
        let unit = unit_landing_at(&units, &landed);
        assert_eq!(unit.dest_identity, None, "{}", landed.display());
        assert_eq!(unit.size, Some(std::fs::metadata(&landed).expect("stat").len() as i64));
    }
}
