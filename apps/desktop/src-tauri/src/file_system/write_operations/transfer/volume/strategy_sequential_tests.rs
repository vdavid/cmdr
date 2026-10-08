//! Tests for the one-pass sequential-archive extraction path in
//! `volume/strategy.rs` (`copy_single_path` → `extract_sequential_subtree`):
//! nested-subtree correctness through the plan + single-decode data pass, the
//! random-vs-sequential routing gate, and cancellation between members.
//!
//! The source is a real `ArchiveVolume` over a `.tar.gz` on disk (a compressed
//! tar is sequential-access, so it takes the one-pass path); the destination is
//! an `InMemoryVolume`, so the write lands through the normal `write_from_stream`.

use super::super::faulty_volume::forward_volume_methods;
use super::super::transfer_error::PathedVolumeError;
use super::test_support::make_state;
use super::*;
use crate::file_system::write_operations::transfer::transfer_driver::LeafProgressLedger;
use std::future::Future;
use std::io::Write;
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::file_system::volume::{InMemoryVolume, StreamLength, StreamWriteProgress, Volume, VolumeError};
use cmdr_archive::{ArchiveFormat, ArchiveVolume, TarCodec};
use cmdr_fs::volume::host::VolumeHost;

use crate::file_system::volume::WriteMode;
use crate::file_system::write_operations::state::OperationIntent;
use crate::test_support::TestDir;

/// A tar entry to build into a fixture.
enum Item<'a> {
    File(&'a str, &'a [u8]),
    /// A file whose tar header records mode `0o755`, the way a release tarball
    /// records a script.
    ExecFile(&'a str, &'a [u8]),
    Dir(&'a str),
    Symlink(&'a str, &'a str),
}

/// Builds a gzip-compressed tar from `(name, bytes)` files and writes it to a
/// unique temp path (the local-backed `ArchiveVolume` reads a real file). Cleaned
/// up by [`TarGzFixture`]'s `Drop`.
fn write_targz(files: &[(&str, &[u8])]) -> (TestDir, PathBuf) {
    let items: Vec<Item> = files.iter().map(|(n, d)| Item::File(n, d)).collect();
    write_targz_items(&items)
}

/// Like [`write_targz`] but accepts dirs and symlinks too.
fn write_targz_items(items: &[Item]) -> (TestDir, PathBuf) {
    let mut builder = ::tar::Builder::new(Vec::new());
    for item in items {
        let mut header = ::tar::Header::new_ustar();
        header.set_mtime(1_700_000_000);
        header.set_mode(0o644);
        match item {
            Item::File(name, data) => {
                header.set_size(data.len() as u64);
                header.set_entry_type(::tar::EntryType::Regular);
                header.set_cksum();
                builder.append_data(&mut header, name, *data).expect("append file");
            }
            Item::ExecFile(name, data) => {
                header.set_size(data.len() as u64);
                header.set_mode(0o755);
                header.set_entry_type(::tar::EntryType::Regular);
                header.set_cksum();
                builder.append_data(&mut header, name, *data).expect("append exec file");
            }
            Item::Dir(name) => {
                header.set_size(0);
                header.set_mode(0o755);
                header.set_entry_type(::tar::EntryType::Directory);
                header.set_cksum();
                builder
                    .append_data(&mut header, name, std::io::empty())
                    .expect("append dir");
            }
            Item::Symlink(name, target) => {
                header.set_size(0);
                header.set_entry_type(::tar::EntryType::Symlink);
                header.set_link_name(target).expect("set link");
                header.set_cksum();
                builder
                    .append_data(&mut header, name, std::io::empty())
                    .expect("append symlink");
            }
        }
    }
    let plain = builder.into_inner().expect("finish tar");
    let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    enc.write_all(&plain).expect("gz write");
    let bytes = enc.finish().expect("gz finish");

    let dir = TestDir::new("seq-extract");
    let path = dir.join("fixture.tar.gz");
    std::fs::write(&path, bytes).expect("write fixture");
    (dir, path)
}

/// Owns a scratch `.tar.gz` and hands out an `ArchiveVolume` over it; the
/// directory goes away on drop.
struct TarGzFixture {
    /// Held for its `Drop`: it owns the directory `path` lives in.
    _dir: TestDir,
    path: PathBuf,
}

