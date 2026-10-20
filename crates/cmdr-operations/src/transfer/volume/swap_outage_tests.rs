//! A file→file Overwrite whose swap landing meets a dropped share, at each of
//! the five sites a safe-replace lands (both drivers, a deep merge child, a
//! cross-volume move, and a one-pass extract), through the real pipeline.
//!
//! The swap moves `orig` aside, renames the temp onto its name, and deletes the
//! aside last, so the original is whole at one of its two names at every
//! moment. A drop at the move aside or at the rename after it waits for the
//! share and SETTLES what happened before acting: a whole temp lands without
//! the file streaming again, a gone temp counts as landed only when the name
//! holds exactly the source's bytes, and a wait that ends without the share
//! leaves the original at its aside, recorded for the share's next arrival,
//! whose sweep puts it back.
//!
//! On a share whose forced rename replaces a file in one step (SMB's
//! `ReplaceIfExists`), the swap is that one rename: the name never goes empty,
//! and a wait that ends without the share leaves the original whole.
//!
//! The landing alone, without a wait, is `swap_landing_tests.rs`; the same
//! sites without an outage are `safe_replace_grid_tests.rs`.

use cmdr_operation_log::types::OperationId;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use cmdr_fs::ignore_poison::IgnorePoison as _;
use cmdr_fs::testing::wait_until_async;
use cmdr_fs::volume::{InMemoryVolume, Volume};

use super::safe_replace_grid_tests::TarGzFixture;
use super::safety_oracle::{SafetySpec, assert_operation_was_safe};
use crate::event_sinks::CollectorEventSink;
use crate::in_flight_temps::Ledger;
use crate::outage_slot::OutagePolicy;
use crate::state::WriteOperationState;
use crate::testing::TestOperations;
use crate::transfer::outage_volume_test_support::{OutageVolume, RenameDrop};
use crate::transfer::staged_write::{LandingError, Resumed, StagedWrite, WriteStaging};
use crate::types::{ConflictResolution, VolumeCopyConfig, WriteOperationError};

/// The user's file the Overwrite replaces, and the new one: the SAME size, so
/// only a byte compare can tell them apart.
const OLD: &[u8] = b"the user's notes, before";
const NEW: &[u8] = b"the new notes, from here";
const KEEP: &[u8] = b"a dest-only file the source never shadows";
const FRESH: &[u8] = b"a source-only file, new to the destination";
const SIBLING_A: &[u8] = b"sibling a, new to the destination";
const SIBLING_Z: &[u8] = b"sibling z, new to the destination";

/// The volume id the destination's records name.
const NAS_ID: &str = "swap-outage-nas";
const WAIT: Duration = Duration::from_secs(10);
const CADENCE: &[Duration] = &[Duration::from_millis(10), Duration::from_millis(20)];

/// The five places a safe-replace lands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Site {
    Serial,
    Concurrent,
    MergeChild,
    Move,
    Extract,
}

impl Site {
    const ALL: [Site; 5] = [
        Site::Serial,
        Site::Concurrent,
        Site::MergeChild,
        Site::Move,
        Site::Extract,
    ];

    fn orig(self) -> &'static str {
        match self {
            Site::Serial | Site::Concurrent | Site::Move => "/dest/notes.txt",
            Site::MergeChild | Site::Extract => "/dest/album/notes.txt",
        }
    }

    fn folder(self) -> &'static str {
        match self {
            Site::Serial | Site::Concurrent | Site::Move => "/dest",
            Site::MergeChild | Site::Extract => "/dest/album",
        }
    }

    fn source_notes(self) -> &'static str {
        match self {
            Site::Serial | Site::Concurrent | Site::Move => "/notes.txt",
            Site::MergeChild | Site::Extract => "/album/notes.txt",
        }
    }

    /// A one-pass extract can't stream a member again.
    fn streams_again(self) -> bool {
        !matches!(self, Site::Extract)
    }
}

