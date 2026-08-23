//! A real copy in both directions between a local disk and a live SFTP server,
//! driven through the app's own `copy_between_volumes`.
//!
//! ❗ **The point is the entry point.** `cmdr-sftp`'s own Docker suite exercises
//! every method the copy engine calls, and it was fully green while both
//! directions of an actual copy were broken in the app: one on a capability
//! predicate the crate never states (`supports_export`), the other on a
//! pre-flight check that read the backend's honest "I can't measure free space"
//! as "there's no room". Neither is reachable from inside the crate, because
//! neither lives there. So these cells start where the transfer dialog starts.
//!
//! Both cells checksum the bytes at BOTH ends. A copy that lands a file of the
//! right length full of the wrong bytes is a data-loss bug that an `exists()`
//! assertion reports as a pass.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use cmdr_fs::volume::Volume;
use cmdr_sftp::volume::testing::{FIXTURE_PASSWORD, clean_scratch, connect_fixture, fixture_host, fixture_params, scratch_dir};
use sha2::{Digest, Sha256};

use super::event_sinks::{CollectorEventSink, OperationEventSink};
use super::types::VolumeCopyConfig;
use crate::file_system::volume::LocalPosixVolume;
use crate::ignore_poison::IgnorePoison;
use crate::operation_log::types::Initiator;
use crate::test_support::TestDir;

/// Big enough to cross the read and write windows rather than ride in one
/// request, so reassembly and offset bookkeeping are actually exercised.
const PAYLOAD_BYTES: usize = 700_000;

/// A payload whose every position says where it belongs, so a hole or a
/// duplicated span shifts content the checksum can't miss (and a mismatch is
/// diagnosable by eye).
fn payload() -> Vec<u8> {
    let mut out = Vec::with_capacity(PAYLOAD_BYTES);
    let mut line = 0u64;
    while out.len() < PAYLOAD_BYTES {
        out.extend_from_slice(format!("{line:015}\n").as_bytes());
        line += 1;
    }
    out.truncate(PAYLOAD_BYTES);
    out
}

fn sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// Every byte at `path`, read back through the volume's own streaming path
/// (which is what a copy uses), so the check covers the same machinery.
async fn read_all(volume: &dyn Volume, path: &Path) -> Vec<u8> {
    let mut stream = volume
        .open_read_stream(path)
        .await
        .unwrap_or_else(|e| panic!("reading {} back: {e:?}", path.display()));
    let mut out = Vec::new();
    while let Some(chunk) = stream.next_chunk().await {
        out.extend_from_slice(&chunk.unwrap_or_else(|e| panic!("chunk of {}: {e:?}", path.display())));
    }
    out
}

/// Runs one `copy_between_volumes` to completion and fails loudly with whatever
/// the sink collected, since a transfer reports through events rather than by
/// panicking.
async fn run_copy(
    label: &str,
    source: Arc<dyn Volume>,
    source_paths: Vec<PathBuf>,
    dest: Arc<dyn Volume>,
    dest_path: PathBuf,
) {
    let collector = Arc::new(CollectorEventSink::new());
    let events: Arc<dyn OperationEventSink> = collector.clone();

    super::transfer::volume::copy_between_volumes(
        events,
        format!("{label}-source"),
        Arc::clone(&source),
        source_paths,
        format!("{label}-dest"),
        Arc::clone(&dest),
        dest_path,
        VolumeCopyConfig::default(),
        Initiator::User,
        None,
    )
    .await
    .unwrap_or_else(|e| panic!("{label}: the copy must START; it was refused with {e:?}"));

    crate::test_support::wait_until_async(Duration::from_secs(60), "the copy to settle", || {
        !collector.settled.lock_ignore_poison().is_empty()
    })
    .await;

    let errors: Vec<String> = collector
        .errors
        .lock_ignore_poison()
        .iter()
        .map(|e| format!("{:?}", e.error))
        .collect();
    assert!(errors.is_empty(), "{label}: the copy reported {errors:?}");
}

