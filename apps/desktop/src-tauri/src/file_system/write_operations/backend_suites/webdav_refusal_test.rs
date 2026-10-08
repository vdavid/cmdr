//! A WebDAV server that says no in the middle of an operation: 507 when a disk
//! or an account quota fills up, 403 when permissions change under a running
//! copy. Every cell asks the same three things of the refusal:
//!
//! - **It arrives typed.** `DestinationFull` for 507 and `PermissionDenied` for
//!   403, through `cmdr-webdav`'s status table (`errors.rs::map_status`) and the
//!   engine's `map_volume_error`, so the dialog can say what happened.
//! - **Nothing the user had is lost**, and nothing wears a real name that
//!   doesn't hold complete bytes, and no Cmdr scratch is left behind.
//! - ❗ **The server still counts as connected.** A refusal is an ANSWER. A
//!   backend that read it as a dropped connection would flip the volume to
//!   `Disconnected` and start the reconnect backoff over a full disk.
//!
//! The refusal comes from `webdav_refusing_proxy_test_support.rs`, an HTTP
//! proxy in front of the real Apache fixture that answers one kind of request
//! itself. The 507 runs both ways a server sends one: after taking the body in
//! (Apache with a full disk), and the moment the head arrives with the body
//! unread (a quota checked against `Content-Length`, which is what Nextcloud
//! does).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use cmdr_fs::volume::{ConnectionState, Volume};
use cmdr_webdav::volume::testing::scratch_dir;

use super::super::event_sinks::CollectorEventSink;
use super::super::state::WriteOperationState;
use super::super::transfer::volume::{copy_volumes_with_progress, move_within_same_volume_with_progress};
use super::super::types::{ConflictResolution, VolumeCopyConfig, WriteOperationError};
use super::network_semantics_test_support::{local_volume, names_in, seed, try_read};
use super::network_transfer_test_support::{assert_no_staging_litter, clean_deep, self_describing_bytes};
use super::webdav_refusing_proxy_test_support::{Answer, PathRule, Refusal, RefusingProxy, through_a_refusing_proxy};
use super::webdav_test_support::{WebdavFixture, connect};

/// The user's folder on the server: a file nothing may touch.
const USERS_FILE: (&str, &[u8]) = ("keep.txt", b"DEST-keep");

/// A live volume behind a proxy refusing `refusal`, and a scratch directory on
/// it. The proxy has to outlive every request the cell makes.
async fn refusing(refusal: Refusal) -> (RefusingProxy, Arc<dyn Volume>, PathBuf) {
    let (proxy, volume) = through_a_refusing_proxy(refusal).await;
    let dir = scratch_dir(&volume).await;
    (proxy, Arc::new(volume), dir)
}

/// Removes a cell's scratch directory through a DIRECT connection, since the
/// proxy would refuse some of the cleanup too (a cell refusing `DELETE` of a
/// name refuses it for the cleanup as well). The proxied volume's paths carry
/// the proxy's port, so the directory is found again by name.
async fn clean_up_past_the_proxy(dir: &Path) {
    let direct = connect(WebdavFixture::Stock).await;
    let name = dir.file_name().expect("a scratch dir has a name");
    clean_deep(&direct, &direct.root().join(name)).await;
}

fn config(policy: ConflictResolution) -> VolumeCopyConfig {
    VolumeCopyConfig {
        conflict_resolution: policy,
        ..VolumeCopyConfig::default()
    }
}

/// The three claims every refusal owes, after the operation has ended.
async fn assert_refused_safely(
    remote: &Arc<dyn Volume>,
    proxy: &RefusingProxy,
    error: &WriteOperationError,
    expected: fn(&WriteOperationError) -> bool,
    what: &str,
) {
    assert!(
        proxy.refused() >= 1,
        "{what}: the premise is that the server refused a request, and the proxy refused none"
    );
    assert!(expected(error), "{what}: the refusal must arrive typed, got {error:?}");
    assert_eq!(
        remote.connection_state(),
        Some(ConnectionState::Direct),
        "❗ {what}: a refusal is an answer, so the server must still count as connected"
    );
}