/// Where the share drops inside the swap.
#[derive(Clone, Copy, Debug)]
enum Cut {
    /// The move of `orig` aside never reaches the server.
    AsideUnsent,
    /// The server moved `orig` aside; the reply is lost.
    AsideUnanswered,
    /// The move aside went through; the rename onto `orig` never reaches the
    /// server. On a share that replaces in one step there's no move aside: the
    /// replacing rename never reaches the server.
    RenameUnsent,
    /// The move aside went through; the server renamed the temp onto `orig`,
    /// and the reply is lost. (On a one-step share: the replacing rename.)
    RenameUnanswered,
    /// The move aside answers the error of a session someone else already
    /// replaced: the share reads live on a newer generation (it never reached
    /// the server).
    StaleAsideError,
}

/// What happens while the copy waits for the share.
#[derive(Clone, Copy, Debug)]
enum Meanwhile {
    /// The share comes back.
    ComesBack,
    /// Someone removes our temp, then the share comes back.
    TempRemovedThenBack,
    /// The person cancels.
    Cancel,
    /// The share never comes back: the wait gives up.
    StaysGone,
}

/// How the share replaces a file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Replace {
    /// Delete, then rename: every backend without a one-step replace.
    DeleteThenRename,
    /// One forced rename (`Volume::supports_atomic_replace_rename`), as SMB's
    /// `ReplaceIfExists`: the name never goes empty.
    InOneStep,
}

/// A finished cell.
struct Ran {
    site: Site,
    result: Result<(), WriteOperationError>,
    engine: TestOperations,
    nas: Arc<OutageVolume>,
    dest_inner: Arc<InMemoryVolume>,
    source: Arc<dyn Volume>,
    source_inner: Option<Arc<InMemoryVolume>>,
    archive_root: Option<String>,
    ledger: Ledger,
    state: Arc<WriteOperationState>,
    _archive: Option<TarGzFixture>,
}

/// Runs one Overwrite at `site` with the share armed to drop at `cut`.
async fn run(site: Site, cut: Cut, meanwhile: Meanwhile) -> Ran {
    run_on(site, cut, meanwhile, Replace::DeleteThenRename).await
}

