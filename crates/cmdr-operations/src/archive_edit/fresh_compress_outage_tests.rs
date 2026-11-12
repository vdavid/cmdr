//! A fresh compress onto a share that drops: at the upload, while the archive
//! is generated straight onto the share, and at each publish mode's rename.
//!
//! The producer can't resume mid-stream, so a drop while it writes makes the
//! archive again once the share is back; one while a spool uploads uploads it
//! again. A publish rename the drop cut off may have been performed, so the
//! publish settles first: a temp gone and an archive at the size written is
//! published, ❌ never reported as a failure, and a whole temp is published
//! without making the archive again.

use std::future::Future;
use std::io::Cursor;
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use super::super::engine::MutatorHooks;
use super::run;
use crate::archive_edit::EditError;
use crate::event_sinks::CollectorEventSink;
use crate::in_flight_temps::Ledger;
use crate::outage_slot::OutagePolicy;
use crate::testing::TestOperations;
use crate::transfer::outage_volume_test_support::{OutageVolume, RenameDrop};
use crate::transfer::volume::{FaultyOp, FaultyVolume, forward_volume_methods};
use crate::types::{ConflictResolution, WriteOperationError, WriteOperationType};
use cmdr_fs::testing::wait_until_async;
use cmdr_fs::volume::{
    BackendKind, InMemoryVolume, StreamLength, StreamWriteProgress, StreamWritten, Volume, VolumeError,
    VolumeReadStream, WriteMode,
};
use cmdr_local::LocalPosixVolume;

const NAS_ID: &str = "compress-nas";
const ARCHIVE: &str = "/share/photos.zip";
const WAIT: Duration = Duration::from_secs(10);
const CADENCE: &[Duration] = &[Duration::from_millis(10), Duration::from_millis(20)];
const PAYLOAD: &[u8] = b"the photo's bytes, compressed into a fresh archive";

type Fut<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// The share an archive is compressed onto: an [`OutageVolume`] that takes the
/// direct route (an SMB share that accepts unknown-length writes) or the
/// spool's, and replaces atomically or not.
struct CompressNas {
    outage: Arc<OutageVolume>,
    /// The bytes behind it, for the ranged reads the stage's validation takes
    /// (the outage double has none of its own).
    inner: Arc<InMemoryVolume>,
    direct: bool,
    atomic: bool,
}

impl Volume for CompressNas {
    forward_volume_methods!(outage =>
        name, root, lane_key, connection_state, connection_generation, attempt_reconnect,
        list_directory, get_metadata, exists, is_directory, create_file, create_directory,
        create_directory_all, delete, rename, get_space_info, open_read_stream,
        create_directory_errors_on_existing_dir, apply_metadata,
    );

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn backend_kind(&self) -> BackendKind {
        if self.direct {
            BackendKind::Smb
        } else {
            BackendKind::Webdav
        }
    }

    fn supports_unknown_length_writes(&self) -> bool {
        self.direct
    }

    fn supports_local_fs_access(&self) -> bool {
        false
    }

    fn supports_atomic_replace_rename(&self) -> bool {
        self.atomic
    }

    fn read_range<'a>(&'a self, path: &'a Path, offset: u64, len: usize) -> Fut<'a, Result<Vec<u8>, VolumeError>> {
        self.inner.read_range(path, offset, len)
    }

    /// An unknown-length write is buffered here, then written through the
    /// outage double at its length, so the drop arms count it like any write.
    fn write_from_stream<'a>(
        &'a self,
        dest: &'a Path,
        mode: WriteMode,
        length: StreamLength,
        mut stream: Box<dyn VolumeReadStream>,
        on_progress: &'a (dyn Fn(StreamWriteProgress) -> ControlFlow<()> + Sync),
    ) -> Fut<'a, Result<StreamWritten, VolumeError>> {
        Box::pin(async move {
            if length.known().is_some() {
                return self
                    .outage
                    .write_from_stream(dest, mode, length, stream, on_progress)
                    .await;
            }
            let mut bytes = Vec::new();
            while let Some(chunk) = stream.next_chunk().await {
                bytes.extend_from_slice(&chunk?);
            }
            let buffer = InMemoryVolume::new("buffer");
            buffer.create_file(Path::new("/zip"), &bytes).await?;
            let buffered = buffer.open_read_stream(Path::new("/zip")).await?;
            self.outage
                .write_from_stream(
                    dest,
                    mode,
                    StreamLength::Known(bytes.len() as u64),
                    buffered,
                    on_progress,
                )
                .await
        })
    }
}

