//! A copy that never leaves the server, and the fallback for a server that can't
//! do one.
//!
//! ❗ Every cell works inside a scratch directory of its own. The whole binary
//! shares one export and `nextest` runs its cells in parallel, so a fixed name
//! would have two of them deleting each other's files and reporting it as a
//! backend bug.

use std::ops::ControlFlow;
use std::path::Path;
use std::sync::Mutex;

use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::volume::{Volume, VolumeError};

use super::super::SftpVolume;
use super::super::testing::*;

const FIXTURE: &str = "sftp-servers/start.sh (sftp-fixture)";

/// Enough bytes to be worth copying and small enough to seed over the wire in a
/// test: several read chunks, and a tail that isn't chunk-aligned.
const PAYLOAD: usize = 700_003;

/// A connected volume with an empty scratch directory on it.
async fn scratch_on(service: &str, fallback_port: u16, what: &str) -> (SftpVolume, String) {
    let params = fixture_params(service, fallback_port);
    let host = fixture_host(&params, Some(FIXTURE_PASSWORD));
    let volume = connect_fixture(&host, params).await;
    let dir = scratch_dir(what);
    clean_scratch(&volume, &dir).await;
    volume.create_directory(Path::new(&dir)).await.expect(FIXTURE);
    (volume, dir)
}

/// Reads a file back whole, so byte-exactness is asserted against what the server
/// actually holds.
async fn read_back(volume: &SftpVolume, path: &str, len: usize) -> Vec<u8> {
    volume.read_range(Path::new(path), 0, len).await.expect(FIXTURE)
}

/// The stock server copies for itself, byte for byte, ❗ without the bytes ever
/// crossing the link.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn sftp_integration_a_server_side_copy_lands_byte_for_byte() {
    let (volume, dir) = scratch_on("OPENSSH", 12480, "copy-within").await;
    let payload = fixture_large_bytes(PAYLOAD);
    volume
        .create_file(Path::new(&format!("{dir}/source.bin")), &payload)
        .await
        .expect(FIXTURE);

    let copied = volume
        .copy_within(
            Path::new(&format!("{dir}/source.bin")),
            Path::new(&format!("{dir}/copy.bin")),
            &|_, _| ControlFlow::Continue(()),
        )
        .await
        .expect(FIXTURE);

    assert_eq!(copied, PAYLOAD as u64);
    let landed = read_back(&volume, &format!("{dir}/copy.bin"), PAYLOAD).await;
    assert_same_bytes(&landed, &payload, "a server-side copy");

    clean_scratch(&volume, &dir).await;
}

/// Progress climbs to the whole file and never past it.
///
/// The chunk loop is the only reason there is progress at all: one `copy-data`
/// for the whole file would be a single unanswered request with nothing to
/// report and no place to stop.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn sftp_integration_a_server_side_copy_reports_progress_up_to_the_whole_file() {
    let (volume, dir) = scratch_on("OPENSSH", 12480, "copy-progress").await;
    let payload = fixture_large_bytes(PAYLOAD);
    volume
        .create_file(Path::new(&format!("{dir}/source.bin")), &payload)
        .await
        .expect(FIXTURE);

    let seen: Mutex<Vec<(u64, u64)>> = Mutex::new(Vec::new());
    volume
        .copy_within(
            Path::new(&format!("{dir}/source.bin")),
            Path::new(&format!("{dir}/copy.bin")),
            &|done, total| {
                seen.lock_ignore_poison().push((done, total));
                ControlFlow::Continue(())
            },
        )
        .await
        .expect(FIXTURE);

    // ❗ Scoped, ❌ not a `drop()`: `clippy::await_holding_lock` reads the
    // guard's LEXICAL span, so an explicit drop leaves it complaining.
    {
        let seen = seen.lock_ignore_poison();
        assert!(!seen.is_empty(), "a copy that reports nothing is a frozen progress bar");
        assert!(
            seen.iter()
                .all(|(done, total)| done <= total && *total == PAYLOAD as u64),
            "progress never claims more than the file holds: {seen:?}"
        );
        assert_eq!(seen.last(), Some(&(PAYLOAD as u64, PAYLOAD as u64)));
    }

    clean_scratch(&volume, &dir).await;
}

