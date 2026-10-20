//! A landing step that meets a transport blip with nothing to wait for (a stale
//! MTP handle, an OS-mounted share's timeout, a live session's timeout) gets the
//! same per-file retry a write does (`retry.rs`), and the resume after it
//! settles what the step did (`CutOffLanding::resume`).
//!
//! What these prove: a blip never parks the new bytes under ` (recovered)` with
//! the original already gone, a rename whose reply was lost counts as landed
//! rather than reporting a temp that no longer exists, and the retry is still
//! capped.

use super::super::faulty_volume::forward_volume_methods;
use super::test_support::make_state;
use super::*;
use crate::testing::TestOperations;
use crate::transfer::recovered_name::LandingAftermath;
use crate::transfer::retry;
use crate::transfer::transfer_driver::LeafProgressLedger;
use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::volume::{InMemoryVolume, Volume, VolumeError};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

const PAYLOAD: &[u8] = b"the-bytes-that-must-arrive-whole";

/// What a blipping rename did on the server before its reply was lost.
#[derive(Clone, Copy)]
enum Blip {
    /// Nothing: the request never reached it.
    Unperformed,
    /// The rename: only the reply went missing.
    Performed,
}

/// An in-memory destination whose renames onto `/a.txt` time out `blips`
/// times, the timed-out ones performed or not as `blip` says. No connection
/// state, so the outage wait has nothing to wait for (`NotWaitable`).
struct LandingBlips {
    inner: Arc<InMemoryVolume>,
    blip: Blip,
    blips: usize,
    renames_onto_the_name: AtomicUsize,
}

impl LandingBlips {
    fn new(blip: Blip, blips: usize) -> Arc<Self> {
        Arc::new(Self {
            inner: Arc::new(InMemoryVolume::new("Dest").with_space_info(10_000_000, 10_000_000)),
            blip,
            blips,
            renames_onto_the_name: AtomicUsize::new(0),
        })
    }

    fn renames_onto_the_name(&self) -> usize {
        self.renames_onto_the_name.load(Ordering::SeqCst)
    }

    async fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .inner
            .list_directory(Path::new("/"), None)
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|e| e.name)
            .collect();
        names.sort();
        names
    }

    async fn read(&self, path: &str) -> Option<Vec<u8>> {
        let mut stream = self.inner.open_read_stream(Path::new(path)).await.ok()?;
        let mut buf = Vec::new();
        while let Some(Ok(chunk)) = stream.next_chunk().await {
            buf.extend_from_slice(&chunk);
        }
        Some(buf)
    }
}

impl Volume for LandingBlips {
    forward_volume_methods!(
        inner => name,
        root,
        list_directory,
        get_metadata,
        exists,
        is_directory,
        create_directory,
        create_directory_all,
        create_file,
        delete,
        get_space_info,
        supports_streaming,
        open_read_stream,
        write_from_stream,
        create_directory_errors_on_existing_dir,
    );

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn rename<'a>(
        &'a self,
        from: &'a Path,
        to: &'a Path,
        force: bool,
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            // Elsewhere, or onto a name that's taken (which answers as it
            // would), the rename goes through untouched and uncounted.
            if to != Path::new("/a.txt") || self.inner.exists(to).await {
                return self.inner.rename(from, to, force).await;
            }
            let call = self.renames_onto_the_name.fetch_add(1, Ordering::SeqCst);
            if call >= self.blips {
                return self.inner.rename(from, to, force).await;
            }
            if matches!(self.blip, Blip::Performed) {
                self.inner.rename(from, to, force).await?;
            }
            Err(VolumeError::ConnectionTimeout("the rename's reply never came".into()))
        })
    }
}

async fn source_with_payload() -> Arc<dyn Volume> {
    let inner = Arc::new(InMemoryVolume::new("source").with_space_info(10_000_000, 10_000_000));
    inner.create_file(Path::new("/a.txt"), PAYLOAD).await.unwrap();
    inner as Arc<dyn Volume>
}

/// Runs `/a.txt` through the copy engine onto `/a.txt` at `dest`.
async fn copy_one(
    dest: &Arc<LandingBlips>,
    state: &Arc<WriteOperationState>,
    staging: WriteStaging,
) -> Result<u64, PathedVolumeError> {
    let source = source_with_payload().await;
    let dest: Arc<dyn Volume> = Arc::clone(dest) as Arc<dyn Volume>;
    copy_single_path(
        &source,
        Path::new("/a.txt"),
        Some(false),
        SourceFileFacts::default(),
        &dest,
        Path::new("/a.txt"),
        state,
        &CreatedPaths::default(),
        &LeafProgressLedger::silent_source(Arc::clone(state)),
        None,
        staging,
    )
    .await
}