/// [`run`], on a share that replaces a file the way `replace` says.
async fn run_on(site: Site, cut: Cut, meanwhile: Meanwhile, replace: Replace) -> Ran {
    let op_id = format!("op-swap-outage-{site:?}-{cut:?}-{meanwhile:?}-{replace:?}");
    let engine = TestOperations::new();

    let dest_inner = Arc::new(InMemoryVolume::new("Naspolya").with_space_info(10_000_000, 10_000_000));
    dest_inner.create_directory(Path::new("/dest")).await.unwrap();
    if site.folder() != "/dest" {
        dest_inner.create_directory(Path::new(site.folder())).await.unwrap();
    }
    dest_inner
        .create_file(&Path::new(site.folder()).join("keep.txt"), KEEP)
        .await
        .unwrap();
    dest_inner.create_file(Path::new(site.orig()), OLD).await.unwrap();
    let nas = OutageVolume::wrap(Arc::clone(&dest_inner), &op_id);
    if replace == Replace::InOneStep {
        nas.replace_atomically();
    }
    let orig = Path::new(site.orig());
    match cut {
        Cut::AsideUnsent => nas.drop_at_rename_of(orig, RenameDrop::BeforeTheServerSeesIt),
        Cut::AsideUnanswered => nas.drop_at_rename_of(orig, RenameDrop::AfterTheServerPerformsIt),
        Cut::RenameUnsent => nas.drop_at_rename_onto(orig, RenameDrop::BeforeTheServerSeesIt),
        Cut::RenameUnanswered => nas.drop_at_rename_onto(orig, RenameDrop::AfterTheServerPerformsIt),
        Cut::StaleAsideError => nas.stale_error_at_rename_of(orig, RenameDrop::BeforeTheServerSeesIt),
    }

    let (source, source_inner, sources, archive) = if let Site::Extract = site {
        let archive = TarGzFixture::new(&[("album/notes.txt", NEW), ("album/fresh.txt", FRESH)]);
        let source = archive.volume();
        let sources = vec![archive.inner("album")];
        (source, None, sources, Some(archive))
    } else {
        let inner = Arc::new(InMemoryVolume::new("Source").with_space_info(10_000_000, 10_000_000));
        let sources = match site {
            Site::Serial | Site::Move => vec![PathBuf::from("/notes.txt")],
            Site::Concurrent => {
                inner.create_file(Path::new("/a.txt"), SIBLING_A).await.unwrap();
                inner.create_file(Path::new("/z.txt"), SIBLING_Z).await.unwrap();
                vec![
                    PathBuf::from("/a.txt"),
                    PathBuf::from("/notes.txt"),
                    PathBuf::from("/z.txt"),
                ]
            }
            Site::MergeChild => {
                inner.create_directory(Path::new("/album")).await.unwrap();
                inner.create_file(Path::new("/album/fresh.txt"), FRESH).await.unwrap();
                vec![PathBuf::from("/album")]
            }
            Site::Extract => unreachable!("handled above"),
        };
        inner.create_file(Path::new(site.source_notes()), NEW).await.unwrap();
        (Arc::clone(&inner) as Arc<dyn Volume>, Some(inner), sources, None)
    };

    let budget = match meanwhile {
        Meanwhile::StaysGone => Duration::from_millis(200),
        _ => Duration::from_secs(30),
    };
    let ledger = Ledger::for_test();
    let state = Arc::new(
        engine
            .new_operation_state(Duration::from_millis(50))
            .with_journal_volumes("swap-outage-source".into(), NAS_ID.into())
            .with_in_flight_ledger(ledger.clone())
            .with_outage_policy(OutagePolicy {
                budget,
                cadence: CADENCE,
                poll_every: Duration::from_millis(5),
                attempt_timeout: Duration::from_secs(1),
            }),
    );
    let op = engine.register_state(&op_id, Arc::clone(&state));
    let during = act_once_it_waits(&engine, &state, &nas, &dest_inner, site, meanwhile, op.id().as_str());

    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Overwrite,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };
    let events = Arc::new(CollectorEventSink::new());
    let dest = Arc::clone(&nas) as Arc<dyn Volume>;
    let result = tokio::time::timeout(Duration::from_secs(30), async {
        match site {
            Site::Move => {
                super::move_cross::move_volumes_with_progress(
                    events,
                    op.id().as_str(),
                    &state,
                    Arc::clone(&source),
                    &sources,
                    dest,
                    Path::new("/dest"),
                    &config,
                )
                .await
            }
            _ => {
                super::copy::copy_volumes_with_progress(
                    events,
                    op.id().as_str(),
                    &state,
                    Arc::clone(&source),
                    &sources,
                    dest,
                    Path::new("/dest"),
                    &config,
                )
                .await
            }
        }
    })
    .await
    .expect("the copy ends, one way or the other")
    .map(|_| ())
    .map_err(|failure| failure.error);
    during.abort();

    Ran {
        site,
        result,
        engine,
        nas,
        dest_inner,
        source,
        source_inner,
        archive_root: archive.as_ref().map(|a| a.path().display().to_string()),
        ledger,
        state,
        _archive: archive,
    }
}

