//! What only a real server can answer about the write path: the window's bytes,
//! the two renames, the error policy over the wire, and the pane updates each
//! mutation leaves behind.
//!
//! ❗ Every cell here works inside a scratch directory of its own
//! ([`scratch_dir`]). The whole binary shares one export and `nextest` runs its
//! cells in parallel, so a fixed name would have two of them deleting each
//! other's files and reporting it as a backend bug.

use std::ops::ControlFlow;
use std::path::Path;
use std::pin::Pin;
use std::sync::Mutex;

use cmdr_fs::volume::{DirectoryChange, DirectoryCreation, Volume, VolumeError, VolumeReadStream};

use super::SftpVolume;
use super::testing::*;

const FIXTURE: &str = "sftp-servers/start.sh (sftp-fixture)";

/// Enough bytes that the window has real work: several chunks, and a tail that
/// isn't chunk-aligned.
const PAYLOAD: usize = 1_000_003;

// ── The window's bytes ───────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn a_file_written_through_the_window_arrives_byte_for_byte() {
    // The window writes chunks out of order by construction, so a wrong offset
    // shows up here as bytes whose contents no longer match where they sit.
    let (volume, dir) = scratch_on("OPENSSH", 12480, "write-bytes").await;
    let path = format!("{dir}/copied.bin");
    let bytes = fixture_large_bytes(PAYLOAD);

    let written = write(&volume, &path, bytes.clone()).await.expect(FIXTURE);

    assert_eq!(written, PAYLOAD as u64);
    assert_same_bytes(&read_whole(&volume, &path).await, &bytes, "a windowed write");
    clean_scratch(&volume, &dir).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn a_server_with_stingy_limits_still_writes_byte_exact() {
    // `sftp-fixture-smalllimits` answers `limits@openssh.com` with numbers far
    // under OpenSSH's own, so the engine splits every chunk and each one takes
    // several requests. The offsets have to survive that.
    let (volume, dir) = scratch_on("SMALLLIMITS", 12488, "write-small-limits").await;
    let path = format!("{dir}/copied.bin");
    let bytes = fixture_large_bytes(300_000);

    write(&volume, &path, bytes.clone()).await.expect(FIXTURE);

    assert_same_bytes(&read_whole(&volume, &path).await, &bytes, "a stingy-limits write");
    clean_scratch(&volume, &dir).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn an_empty_file_is_written_and_is_empty() {
    let (volume, dir) = scratch_on("OPENSSH", 12480, "write-empty").await;
    let path = format!("{dir}/empty.bin");

    let written = write(&volume, &path, Vec::new()).await.expect(FIXTURE);

    assert_eq!(written, 0);
    assert_eq!(
        volume.get_metadata(Path::new(&path)).await.expect(FIXTURE).size,
        Some(0)
    );
    clean_scratch(&volume, &dir).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn progress_counts_up_to_the_whole_file() {
    // Honest progress is a product promise, and a window makes it easy to get
    // wrong: reporting bytes as they are ISSUED rather than as they land would
    // finish the bar long before the file.
    let (volume, dir) = scratch_on("OPENSSH", 12480, "write-progress").await;
    let path = format!("{dir}/copied.bin");
    let reported = Mutex::new(Vec::new());

    let bytes = fixture_large_bytes(PAYLOAD);
    let size = bytes.len() as u64;
    volume
        .write_from_stream(Path::new(&path), size, source(bytes), &|done, total| {
            reported
                .lock()
                .expect("no cell panics holding this")
                .push((done, total));
            ControlFlow::Continue(())
        })
        .await
        .expect(FIXTURE);

    let reported = reported.into_inner().expect("no cell panics holding this");
    assert!(reported.len() > 1, "a multi-chunk write reports more than once");
    assert!(
        reported.iter().all(|(_, total)| *total == size),
        "every tick carries the size the caller promised"
    );
    assert_eq!(reported.last().map(|(done, _)| *done), Some(size), "and it reaches it");
    clean_scratch(&volume, &dir).await;
}

// ── Every failure takes its partial with it ──────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn a_cancelled_write_leaves_nothing_behind() {
    // ❗ Cancellation reaches this backend ONLY as `Break` from the progress
    // callback. There is no token on the write path, so a backend that never
    // called back would be uncancelable.
    let (volume, dir) = scratch_on("OPENSSH", 12480, "write-cancelled").await;
    let path = format!("{dir}/copied.bin");

    let outcome = volume
        .write_from_stream(
            Path::new(&path),
            PAYLOAD as u64,
            source(fixture_large_bytes(PAYLOAD)),
            &|_, _| ControlFlow::Break(()),
        )
        .await;

    assert!(matches!(outcome, Err(VolumeError::Cancelled(_))), "got {outcome:?}");
    assert!(
        !volume.exists(Path::new(&path)).await,
        "a cancelled write must not leave a partial on the server"
    );
    // And the session is still the session: a cancel abandons in-flight write
    // requests, and what must not survive it is a stale response poisoning the
    // channel for whatever the pane does next.
    assert!(volume.exists(Path::new("hello.txt")).await);
    clean_scratch(&volume, &dir).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn a_source_that_stops_partway_takes_the_partial_with_it() {
    // The far side of a cross-volume copy can go away mid-transfer: a share that
    // dropped, a phone unplugged. What must not survive it is bytes on the
    // destination that nothing will finish.
    let (volume, dir) = scratch_on("OPENSSH", 12480, "write-source-failed").await;
    let path = format!("{dir}/copied.bin");

    let outcome = volume
        .write_from_stream(
            Path::new(&path),
            PAYLOAD as u64,
            failing_source(fixture_large_bytes(PAYLOAD)),
            &|_, _| ControlFlow::Continue(()),
        )
        .await;

    assert!(
        matches!(outcome, Err(VolumeError::DeviceDisconnected(_))),
        "the source's own failure is what the caller has to see; got {outcome:?}"
    );
    assert!(!volume.exists(Path::new(&path)).await);
    clean_scratch(&volume, &dir).await;
}

// ── The two renames ──────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn a_forced_rename_replaces_the_destination_on_a_posix_rename_server() {
    // The other half of the no-clobber promise, and the one a remote archive
    // edit spends: `posix-rename@openssh.com` swaps the new bytes in atomically,
    // so the edit never passes through a moment with no archive at the name.
    let (volume, dir) = scratch_on("OPENSSH", 12480, "rename-forced").await;
    let source = format!("{dir}/new.zip");
    let target = format!("{dir}/archive.zip");
    volume
        .create_file(Path::new(&source), b"the new bytes")
        .await
        .expect(FIXTURE);
    volume.create_file(Path::new(&target), b"old").await.expect(FIXTURE);

    volume
        .rename(Path::new(&source), Path::new(&target), true)
        .await
        .expect(FIXTURE);

    assert!(!volume.exists(Path::new(&source)).await);
    assert_eq!(
        volume.get_metadata(Path::new(&target)).await.expect(FIXTURE).size,
        Some(13)
    );
    clean_scratch(&volume, &dir).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn a_forced_rename_replaces_the_destination_without_the_extension_too() {
    // `sftp-fixture-noposixrename` sends plain `SSH_FXP_RENAME`, which REFUSES an
    // occupied destination — so a forced rename there has to clear the way
    // itself. ❗ And only once something is proven to be in it: clearing on any
    // failure is the shape the app-side landing fix exists to stop.
    let (volume, dir) = scratch_on("NOPOSIXRENAME", 12486, "rename-forced-plain").await;
    let source = format!("{dir}/new.zip");
    let target = format!("{dir}/archive.zip");
    volume
        .create_file(Path::new(&source), b"the new bytes")
        .await
        .expect(FIXTURE);
    volume.create_file(Path::new(&target), b"old").await.expect(FIXTURE);

    volume
        .rename(Path::new(&source), Path::new(&target), true)
        .await
        .expect(FIXTURE);

    assert_eq!(
        volume.get_metadata(Path::new(&target)).await.expect(FIXTURE).size,
        Some(13)
    );
    clean_scratch(&volume, &dir).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn a_forceless_rename_refuses_on_a_server_without_the_extension() {
    // The cheap half: plain `SSH_FXP_RENAME` refuses an occupied destination all
    // by itself, so there is nothing to claim and nothing to add. What this cell
    // pins is that the refusal still arrives TYPED, which takes the probe: v3
    // answers it with the same catch-all it answers a full disk with.
    let (volume, dir) = scratch_on("NOPOSIXRENAME", 12486, "rename-plain-refusal").await;
    let source = format!("{dir}/source.txt");
    let target = format!("{dir}/target.txt");
    volume.create_file(Path::new(&source), b"source").await.expect(FIXTURE);
    volume
        .create_file(Path::new(&target), b"the user's file")
        .await
        .expect(FIXTURE);

    let outcome = volume.rename(Path::new(&source), Path::new(&target), false).await;

    assert!(matches!(outcome, Err(VolumeError::AlreadyExists(_))), "got {outcome:?}");
    assert_eq!(
        volume.get_metadata(Path::new(&target)).await.expect(FIXTURE).size,
        Some(15)
    );
    clean_scratch(&volume, &dir).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn a_directory_moves_onto_a_free_name_and_refuses_an_occupied_one() {
    // A directory can't land on the file-shaped claim the common path makes, so
    // this is the branch that notices and claims a directory instead. ❗ And the
    // refusal has to hold for directories too: `posix-rename` replaces an EMPTY
    // destination directory without a word.
    let (volume, dir) = scratch_on("OPENSSH", 12480, "rename-directory").await;
    let album = format!("{dir}/album");
    let moved = format!("{dir}/moved");
    let occupied = format!("{dir}/occupied");
    volume.create_directory(Path::new(&album)).await.expect(FIXTURE);
    volume.create_directory(Path::new(&occupied)).await.expect(FIXTURE);
    volume
        .create_file(Path::new(&format!("{album}/child.txt")), b"x")
        .await
        .expect(FIXTURE);

    volume
        .rename(Path::new(&album), Path::new(&moved), false)
        .await
        .expect(FIXTURE);
    assert!(volume.exists(Path::new(&format!("{moved}/child.txt"))).await);

    let refused = volume.rename(Path::new(&moved), Path::new(&occupied), false).await;
    assert!(
        matches!(refused, Err(VolumeError::AlreadyExists(_))),
        "an occupied name refuses whatever shape the source is; got {refused:?}"
    );
    assert!(
        volume.exists(Path::new(&format!("{moved}/child.txt"))).await,
        "and the refused source keeps its contents"
    );

    let _ = volume.delete(Path::new(&format!("{moved}/child.txt"))).await;
    let _ = volume.delete(Path::new(&moved)).await;
    let _ = volume.delete(Path::new(&occupied)).await;
    clean_scratch(&volume, &dir).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn a_forceless_rename_that_finds_the_name_free_leaves_nothing_extra() {
    // The claim is a real file for a moment. ❗ If a failure ever left it
    // behind, a zero-byte file would be wearing the name the user chose — which
    // is the one artifact staging exists to prevent.
    let (volume, dir) = scratch_on("OPENSSH", 12480, "rename-claim-cleanup").await;
    let missing = format!("{dir}/never-existed.txt");
    let target = format!("{dir}/landing.txt");

    let outcome = volume.rename(Path::new(&missing), Path::new(&target), false).await;

    assert!(outcome.is_err(), "renaming something that isn't there must fail");
    assert!(
        !volume.exists(Path::new(&target)).await,
        "and it must not leave a placeholder at the destination"
    );
    clean_scratch(&volume, &dir).await;
}

// ── The error policy over the wire ───────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn a_directory_that_still_holds_something_refuses_with_the_shared_errno() {
    // The app renders "this folder still has something in it" from the errno,
    // never from wording, and LocalPosix and MTP both answer this way. SFTP v3
    // sends the catch-all, so the number has to be put back by the probe.
    let (volume, dir) = scratch_on("OPENSSH", 12480, "delete-not-empty").await;
    let album = format!("{dir}/album");
    volume.create_directory(Path::new(&album)).await.expect(FIXTURE);
    volume
        .create_file(Path::new(&format!("{album}/child.txt")), b"x")
        .await
        .expect(FIXTURE);

    let outcome = volume.delete(Path::new(&album)).await;

    assert!(
        matches!(outcome, Err(VolumeError::IoError { raw_os_error: Some(errno), .. }) if errno == crate::errors::ENOTEMPTY),
        "got {outcome:?}"
    );

    let _ = volume.delete(Path::new(&format!("{album}/child.txt"))).await;
    let _ = volume.delete(Path::new(&album)).await;
    clean_scratch(&volume, &dir).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn a_file_that_is_not_there_reads_as_missing_rather_than_as_a_catch_all() {
    // ❗ The probe may only make a report MORE accurate. A code the protocol
    // does distinguish is never second-guessed by it.
    let (volume, dir) = scratch_on("OPENSSH", 12480, "delete-missing").await;

    let outcome = volume.delete(Path::new(&format!("{dir}/never-existed.txt"))).await;

    assert!(matches!(outcome, Err(VolumeError::NotFound(_))), "got {outcome:?}");
    clean_scratch(&volume, &dir).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn a_deep_destination_is_created_whole_and_reported_as_created() {
    // The override's other half: several missing levels in one pass, and an
    // honest `Created` for a leaf that really was made here.
    let params = fixture_params("OPENSSH", 12480);
    let (host, listings) = fixture_host_recording(&params, Some(FIXTURE_PASSWORD));
    let volume = connect_fixture(&host, params).await;
    let dir = scratch_dir("mkdir-p-deep");
    clean_scratch(&volume, &dir).await;
    volume.create_directory(Path::new(&dir)).await.expect(FIXTURE);
    let before = listings.change_count();
    let leaf = format!("{dir}/2026/08/photos");

    let made = volume.create_directory_all(Path::new(&leaf)).await.expect(FIXTURE);

    assert_eq!(made, DirectoryCreation::Created);
    assert_eq!(
        listings
            .changes()
            .into_iter()
            .skip(before)
            .filter(|(_, parent, _)| parent == Path::new(&format!("{FIXTURE_ROOT}/{dir}")))
            .count(),
        1,
        "the SHALLOWEST new directory is the one a pane could be showing the parent of; \
         patching only the leaf leaves that pane a level short"
    );
    assert!(volume.is_directory(Path::new(&leaf)).await.expect(FIXTURE));
    assert!(
        volume
            .is_directory(Path::new(&format!("{dir}/2026")))
            .await
            .expect(FIXTURE)
    );

    // And running it again is a no-op that says so, which is what the transfer
    // driver reads as "this may already hold something".
    let again = volume.create_directory_all(Path::new(&leaf)).await.expect(FIXTURE);
    assert_eq!(again, DirectoryCreation::AlreadyExisted);

    for path in [&leaf, &format!("{dir}/2026/08"), &format!("{dir}/2026")] {
        let _ = volume.delete(Path::new(path)).await;
    }
    clean_scratch(&volume, &dir).await;
}

// ── The pane updates ─────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn every_mutation_patches_the_listing_once() {
    // ❗ There is no watcher on this backend, so this patch is the only thing
    // that keeps a destination pane honest after a copy. ❗ And it is ONE call
    // per changed directory: the host walks every cached listing on the volume,
    // so a per-entry caller turns one directory into a quadratic sweep.
    let params = fixture_params("OPENSSH", 12480);
    let (host, listings) = fixture_host_recording(&params, Some(FIXTURE_PASSWORD));
    let volume = connect_fixture(&host, params).await;
    let dir = scratch_dir("notify");
    clean_scratch(&volume, &dir).await;
    volume.create_directory(Path::new(&dir)).await.expect(FIXTURE);

    let before = listings.change_count();
    let notes = format!("{dir}/notes.txt");
    let renamed = format!("{dir}/renamed.txt");
    volume.create_file(Path::new(&notes), b"hello").await.expect(FIXTURE);
    volume
        .rename(Path::new(&notes), Path::new(&renamed), false)
        .await
        .expect(FIXTURE);
    volume.delete(Path::new(&renamed)).await.expect(FIXTURE);

    let changes: Vec<DirectoryChange> = listings
        .changes()
        .into_iter()
        .skip(before)
        .map(|(_, _, change)| change)
        .collect();
    assert_eq!(changes.len(), 3, "one per mutation, never one per entry");
    assert!(matches!(changes[0], DirectoryChange::Added(_)));
    assert!(matches!(changes[1], DirectoryChange::Renamed { .. }));
    assert!(matches!(changes[2], DirectoryChange::Removed(_)));

    clean_scratch(&volume, &dir).await;
}

// ── Helpers ──────────────────────────────────────────────────────────

/// A volume on `service`, with an empty scratch directory of this cell's own.
async fn scratch_on(service: &str, fallback_port: u16, what: &str) -> (SftpVolume, String) {
    let params = fixture_params(service, fallback_port);
    let host = fixture_host(&params, Some(FIXTURE_PASSWORD));
    let volume = connect_fixture(&host, params).await;
    let dir = scratch_dir(what);
    clean_scratch(&volume, &dir).await;
    volume.create_directory(Path::new(&dir)).await.expect(FIXTURE);
    (volume, dir)
}

/// Streams `bytes` onto `path` the way a cross-volume copy does.
async fn write(volume: &SftpVolume, path: &str, bytes: Vec<u8>) -> Result<u64, VolumeError> {
    let size = bytes.len() as u64;
    volume
        .write_from_stream(Path::new(path), size, source(bytes), &|_, _| ControlFlow::Continue(()))
        .await
}

/// Reads a whole file back, the way the copy path does.
async fn read_whole(volume: &SftpVolume, path: &str) -> Vec<u8> {
    let mut stream = volume.open_read_stream(Path::new(path)).await.expect(FIXTURE);
    let mut out = Vec::new();
    while let Some(chunk) = stream.next_chunk().await {
        out.extend_from_slice(&chunk.expect(FIXTURE));
    }
    out
}

/// A source stream over a buffer, handed over in pieces that don't line up with
/// the write window's chunks.
///
/// Deliberately misaligned: a source's chunk size is its own business, and the
/// window has to coalesce whatever arrives into requests the server takes.
struct ScriptedSource {
    bytes: Vec<u8>,
    at: usize,
    /// Where the source gives up, for the cells about a far side that went away.
    fails_at: Option<usize>,
}

impl VolumeReadStream for ScriptedSource {
    fn next_chunk(&mut self) -> Pin<Box<dyn Future<Output = Option<Result<Vec<u8>, VolumeError>>> + Send + '_>> {
        Box::pin(async move {
            if self.at >= self.bytes.len() {
                return None;
            }
            if self.fails_at.is_some_and(|at| self.at >= at) {
                return Some(Err(VolumeError::DeviceDisconnected(
                    "the scripted source stopped".to_string(),
                )));
            }
            let take = 40_003.min(self.bytes.len() - self.at);
            let chunk = self.bytes[self.at..self.at + take].to_vec();
            self.at += take;
            Some(Ok(chunk))
        })
    }

    fn total_size(&self) -> u64 {
        self.bytes.len() as u64
    }

    fn bytes_read(&self) -> u64 {
        self.at as u64
    }
}

fn source(bytes: Vec<u8>) -> Box<dyn VolumeReadStream> {
    Box::new(ScriptedSource {
        bytes,
        at: 0,
        fails_at: None,
    })
}

fn failing_source(bytes: Vec<u8>) -> Box<dyn VolumeReadStream> {
    let fails_at = Some(bytes.len() / 3);
    Box::new(ScriptedSource { bytes, at: 0, fails_at })
}
