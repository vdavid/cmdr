//! The shared `Volume` conformance promises, against the fake ADB server, plus
//! the cells that pin what this backend does on its own.
//!
//! No `#[ignore]`: the fake is in-process, so these run in every lane.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use cmdr_fs::staging::is_staging_temp_name;
use cmdr_fs::volume::conformance;
use cmdr_fs::volume::{DirectoryCreation, StreamLength, Volume, VolumeError, VolumeReadStream};

use super::AdbVolume;
use super::testing::{FIXTURE_SERIAL, connect_fake, fixture_path};
use crate::testing::{FakeAdbServer, FakeTree};
use cmdr_fs::volume::WriteMode;

/// A phone with the usual `/sdcard` and a handful of files in it.
fn seeded_tree() -> FakeTree {
    let mut tree = FakeTree::new();
    tree.add_dir("/sdcard")
        .add_file("/sdcard/notes.txt", b"the user's notes")
        .add_file("/sdcard/source.txt", b"source")
        .add_file("/sdcard/target.txt", b"the user's target file")
        .add_dir("/sdcard/album")
        .add_file("/sdcard/album/keep.txt", b"content")
        .add_file("/sdcard/exported.txt", b"the bytes a copy would move");
    tree
}

async fn seeded() -> (FakeAdbServer, Arc<AdbVolume>) {
    let server = FakeAdbServer::start(seeded_tree()).await;
    let (volume, _) = connect_fake(&server, FIXTURE_SERIAL).await;
    (server, volume)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_forceless_rename_refuses_an_existing_destination() {
    // `mv -n` exits 0 whether it moved or not, so the refusal here is the
    // pre-flight stat's: this is the cell that would notice it going missing.
    let (_server, volume) = seeded().await;
    conformance::assert_rename_refuses_an_existing_destination(
        volume.as_ref(),
        &fixture_path("/sdcard/source.txt"),
        &fixture_path("/sdcard/target.txt"),
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn create_file_refuses_to_clobber() {
    // `SEND` truncates unconditionally, so the refusal is the pre-flight
    // stat's here too.
    let (_server, volume) = seeded().await;
    conformance::assert_create_file_refuses_to_clobber(volume.as_ref(), &fixture_path("/sdcard/notes.txt"), b"new")
        .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn write_from_stream_create_new_refuses_to_clobber() {
    // The landing `mv -f` replaces, so the refusal is the stat just before it.
    let (_server, volume) = seeded().await;
    conformance::assert_write_from_stream_create_new_refuses_to_clobber(
        volume.as_ref(),
        &fixture_path("/sdcard/notes.txt"),
        &fixture_path("/sdcard/fresh.txt"),
        b"new",
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn create_directory_all_reports_an_existing_directory_honestly() {
    // `mkdir -p` says nothing about whether the directory was there, so the
    // honesty rests on the stat before it.
    let (_server, volume) = seeded().await;
    conformance::assert_create_directory_all_reports_an_existing_dir_honestly(
        volume.as_ref(),
        &fixture_path("/sdcard/album"),
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn create_directory_all_refuses_a_file_in_the_way() {
    // `mkdir -p` names the problem only on stderr ("File exists", "Not a
    // directory"), which nothing here may read, so the answer is the probe's.
    let (_server, volume) = seeded().await;
    conformance::assert_create_directory_all_refuses_a_file_in_the_way(
        volume.as_ref(),
        &fixture_path("/sdcard/notes.txt"),
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn create_directory_all_goes_through_a_link_to_a_folder() {
    // ❗ `/sdcard` is itself a link on a real phone, so a refusal that judged an
    // occupied name without following it would refuse the whole shared storage.
    let mut tree = seeded_tree();
    tree.add_symlink("/sdcard/link", "/sdcard/album");
    let server = FakeAdbServer::start(tree).await;
    let (volume, _) = connect_fake(&server, FIXTURE_SERIAL).await;

    conformance::assert_create_directory_all_goes_through_a_link_to_a_folder(
        volume.as_ref(),
        &fixture_path("/sdcard/link"),
        &fixture_path("/sdcard/album"),
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn delete_leaves_a_non_empty_directory_intact() {
    let (_server, volume) = seeded().await;
    conformance::assert_delete_leaves_a_non_empty_dir_intact(
        volume.as_ref(),
        &fixture_path("/sdcard/album"),
        "keep.txt",
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn writability_matches_the_mutations_offered() {
    let (_server, volume) = seeded().await;
    conformance::assert_writability_matches_the_mutations_offered(volume.as_ref(), &fixture_path("/sdcard/scratch"))
        .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn export_matches_the_bytes_offered() {
    let (_server, volume) = seeded().await;
    conformance::assert_export_matches_the_bytes_offered(
        volume.as_ref(),
        &fixture_path("/sdcard/exported.txt"),
        b"the bytes a copy would move",
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn not_found_carries_the_path() {
    let (_server, volume) = seeded().await;
    conformance::assert_not_found_carries_the_path(volume.as_ref(), &fixture_path("/sdcard/no-such-file.txt")).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_copy_keeps_the_source_date() {
    let (_server, volume) = seeded().await;
    let dated = fixture_path("/sdcard/dated.txt");
    conformance::assert_write_from_stream_keeps_the_source_date(volume.as_ref(), &dated, std::time::Duration::ZERO)
        .await;
    conformance::assert_read_stream_reports_the_listed_date(volume.as_ref(), &dated).await;
}

/// The same promise over the v1 verbs, whose `STAT` carries the mtime as a u32
/// (older phones, and any device that doesn't advertise `stat_v2`).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_copy_keeps_the_source_date_over_the_v1_verbs() {
    let server = FakeAdbServer::start(seeded_tree()).await;
    server.set_features("shell_v2");
    let (volume, _) = connect_fake(&server, FIXTURE_SERIAL).await;
    let dated = fixture_path("/sdcard/dated.txt");
    conformance::assert_write_from_stream_keeps_the_source_date(volume.as_ref(), &dated, std::time::Duration::ZERO)
        .await;
    conformance::assert_read_stream_reports_the_listed_date(volume.as_ref(), &dated).await;
}

// ── This backend's own cells ─────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_path_escaping_the_root_is_refused_before_any_io() {
    let (server, volume) = seeded().await;
    let outcome = volume.get_metadata(&fixture_path("/sdcard/../../etc/passwd")).await;
    assert!(matches!(outcome, Err(VolumeError::NotFound(_))), "{outcome:?}");
    // And nothing was created or asked for under a wrong name.
    assert!(server.tree().lock().unwrap().get("/etc/passwd").is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_write_lands_through_a_staging_sibling_and_leaves_no_partial() {
    let (server, volume) = seeded().await;
    let payload: Vec<u8> = (0..200_000u32).map(|i| (i % 251) as u8).collect();
    let source = Box::new(super::streams::BytesReadStream::new(payload.clone()));
    let seen = std::sync::Mutex::new(Vec::new());
    let written = volume
        .write_from_stream(
            &fixture_path("/sdcard/big.bin"),
            WriteMode::CreateOrReplace,
            StreamLength::Known(payload.len() as u64),
            source,
            &|progress| {
                seen.lock()
                    .unwrap()
                    .push((progress.bytes_written, progress.expected_length));
                std::ops::ControlFlow::Continue(())
            },
        )
        .await
        .expect("the upload must land");
    assert_eq!(written, payload.len() as u64);

    let tree = server.tree();
    let tree = tree.lock().unwrap();
    assert_eq!(tree.file_bytes("/sdcard/big.bin").as_deref(), Some(payload.as_slice()));
    let leftovers: Vec<String> = tree
        .paths()
        .into_iter()
        .filter(|p| p.rsplit('/').next().is_some_and(is_staging_temp_name))
        .collect();
    assert!(
        leftovers.is_empty(),
        "the staging name must be gone after the rename: {leftovers:?}"
    );
    let seen = seen.lock().unwrap();
    assert!(!seen.is_empty(), "progress must be reported");
    assert_eq!(
        seen.last(),
        Some(&(payload.len() as u64, StreamLength::Known(payload.len() as u64)))
    );
}

struct ChunkedSource {
    chunks: std::collections::VecDeque<Result<Vec<u8>, VolumeError>>,
    read: u64,
}

impl ChunkedSource {
    fn unknown(bytes: &[u8], chunk: usize) -> Self {
        Self {
            chunks: bytes.chunks(chunk).map(|piece| Ok(piece.to_vec())).collect(),
            read: 0,
        }
    }

    fn failing_unknown(bytes: &[u8], chunk: usize) -> Self {
        let mut source = Self::unknown(bytes, chunk);
        source.chunks.push_back(Err(VolumeError::DeviceDisconnected(
            "the scripted source stopped".to_string(),
        )));
        source
    }
}

impl VolumeReadStream for ChunkedSource {
    fn next_chunk(&mut self) -> Pin<Box<dyn Future<Output = Option<Result<Vec<u8>, VolumeError>>> + Send + '_>> {
        Box::pin(async move {
            let next = self.chunks.pop_front()?;
            if let Ok(bytes) = &next {
                self.read += bytes.len() as u64;
            }
            Some(next)
        })
    }

    fn total_size(&self) -> StreamLength {
        StreamLength::Unknown
    }

    fn bytes_read(&self) -> u64 {
        self.read
    }

    fn modified_at(&self) -> Option<std::time::SystemTime> {
        None
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unknown_length_multichunk_write_streams_and_reports_its_actual_count() {
    let (server, volume) = seeded().await;
    let payload: Vec<u8> = (0..300_007u32).map(|i| (i % 251) as u8).collect();
    let seen = std::sync::Mutex::new(Vec::new());

    assert!(volume.supports_unknown_length_writes());
    let written = volume
        .write_from_stream(
            &fixture_path("/sdcard/generated.zip"),
            WriteMode::CreateNew,
            StreamLength::Unknown,
            Box::new(ChunkedSource::unknown(&payload, 37_003)),
            &|progress| {
                seen.lock()
                    .unwrap()
                    .push((progress.bytes_written, progress.expected_length));
                std::ops::ControlFlow::Continue(())
            },
        )
        .await
        .expect("the unknown-length upload must land");

    assert_eq!(written, payload.len() as u64);
    assert_eq!(
        server
            .tree()
            .lock()
            .unwrap()
            .file_bytes("/sdcard/generated.zip")
            .as_deref(),
        Some(payload.as_slice())
    );
    let seen = seen.lock().unwrap();
    assert!(seen.len() > 1, "the source must cross several writer callbacks");
    assert!(seen.iter().all(|(_, expected)| *expected == StreamLength::Unknown));
    assert_eq!(seen.last().map(|(done, _)| *done), Some(payload.len() as u64));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unknown_length_create_new_refuses_and_preserves_an_existing_file() {
    let (server, volume) = seeded().await;
    let payload = vec![19u8; 200_003];

    let outcome = volume
        .write_from_stream(
            &fixture_path("/sdcard/target.txt"),
            WriteMode::CreateNew,
            StreamLength::Unknown,
            Box::new(ChunkedSource::unknown(&payload, 31_003)),
            &|_| std::ops::ControlFlow::Continue(()),
        )
        .await;

    assert!(matches!(outcome, Err(VolumeError::AlreadyExists(_))), "{outcome:?}");
    let tree = server.tree();
    let tree = tree.lock().unwrap();
    assert_eq!(
        tree.file_bytes("/sdcard/target.txt").as_deref(),
        Some(&b"the user's target file"[..])
    );
    assert!(
        !tree
            .paths()
            .iter()
            .any(|p| p.rsplit('/').next().is_some_and(is_staging_temp_name)),
        "the refused stage must be removed: {:?}",
        tree.paths()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cancelled_write_removes_its_partial() {
    let (server, volume) = seeded().await;
    let payload = vec![7u8; 100_000];
    let outcome = volume
        .write_from_stream(
            &fixture_path("/sdcard/never.bin"),
            WriteMode::CreateOrReplace,
            StreamLength::Unknown,
            Box::new(ChunkedSource::unknown(&payload, 17_003)),
            &|_| std::ops::ControlFlow::Break(()),
        )
        .await;
    assert!(matches!(outcome, Err(VolumeError::Cancelled(_))), "{outcome:?}");
    let tree = server.tree();
    let tree = tree.lock().unwrap();
    assert!(tree.get("/sdcard/never.bin").is_none());
    assert!(
        !tree
            .paths()
            .iter()
            .any(|p| p.rsplit('/').next().is_some_and(is_staging_temp_name)),
        "{:?}",
        tree.paths()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unknown_length_source_failure_removes_its_partial_and_keeps_the_cause() {
    let (server, volume) = seeded().await;
    let payload = vec![11u8; 150_003];
    let outcome = volume
        .write_from_stream(
            &fixture_path("/sdcard/source-failed.bin"),
            WriteMode::CreateNew,
            StreamLength::Unknown,
            Box::new(ChunkedSource::failing_unknown(&payload, 31_003)),
            &|_| std::ops::ControlFlow::Continue(()),
        )
        .await;

    assert!(
        matches!(outcome, Err(VolumeError::DeviceDisconnected(_))),
        "{outcome:?}"
    );
    let tree = server.tree();
    let tree = tree.lock().unwrap();
    assert!(tree.get("/sdcard/source-failed.bin").is_none());
    assert!(
        !tree
            .paths()
            .iter()
            .any(|p| p.rsplit('/').next().is_some_and(is_staging_temp_name)),
        "the partial stage must be removed: {:?}",
        tree.paths()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mkdir_on_an_existing_directory_succeeds() {
    let (_server, volume) = seeded().await;
    volume
        .create_directory(&fixture_path("/sdcard/album"))
        .await
        .expect("mkdir -p over an existing directory is a success");
    assert!(!volume.create_directory_errors_on_existing_dir());
    let deep = volume
        .create_directory_all(&fixture_path("/sdcard/a/b/c"))
        .await
        .expect("a deep create must work");
    assert_eq!(deep, DirectoryCreation::Created);
    assert!(volume.is_directory(&fixture_path("/sdcard/a/b/c")).await.unwrap());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancelling_a_read_mid_file_releases_the_socket_and_the_volume_keeps_working() {
    let mut tree = seeded_tree();
    let big: Vec<u8> = (0..(3 * crate::sync::MAX_DATA_CHUNK))
        .map(|i| (i % 253) as u8)
        .collect();
    tree.add_file("/sdcard/big.bin", &big);
    let server = FakeAdbServer::start(tree).await;
    let (volume, _) = connect_fake(&server, FIXTURE_SERIAL).await;

    let mut stream = volume
        .open_read_stream(&fixture_path("/sdcard/big.bin"))
        .await
        .expect("open");
    assert_eq!(stream.total_size(), StreamLength::Known(big.len() as u64));
    let first = stream.next_chunk().await.expect("a first chunk").expect("no error");
    assert_eq!(&first[..], &big[..first.len()]);
    stream.cancel_and_release().await;
    drop(stream);

    // The volume is still perfectly usable afterwards.
    let entries = volume
        .list_directory(&fixture_path("/sdcard"), None)
        .await
        .expect("list after cancel");
    assert!(entries.iter().any(|e| e.name == "big.bin"));
    let mut resumed = volume
        .open_read_stream_at_offset(&fixture_path("/sdcard/big.bin"), 100_000)
        .await
        .expect("open at offset");
    let mut tail = Vec::new();
    while let Some(chunk) = resumed.next_chunk().await {
        tail.extend_from_slice(&chunk.unwrap());
    }
    assert_eq!(&tail[..], &big[100_000..]);
    assert_eq!(resumed.total_size(), StreamLength::Known(big.len() as u64));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_bounded_range_read_seeks_on_device_and_returns_only_the_requested_window() {
    let mut tree = seeded_tree();
    let bytes: Vec<u8> = (0..350_009u32).map(|i| (i % 251) as u8).collect();
    tree.add_file("/sdcard/a 'quoted' archive.zip", &bytes);
    let server = FakeAdbServer::start(tree).await;
    let (volume, _) = connect_fake(&server, FIXTURE_SERIAL).await;
    let path = fixture_path("/sdcard/a 'quoted' archive.zip");

    let window = volume.read_range(&path, 65_539, 200_003).await.expect("range");
    assert_eq!(window, bytes[65_539..265_542]);
    assert_eq!(
        volume.read_range(&path, 349_990, 100).await.expect("short range"),
        bytes[349_990..]
    );
    assert!(
        volume
            .read_range(&path, 400_000, 4096)
            .await
            .expect("past EOF")
            .is_empty()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn conflict_scan_reads_a_missing_destination_as_empty() {
    // `ls` on a directory that isn't there exits non-zero, and the match arm in
    // `scan_for_conflicts_impl` is what turns that into "nothing clashes" rather
    // than into a copy preview that won't open.
    let (_server, volume) = seeded().await;
    conformance::assert_conflict_scan_reads_a_missing_destination_as_empty(
        volume.as_ref(),
        &fixture_path("/sdcard/not-created-yet"),
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_batch_scan_stops_when_it_is_told_to() {
    // A phone reached over `adb exec-out` answers a listing in tens of
    // milliseconds each; a `/sdcard/DCIM` with thousands of photos is where
    // somebody presses Cancel.
    let (_server, volume) = seeded().await;
    conformance::assert_batch_scan_stops_when_told(volume.as_ref(), &fixture_path("/sdcard")).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_batch_scan_asks_its_boundary_inside_the_walk() {
    // This backend's scan is `scan_walk`'s, so the boundary is per entry. The
    // fixture holds four files, a subdirectory, and that subdirectory's child.
    let (_server, volume) = seeded().await;
    conformance::assert_batch_scan_asks_inside_the_walk(volume.as_ref(), &fixture_path("/sdcard"), 6).await;
}