/// Does what `meanwhile` says once the operation waits for the share.
fn act_once_it_waits(
    engine: &TestOperations,
    state: &Arc<WriteOperationState>,
    nas: &Arc<OutageVolume>,
    dest_inner: &Arc<InMemoryVolume>,
    site: Site,
    meanwhile: Meanwhile,
    op_id: &str,
) -> tokio::task::JoinHandle<()> {
    let state = Arc::clone(state);
    let nas = Arc::clone(nas);
    let dest_inner = Arc::clone(dest_inner);
    let engine = crate::Operations::clone(engine);
    let op_id = OperationId::from(op_id);
    tokio::spawn(async move {
        wait_until_async(WAIT, "the copy to wait for the share", || {
            state.outage.waiting_for().is_some()
        })
        .await;
        match meanwhile {
            Meanwhile::ComesBack => nas.become_reachable(),
            Meanwhile::TempRemovedThenBack => {
                for temp in temps_in(&dest_inner, site.folder()).await {
                    dest_inner.delete(&temp).await.expect("the temp goes");
                }
                nas.become_reachable();
            }
            Meanwhile::Cancel => engine.cancel_write_operation(&op_id, false),
            Meanwhile::StaysGone => {}
        }
    })
}

/// Every `.cmdr-temp-*` aside in `folder`.
async fn asides_in(volume: &InMemoryVolume, folder: &str) -> Vec<PathBuf> {
    volume
        .list_directory(Path::new(folder), None)
        .await
        .unwrap()
        .into_iter()
        .filter(|entry| entry.name.contains(".cmdr-temp-"))
        .map(|entry| PathBuf::from(entry.path))
        .collect()
}

