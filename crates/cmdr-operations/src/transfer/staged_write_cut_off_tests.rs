//! How a staged write's landing the transport cut off settles
//! (`staged_write_landing_cut_off.rs`): a give-up that finds the rename went
//! through, and a Cancel during the settle's byte compare.

use super::*;

/// A destination whose rename onto `/notes.txt` goes through and then loses
/// its reply on the transport.
struct RenamesThenLosesTheReply {
    inner: Arc<InMemoryVolume>,
}

impl Volume for RenamesThenLosesTheReply {
    forward_volume_methods!(inner => name, root, get_metadata, exists, delete, list_directory, open_read_stream);

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn rename<'a>(
        &'a self,
        from: &'a Path,
        to: &'a Path,
        force: bool,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            self.inner.rename(from, to, force).await?;
            if to == Path::new("/notes.txt") {
                return Err(VolumeError::ConnectionTimeout("the rename's reply never came".into()));
            }
            Ok(())
        })
    }
}

/// A rename onto the cleared name that the server performed before its reply
/// was lost, given up at once: the temp is gone and the name holds our bytes,
/// so it landed. ❌ Never a rescue that names a temp no longer there.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_given_up_rename_onto_a_cleared_name_that_went_through_counts_as_landed() {
    let inner = Arc::new(InMemoryVolume::new("dest"));
    let dest: Arc<dyn Volume> = Arc::new(RenamesThenLosesTheReply {
        inner: Arc::clone(&inner),
    });
    // A `Rename` pick's placeholder: the name this write claimed.
    inner.create_file(Path::new("/notes.txt"), b"").await.unwrap();
    inner
        .create_file(Path::new("/notes.txt.cmdr-tmp-abc"), b"THE NEW BYTES")
        .await
        .unwrap();

    land_unrecorded(
        &dest,
        Path::new("/notes.txt.cmdr-tmp-abc"),
        Path::new("/notes.txt"),
        LandingName::ClaimedByTheCaller,
    )
    .await
    .expect("the rename went through, so the file landed");

    assert_eq!(size_of(&inner, "/notes.txt").await, Some(13));
    assert!(!inner.exists(Path::new("/notes (recovered).txt")).await);
}

/// A source whose file opens and then never answers a read: what comparing a
/// multi-GB file looks like from the Cancel button. `reading` says the compare
/// got to it.
struct ReadsForever {
    inner: Arc<InMemoryVolume>,
    reading: Arc<std::sync::atomic::AtomicBool>,
}

struct NeverAnswers(Arc<std::sync::atomic::AtomicBool>);

impl cmdr_fs::volume::VolumeReadStream for NeverAnswers {
    fn next_chunk(
        &mut self,
    ) -> std::pin::Pin<Box<dyn Future<Output = Option<Result<Vec<u8>, VolumeError>>> + Send + '_>> {
        self.0.store(true, std::sync::atomic::Ordering::SeqCst);
        Box::pin(std::future::pending())
    }
    fn total_size(&self) -> cmdr_fs::volume::StreamLength {
        cmdr_fs::volume::StreamLength::Known(13)
    }
    fn bytes_read(&self) -> u64 {
        0
    }
    fn modified_at(&self) -> Option<std::time::SystemTime> {
        None
    }
}

impl Volume for ReadsForever {
    forward_volume_methods!(inner => name, root, get_metadata, exists, list_directory);
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn open_read_stream<'a>(
        &'a self,
        _path: &'a Path,
    ) -> std::pin::Pin<
        Box<dyn Future<Output = Result<Box<dyn cmdr_fs::volume::VolumeReadStream>, VolumeError>> + Send + 'a>,
    > {
        let stream = NeverAnswers(Arc::clone(&self.reading));
        Box::pin(async { Ok(Box::new(stream) as Box<dyn cmdr_fs::volume::VolumeReadStream>) })
    }
}

/// ❗ A Cancel while a cut-off landing's settle compares the name against the
/// source ends the compare at once (#577), and a compare cut short proves
/// nothing: ❌ never counted as landed, ❌ never discarded. The landing stays
/// recorded for the volume's next arrival, which settles it, and the stop
/// names the file it was on its way to.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cancel_during_a_cut_off_landings_byte_compare_stops_it_and_keeps_the_landing_recorded() {
    let dir = cmdr_fs::testing::TestDir::new("staged-write-compare-stopped");
    let data_dir = dir.join("data");
    std::fs::create_dir_all(&data_dir).unwrap();
    let ledger = Ledger::recording_in(&data_dir);
    let engine = TestOperations::new();
    let state = state_recording_into(&engine, &ledger);
    let inner = Arc::new(InMemoryVolume::new("dest"));
    // A `Rename` pick's placeholder, so the landing clears the way and its
    // settle must compare bytes.
    inner.create_file(Path::new("/notes.txt"), b"").await.unwrap();
    let dest: Arc<dyn Volume> = Arc::new(RenamesThenLosesTheReply {
        inner: Arc::clone(&inner),
    });
    let staged = StagedWrite::begin_unannounced(&state, Path::new("/notes.txt"), WriteStaging::StageOntoClaimedName);
    let temp = staged.target().to_path_buf();
    inner.create_file(&temp, b"THE NEW BYTES").await.unwrap();
    let Err(LandingError::CutOff(cut_off)) = staged.commit(&dest, 13).await else {
        panic!("the rename onto the cleared name lost its reply, so the landing is cut off");
    };

    let reading = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let source: Arc<dyn Volume> = Arc::new(ReadsForever {
        inner: Arc::new(InMemoryVolume::new("source")),
        reading: Arc::clone(&reading),
    });
    let stopper = Arc::clone(&state);
    tokio::spawn(async move {
        cmdr_fs::testing::wait_until_async(Duration::from_secs(5), "the compare to read the source", || {
            reading.load(std::sync::atomic::Ordering::SeqCst)
        })
        .await;
        stopper.stop_work.cancel();
    });
    let resumed = tokio::time::timeout(
        Duration::from_secs(5),
        cut_off.resume(
            &dest,
            Some(LandingSource {
                volume: &source,
                path: Path::new("/notes.txt"),
            }),
        ),
    )
    .await
    .expect("❗ the Cancel ends the compare, ❌ never waits for both files to be read");

    assert!(
        matches!(resumed, Resumed::Stopped),
        "a compare cut short is no proof, and the work stopped"
    );
    assert_eq!(
        size_of(&inner, "/notes.txt").await,
        Some(13),
        "what's at the name stays"
    );
    assert!(
        records_landing(&Ledger::replay_log(&data_dir), &temp, 13),
        "the landing stays recorded for the sweep"
    );
    assert_eq!(
        *state.kept_landings.lock_ignore_poison(),
        vec![PathBuf::from("/notes.txt")],
        "and the stop names the file it was on its way to"
    );
    drop(engine);
}
