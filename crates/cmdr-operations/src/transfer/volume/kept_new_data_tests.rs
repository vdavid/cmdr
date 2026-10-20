//! A copy or move that stops names the new files its stop kept at the
//! destination: landings left recorded for the destination's next arrival,
//! and bytes a landing saved under a ` (recovered)` name (`KeptNewData`).
//!
//! Without it someone finds their original gone and a `.cmdr-tmp-*` or
//! ` (recovered)` file beside it with no word about it: a failed op's
//! `write-error` named neither, and a stop (or a failure drain) swallowed the
//! rescuing item's own error. The landings themselves are pinned where they
//! keep their bytes (`staged_write_tests.rs`); these cells pin what the
//! terminal event says, at the three drivers.

use std::future::Future;
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use super::faulty_volume::{FaultyOp, FaultyVolume, forward_volume_methods};
use super::{copy_volumes_with_progress, move_volumes_with_progress};
use crate::Operations;
use crate::event_sinks::CollectorEventSink;
use crate::state::WriteOperationState;
use crate::testing::TestOperations;
use crate::types::{StagedLeftoverFate, StagedLeftovers, VolumeCopyConfig, WriteOperationError};
use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::volume::{
    InMemoryVolume, StreamLength, StreamWriteProgress, StreamWritten, Volume, VolumeError, VolumeReadStream, WriteMode,
};

#[derive(Clone, Copy, Debug)]
enum Driver {
    SerialCopy,
    ConcurrentCopy,
    CrossVolumeMove,
}

const DRIVERS: [Driver; 3] = [Driver::SerialCopy, Driver::ConcurrentCopy, Driver::CrossVolumeMove];

/// What earlier landings of this operation kept, as they leave it on `state`.
fn landings_kept_earlier(state: &WriteOperationState) {
    state
        .kept_landings
        .lock_ignore_poison()
        .push(PathBuf::from("/inbox/waiting.txt"));
    state
        .rescued_landings
        .lock_ignore_poison()
        .push(PathBuf::from("/inbox/notes (recovered).txt"));
}

fn the_kept_new_files() -> Vec<StagedLeftovers> {
    vec![
        StagedLeftovers {
            fate: StagedLeftoverFate::LandsWhenBack,
            count: 1,
            example_name: "waiting.txt".to_owned(),
            volume_name: "Dest".to_owned(),
        },
        StagedLeftovers {
            fate: StagedLeftoverFate::Rescued,
            count: 1,
            example_name: "notes (recovered).txt".to_owned(),
            volume_name: "Dest".to_owned(),
        },
    ]
}

async fn run(
    driver: Driver,
    state: &Arc<WriteOperationState>,
    events: &Arc<CollectorEventSink>,
    dest: Arc<dyn Volume>,
) -> Result<(), WriteOperationError> {
    let source = Arc::new(InMemoryVolume::new("Source").with_space_info(10_000_000, 10_000_000));
    let count = match driver {
        Driver::ConcurrentCopy => 3,
        Driver::SerialCopy | Driver::CrossVolumeMove => 1,
    };
    let mut paths = Vec::new();
    for n in 0..count {
        let path = PathBuf::from(format!("/f{n}.txt"));
        source.create_file(&path, b"the user's bytes").await.unwrap();
        paths.push(path);
    }
    let source = source as Arc<dyn Volume>;
    let config = VolumeCopyConfig::default();
    let events = Arc::clone(events);
    let result = match driver {
        Driver::SerialCopy | Driver::ConcurrentCopy => {
            copy_volumes_with_progress(
                events,
                "op-kept",
                state,
                source,
                &paths,
                dest,
                Path::new("/inbox"),
                &config,
            )
            .await
        }
        Driver::CrossVolumeMove => {
            move_volumes_with_progress(
                events,
                "op-kept",
                state,
                source,
                &paths,
                dest,
                Path::new("/inbox"),
                &config,
            )
            .await
        }
    };
    result.map(|_| ()).map_err(|failure| failure.error)
}

async fn dest() -> Arc<InMemoryVolume> {
    let dest = Arc::new(InMemoryVolume::new("Dest").with_space_info(10_000_000, 10_000_000));
    dest.create_directory(Path::new("/inbox")).await.unwrap();
    dest
}

