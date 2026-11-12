//! A remote zip edit whose share drops: at the pull, at the upload, and at
//! each step of the swap that replaces the archive.
//!
//! The swap moves the archive aside, renames the uploaded temp onto its name,
//! and deletes the aside last, so the archive is whole at one of its names at
//! every moment. A drop waits for the share and SETTLES what happened before
//! acting: a whole temp lands without uploading again, a gone temp counts as
//! landed only when the archive holds exactly the edited bytes, and a wait that
//! ends without the share leaves the original at its aside, recorded for the
//! share's next arrival, whose sweep puts it back. A drop before the swap leaves
//! the original as it was.

use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use super::{EditError, pull_apply_upload_swap};
use crate::in_flight_temps::Ledger;
use crate::outage_slot::OutagePolicy;
use crate::state::WriteOperationState;
use crate::testing::TestOperations;
use crate::transfer::outage_volume_test_support::{OutageVolume, RenameDrop};
use crate::transfer::volume::{FaultyOp, FaultyVolume};
use crate::types::WriteOperationError;
use cmdr_archive::mutator::{self, AddEntry, AddSource, Changeset, MutationHooks};
use cmdr_fs::ignore_poison::IgnorePoison as _;
use cmdr_fs::testing::wait_until_async;
use cmdr_fs::volume::{InMemoryVolume, Volume, VolumeError};

const NAS_ID: &str = "zip-edit-nas";
const ARCHIVE: &str = "/share/bundle.zip";
const WAIT: Duration = Duration::from_secs(10);
const CADENCE: &[Duration] = &[Duration::from_millis(10), Duration::from_millis(20)];

struct NoHooks;
impl MutationHooks for NoHooks {}

fn build_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut cursor = Cursor::new(Vec::new());
    {
        let mut writer = zip::ZipWriter::new(&mut cursor);
        let options = zip::write::SimpleFileOptions::default();
        for (name, contents) in entries {
            writer.start_file(*name, options).expect("start file");
            writer.write_all(contents).expect("write entry");
        }
        writer.finish().expect("finish zip");
    }
    cursor.into_inner()
}

fn original_zip() -> Vec<u8> {
    build_zip(&[("keep.txt", b"keep me")])
}

/// Where the share drops.
#[derive(Clone, Copy, Debug)]
enum Cut {
    /// Opening the archive for the pull.
    PullOpen,
    /// The upload of the edited archive.
    Upload,
    /// The swap's move of the archive aside never reaches the server.
    AsideUnsent,
    /// The server moved the archive aside; the reply is lost.
    AsideUnanswered,
    /// The move aside went through; the rename after it never reaches the
    /// server.
    RenameUnsent,
    /// The server renamed the temp onto the archive; the reply is lost.
    RenameUnanswered,
    /// The move aside answers the error of a session someone else already
    /// replaced (it never reached the server).
    StaleAsideError,
}

/// What happens while the edit waits for the share.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Meanwhile {
    ComesBack,
    Cancel,
    StaysGone,
}

struct Ran {
    label: String,
    result: Result<(), EditError>,
    nas: Arc<OutageVolume>,
    inner: Arc<InMemoryVolume>,
    ledger: Ledger,
    state: Arc<WriteOperationState>,
}

async fn run(cut: Cut, meanwhile: Meanwhile) -> Ran {
    run_on(cut, meanwhile, Replace::DeleteThenRename).await
}

/// How the share replaces a file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Replace {
    DeleteThenRename,
    /// One forced rename (`Volume::supports_atomic_replace_rename`), as SMB's
    /// `ReplaceIfExists`.
    InOneStep,
}