/// Where the share drops.
#[derive(Clone, Copy, Debug)]
enum Cut {
    /// The first write onto the share: the direct stage, or the spool's upload.
    Write,
    /// The publish rename onto the archive's name, on the given side.
    Publish(RenameDrop),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Meanwhile {
    ComesBack,
    StaysGone,
}

struct Ran {
    label: String,
    result: Result<usize, EditError>,
    nas: Arc<OutageVolume>,
    inner: Arc<InMemoryVolume>,
    _source_dir: tempfile::TempDir,
}

/// The archive already at the name, for the cells that replace one.
fn old_archive() -> Vec<u8> {
    let mut cursor = Cursor::new(Vec::new());
    {
        let mut writer = zip::ZipWriter::new(&mut cursor);
        writer
            .start_file("old.txt", zip::write::SimpleFileOptions::default())
            .unwrap();
        std::io::Write::write_all(&mut writer, b"the archive that was there").unwrap();
        writer.finish().unwrap();
    }
    cursor.into_inner()
}

async fn compress(direct: bool, atomic: bool, replace: bool, cut: Cut, meanwhile: Meanwhile) -> Ran {
    let label = format!("direct={direct}/atomic={atomic}/replace={replace}/{cut:?}/{meanwhile:?}");
    let engine = TestOperations::new();
    let source_dir = tempfile::tempdir().unwrap();
    std::fs::write(source_dir.path().join("photo.jpg"), PAYLOAD).unwrap();
    let source: Arc<dyn Volume> = Arc::new(LocalPosixVolume::local_folder(
        "src",
        source_dir.path().to_path_buf(),
        crate::host::testing::pane_order_host(),
    ));

    let inner = Arc::new(InMemoryVolume::new("Testnas"));
    inner.create_directory(Path::new("/share")).await.unwrap();
    if replace {
        inner.create_file(Path::new(ARCHIVE), &old_archive()).await.unwrap();
    }
    let nas = OutageVolume::wrap(Arc::clone(&inner), &format!("compress-{label}"));
    match cut {
        Cut::Write => nas.drop_at_write(0),
        Cut::Publish(side) => nas.drop_at_rename_onto(Path::new(ARCHIVE), side),
    }
    let dest: Arc<dyn Volume> = Arc::new(CompressNas {
        outage: Arc::clone(&nas),
        inner: Arc::clone(&inner),
        direct,
        atomic,
    });

    let budget = match meanwhile {
        Meanwhile::StaysGone => Duration::from_millis(200),
        Meanwhile::ComesBack => Duration::from_secs(30),
    };
    let state = Arc::new(
        engine
            .new_operation_state(Duration::ZERO)
            .with_journal_volumes("compress-source".into(), NAS_ID.into())
            .with_in_flight_ledger(Ledger::for_test())
            .with_outage_policy(OutagePolicy {
                budget,
                cadence: CADENCE,
                poll_every: Duration::from_millis(5),
                attempt_timeout: Duration::from_secs(1),
            }),
    );
    let during = {
        let state = Arc::clone(&state);
        let nas = Arc::clone(&nas);
        tokio::spawn(async move {
            wait_until_async(WAIT, "the compress to wait for the share", || {
                state.outage.waiting_for().is_some()
            })
            .await;
            if meanwhile == Meanwhile::ComesBack {
                nas.become_reachable();
            }
        })
    };
    let events = Arc::new(CollectorEventSink::new());
    let hooks = Arc::new(MutatorHooks::new(
        Arc::clone(&state),
        Arc::clone(&events) as Arc<dyn crate::OperationEventSink>,
        "op-compress-outage".to_owned(),
        WriteOperationType::Compress,
        Duration::ZERO,
    ));
    let result = tokio::time::timeout(
        Duration::from_secs(30),
        run(
            source,
            vec![PathBuf::from("photo.jpg")],
            dest,
            PathBuf::from(ARCHIVE),
            replace,
            ConflictResolution::Stop,
            Arc::clone(&state),
            hooks,
            None,
            events.as_ref(),
            "op-compress-outage",
        ),
    )
    .await
    .expect("the compress ends, one way or the other");
    during.abort();
    Ran {
        label,
        result,
        nas,
        inner,
        _source_dir: source_dir,
    }
}

async fn read_all(volume: &dyn Volume, path: &Path) -> Option<Vec<u8>> {
    let mut stream = volume.open_read_stream(path).await.ok()?;
    let mut buf = Vec::new();
    while let Some(chunk) = stream.next_chunk().await {
        buf.extend_from_slice(&chunk.ok()?);
    }
    Some(buf)
}

impl Ran {
    fn assert_the_share_dropped(&self) {
        assert!(self.nas.reconnect_attempts() > 0, "{}: the share dropped", self.label);
    }

    async fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .inner
            .list_directory(Path::new("/share"), None)
            .await
            .unwrap()
            .into_iter()
            .map(|entry| entry.name)
            .collect();
        names.sort();
        names
    }

    /// The fresh archive is at its name, holding the photo, with nothing
    /// beside it.
    async fn assert_published(&self) {
        let label = &self.label;
        assert!(
            self.result.is_ok(),
            "{label}: the compress publishes, got {:?}",
            self.result.as_ref().err().map(describe)
        );
        let bytes = read_all(&*self.inner, Path::new(ARCHIVE)).await.expect("the archive");
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).expect("a valid zip");
        let mut entry = archive.by_name("photo.jpg").expect("the photo is in it");
        let mut content = Vec::new();
        std::io::Read::read_to_end(&mut entry, &mut content).unwrap();
        assert_eq!(content, PAYLOAD, "{label}: the photo's bytes");
        assert_eq!(
            self.names().await,
            vec!["photos.zip".to_owned()],
            "{label}: nothing beside it"
        );
    }

    fn writes(&self) -> usize {
        self.nas.writes_for(Path::new(ARCHIVE))
    }
}