/// Copies a local `album` holding `files` into the server's `album`, which
/// already holds the user's file, and returns the copy's failure.
async fn copy_album_onto_the_server(
    remote: &Arc<dyn Volume>,
    dir: &Path,
    files: &[(&str, &[u8])],
    label: &str,
) -> WriteOperationError {
    seed(remote.as_ref(), &dir.join("album"), &[USERS_FILE]).await;
    let (_local_dir, local) = local_volume(label);
    seed(local.as_ref(), Path::new("album"), files).await;

    let state = Arc::new(WriteOperationState::new(Duration::from_millis(200)));
    let result = copy_volumes_with_progress(
        Arc::new(CollectorEventSink::new()),
        label,
        &state,
        Arc::clone(&local),
        &[PathBuf::from("album")],
        Arc::clone(remote),
        dir,
        &config(ConflictResolution::Skip),
    )
    .await;
    match result {
        Ok(()) => panic!("{label}: a copy the server refused a file of must not report success"),
        Err(failure) => failure.error,
    }
}

/// What a folder copy the server ran out of room in must leave: the user's file,
/// every file that landed complete at its name, the refused one nowhere, and no
/// scratch.
async fn assert_the_album_is_whole(remote: &Arc<dyn Volume>, dir: &Path, files: &[(&str, &[u8])], refused: &str) {
    let album = dir.join("album");
    assert_eq!(
        try_read(remote.as_ref(), &album.join(USERS_FILE.0)).await.as_deref(),
        Some(USERS_FILE.1),
        "❗ the user's file in the folder the copy merged into must survive the refusal"
    );
    assert!(
        !remote.exists(&album.join(refused)).await,
        "❗ the file the server had no room for must not appear at its name"
    );
    for (name, bytes) in files.iter().filter(|(name, _)| *name != refused) {
        if let Some(landed) = try_read(remote.as_ref(), &album.join(name)).await {
            assert_eq!(landed.as_slice(), *bytes, "{name} landed, so it must be complete");
        }
    }
    assert_no_staging_litter(remote.as_ref(), &album, "a copy the server ran out of room in").await;
}

/// Three files, the middle one big enough that a server answering before the
/// body leaves most of it unsent.
fn arriving_album() -> Vec<(&'static str, Vec<u8>)> {
    vec![
        ("a-first.bin", b"SRC-first".to_vec()),
        ("no-room.bin", self_describing_bytes(3 * 1024 * 1024, "no-room")),
        ("z-last.bin", b"SRC-last".to_vec()),
    ]
}

async fn out_of_room_mid_folder(answer: Answer, label: &str) {
    let (proxy, remote, dir) = refusing(Refusal {
        method: "PUT",
        path: PathRule::Contains("no-room.bin"),
        status: 507,
        answer,
    })
    .await;
    let owned = arriving_album();
    let files: Vec<(&str, &[u8])> = owned.iter().map(|(n, b)| (*n, b.as_slice())).collect();

    let error = copy_album_onto_the_server(&remote, &dir, &files, label).await;

    assert_refused_safely(
        &remote,
        &proxy,
        &error,
        |e| matches!(e, WriteOperationError::DestinationFull { .. }),
        label,
    )
    .await;
    assert_the_album_is_whole(&remote, &dir, &files, "no-room.bin").await;
    clean_up_past_the_proxy(&dir).await;
}

// ── 507 on an upload ─────────────────────────────────────────────────

/// A server that took the body in and found no room to keep it.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the WebDAV fixture stack: apps/desktop/test/webdav-servers/start.sh (webdav-fixture)"]
async fn webdav_integration_a_server_out_of_room_mid_folder_fails_as_full_and_loses_nothing() {
    out_of_room_mid_folder(Answer::AfterTheBody, "out-of-room-after-body").await;
}

/// ❗ A server that refused the moment the head arrived, with most of the body
/// unsent: the client is still writing when the answer and the close come in.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the WebDAV fixture stack: apps/desktop/test/webdav-servers/start.sh (webdav-fixture)"]
async fn webdav_integration_a_quota_refused_before_the_body_fails_as_full_and_loses_nothing() {
    out_of_room_mid_folder(Answer::BeforeTheBody, "out-of-room-before-body").await;
}

// ── 403 while replacing the user's file ─────────────────────────────