/// Every `.cmdr-tmp-*` in `folder`.
async fn temps_in(volume: &InMemoryVolume, folder: &str) -> Vec<PathBuf> {
    volume
        .list_directory(Path::new(folder), None)
        .await
        .unwrap()
        .into_iter()
        .filter(|entry| entry.name.contains(".cmdr-tmp-"))
        .map(|entry| PathBuf::from(entry.path))
        .collect()
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
    fn label(&self) -> String {
        format!("{:?}", self.site)
    }

    async fn orig(&self) -> Option<Vec<u8>> {
        read_all(&*self.dest_inner, Path::new(self.site.orig())).await
    }

    async fn temps(&self) -> Vec<PathBuf> {
        temps_in(&self.dest_inner, self.site.folder()).await
    }

    async fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .dest_inner
            .list_directory(Path::new(self.site.folder()), None)
            .await
            .unwrap()
            .into_iter()
            .map(|entry| entry.name)
            .collect();
        names.sort();
        names
    }

    /// The Overwrite landed: the new bytes at the original's name, nothing
    /// rescued, no temp left, and the merge invariant held.
    async fn assert_landed(&self) {
        let label = self.label();
        assert!(
            self.result.is_ok(),
            "{label}: the overwrite lands, got {:?}",
            self.result
        );
        assert_eq!(
            self.orig().await.as_deref(),
            Some(NEW),
            "{label}: the new bytes at the name"
        );
        assert!(
            !self.names().await.iter().any(|name| name.contains("(recovered)")),
            "{label}: nothing rescued: {:?}",
            self.names().await
        );
        assert_eq!(self.temps().await, Vec::<PathBuf>::new(), "{label}: no temp left");
        assert_eq!(
            asides_in(&self.dest_inner, self.site.folder()).await,
            Vec::<PathBuf>::new(),
            "{label}: no aside left"
        );
        assert_eq!(
            self.ledger.recorded_landings(),
            Vec::<PathBuf>::new(),
            "{label}: no landing left recorded"
        );
        self.assert_safe(true).await;
    }

    /// How many times the replacing file streamed to the share.
    fn streams_of_the_file(&self) -> usize {
        self.nas.writes_for(Path::new(self.site.orig()))
    }

    /// The file wasn't streamed again.
    fn assert_streamed_once(&self) {
        assert_eq!(
            self.streams_of_the_file(),
            1,
            "{}: ❗ a whole temp lands without the file streaming again",
            self.label()
        );
    }

    /// The original is whole at its aside, its name empty, nothing recorded as
    /// a landing: the share's next arrival puts it back. Answers the aside.
    async fn assert_original_kept_aside(&self) -> PathBuf {
        let label = self.label();
        assert_eq!(self.orig().await, None, "{label}: the original left its name");
        let asides = asides_in(&self.dest_inner, self.site.folder()).await;
        let [aside] = asides.as_slice() else {
            panic!("{label}: exactly one aside holds the original, got {asides:?}");
        };
        assert_eq!(
            read_all(&*self.dest_inner, aside).await.as_deref(),
            Some(OLD),
            "{label}: ❗ the original survives, whole"
        );
        assert_eq!(
            self.ledger.recorded_landings(),
            Vec::<PathBuf>::new(),
            "{label}: no landing: the new bytes were never the only copy"
        );
        assert!(
            self.state.kept_landings.lock_ignore_poison().is_empty(),
            "{label}: and the stop names no unfinished landing"
        );
        if let Site::Move = self.site {
            assert!(
                self.source_still_holds_the_file().await,
                "{label}: a failed move keeps its source"
            );
        }
        aside.clone()
    }

    /// The share comes back and the registry announces it: the arrival sweep
    /// puts the original back at its name.
    async fn assert_the_arrival_puts_the_original_back(&self, aside: &Path) {
        let label = self.label();
        self.nas.reconnected_elsewhere();
        self.engine
            .volumes()
            .register(NAS_ID, Arc::clone(&self.nas) as Arc<dyn Volume>);
        wait_until_async(WAIT, "the arrival sweep to settle the aside's record", || {
            !self.ledger.live_paths().iter().any(|path| path == aside)
        })
        .await;
        assert!(!self.dest_inner.exists(aside).await, "{label}: and the aside is gone");
        assert_eq!(
            self.orig().await.as_deref(),
            Some(OLD),
            "{label}: the arrival sweep puts the original back at its name"
        );
    }

    async fn source_still_holds_the_file(&self) -> bool {
        let inner = self.source_inner.as_ref().expect("an in-memory source");
        read_all(&**inner, Path::new(self.site.source_notes())).await.as_deref() == Some(NEW)
    }

    async fn assert_safe(&self, delivered: bool) {
        let label = self.label();
        let source_root = self.archive_root.as_deref().unwrap_or("/");
        let mut source_files: Vec<(&str, &[u8])> = vec![("/notes.txt", NEW)];
        let mut deliveries: Vec<(&str, &[u8])> = Vec::new();
        let untouched: Vec<(&str, &[u8])>;
        match self.site {
            Site::Serial | Site::Move => {
                untouched = vec![("/keep.txt", KEEP)];
                deliveries.push(("/notes.txt", NEW));
            }
            Site::Concurrent => {
                source_files.extend([("/a.txt", SIBLING_A), ("/z.txt", SIBLING_Z)]);
                untouched = vec![("/keep.txt", KEEP)];
                deliveries.extend([("/notes.txt", NEW), ("/a.txt", SIBLING_A), ("/z.txt", SIBLING_Z)]);
            }
            Site::MergeChild | Site::Extract => {
                source_files.push(("/album/fresh.txt", FRESH));
                untouched = vec![("/album/keep.txt", KEEP)];
                deliveries.extend([("/album/notes.txt", NEW), ("/album/fresh.txt", FRESH)]);
            }
        }
        assert_operation_was_safe(
            &self.source,
            &(Arc::clone(&self.nas) as Arc<dyn Volume>),
            &SafetySpec {
                label: &label,
                source_root,
                dest_root: "/dest",
                source_files: &source_files,
                delivered: if delivered { &deliveries } else { &[] },
                untouched_dest: &untouched,
            },
        )
        .await;
    }
}

// ── The cells ───────────────────────────────────────────────────────────────