/// A cancel takes the partial with it.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn sftp_integration_a_cancelled_server_side_copy_leaves_nothing_behind() {
    let (volume, dir) = scratch_on("OPENSSH", 12480, "copy-cancel").await;
    volume
        .create_file(Path::new(&format!("{dir}/source.bin")), b"stop right there")
        .await
        .expect(FIXTURE);

    let outcome = volume
        .copy_within(
            Path::new(&format!("{dir}/source.bin")),
            Path::new(&format!("{dir}/copy.bin")),
            &|_, _| ControlFlow::Break(()),
        )
        .await;

    assert!(
        matches!(outcome, Err(VolumeError::Cancelled(_))),
        "the only cancellation this path has is the callback: {outcome:?}"
    );
    assert!(
        !volume.exists(Path::new(&format!("{dir}/copy.bin"))).await,
        "❗ every error path takes the partial with it, cancellation included"
    );

    clean_scratch(&volume, &dir).await;
}

/// An empty file copies to an empty file, and says so once.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn sftp_integration_an_empty_file_copies_to_an_empty_file() {
    let (volume, dir) = scratch_on("OPENSSH", 12480, "copy-empty").await;
    volume
        .create_file(Path::new(&format!("{dir}/empty.bin")), b"")
        .await
        .expect(FIXTURE);

    let copied = volume
        .copy_within(
            Path::new(&format!("{dir}/empty.bin")),
            Path::new(&format!("{dir}/empty-copy.bin")),
            &|_, _| ControlFlow::Continue(()),
        )
        .await
        .expect(FIXTURE);

    assert_eq!(copied, 0);
    let entry = volume
        .get_metadata(Path::new(&format!("{dir}/empty-copy.bin")))
        .await
        .expect(FIXTURE);
    assert_eq!(entry.size, Some(0));

    clean_scratch(&volume, &dir).await;
}

/// ⚠️ **A server without `copy-data@openssh.com` says so**, which is what makes
/// the caller stream the file instead.
///
/// `sftp-fixture-noposixrename` drops the extension from its hello, the way
/// proprietary NAS firmware does. ❌ Answering anything but `NotSupported` here
/// would turn "this server can't" into a failed copy.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn sftp_integration_a_server_without_the_extension_refuses_rather_than_failing() {
    let (volume, dir) = scratch_on("NOPOSIXRENAME", 12486, "copy-unsupported").await;
    volume
        .create_file(Path::new(&format!("{dir}/source.bin")), b"still here")
        .await
        .expect(FIXTURE);

    let outcome = volume
        .copy_within(
            Path::new(&format!("{dir}/source.bin")),
            Path::new(&format!("{dir}/copy.bin")),
            &|_, _| ControlFlow::Continue(()),
        )
        .await;

    assert!(
        matches!(outcome, Err(VolumeError::NotSupported)),
        "the caller reads this as 'stream it', so it can be nothing else: {outcome:?}"
    );
    assert!(
        !volume.exists(Path::new(&format!("{dir}/copy.bin"))).await,
        "and a refusal touches nothing: the destination must be free for the streamed attempt"
    );
    assert!(
        volume.exists(Path::new(&format!("{dir}/source.bin"))).await,
        "the source is untouched too"
    );

    clean_scratch(&volume, &dir).await;
}

/// A path outside the volume root is refused before anything is opened.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn sftp_integration_a_copy_out_of_the_root_is_refused() {
    let (volume, dir) = scratch_on("OPENSSH", 12480, "copy-escape").await;

    let outcome = volume
        .copy_within(
            Path::new("/etc/passwd"),
            Path::new(&format!("{dir}/stolen")),
            &|_, _| ControlFlow::Continue(()),
        )
        .await;

    assert!(
        matches!(outcome, Err(VolumeError::NotFound(_))),
        "an out-of-root path is refused, ❌ never anchored: {outcome:?}"
    );

    clean_scratch(&volume, &dir).await;
}