/// [`run`], on a share that replaces a file the way `replace` says.
async fn run_on(cut: Cut, meanwhile: Meanwhile, replace: Replace) -> Ran {
    let label = format!("{cut:?}/{meanwhile:?}/{replace:?}");
    let engine = TestOperations::new();
    let inner = Arc::new(InMemoryVolume::new("Testnas"));
    inner.create_directory(Path::new("/share")).await.unwrap();
    inner.create_file(Path::new(ARCHIVE), &original_zip()).await.unwrap();
    let nas = OutageVolume::wrap(Arc::clone(&inner), &format!("zip-edit-{label}"));
    if replace == Replace::InOneStep {
        nas.replace_atomically();
    }
    let archive = Path::new(ARCHIVE);
    match cut {
        Cut::PullOpen => nas.drop_at_open(0),
        Cut::Upload => nas.drop_at_write(0),
        Cut::AsideUnsent => nas.drop_at_rename_of(archive, RenameDrop::BeforeTheServerSeesIt),
        Cut::AsideUnanswered => nas.drop_at_rename_of(archive, RenameDrop::AfterTheServerPerformsIt),
        Cut::RenameUnsent => nas.drop_at_rename_onto(archive, RenameDrop::BeforeTheServerSeesIt),
        Cut::RenameUnanswered => nas.drop_at_rename_onto(archive, RenameDrop::AfterTheServerPerformsIt),
        Cut::StaleAsideError => nas.stale_error_at_rename_of(archive, RenameDrop::BeforeTheServerSeesIt),
    }
    let ledger = Ledger::for_test();
    let budget = match meanwhile {
        Meanwhile::StaysGone => Duration::from_millis(200),
        _ => Duration::from_secs(30),
    };
    let state = Arc::new(
        engine
            .new_operation_state(Duration::from_millis(50))
            .with_in_flight_ledger(ledger.clone())
            .with_outage_policy(OutagePolicy {
                budget,
                cadence: CADENCE,
                poll_every: Duration::from_millis(5),
                attempt_timeout: Duration::from_secs(1),
            }),
    );
    let op = engine.register_state(&format!("op-{label}"), Arc::clone(&state));
    let during = {
        let state = Arc::clone(&state);
        let nas = Arc::clone(&nas);
        let engine = crate::Operations::clone(&engine);
        let op_id = op.id().to_owned();
        tokio::spawn(async move {
            if matches!(cut, Cut::StaleAsideError) {
                return;
            }
            wait_until_async(WAIT, "the edit to wait for the share", || {
                state.outage.waiting_for().is_some()
            })
            .await;
            match meanwhile {
                Meanwhile::ComesBack => nas.become_reachable(),
                Meanwhile::Cancel => engine.cancel_write_operation(&op_id, false),
                Meanwhile::StaysGone => {}
            }
        })
    };
    let result = tokio::time::timeout(
        Duration::from_secs(30),
        pull_apply_upload_swap(
            Arc::clone(&nas) as Arc<dyn Volume>,
            NAS_ID,
            PathBuf::from(ARCHIVE),
            Arc::clone(&state),
            None,
            |working: &Path| -> Result<(), EditError> {
                let changeset = Changeset {
                    adds: vec![AddEntry {
                        inner_path: "added.txt".to_string(),
                        source: AddSource::Bytes(b"fresh bytes".to_vec()),
                    }],
                    ..Default::default()
                };
                mutator::apply(working, &changeset, &NoHooks).expect("local mutator apply");
                Ok(())
            },
        ),
    )
    .await
    .expect("the edit ends, one way or the other");
    during.abort();
    Ran {
        label,
        result,
        nas,
        inner,
        ledger,
        state,
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

fn entry_names(bytes: &[u8]) -> Vec<String> {
    let archive = zip::ZipArchive::new(Cursor::new(bytes.to_vec())).expect("the archive parses");
    let mut names: Vec<String> = archive
        .file_names()
        .map(|name| name.expect("a decodable name").into_owned())
        .collect();
    names.sort();
    names
}

impl Ran {
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

    async fn temps(&self) -> Vec<PathBuf> {
        self.names()
            .await
            .into_iter()
            .filter(|name| name.contains(".cmdr-tmp-"))
            .map(|name| Path::new("/share").join(name))
            .collect()
    }

    async fn archive_entries(&self) -> Option<Vec<String>> {
        read_all(&*self.inner, Path::new(ARCHIVE))
            .await
            .map(|bytes| entry_names(&bytes))
    }

    fn assert_the_share_dropped(&self) {
        assert!(
            self.nas.reconnect_attempts() > 0,
            "{}: the share dropped and the edit dialed it",
            self.label
        );
    }

    /// The edit landed: the edited archive at its name, nothing rescued, no
    /// temp left, and no landing left recorded.
    async fn assert_landed(&self) {
        let label = &self.label;
        assert!(self.result.is_ok(), "{label}: the edit lands");
        assert_eq!(
            self.archive_entries().await,
            Some(vec!["added.txt".to_owned(), "keep.txt".to_owned()]),
            "{label}: the edited archive at its name"
        );
        assert!(
            !self.names().await.iter().any(|name| name.contains("(recovered)")),
            "{label}: nothing rescued: {:?}",
            self.names().await
        );
        assert_eq!(self.temps().await, Vec::<PathBuf>::new(), "{label}: no temp left");
        assert!(
            !self.names().await.iter().any(|name| name.contains(".cmdr-temp-")),
            "{label}: no aside left: {:?}",
            self.names().await
        );
        assert_eq!(
            self.ledger.recorded_landings(),
            Vec::<PathBuf>::new(),
            "{label}: no landing left recorded"
        );
    }

    /// The original archive, byte for byte, and no temp beside it.
    async fn assert_original_intact(&self) {
        assert_eq!(
            read_all(&*self.inner, Path::new(ARCHIVE)).await,
            Some(original_zip()),
            "{}: the original archive is untouched",
            self.label
        );
    }

    fn uploads(&self) -> usize {
        self.nas.writes_for(Path::new(ARCHIVE))
    }
}

/// The share drops as the pull opens the archive: once it's back, the edit
/// pulls it again and lands.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_drop_at_the_pull_waits_and_pulls_again() {
    let ran = run(Cut::PullOpen, Meanwhile::ComesBack).await;
    ran.assert_the_share_dropped();
    ran.assert_landed().await;
    assert_eq!(ran.uploads(), 1, "one upload");
}

/// The share drops as the upload starts: once it's back, the edit uploads the
/// edited copy again from its local scratch and lands.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_drop_at_the_upload_waits_and_uploads_again() {
    let ran = run(Cut::Upload, Meanwhile::ComesBack).await;
    ran.assert_the_share_dropped();
    ran.assert_landed().await;
    assert_eq!(ran.uploads(), 2, "the upload ran again once the share was back");
}