impl TarGzFixture {
    fn new(files: &[(&str, &[u8])]) -> Self {
        let (dir, path) = write_targz(files);
        Self { _dir: dir, path }
    }

    fn from_items(items: &[Item]) -> Self {
        let (dir, path) = write_targz_items(items);
        Self { _dir: dir, path }
    }

    fn volume(&self) -> Arc<dyn Volume> {
        // A local-backed parent, so the archive reads its real temp file via the
        // `LocalFileSource` fast path.
        let parent: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("parent").with_local_fs_access());
        Arc::new(ArchiveVolume::new(
            parent,
            self.path.clone(),
            ArchiveFormat::Tar(TarCodec::Gzip),
            VolumeHost::detached(),
        ))
    }

    fn inner(&self, rel: &str) -> PathBuf {
        self.path.join(rel)
    }
}

async fn read_dest(dest: &Arc<dyn Volume>, path: &str) -> Result<Vec<u8>, VolumeError> {
    let mut stream = dest.open_read_stream(Path::new(path)).await?;
    let mut out = Vec::new();
    while let Some(chunk) = stream.next_chunk().await {
        out.extend_from_slice(&chunk?);
    }
    Ok(out)
}

/// The one-pass extractor writes its files itself rather than going through
/// `stream_pipe_file`, so it needs its own proof that the mode lands. The plan
/// pass is the only one that lists the archive; the data pass has to carry what
/// it recorded, or a script in a `.tar.gz` arrives disarmed while the same
/// script in a `.zip` doesn't.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_sequential_extract_carries_the_executable_bit() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = TarGzFixture::from_items(&[
        Item::ExecFile("bin/run.sh", b"#!/bin/sh\n"),
        Item::File("bin/notes.txt", b"nnn"),
    ]);
    let source = fixture.volume();
    let dest_dir = TestDir::new("seq-extract-dest");
    let dest: Arc<dyn Volume> = Arc::new(crate::file_system::volume::LocalPosixVolume::new(
        "Dest",
        dest_dir.to_str().expect("dest path"),
    ));

    let state = make_state();
    copy_single_path(
        &source,
        &fixture.inner("bin"),
        Some(true),
        SourceFileFacts::default(),
        &dest,
        Path::new("out"),
        &state,
        &CreatedPaths::default(),
        &LeafProgressLedger::silent_source(Arc::clone(&state)),
        None,
        WriteStaging::Stage,
    )
    .await
    .expect("sequential extract");

    let mode_of = |rel: &str| {
        std::fs::metadata(dest_dir.join(rel))
            .unwrap_or_else(|e| panic!("stat {rel}: {e}"))
            .permissions()
            .mode()
            & 0o777
    };
    assert_eq!(
        mode_of("out/run.sh") & 0o111,
        0o111,
        "the tar header said 0o755, so the extracted script has to run"
    );
    assert_eq!(
        mode_of("out/notes.txt") & 0o111,
        0,
        "and a plain member stays non-executable"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sequential_extract_materializes_a_nested_subtree() {
    // A later, chunk-spanning member proves the single decode reaches deep files.
    let big: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
    let fixture = TarGzFixture::new(&[
        ("docs/a.txt", b"alpha"),
        ("docs/sub/b.txt", b"bravo"),
        ("docs/sub/deep/c.bin", &big),
        // Outside the extracted subtree: must not be materialized.
        ("other/x.txt", b"nope"),
    ]);
    let source = fixture.volume();
    let dest: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("dest"));
    let state = make_state();

    let bytes = copy_single_path(
        &source,
        &fixture.inner("docs"),
        Some(true), // source is a directory
        SourceFileFacts::default(),
        &dest,
        Path::new("/out"),
        &state,
        &CreatedPaths::default(),
        &LeafProgressLedger::silent_source(Arc::clone(&state)),
        None,
        WriteStaging::Stage,
    )
    .await
    .expect("sequential extract");

    assert_eq!(bytes, 5 + 5 + big.len() as u64, "total bytes = the three files");
    assert_eq!(read_dest(&dest, "/out/a.txt").await.unwrap(), b"alpha");
    assert_eq!(read_dest(&dest, "/out/sub/b.txt").await.unwrap(), b"bravo");
    assert_eq!(read_dest(&dest, "/out/sub/deep/c.bin").await.unwrap(), big);
    assert!(
        !dest.exists(Path::new("/out/../other/x.txt")).await,
        "files outside the subtree are never extracted"
    );
}