/// (a) The drop hits the move of `orig` aside before the server sees it: once
/// the share is back, the original is still at its name, so the swap moves it
/// aside again and lands.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_drop_before_the_original_moves_aside_waits_and_lands_at_every_site() {
    for site in Site::ALL {
        let ran = run(site, Cut::AsideUnsent, Meanwhile::ComesBack).await;
        assert!(ran.nas.reconnect_attempts() > 0, "{}: the share dropped", ran.label());
        ran.assert_landed().await;
        ran.assert_streamed_once();
    }
}

/// (b) The server moved `orig` aside and the drop swallowed the reply: once
/// the share is back, the aside is there, so the rename alone lands the temp,
/// ❌ without streaming the file again or parking it under ` (recovered)`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_move_aside_whose_reply_was_lost_resumes_the_landing_without_streaming_again_at_every_site() {
    for site in Site::ALL {
        let ran = run(site, Cut::AsideUnanswered, Meanwhile::ComesBack).await;
        assert!(ran.nas.reconnect_attempts() > 0, "{}: the share dropped", ran.label());
        ran.assert_landed().await;
        ran.assert_streamed_once();
    }
}

/// (c) The move aside went through and the drop hits the rename after it:
/// once the share is back, the rename lands it and the aside goes.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_drop_at_the_rename_after_the_move_aside_resumes_and_lands_at_every_site() {
    for site in Site::ALL {
        let ran = run(site, Cut::RenameUnsent, Meanwhile::ComesBack).await;
        assert!(ran.nas.reconnect_attempts() > 0, "{}: the share dropped", ran.label());
        ran.assert_landed().await;
        ran.assert_streamed_once();
    }
}

/// The server performed the rename after the move aside and the reply was
/// lost: the temp is gone and the name holds exactly the source's bytes, so the
/// file counts as landed and the aside goes. ❗ A one-pass extract has no source
/// to compare against, so it never claims the file: it reports, the file is
/// there anyway, and the aside stays recorded for the next sweep.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_rename_whose_reply_was_lost_counts_as_landed_only_on_the_sources_bytes() {
    for site in Site::ALL {
        let ran = run(site, Cut::RenameUnanswered, Meanwhile::ComesBack).await;
        let label = ran.label();
        assert!(ran.nas.reconnect_attempts() > 0, "{label}: the share dropped");
        if site.streams_again() {
            ran.assert_landed().await;
            ran.assert_streamed_once();
        } else {
            assert!(
                ran.result.is_err(),
                "{label}: ❗ never counted as landed without the proof"
            );
            assert_eq!(
                ran.orig().await.as_deref(),
                Some(NEW),
                "{label}: what the server landed stays"
            );
            assert_eq!(ran.temps().await, Vec::<PathBuf>::new(), "{label}");
            assert_eq!(ran.ledger.recorded_landings(), Vec::<PathBuf>::new(), "{label}");
        }
    }
}

/// (d) The outage outlasts the budget after the move aside: the original is
/// whole at its aside, recorded, and the share's next arrival puts it back.
/// The new bytes were never the only copy (the source holds them), so nothing
/// is kept as a landing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_outage_that_outlasts_the_wait_after_the_move_aside_keeps_the_original_for_the_arrival_sweep_at_every_site()
{
    for site in Site::ALL {
        let ran = run(site, Cut::RenameUnsent, Meanwhile::StaysGone).await;
        let label = ran.label();
        // In the concurrent window a sibling can meet the dead share first,
        // and the first failure is the operation's.
        if site != Site::Concurrent {
            assert!(
                matches!(ran.result, Err(WriteOperationError::ServerConnectionLost { .. })),
                "{label}: the failure says it was the connection, got {:?}",
                ran.result
            );
        } else {
            assert!(ran.result.is_err(), "{label}: the op fails");
        }
        let aside = ran.assert_original_kept_aside().await;
        ran.assert_the_arrival_puts_the_original_back(&aside).await;
    }
}