/// An Overwrite whose new bytes are in, and whose DELETE of the user's file is
/// refused: the one moment in a safe-replace where the original is at stake.
/// `finalize_safe_replace` must stop there, keep the original, and clear away
/// the complete-but-unplaced temp.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the WebDAV fixture stack: apps/desktop/test/webdav-servers/start.sh (webdav-fixture)"]
async fn webdav_integration_a_refused_replace_keeps_the_users_file() {
    let (proxy, remote, dir) = refusing(Refusal {
        method: "DELETE",
        path: PathRule::EndsWith("/report.txt"),
        status: 403,
        answer: Answer::AfterTheBody,
    })
    .await;
    seed(remote.as_ref(), &dir, &[("report.txt", b"DEST-old-report")]).await;
    let (_local_dir, local) = local_volume("refused-replace");
    seed(
        local.as_ref(),
        Path::new(""),
        &[("report.txt", b"SRC-new-report, longer")],
    )
    .await;

    let state = Arc::new(WriteOperationState::new(Duration::from_millis(200)));
    let result = copy_volumes_with_progress(
        Arc::new(CollectorEventSink::new()),
        "refused-replace",
        &state,
        Arc::clone(&local),
        &[PathBuf::from("report.txt")],
        Arc::clone(&remote),
        &dir,
        &config(ConflictResolution::Overwrite),
    )
    .await;
    let Err(failure) = result else {
        panic!("a replace the server refused must not report success");
    };

    assert_refused_safely(
        &remote,
        &proxy,
        &failure.error,
        |e| matches!(e, WriteOperationError::PermissionDenied { .. }),
        "a refused replace",
    )
    .await;
    assert_eq!(
        try_read(remote.as_ref(), &dir.join("report.txt")).await.as_deref(),
        Some(b"DEST-old-report".as_slice()),
        "❗ the user's file the server wouldn't let us remove must be exactly as it was"
    );
    // The swap couldn't happen, so the finalize takes its complete-but-unplaced
    // temp away at once (the source still holds those bytes): nothing of ours
    // waits in the user's folder for the hourly stale-temp reap.
    assert_no_staging_litter(remote.as_ref(), &dir, "a refused replace").await;
    clean_up_past_the_proxy(&dir).await;
}

/// An upload onto a folder whose permissions changed under the copy: every PUT
/// there is refused, and the user's file it would have replaced stays.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the WebDAV fixture stack: apps/desktop/test/webdav-servers/start.sh (webdav-fixture)"]
async fn webdav_integration_a_refused_upload_over_the_users_file_keeps_it() {
    // The staged uploads only: every one of them rides a `.cmdr-tmp-*` sibling,
    // and the cell's own seed of the user's file must still go through.
    let (proxy, remote, dir) = refusing(Refusal {
        method: "PUT",
        path: PathRule::Contains("report.txt.cmdr-tmp"),
        status: 403,
        answer: Answer::AfterTheBody,
    })
    .await;
    seed(remote.as_ref(), &dir, &[("report.txt", b"DEST-old-report")]).await;
    let (_local_dir, local) = local_volume("refused-upload");
    seed(
        local.as_ref(),
        Path::new(""),
        &[("report.txt", b"SRC-new-report, longer")],
    )
    .await;

    let state = Arc::new(WriteOperationState::new(Duration::from_millis(200)));
    let result = copy_volumes_with_progress(
        Arc::new(CollectorEventSink::new()),
        "refused-upload",
        &state,
        Arc::clone(&local),
        &[PathBuf::from("report.txt")],
        Arc::clone(&remote),
        &dir,
        &config(ConflictResolution::Overwrite),
    )
    .await;
    let Err(failure) = result else {
        panic!("an upload the server refused must not report success");
    };

    assert_refused_safely(
        &remote,
        &proxy,
        &failure.error,
        |e| matches!(e, WriteOperationError::PermissionDenied { .. }),
        "a refused upload",
    )
    .await;
    assert_eq!(
        try_read(remote.as_ref(), &dir.join("report.txt")).await.as_deref(),
        Some(b"DEST-old-report".as_slice()),
        "❗ the user's file an upload was refused over must be exactly as it was"
    );
    assert_no_staging_litter(remote.as_ref(), &dir, "a refused upload").await;
    clean_up_past_the_proxy(&dir).await;
}

// ── Refusals the server raises on its own copy and move ─────────────

