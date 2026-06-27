//! Stale-destination-handle retry test for `volume_strategy.rs`'s
//! `copy_single_path`.
//!
//! `stream_pipe_file` retries once on `VolumeError::StaleDestinationHandle` (a
//! re-keyed MTP folder handle): it re-opens the source and re-runs the write
//! rather than surfacing the stale-handle rejection to the user. The
//! `FailOnceStaleDest` double rejects the first `write_from_stream` and accepts
//! the second, pinning that the engine calls `write_from_stream` exactly twice.

use super::test_support::FailOnceStaleDest;
use super::*;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use crate::file_system::volume::{LocalPosixVolume, Volume};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stream_pipe_file_retries_once_on_stale_destination_handle() {
    use std::fs;

    let src_dir = std::env::temp_dir().join("cmdr_retry_stale_src");
    let _ = fs::remove_dir_all(&src_dir);
    fs::create_dir_all(&src_dir).unwrap();
    fs::write(src_dir.join("a.txt"), "payload-bytes").unwrap(); // 13 bytes

    let source: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("Source", src_dir.to_str().unwrap()));
    let dest: Arc<dyn Volume> = Arc::new(FailOnceStaleDest {
        calls: AtomicUsize::new(0),
    });

    let state = Arc::new(WriteOperationState::new(Duration::from_millis(200)));

    let bytes = copy_single_path(
        &source,
        Path::new("a.txt"),
        false,
        None,
        &dest,
        Path::new("a.txt"),
        &state,
        &CreatedPaths::default(),
        &|_, _| ControlFlow::Continue(()),
        &|_| {},
        None,
    )
    .await
    .expect("a stale destination handle must be retried, not surfaced as a copy failure");

    assert_eq!(bytes, 13, "the retried copy reports the full byte count");
    let dest = dest.as_any().downcast_ref::<FailOnceStaleDest>().unwrap();
    assert_eq!(
        dest.calls.load(Ordering::SeqCst),
        2,
        "write_from_stream must be called exactly twice: the stale-handle rejection, then the retry"
    );

    let _ = fs::remove_dir_all(&src_dir);
}