/// The extracted folders keep the archive's dates, set only once the data pass
/// landed every member: the planning pass creates the folders and writes
/// nothing, so dating them there would lose to the members landing after.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sequential_extract_keeps_the_folders_dates() {
    let fixture = TarGzFixture::from_items(&[
        Item::Dir("docs/"),
        Item::Dir("docs/sub/"),
        Item::File("docs/a.txt", b"alpha"),
        Item::File("docs/sub/b.txt", b"bravo"),
    ]);
    let source = fixture.volume();
    let dest: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("dest"));
    let state = make_state();
    // The top-level folder's date arrives the way a copy's does: from the scan.
    let scan = source.scan_for_copy(&fixture.inner("docs")).await.expect("scan");

    copy_single_path(
        &source,
        &fixture.inner("docs"),
        Some(true),
        SourceFileFacts {
            modified_at: scan.top_level_modified_at,
            ..SourceFileFacts::default()
        },
        &dest,
        Path::new("/out"),
        &state,
        &CreatedPaths::default(),
        &LeafProgressLedger::silent_source(Arc::clone(&state)),
        None,
        WriteStaging::Stage,
    )
    .await
    .expect("sequential extract");

    for folder in ["/out", "/out/sub"] {
        let listed = dest.get_metadata(Path::new(folder)).await.unwrap().modified_at;
        assert_eq!(
            listed,
            Some(1_700_000_000),
            "{folder}: the tar header's date must survive"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn compressed_tar_is_sequential_but_zip_is_random() {
    // The routing gate: `copy_single_path` sends a directory source to the
    // one-pass extractor iff `extraction_is_sequential` is true. A compressed tar
    // is; a zip (and a plain tar) is not, so those keep the per-entry walk.
    let targz = TarGzFixture::new(&[("d/a.txt", b"x")]);
    assert!(
        targz.volume().extraction_is_sequential(&targz.inner("d")),
        "a .tar.gz must take the one-pass path"
    );

    // A zip archive volume over the same-shaped path reports random-access.
    let zip_parent: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("p").with_local_fs_access());
    let zip_vol = ArchiveVolume::new(
        zip_parent,
        PathBuf::from("/tmp/whatever.zip"),
        ArchiveFormat::Zip,
        VolumeHost::detached(),
    );
    assert!(
        !zip_vol.extraction_is_sequential(Path::new("/tmp/whatever.zip/d")),
        "a zip must keep the random-access per-entry path"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sequential_extract_lands_empty_dirs_and_symlinks() {
    // The directory structure (including an EMPTY explicit dir with no file
    // members, and a dir entry appearing AFTER a file already inside it) comes
    // from the parsed tree in the plan pass, not the byte stream. A symlink entry
    // carries no data, so it extracts to a benign empty file — the same as the
    // per-entry path.
    let fixture = TarGzFixture::from_items(&[
        Item::File("docs/sub/b.txt", b"bravo"),
        // The explicit `docs/sub/` dir entry arrives AFTER its child above.
        Item::Dir("docs/sub/"),
        // An empty explicit directory with nothing inside it.
        Item::Dir("docs/empty/"),
        Item::Symlink("docs/link", "../../etc/passwd"),
    ]);
    let source = fixture.volume();
    let dest: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("dest"));
    let state = make_state();

    copy_single_path(
        &source,
        &fixture.inner("docs"),
        Some(true),
        SourceFileFacts::default(),
        &dest,
        Path::new("/out"),
        &state,
        &CreatedPaths::default(),
        &LeafProgressLedger::silent_source(Arc::clone(&state)),
        None,
        WriteStaging::Stage,
    )
    .await
    .expect("extract");

    assert_eq!(read_dest(&dest, "/out/sub/b.txt").await.unwrap(), b"bravo");
    assert!(
        dest.is_directory(Path::new("/out/empty")).await.unwrap_or(false),
        "the empty explicit directory lands even with no file members"
    );
    // A symlink entry extracts to an empty file, never a symlink out of the root.
    assert_eq!(
        read_dest(&dest, "/out/link").await.unwrap(),
        Vec::<u8>::new(),
        "a symlink member extracts to a benign empty file"
    );
}

/// A destination that trips a switch the first time a member lands.
///
/// The one-pass extractor's stop checks sit BETWEEN members, so a test has to
/// act at exactly the moment one finishes writing. It used to do that with a
/// completion callback of its own; the engine owns leaf completion now, so the
/// trigger moves to where the bytes actually land — which is also the more
/// honest place, since it is a real write finishing rather than a test hook.
struct StopAfterFirstWrite {
    inner: Arc<InMemoryVolume>,
    writes: Arc<AtomicUsize>,
    on_first: Arc<dyn Fn() + Send + Sync>,
}

impl Volume for StopAfterFirstWrite {
    forward_volume_methods!(inner =>
        name, root, list_directory, get_metadata, exists, is_directory, create_file, create_directory,
        create_directory_all, delete, rename, get_space_info, supports_streaming, supports_export,
        operations_are_local, max_concurrent_ops, scan_for_copy, open_read_stream, supports_unknown_length_writes,
        write_is_single_shot,
        create_directory_errors_on_existing_dir,
    );

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn write_from_stream<'a>(
        &'a self,
        dest: &'a Path,
        mode: WriteMode,
        length: StreamLength,
        stream: Box<dyn VolumeReadStream>,
        on_progress: &'a (dyn Fn(StreamWriteProgress) -> ControlFlow<()> + Sync),
    ) -> Pin<Box<dyn Future<Output = Result<u64, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let written = self
                .inner
                .write_from_stream(dest, mode, length, stream, on_progress)
                .await?;
            if self.writes.fetch_add(1, Ordering::SeqCst) == 0 {
                (self.on_first)();
            }
            Ok(written)
        })
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sequential_extract_cancels_between_members() {
    let fixture = TarGzFixture::new(&[
        ("docs/a.txt", b"alpha"),
        ("docs/b.txt", b"bravo"),
        ("docs/c.txt", b"charlie"),
    ]);
    let source = fixture.volume();
    let state = make_state();

    // Cancel right after the FIRST file lands: the data pass's between-member
    // `is_cancelled` check must then stop before writing the second file.
    let completed = Arc::new(AtomicUsize::new(0));
    let inner_dest = Arc::new(InMemoryVolume::new("dest"));
    let dest: Arc<dyn Volume> = Arc::new(StopAfterFirstWrite {
        inner: Arc::clone(&inner_dest),
        writes: Arc::clone(&completed),
        on_first: {
            let state = Arc::clone(&state);
            Arc::new(move || {
                state.intent.store(OperationIntent::Stopped as u8, Ordering::Relaxed);
            })
        },
    });

    let result = copy_single_path(
        &source,
        &fixture.inner("docs"),
        Some(true),
        SourceFileFacts::default(),
        &dest,
        Path::new("/out"),
        &state,
        &CreatedPaths::default(),
        &LeafProgressLedger::silent_source(Arc::clone(&state)),
        None,
        WriteStaging::Stage,
    )
    .await;

    assert!(
        matches!(
            result,
            Err(PathedVolumeError {
                error: VolumeError::Cancelled(_),
                ..
            })
        ),
        "cancel mid-extract surfaces as Cancelled, got {result:?}"
    );
    assert_eq!(completed.load(Ordering::SeqCst), 1, "exactly one member was written");
    assert_eq!(
        read_dest(&dest, "/out/a.txt").await.unwrap(),
        b"alpha",
        "first file landed"
    );
    assert!(!dest.exists(Path::new("/out/b.txt")).await, "second file never started");
    assert!(!dest.exists(Path::new("/out/c.txt")).await, "third file never started");
}