/// A first rename that blips with nothing to wait for runs the landing again,
/// and the file lands, instead of the copy failing on a blip a write would
/// have ridden out.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_first_rename_that_blips_lands_on_the_retry() {
    let engine = TestOperations::new();
    let guard = engine.register_state("landing-blip-first", make_state(&engine));
    let dest = LandingBlips::new(Blip::Unperformed, 1);

    let bytes = copy_one(&dest, guard.state(), WriteStaging::Stage)
        .await
        .expect("a blip on the landing rename must not end the file");

    assert_eq!(bytes, PAYLOAD.len() as u64);
    assert_eq!(dest.renames_onto_the_name(), 2, "the blip, then the retry that landed");
    assert_eq!(dest.read("/a.txt").await.as_deref(), Some(PAYLOAD));
    assert_eq!(dest.names().await, vec!["a.txt".to_owned()], "no temp left beside it");
    assert!(guard.state().in_flight_temps.lock_ignore_poison().is_empty());
}

/// ❗ The rename onto a name the landing already cleared: a blip there used to
/// park the new bytes under ` (recovered)` with the original already gone. The
/// retry lands them at their name.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_rename_onto_a_cleared_name_that_blips_lands_instead_of_being_rescued() {
    let engine = TestOperations::new();
    let guard = engine.register_state("landing-blip-cleared", make_state(&engine));
    let dest = LandingBlips::new(Blip::Unperformed, 1);
    // A `Rename` pick's placeholder: the name this write claimed.
    dest.inner.create_file(Path::new("/a.txt"), b"").await.unwrap();

    copy_one(&dest, guard.state(), WriteStaging::StageOntoClaimedName)
        .await
        .expect("a blip on the rename onto the cleared name must not end the file");

    assert_eq!(dest.read("/a.txt").await.as_deref(), Some(PAYLOAD));
    assert_eq!(
        dest.names().await,
        vec!["a.txt".to_owned()],
        "no ` (recovered)` file and no temp: the bytes are at their name"
    );
    assert!(guard.state().kept_landings.lock_ignore_poison().is_empty());
}

/// A rename the server performed before its reply was lost: the resume finds
/// the temp gone and the name holding the source's bytes, so the file counts as
/// landed. Before, the rescue's rename found no temp and the report named one
/// that no longer existed.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_rename_whose_reply_was_lost_counts_as_landed_and_names_no_missing_temp() {
    let engine = TestOperations::new();
    let guard = engine.register_state("landing-blip-performed", make_state(&engine));
    let dest = LandingBlips::new(Blip::Performed, 1);
    // A `Rename` pick's placeholder: the name this write claimed.
    dest.inner.create_file(Path::new("/a.txt"), b"").await.unwrap();

    let bytes = copy_one(&dest, guard.state(), WriteStaging::StageOntoClaimedName)
        .await
        .expect("a rename the server performed is a landed file");

    assert_eq!(bytes, PAYLOAD.len() as u64);
    assert_eq!(dest.read("/a.txt").await.as_deref(), Some(PAYLOAD));
    assert_eq!(dest.names().await, vec!["a.txt".to_owned()]);
    assert!(guard.state().kept_landings.lock_ignore_poison().is_empty());
}

/// The landing's retry is the write's: capped. A rename that never stops
/// blipping fails the file after `MAX_ATTEMPTS` tries.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_landing_that_never_stops_blipping_fails_at_the_attempt_cap() {
    let engine = TestOperations::new();
    let guard = engine.register_state("landing-blip-cap", make_state(&engine));
    let dest = LandingBlips::new(Blip::Unperformed, usize::MAX);

    let failed = copy_one(&dest, guard.state(), WriteStaging::Stage)
        .await
        .expect_err("a landing that never answers must surface");

    assert!(
        matches!(failed.error, VolumeError::ConnectionTimeout(_)),
        "got {failed:?}"
    );
    assert_eq!(dest.renames_onto_the_name(), retry::MAX_ATTEMPTS as usize);
    assert_eq!(dest.read("/a.txt").await, None, "nothing wears the name");
}

/// Once the cap is spent on a cleared name, the share still answers, so the
/// new bytes leave temp space under a ` (recovered)` name and the failure
/// names it: the original is gone, and this is the only copy.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cleared_name_that_never_takes_the_rename_rescues_the_bytes_after_the_cap() {
    let engine = TestOperations::new();
    let guard = engine.register_state("landing-blip-cap-cleared", make_state(&engine));
    let dest = LandingBlips::new(Blip::Unperformed, usize::MAX);
    // A `Rename` pick's placeholder: the name this write claimed.
    dest.inner.create_file(Path::new("/a.txt"), b"").await.unwrap();

    let failed = copy_one(&dest, guard.state(), WriteStaging::StageOntoClaimedName)
        .await
        .expect_err("a landing that never answers must surface");

    assert_eq!(dest.renames_onto_the_name(), retry::MAX_ATTEMPTS as usize);
    assert_eq!(
        failed.aftermath,
        Some(LandingAftermath::NewDataKeptAt(PathBuf::from("/a (recovered).txt")))
    );
    assert_eq!(dest.read("/a (recovered).txt").await.as_deref(), Some(PAYLOAD));
}