fn describe(error: &EditError) -> String {
    match error {
        EditError::Cancelled => "Cancelled".to_owned(),
        EditError::Op(error) => format!("{error:?}"),
    }
}

/// The spool route: a drop at the upload waits and uploads the spool again,
/// ❌ without making the archive again.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_drop_at_the_spools_upload_waits_and_uploads_it_again() {
    let ran = compress(false, false, false, Cut::Write, Meanwhile::ComesBack).await;
    ran.assert_the_share_dropped();
    ran.assert_published().await;
    assert_eq!(ran.writes(), 2, "the spool was uploaded again");
}

/// The direct route: a drop while the archive streams onto the share waits,
/// then makes the archive again into a fresh stage.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_drop_while_the_archive_streams_onto_the_share_makes_it_again() {
    let ran = compress(true, false, false, Cut::Write, Meanwhile::ComesBack).await;
    ran.assert_the_share_dropped();
    ran.assert_published().await;
    assert_eq!(ran.writes(), 2, "the archive was made again once the share was back");
}

/// Every publish mode, the drop on either side of its rename: once the share
/// is back the publish settles, ❌ without making the archive again, and a
/// rename the server performed counts as published.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_drop_at_the_publish_rename_settles_and_publishes_once() {
    for (direct, atomic, replace) in [(true, false, false), (true, true, true), (false, false, true)] {
        for side in [RenameDrop::BeforeTheServerSeesIt, RenameDrop::AfterTheServerPerformsIt] {
            let ran = compress(direct, atomic, replace, Cut::Publish(side), Meanwhile::ComesBack).await;
            ran.assert_the_share_dropped();
            ran.assert_published().await;
            assert_eq!(ran.writes(), 1, "{}: ❗ published without making it again", ran.label);
        }
    }
}

/// The share stays gone at the upload: "Lost the connection to {server}", and
/// the archive that was there is untouched.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_share_that_stays_gone_reports_the_lost_connection_and_keeps_the_old_archive() {
    for direct in [true, false] {
        let ran = compress(direct, false, true, Cut::Write, Meanwhile::StaysGone).await;
        ran.assert_the_share_dropped();
        assert!(
            matches!(
                &ran.result,
                Err(EditError::Op(WriteOperationError::ServerConnectionLost {
                    gave_up_after_secs: Some(_),
                    ..
                }))
            ),
            "{}: expected ServerConnectionLost, got {:?}",
            ran.label,
            ran.result.as_ref().err().map(describe)
        );
        ran.nas.become_reachable();
        assert_eq!(
            read_all(&*ran.inner, Path::new(ARCHIVE)).await,
            Some(old_archive()),
            "{}: the old archive is untouched",
            ran.label
        );
    }
}

/// A refusal (no outage) at the upload still fails at once.
#[tokio::test]
async fn a_refused_upload_fails_at_once() {
    let source_dir = tempfile::tempdir().unwrap();
    std::fs::write(source_dir.path().join("photo.jpg"), PAYLOAD).unwrap();
    let source: Arc<dyn Volume> = Arc::new(LocalPosixVolume::local_folder(
        "src",
        source_dir.path().to_path_buf(),
        crate::host::testing::pane_order_host(),
    ));
    let inner = Arc::new(InMemoryVolume::new("Remote"));
    inner.create_directory(Path::new("/share")).await.unwrap();
    let faulty = FaultyVolume::wrapping(Arc::clone(&inner))
        .failing_call(
            FaultyOp::WriteFromStream,
            1,
            VolumeError::PermissionDenied {
                path: ARCHIVE.to_owned(),
                raw_os_error: None,
            },
        )
        .arc();
    let engine = TestOperations::new();
    let state = Arc::new(engine.new_operation_state(Duration::ZERO));
    let events = Arc::new(CollectorEventSink::new());
    let hooks = Arc::new(MutatorHooks::new(
        Arc::clone(&state),
        Arc::clone(&events) as Arc<dyn crate::OperationEventSink>,
        "op-refused".to_owned(),
        WriteOperationType::Compress,
        Duration::ZERO,
    ));
    let result = run(
        source,
        vec![PathBuf::from("photo.jpg")],
        Arc::clone(&faulty) as Arc<dyn Volume>,
        PathBuf::from(ARCHIVE),
        false,
        ConflictResolution::Stop,
        state,
        hooks,
        None,
        events.as_ref(),
        "op-refused",
    )
    .await;
    assert!(
        faulty.fault_fired(FaultyOp::WriteFromStream),
        "the refusal reached the upload"
    );
    assert!(result.is_err(), "the refusal ends the compress");
    assert!(!inner.exists(Path::new(ARCHIVE)).await, "nothing published");
}