/// The same boundary honors Pause, and resuming finishes the extract.
///
/// One decode pass over a solid archive can't be re-entered, so the member
/// boundary is the only place this path can stop, and one iteration covers the
/// remainder of the subtree once skipped members are drained through it. Without
/// the gate, "Paused" stood over an extract running to completion.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sequential_extract_pauses_between_members_and_resumes() {
    let fixture = TarGzFixture::new(&[
        ("docs/a.txt", b"alpha"),
        ("docs/b.txt", b"bravo"),
        ("docs/c.txt", b"charlie"),
    ]);
    let source = fixture.volume();
    let state = make_state();

    // Pause right after the FIRST member lands, so the pause is tied to the
    // extract's own progress rather than a wall clock.
    let completed = Arc::new(AtomicUsize::new(0));
    let dest: Arc<dyn Volume> = Arc::new(StopAfterFirstWrite {
        inner: Arc::new(InMemoryVolume::new("dest")),
        writes: Arc::clone(&completed),
        on_first: {
            let state = Arc::clone(&state);
            Arc::new(move || state.pause_gate.pause())
        },
    });
    let source_root = fixture.inner("docs");
    let dest_for_extract = Arc::clone(&dest);
    let state_for_extract = Arc::clone(&state);
    let progress_for_extract = LeafProgressLedger::silent_source(Arc::clone(&state));
    let extractor = tokio::spawn(async move {
        copy_single_path(
            &source,
            &source_root,
            Some(true),
            SourceFileFacts::default(),
            &dest_for_extract,
            Path::new("/out"),
            &state_for_extract,
            &CreatedPaths::default(),
            &progress_for_extract,
            None,
            WriteStaging::Stage,
        )
        .await
    });

    crate::test_support::wait_until_async(Duration::from_secs(5), "the first member to land", || {
        completed.load(Ordering::SeqCst) >= 1
    })
    .await;

    // Parking has no "parked now" signal, so hold a window open: an ungated pass
    // would have decoded the remaining members many times over inside it.
    // allowed-test-sleep: negative assertion over a window; the park has nothing to await.
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(
        completed.load(Ordering::SeqCst),
        1,
        "a paused extract holds at the member boundary"
    );
    assert!(
        !dest.exists(Path::new("/out/b.txt")).await,
        "the second member hasn't been written"
    );

    state.pause_gate.resume();

    let bytes = tokio::time::timeout(Duration::from_secs(10), extractor)
        .await
        .expect("a resumed extract finishes")
        .expect("the extract task joins")
        .expect("sequential extract");
    assert_eq!(bytes, 5 + 5 + 7, "every member's bytes land once the user resumes");
    assert_eq!(read_dest(&dest, "/out/b.txt").await.unwrap(), b"bravo");
    assert_eq!(read_dest(&dest, "/out/c.txt").await.unwrap(), b"charlie");
}