/// Every step of the swap a drop can hit: once the share is back, the swap
/// settles what the server did and lands the uploaded temp, ❌ without
/// uploading again or parking it under ` (recovered)`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_drop_inside_the_swap_waits_and_lands_without_uploading_again() {
    for cut in [
        Cut::AsideUnsent,
        Cut::AsideUnanswered,
        Cut::RenameUnsent,
        Cut::RenameUnanswered,
    ] {
        let ran = run(cut, Meanwhile::ComesBack).await;
        ran.assert_the_share_dropped();
        ran.assert_landed().await;
        assert_eq!(
            ran.uploads(),
            1,
            "{}: ❗ a whole temp lands without uploading again",
            ran.label
        );
    }
}

/// A stale transport error on the swap's move aside, while the share already
/// reads live on a newer session: ❌ no discard; the swap runs again and lands.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_stale_error_on_the_swaps_move_aside_runs_the_swap_again() {
    let ran = run(Cut::StaleAsideError, Meanwhile::ComesBack).await;
    ran.assert_landed().await;
    assert_eq!(ran.uploads(), 1, "the temp was never thrown away");
}

/// The share stays gone at the upload: the edit stops with "Lost the connection
/// to {server}", and the original archive is untouched.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_share_that_stays_gone_at_the_upload_reports_the_lost_connection() {
    let ran = run(Cut::Upload, Meanwhile::StaysGone).await;
    ran.assert_the_share_dropped();
    match &ran.result {
        Err(EditError::Op(WriteOperationError::ServerConnectionLost {
            volume_name,
            gave_up_after_secs,
            ..
        })) => {
            assert_eq!(volume_name, "Testnas");
            assert!(gave_up_after_secs.is_some(), "it waited before stopping");
        }
        _ => panic!("expected ServerConnectionLost"),
    }
    ran.nas.become_reachable();
    ran.assert_original_intact().await;
}

/// The archive went aside and its original is all the share has of it, whole.
/// Answers the aside, recorded for the share's next arrival.
async fn assert_original_kept_aside(ran: &Ran) -> PathBuf {
    let label = &ran.label;
    let asides: Vec<PathBuf> = ran
        .names()
        .await
        .into_iter()
        .filter(|name| name.contains(".cmdr-temp-"))
        .map(|name| Path::new("/share").join(name))
        .collect();
    let [aside] = asides.as_slice() else {
        panic!("{label}: exactly one aside holds the original archive, got {asides:?}");
    };
    assert_eq!(
        read_all(&*ran.inner, aside).await,
        Some(original_zip()),
        "{label}: ❗ the original survives, whole"
    );
    assert!(
        ran.ledger.pending_paths().contains(aside),
        "{label}: recorded for the share's next arrival"
    );
    assert_eq!(
        ran.ledger.recorded_landings(),
        Vec::<PathBuf>::new(),
        "{label}: no landing: the edit was never the only copy of anything"
    );
    aside.clone()
}

