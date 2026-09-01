//! A real copy in both directions between a local disk and a live WebDAV
//! server, driven through the app's own `copy_between_volumes`.
//!
//! ❗ **The point is the entry point.** `cmdr-webdav`'s own Docker suite
//! exercises every method the copy engine calls, and SFTP's was fully green
//! while both directions of an actual copy were broken in the app: one on a
//! capability predicate the crate never stated (`supports_export`), the other on
//! a pre-flight check that read the backend's honest "I can't measure free
//! space" as "there's no room". Neither is reachable from inside a crate,
//! because neither lives there. So these cells start where the transfer dialog
//! starts.
//!
//! Both cells checksum the bytes at BOTH ends. A copy that lands a file of the
//! right length full of the wrong bytes is a data-loss bug that an `exists()`
//! assertion reports as a pass.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use cmdr_fs::volume::Volume;
use cmdr_webdav::volume::testing::{connect_fixture, scratch_dir};
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

/// Removes everything a cell built, deepest first, and the scratch dir itself.
async fn clean_scratch(volume: &dyn Volume, dir: &Path) {
    if let Ok(entries) = volume.list_directory(dir, None).await {
        for entry in entries {
            let _ = volume.delete(&dir.join(&entry.name)).await;
        }
    }
    let _ = volume.delete(dir).await;
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
#[ignore = "needs the WebDAV fixture stack: apps/desktop/test/webdav-servers/start.sh (webdav-fixture)"]
async fn webdav_integration_copying_off_a_server_lands_every_byte() {
    let remote = Arc::new(connect_fixture("APACHE", 13480).await);
    let dir = scratch_dir(&remote).await;

    let content = payload();
    let remote_file = dir.join("downloaded.bin");
    remote
        .create_file(&remote_file, &content)
        .await
        .expect("seed the file on the server");

    // The checksum at the SOURCE end, taken off the server itself rather than
    // from the buffer we wrote, so a bad seed can't make a bad copy look good.
    let source_digest = sha256(&read_all(remote.as_ref(), &remote_file).await);
    assert_eq!(source_digest, sha256(&content), "the fixture seed must round-trip");

    let local_dir = TestDir::new("webdav_copy_off_server");
    let local: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("Local", &*local_dir));

    run_copy(
        "copy-off-webdav",
        Arc::clone(&remote) as Arc<dyn Volume>,
        vec![remote_file.clone()],
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

    clean_scratch(remote.as_ref(), &dir).await;
}

/// Copy ONTO the server: the direction the free-space pre-flight killed on
/// SFTP.
///
/// Apache's `mod_dav_fs` reports no quota properties, so `get_space_info` has
/// nothing honest to say here either, and the pre-flight has to read that as
/// "unknown" rather than as "no room".
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the WebDAV fixture stack: apps/desktop/test/webdav-servers/start.sh (webdav-fixture)"]
async fn webdav_integration_copying_onto_a_server_lands_every_byte() {
    let remote = Arc::new(connect_fixture("APACHE", 13480).await);
    let dir = scratch_dir(&remote).await;

    let content = payload();
    let local_dir = TestDir::new("webdav_copy_onto_server");
    let local: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("Local", &*local_dir));
    local
        .create_file(Path::new("uploaded.bin"), &content)
        .await
        .expect("seed the local file");

    let source_digest = sha256(&read_all(local.as_ref(), Path::new("uploaded.bin")).await);
    assert_eq!(source_digest, sha256(&content), "the local seed must round-trip");

    run_copy(
        "copy-onto-webdav",
        Arc::clone(&local),
        vec![PathBuf::from("uploaded.bin")],
        Arc::clone(&remote) as Arc<dyn Volume>,
        dir.clone(),
    )
    .await;

    let landed = read_all(remote.as_ref(), &dir.join("uploaded.bin")).await;
    assert_eq!(landed.len(), content.len(), "the copy landed the wrong number of bytes");
    assert_eq!(
        sha256(&landed),
        source_digest,
        "the bytes on the server must checksum to what local disk holds"
    );

    clean_scratch(remote.as_ref(), &dir).await;
}