/// A same-server copy tries a server-side `COPY` first, and a refused one is
/// never the end of it: the engine streams the file the ordinary way instead
/// (`server_side_copy.rs::try_server_side_copy`, which treats anything short of success
/// or a cancel as "do it the ordinary way"). Here only the `COPY` is refused,
/// so the streamed copy lands, complete, with the refused attempt's staging
/// gone. On a server that is genuinely full the streamed PUT is refused too,
/// which is the first cell's case.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the WebDAV fixture stack: apps/desktop/test/webdav-servers/start.sh (webdav-fixture)"]
async fn webdav_integration_a_refused_server_side_copy_falls_back_to_streaming_and_lands_whole() {
    let (proxy, remote, dir) = refusing(Refusal {
        method: "COPY",
        path: PathRule::Contains("photo.jpg"),
        status: 507,
        answer: Answer::AfterTheBody,
    })
    .await;
    seed(remote.as_ref(), &dir.join("from"), &[("photo.jpg", b"SRC-photo")]).await;
    seed(remote.as_ref(), &dir.join("to"), &[USERS_FILE]).await;

    let state = Arc::new(WriteOperationState::new(Duration::from_millis(200)));
    let result = copy_volumes_with_progress(
        Arc::new(CollectorEventSink::new()),
        "server-copy-refused",
        &state,
        Arc::clone(&remote),
        &[dir.join("from/photo.jpg")],
        Arc::clone(&remote),
        &dir.join("to"),
        &config(ConflictResolution::Skip),
    )
    .await;

    assert!(
        result.is_ok(),
        "a refused server-side copy falls back to streaming: {:?}",
        result.err().map(|f| f.error)
    );
    assert!(proxy.refused() >= 1, "the premise: the server refused the `COPY`");
    assert_eq!(
        remote.connection_state(),
        Some(ConnectionState::Direct),
        "❗ a refusal is an answer, so the server must still count as connected"
    );
    assert_eq!(
        try_read(remote.as_ref(), &dir.join("to/photo.jpg")).await.as_deref(),
        Some(b"SRC-photo".as_slice()),
        "the streamed copy lands whole"
    );
    assert_eq!(
        try_read(remote.as_ref(), &dir.join("from/photo.jpg")).await.as_deref(),
        Some(b"SRC-photo".as_slice()),
        "the source of a copy stays"
    );
    assert_eq!(
        names_in(remote.as_ref(), &dir.join("to")).await,
        vec![USERS_FILE.0.to_string(), "photo.jpg".to_string()],
        "❗ the user's file and the copy, and no staging left by the refused attempt"
    );
    clean_up_past_the_proxy(&dir).await;
}

/// A same-server move is a server-side `MOVE`; a 403 on it must leave both
/// ends exactly as they were.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the WebDAV fixture stack: apps/desktop/test/webdav-servers/start.sh (webdav-fixture)"]
async fn webdav_integration_a_refused_server_side_move_leaves_both_ends_as_they_were() {
    let (proxy, remote, dir) = refusing(Refusal {
        method: "MOVE",
        path: PathRule::Contains("photo.jpg"),
        status: 403,
        answer: Answer::AfterTheBody,
    })
    .await;
    seed(remote.as_ref(), &dir.join("from"), &[("photo.jpg", b"SRC-photo")]).await;
    seed(remote.as_ref(), &dir.join("to"), &[USERS_FILE]).await;

    let state = Arc::new(WriteOperationState::new(Duration::from_millis(200)));
    let result = move_within_same_volume_with_progress(
        Arc::new(CollectorEventSink::new()),
        "server-move-refused",
        &state,
        Arc::clone(&remote),
        &[dir.join("from/photo.jpg")],
        &dir.join("to"),
        &config(ConflictResolution::Skip),
    )
    .await;
    let Err(error) = result else {
        panic!("a server-side move the server refused must not report success");
    };

    assert_refused_safely(
        &remote,
        &proxy,
        &error,
        |e| matches!(e, WriteOperationError::PermissionDenied { .. }),
        "a refused server-side move",
    )
    .await;
    assert_eq!(
        try_read(remote.as_ref(), &dir.join("from/photo.jpg")).await.as_deref(),
        Some(b"SRC-photo".as_slice()),
        "❗ the source of a refused move stays where it was"
    );
    assert_eq!(
        names_in(remote.as_ref(), &dir.join("to")).await,
        vec![USERS_FILE.0.to_string()],
        "the destination holds what it held"
    );
    clean_up_past_the_proxy(&dir).await;
}