/// The share stays gone after the swap moved the archive aside: the edit stops
/// with "Lost the connection to {server}", and the original archive is whole at
/// its aside, recorded, for the share's next arrival to put back.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_share_that_stays_gone_after_the_move_aside_keeps_the_original_for_its_return() {
    let ran = run(Cut::RenameUnsent, Meanwhile::StaysGone).await;
    ran.assert_the_share_dropped();
    assert!(
        matches!(
            &ran.result,
            Err(EditError::Op(WriteOperationError::ServerConnectionLost { .. }))
        ),
        "the error says it was the connection"
    );
    ran.nas.become_reachable();
    assert_original_kept_aside(&ran).await;
}

/// A Cancel while the swap waits after the move aside: reported as the cancel
/// it is, the original archive whole at its aside, recorded, ❌ never cleaned.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cancel_while_the_swap_waits_after_the_move_aside_keeps_the_original() {
    let ran = run(Cut::RenameUnsent, Meanwhile::Cancel).await;
    assert!(
        matches!(ran.result, Err(EditError::Cancelled)),
        "reported as the cancel it is"
    );
    ran.nas.become_reachable();
    assert_original_kept_aside(&ran).await;
    assert!(
        ran.state.kept_landings.lock_ignore_poison().is_empty(),
        "and the stop names no unfinished landing"
    );
}

/// A Cancel while the upload waits: the original is untouched.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cancel_while_the_upload_waits_leaves_the_original() {
    let ran = run(Cut::Upload, Meanwhile::Cancel).await;
    assert!(
        matches!(ran.result, Err(EditError::Cancelled)),
        "reported as the cancel it is"
    );
    ran.nas.become_reachable();
    ran.assert_original_intact().await;
}

/// A refusal (no outage) at the upload still fails at once, with the original
/// untouched.
#[tokio::test]
async fn a_refused_upload_fails_at_once() {
    let inner = Arc::new(InMemoryVolume::new("Remote"));
    inner.create_directory(Path::new("/share")).await.unwrap();
    inner.create_file(Path::new(ARCHIVE), &original_zip()).await.unwrap();
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
    let state = Arc::new(TestOperations::new().new_operation_state(Duration::from_millis(50)));
    let result = pull_apply_upload_swap(
        Arc::clone(&faulty) as Arc<dyn Volume>,
        NAS_ID,
        PathBuf::from(ARCHIVE),
        state,
        None,
        |working: &Path| -> Result<(), EditError> {
            let changeset = Changeset {
                deletes: vec!["keep.txt".to_string()],
                ..Default::default()
            };
            mutator::apply(working, &changeset, &NoHooks).expect("local mutator apply");
            Ok(())
        },
    )
    .await;
    assert!(
        faulty.fault_fired(FaultyOp::WriteFromStream),
        "the refusal reached the upload"
    );
    assert!(
        matches!(result, Err(EditError::Op(WriteOperationError::PermissionDenied { .. }))),
        "the refusal is reported as itself"
    );
    assert_eq!(
        read_all(&*inner, Path::new(ARCHIVE)).await,
        Some(original_zip()),
        "the original is untouched"
    );
}

/// On a share that replaces a file in one step (SMB), the zip edit's swap is
/// that one rename: a drop at it waits, and lands without uploading again.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_drop_at_a_one_step_replace_waits_and_lands_without_uploading_again() {
    for cut in [Cut::RenameUnsent, Cut::RenameUnanswered] {
        let ran = run_on(cut, Meanwhile::ComesBack, Replace::InOneStep).await;
        ran.assert_the_share_dropped();
        ran.assert_landed().await;
        assert_eq!(
            ran.uploads(),
            1,
            "{}: ❗ a whole temp lands without uploading again",
            ran.label
        );
    }
}

/// ❗ And when the share stays gone at that one step, the original archive is
/// still whole at its name, with nothing recorded as a landing: the replace
/// happened in full or not at all.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_share_that_stays_gone_at_a_one_step_replace_leaves_the_original_archive() {
    let ran = run_on(Cut::RenameUnsent, Meanwhile::StaysGone, Replace::InOneStep).await;
    ran.assert_the_share_dropped();
    assert!(
        matches!(
            &ran.result,
            Err(EditError::Op(WriteOperationError::ServerConnectionLost { .. }))
        ),
        "the error says it was the connection"
    );
    assert_eq!(
        ran.ledger.recorded_landings(),
        Vec::<PathBuf>::new(),
        "nothing was cleared"
    );
    ran.nas.become_reachable();
    ran.assert_original_intact().await;
}