/// (e) The temp vanished during the outage while `orig` still holds the OLD
/// bytes at the same size: nothing proves the file landed (the byte compare
/// catches what the size can't), so it runs again and lands; a one-pass
/// extract can't, so it fails with the original in place.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_temp_gone_with_the_old_bytes_at_the_name_is_not_landed_at_every_site() {
    for site in Site::ALL {
        let ran = run(site, Cut::AsideUnsent, Meanwhile::TempRemovedThenBack).await;
        let label = ran.label();
        if site.streams_again() {
            ran.assert_landed().await;
            assert_eq!(ran.streams_of_the_file(), 2, "{label}: the file streamed again");
        } else {
            assert!(
                ran.result.is_err(),
                "{label}: ❗ never counted as landed on the old bytes"
            );
            assert_eq!(ran.orig().await.as_deref(), Some(OLD), "{label}: the original stays");
            assert_eq!(ran.temps().await, Vec::<PathBuf>::new(), "{label}");
        }
    }
}

/// (f) A stale transport error on the move aside while the share already
/// reads live on a newer generation: no discard and no refused-move stat on
/// the fresh session; the swap re-runs from the move aside, at once, with
/// nothing to dial.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_stale_error_on_the_move_aside_re_runs_the_swap_at_every_site() {
    for site in Site::ALL {
        let ran = run(site, Cut::StaleAsideError, Meanwhile::ComesBack).await;
        assert_eq!(ran.nas.reconnect_attempts(), 0, "{}: nothing to dial", ran.label());
        assert!(ran.nas.generation() > 0, "{}: the session was replaced", ran.label());
        ran.assert_landed().await;
        ran.assert_streamed_once();
    }
}

/// The same classify-before-acting for a claimed name (a `Rename` pick's
/// `O_EXCL` placeholder, or a name a cross-type Overwrite cleared, holding
/// something when the bytes come): the delete that clears it answers a stale
/// transport error with the share already live on a newer session. The
/// refused-delete stat on that fresh session would read the name as still
/// taken, throw the temp away, and end the copy on the first rename's
/// `AlreadyExists`, which no wait accepts. Cut off instead, the landing keeps
/// its temp, and its resume clears the name again and lands.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_stale_error_on_a_claimed_names_delete_keeps_the_temp_and_its_resume_lands() {
    let engine = TestOperations::new();
    let inner = Arc::new(InMemoryVolume::new("Naspolya"));
    inner.create_file(Path::new("/notes.txt"), b"").await.unwrap();
    let nas = OutageVolume::wrap(Arc::clone(&inner), "swap-outage-claimed-name");
    nas.stale_error_at_delete_of(Path::new("/notes.txt"), RenameDrop::BeforeTheServerSeesIt);
    let dest = Arc::clone(&nas) as Arc<dyn Volume>;
    let state = Arc::new(
        engine
            .new_operation_state(Duration::from_millis(50))
            .with_journal_volumes("swap-outage-source".into(), NAS_ID.into())
            .with_in_flight_ledger(Ledger::for_test()),
    );
    let claimed = state.scope().host().announce_on(&*dest, Path::new("/notes.txt"));
    let staged = StagedWrite::begin(&state, &*dest, claimed, WriteStaging::StageOntoClaimedName);
    let temp = staged.target().to_path_buf();
    inner.create_file(&temp, NEW).await.unwrap();

    let landed = staged.commit(&dest, NEW.len() as u64).await;

    assert!(nas.generation() > 0, "the session was replaced under the delete");
    assert_eq!(
        read_all(&*inner, &temp).await.as_deref(),
        Some(NEW),
        "❗ no discard on a guess made over the fresh session"
    );
    let Err(LandingError::CutOff(cut_off)) = landed else {
        panic!("a transport fault on the clearing delete cuts the landing off, got {landed:?}");
    };
    assert!(
        matches!(cut_off.resume(&dest, None).await, Resumed::Landed),
        "the resume lands it"
    );
    assert_eq!(read_all(&*inner, Path::new("/notes.txt")).await.as_deref(), Some(NEW));
    assert_eq!(temps_in(&inner, "/").await, Vec::<PathBuf>::new(), "no temp left");
}