/// Copy OFF the server: the direction `supports_export()` gates.
///
/// Before that predicate was stated, this never started at all — `copy_between_volumes`
/// refused it synchronously and logged nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn sftp_integration_copying_off_a_server_lands_every_byte() {
    let params = fixture_params("OPENSSH", 12480);
    let host = fixture_host(&params, Some(FIXTURE_PASSWORD));
    let remote = Arc::new(connect_fixture(&host, params).await);
    let dir = scratch_dir("app-copy-from");
    clean_scratch(&remote, &dir).await;
    remote.create_directory(Path::new(&dir)).await.expect("scratch dir");

    let content = payload();
    let remote_file = format!("{dir}/downloaded.bin");
    remote
        .create_file(Path::new(&remote_file), &content)
        .await
        .expect("seed the file on the server");

    // The checksum at the SOURCE end, taken off the server itself rather than
    // from the buffer we wrote, so a bad seed can't make a bad copy look good.
    let source_digest = sha256(&read_all(remote.as_ref(), Path::new(&remote_file)).await);
    assert_eq!(source_digest, sha256(&content), "the fixture seed must round-trip");

    let local_dir = TestDir::new("sftp_copy_off_server");
    let local: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("Local", &*local_dir));

    run_copy(
        "copy-off-sftp",
        Arc::clone(&remote) as Arc<dyn Volume>,
        vec![PathBuf::from(&remote_file)],
        Arc::clone(&local),
        PathBuf::from(""),
    )
    .await;

    let landed = read_all(local.as_ref(), Path::new("downloaded.bin")).await;
    assert_eq!(landed.len(), content.len(), "the copy landed the wrong number of bytes");
    assert_eq!(
        sha256(&landed),
        source_digest,
        "the bytes on local disk must checksum to what the server holds"
    );

    clean_scratch(&remote, &dir).await;
}

/// Copy ONTO the server: the direction the free-space pre-flight killed.
///
/// SFTP answers `get_space_info` with `NotSupported` on purpose, and the
/// pre-flight used to propagate that as a failure, so every copy in died after
/// roughly half a second with a message that named neither the check nor the
/// reason.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn sftp_integration_copying_onto_a_server_lands_every_byte() {
    let params = fixture_params("OPENSSH", 12480);
    let host = fixture_host(&params, Some(FIXTURE_PASSWORD));
    let remote = Arc::new(connect_fixture(&host, params).await);
    let dir = scratch_dir("app-copy-to");
    clean_scratch(&remote, &dir).await;
    remote.create_directory(Path::new(&dir)).await.expect("scratch dir");

    // ❗ The destination really can't answer the space question, which is the
    // whole point of this cell.
    assert!(
        matches!(remote.get_space_info().await, Err(cmdr_fs::volume::VolumeError::NotSupported)),
        "this cell only means something while the server can't report free space"
    );

    let content = payload();
    let local_dir = TestDir::new("sftp_copy_onto_server");
    let local: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("Local", &*local_dir));
    local
        .create_file(Path::new("uploaded.bin"), &content)
        .await
        .expect("seed the local file");

    let source_digest = sha256(&read_all(local.as_ref(), Path::new("uploaded.bin")).await);
    assert_eq!(source_digest, sha256(&content), "the local seed must round-trip");

    run_copy(
        "copy-onto-sftp",
        Arc::clone(&local),
        vec![PathBuf::from("uploaded.bin")],
        Arc::clone(&remote) as Arc<dyn Volume>,
        PathBuf::from(&dir),
    )
    .await;

    let landed = read_all(remote.as_ref(), Path::new(&format!("{dir}/uploaded.bin"))).await;
    assert_eq!(landed.len(), content.len(), "the copy landed the wrong number of bytes");
    assert_eq!(
        sha256(&landed),
        source_digest,
        "the bytes on the server must checksum to what local disk holds"
    );

    clean_scratch(&remote, &dir).await;
}