/// Extracting from a solid archive reserves every clashing ` (N)` name in the
/// PLAN pass, then streams in one decode pass. A write that fails partway leaves
/// every reservation it hadn't filled yet, and none of them may stay behind as
/// an empty file the user never asked for.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_failed_sequential_extract_takes_back_every_unfilled_reservation() {
    use super::super::faulty_volume::{FaultyOp, FaultyVolume};
    use crate::file_system::volume::LocalPosixVolume;
    use crate::file_system::write_operations::event_sinks::CollectorEventSink;
    use crate::file_system::write_operations::types::{ConflictResolution, VolumeCopyConfig};

    let fixture = TarGzFixture::new(&[("album/a.txt", b"incoming a"), ("album/b.txt", b"incoming b")]);
    let source = fixture.volume();
    let dest_dir = TestDir::new("seq-extract-reservations");
    std::fs::create_dir(dest_dir.join("album")).unwrap();
    std::fs::write(dest_dir.join("album/a.txt"), b"the user's a").unwrap();
    std::fs::write(dest_dir.join("album/b.txt"), b"the user's b").unwrap();
    let dest = FaultyVolume::wrapping(Arc::new(LocalPosixVolume::new(
        "Dest",
        dest_dir.to_str().expect("dest path"),
    )))
    .failing_call(
        FaultyOp::WriteFromStream,
        1,
        VolumeError::IoError {
            message: "simulated write failure".to_string(),
            raw_os_error: None,
        },
    )
    .arc();

    let result = super::super::copy::copy_volumes_with_progress(
        Arc::new(CollectorEventSink::new()),
        "op-seq-extract-reservations",
        &make_state(),
        source,
        &[fixture.inner("album")],
        Arc::clone(&dest) as Arc<dyn Volume>,
        Path::new("/"),
        &VolumeCopyConfig {
            // Rename is what reserves the ` (N)` placeholders.
            conflict_resolution: ConflictResolution::Rename,
            progress_interval_ms: 0,
            ..VolumeCopyConfig::default()
        },
    )
    .await;

    assert!(dest.fault_fired(FaultyOp::WriteFromStream));
    assert!(result.is_err(), "the failed write fails the extract: {result:?}");
    let mut names: Vec<String> = std::fs::read_dir(dest_dir.join("album"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    names.sort();
    assert_eq!(
        names,
        vec!["a.txt".to_string(), "b.txt".to_string()],
        "no reservation may outlive the extract that never filled it"
    );
}