/// (g) A Cancel during the wait after the move aside went through: the cancel
/// is reported as one, the original stays whole at its aside, recorded, and the
/// share's next arrival puts it back at its name.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cancel_during_the_wait_after_the_move_aside_leaves_the_original_for_the_arrival_sweep_at_every_site() {
    for site in Site::ALL {
        let ran = run(site, Cut::AsideUnanswered, Meanwhile::Cancel).await;
        let label = ran.label();
        assert!(
            matches!(ran.result, Err(WriteOperationError::Cancelled { .. })),
            "{label}: a cancel is reported as one, got {:?}",
            ran.result
        );
        let aside = ran.assert_original_kept_aside().await;
        ran.assert_the_arrival_puts_the_original_back(&aside).await;
    }
}

// ── A share that replaces a file in one step ────────────────────────────────

/// The drop hits a one-step replace before the server sees it: once the share
/// is back, the temp is still whole and the original still in place, so the
/// replace runs again and lands, without the file streaming again.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_drop_before_a_one_step_replace_waits_and_lands_at_every_site() {
    for site in Site::ALL {
        let ran = run_on(site, Cut::RenameUnsent, Meanwhile::ComesBack, Replace::InOneStep).await;
        assert!(ran.nas.reconnect_attempts() > 0, "{}: the share dropped", ran.label());
        ran.assert_landed().await;
        ran.assert_streamed_once();
    }
}

/// The server performed the one-step replace and the reply was lost: the temp
/// is gone and the name holds exactly the source's bytes, so the file counts as
/// landed. A one-pass extract has no source to compare against, so it reports,
/// and the file is there anyway.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_one_step_replace_whose_reply_was_lost_counts_as_landed_only_on_the_sources_bytes() {
    for site in Site::ALL {
        let ran = run_on(site, Cut::RenameUnanswered, Meanwhile::ComesBack, Replace::InOneStep).await;
        let label = ran.label();
        assert!(ran.nas.reconnect_attempts() > 0, "{label}: the share dropped");
        if site.streams_again() {
            ran.assert_landed().await;
            ran.assert_streamed_once();
        } else {
            assert!(
                ran.result.is_err(),
                "{label}: ❗ never counted as landed without the proof"
            );
            assert_eq!(
                ran.orig().await.as_deref(),
                Some(NEW),
                "{label}: what the server landed stays"
            );
            assert_eq!(ran.temps().await, Vec::<PathBuf>::new(), "{label}");
        }
        assert_eq!(ran.ledger.recorded_landings(), Vec::<PathBuf>::new(), "{label}");
    }
}

/// ❗ The point of the one-step replace: an outage that outlasts the wait at it
/// leaves the user's ORIGINAL whole at its name, with nothing recorded as a
/// landing or set aside, where the aside swap would have left the original at
/// its aside for the next sweep to put back.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_outage_that_outlasts_the_wait_at_a_one_step_replace_leaves_the_original_whole_at_every_site() {
    for site in Site::ALL {
        let ran = run_on(site, Cut::RenameUnsent, Meanwhile::StaysGone, Replace::InOneStep).await;
        let label = ran.label();
        assert!(ran.result.is_err(), "{label}: the op fails");
        assert_eq!(
            ran.orig().await.as_deref(),
            Some(OLD),
            "{label}: ❗ the original stays whole"
        );
        assert_eq!(
            ran.ledger.recorded_landings(),
            Vec::<PathBuf>::new(),
            "{label}: nothing was cleared, so nothing is a landing"
        );
        if site != Site::Concurrent {
            assert!(
                matches!(ran.result, Err(WriteOperationError::ServerConnectionLost { .. })),
                "{label}: the failure says it was the connection, got {:?}",
                ran.result
            );
        }
    }
}