/// A failed transfer names the new files its stop kept beside its cause, at
/// every driver: the person learns where their new data is, and the cause
/// keeps its own advice.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_failure_names_the_new_files_its_stop_kept_beside_its_cause() {
    for driver in DRIVERS {
        let engine = TestOperations::new();
        let state = Arc::new(engine.new_operation_state(Duration::from_millis(50)));
        let events = Arc::new(CollectorEventSink::new());
        landings_kept_earlier(&state);
        let refused = VolumeError::PermissionDenied {
            path: "/the share's own words".to_owned(),
            raw_os_error: None,
        };
        let dest = FaultyVolume::wrapping(dest().await)
            .failing_call(FaultyOp::WriteFromStream, 1, refused)
            .arc();

        let error = run(driver, &state, &events, Arc::clone(&dest) as Arc<dyn Volume>)
            .await
            .expect_err("a refused write fails the transfer");

        assert!(dest.fault_fired(FaultyOp::WriteFromStream), "{driver:?}");
        let WriteOperationError::NewFilesKept { cause, kept } = &error else {
            panic!("{driver:?}: the failure names the kept files, got {error:?}");
        };
        assert!(
            matches!(**cause, WriteOperationError::PermissionDenied { .. }),
            "{driver:?}: the cause stays the refusal, got {cause:?}"
        );
        assert_eq!(kept, &the_kept_new_files(), "{driver:?}");
    }
}

/// A failure whose own error already names a rescue (`NewDataKeptAt`) isn't
/// told about it twice: only the OTHER kept files ride beside it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_failure_that_names_its_own_rescue_lists_only_the_others() {
    let engine = TestOperations::new();
    let state = Arc::new(engine.new_operation_state(Duration::from_millis(50)));
    let events = Arc::new(CollectorEventSink::new());
    let inner = dest().await;
    // The copy's own rescue: the name takes no rename once the way is cleared.
    inner.create_file(Path::new("/inbox/f0.txt"), b"OLD").await.unwrap();
    inner.set_rename_to_failing(Path::new("/inbox/f0.txt"));
    landings_kept_earlier(&state);
    let config = VolumeCopyConfig {
        conflict_resolution: crate::types::ConflictResolution::Overwrite,
        ..VolumeCopyConfig::default()
    };
    let source = Arc::new(InMemoryVolume::new("Source").with_space_info(10_000_000, 10_000_000));
    source.create_file(Path::new("/f0.txt"), b"NEW").await.unwrap();

    let error = copy_volumes_with_progress(
        events,
        "op-kept-own",
        &state,
        source as Arc<dyn Volume>,
        &[PathBuf::from("/f0.txt")],
        Arc::clone(&inner) as Arc<dyn Volume>,
        Path::new("/inbox"),
        &config,
    )
    .await
    .expect_err("a name that takes no rename fails the copy")
    .error;

    let WriteOperationError::NewFilesKept { cause, kept } = &error else {
        panic!("the earlier kept files ride beside the failure, got {error:?}");
    };
    assert!(
        matches!(&**cause, WriteOperationError::NewDataKeptAt { kept_at, .. } if kept_at == "/inbox/f0 (recovered).txt"),
        "got {cause:?}"
    );
    assert_eq!(kept, &the_kept_new_files(), "its own rescue isn't listed again");
}

/// A destination whose first write is where the person clicks Cancel: the
/// write ends as the backend's tier-1 cancel does.
struct CancelsAtTheFirstWrite {
    inner: Arc<InMemoryVolume>,
    engine: Operations,
    operation_id: String,
}

impl Volume for CancelsAtTheFirstWrite {
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
        rename,
        get_space_info,
        supports_streaming,
        open_read_stream,
        create_directory_errors_on_existing_dir,
    );

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn write_from_stream<'a>(
        &'a self,
        _dest: &'a Path,
        _mode: WriteMode,
        _length: StreamLength,
        _stream: Box<dyn VolumeReadStream>,
        _on_progress: &'a (dyn Fn(StreamWriteProgress) -> ControlFlow<()> + Sync),
    ) -> Pin<Box<dyn Future<Output = Result<StreamWritten, VolumeError>> + Send + 'a>> {
        self.engine.cancel_write_operation(&self.operation_id, false);
        Box::pin(async { Err(VolumeError::Cancelled("Operation cancelled by user".into())) })
    }
}

/// A Cancel swallows the item's own error, so `write-cancelled` names a rescue
/// the stop kept, as it names a landing waiting for the server.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cancel_names_the_new_files_its_stop_kept() {
    for driver in DRIVERS {
        let engine = TestOperations::new();
        let operation_id = format!("op-kept-cancel-{driver:?}");
        let guard = engine.register_state(
            &operation_id,
            Arc::new(engine.new_operation_state(Duration::from_millis(50))),
        );
        let state = Arc::clone(guard.state());
        let events = Arc::new(CollectorEventSink::new());
        landings_kept_earlier(&state);
        let dest = Arc::new(CancelsAtTheFirstWrite {
            inner: dest().await,
            engine: Operations::clone(&engine),
            operation_id,
        });

        let _ = run(driver, &state, &events, dest as Arc<dyn Volume>).await;

        let cancelled = events.cancelled.lock_ignore_poison();
        let [cancelled] = cancelled.as_slice() else {
            panic!("{driver:?}: one `write-cancelled`, got {cancelled:?}");
        };
        assert_eq!(cancelled.rollback.staged_leftovers, the_kept_new_files(), "{driver:?}");
    }
}
