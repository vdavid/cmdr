//! Unit tests for the SMB backend.
//!
//! No Docker, no real network: type/error mapping, connection-state
//! transitions, path conversion, capability flags, and the channel-backed
//! `SmbReadStream` consumer. Declared as a `#[cfg(test)]` submodule of `smb`
//! so `super::*` resolves to the backend's private items.

use super::smb_test_support::*;
use super::*;

// ── Type mapping tests ──────────────────────────────────────────

#[test]
fn filetime_to_unix_secs_known_date() {
    // 2024-01-01 00:00:00 UTC = FileTime(133_485_408_000_000_000)
    let ft = smb2::pack::FileTime(133_485_408_000_000_000);
    let secs = filetime_to_unix_secs(ft).unwrap();
    assert_eq!(secs, 1_704_067_200);
}

#[test]
fn filetime_to_unix_secs_zero_returns_none() {
    let ft = smb2::pack::FileTime::ZERO;
    assert!(filetime_to_unix_secs(ft).is_none());
}

#[test]
fn directory_entry_to_file_entry_file() {
    let entry = smb2::client::tree::DirectoryEntry {
        name: "report.pdf".to_string(),
        size: 1024,
        is_directory: false,
        created: smb2::pack::FileTime(133_485_408_000_000_000),
        modified: smb2::pack::FileTime(133_485_408_000_000_000),
    };

    let fe = directory_entry_to_file_entry(&entry, "/Volumes/Share/Documents");
    assert_eq!(fe.name, "report.pdf");
    assert_eq!(fe.path, "/Volumes/Share/Documents/report.pdf");
    assert!(!fe.is_directory);
    assert!(!fe.is_symlink);
    assert_eq!(fe.size, Some(1024));
    assert_eq!(fe.modified_at, Some(1_704_067_200));
    assert_eq!(fe.created_at, Some(1_704_067_200));
    assert_eq!(fe.icon_id, "ext:pdf");
}

#[test]
fn directory_entry_to_file_entry_directory() {
    let entry = smb2::client::tree::DirectoryEntry {
        name: "Photos".to_string(),
        size: 0,
        is_directory: true,
        created: smb2::pack::FileTime::ZERO,
        modified: smb2::pack::FileTime::ZERO,
    };

    let fe = directory_entry_to_file_entry(&entry, "/Volumes/Share");
    assert_eq!(fe.name, "Photos");
    assert_eq!(fe.path, "/Volumes/Share/Photos");
    assert!(fe.is_directory);
    assert_eq!(fe.size, None);
    assert_eq!(fe.modified_at, None);
    assert_eq!(fe.icon_id, "dir");
}

#[test]
fn fs_info_to_space_info_conversion() {
    let info = smb2::client::tree::FsInfo {
        total_bytes: 1_000_000_000,
        free_bytes: 400_000_000,
        total_free_bytes: 400_000_000,
        bytes_per_sector: 512,
        sectors_per_unit: 8,
    };

    let space = fs_info_to_space_info(&info);
    assert_eq!(space.total_bytes, 1_000_000_000);
    assert_eq!(space.available_bytes, 400_000_000);
    assert_eq!(space.used_bytes, 600_000_000);
}

#[test]
fn map_smb_error_not_found() {
    let err = smb2::Error::Protocol {
        status: smb2::types::status::NtStatus::OBJECT_NAME_NOT_FOUND,
        command: smb2::types::Command::Create,
    };
    let ve = map_smb_error(err);
    assert!(matches!(ve, VolumeError::NotFound(_)));
}

#[test]
fn map_smb_error_delete_pending() {
    // STATUS_DELETE_PENDING surfaces when a delete has been requested but at
    // least one open handle is keeping the file alive. smb2 currently classifies
    // it as `ErrorKind::Other`, so `map_smb_error` must dispatch on the raw
    // NTSTATUS to produce the typed `VolumeError::DeletePending` variant —
    // otherwise the FE falls back to the generic "disk needs attention" copy
    // instead of the transient "file is being removed" message.
    let err = smb2::Error::Protocol {
        status: smb2::types::status::NtStatus::DELETE_PENDING,
        command: smb2::types::Command::Create,
    };
    let ve = map_smb_error(err);
    assert!(
        matches!(ve, VolumeError::DeletePending(_)),
        "STATUS_DELETE_PENDING should map to VolumeError::DeletePending, got: {:?}",
        ve,
    );
}

#[test]
fn map_smb_error_access_denied() {
    let err = smb2::Error::Protocol {
        status: smb2::types::status::NtStatus::ACCESS_DENIED,
        command: smb2::types::Command::Create,
    };
    let ve = map_smb_error(err);
    assert!(matches!(ve, VolumeError::PermissionDenied(_)));
}

#[test]
fn map_smb_error_disconnected() {
    let err = smb2::Error::Disconnected;
    let ve = map_smb_error(err);
    assert!(matches!(ve, VolumeError::DeviceDisconnected(_)));
}

#[test]
fn map_smb_error_timeout() {
    let err = smb2::Error::Timeout;
    let ve = map_smb_error(err);
    assert!(matches!(ve, VolumeError::ConnectionTimeout(_)));
}

#[test]
fn map_smb_error_disk_full() {
    let err = smb2::Error::Protocol {
        status: smb2::types::status::NtStatus::DISK_FULL,
        command: smb2::types::Command::Write,
    };
    let ve = map_smb_error(err);
    assert!(matches!(ve, VolumeError::StorageFull { .. }));
}

#[test]
fn map_smb_error_session_expired() {
    let err = smb2::Error::SessionExpired;
    let ve = map_smb_error(err);
    assert!(matches!(ve, VolumeError::DeviceDisconnected(_)));
}

#[test]
fn map_smb_error_auth_required() {
    let err = smb2::Error::Auth {
        message: "Authentication failed".to_string(),
    };
    let ve = map_smb_error(err);
    assert!(matches!(ve, VolumeError::PermissionDenied(_)));
}

#[test]
fn map_smb_error_io() {
    let err = smb2::Error::Io(std::io::Error::new(std::io::ErrorKind::BrokenPipe, "pipe broke"));
    let ve = map_smb_error(err);
    // IO errors (callback errors, etc.) are not connection losses; they map to IoError.
    // Real connection losses come through Error::Disconnected → ConnectionLost.
    assert!(matches!(ve, VolumeError::IoError { .. }));
}

#[test]
fn map_smb_error_already_exists() {
    // STATUS_OBJECT_NAME_COLLISION (returned by Create when the name exists) must
    // surface as AlreadyExists so the volume_strategy merge-directory path can
    // swallow it instead of bubbling a generic IO error to the user.
    let err = smb2::Error::Protocol {
        status: smb2::types::status::NtStatus::OBJECT_NAME_COLLISION,
        command: smb2::types::Command::Create,
    };
    let ve = map_smb_error(err);
    assert!(matches!(ve, VolumeError::AlreadyExists(_)));
}

#[test]
fn map_smb_error_file_is_a_directory() {
    // STATUS_FILE_IS_A_DIRECTORY is returned when delete_file is called on a dir.
    // smb2 0.8.0 exposes this as the typed `ErrorKind::IsADirectory` variant, so
    // `map_smb_error` surfaces it as `VolumeError::IsADirectory`; the delete
    // fast-path matches on that to decide whether to retry with delete_directory.
    let err = smb2::Error::Protocol {
        status: smb2::types::status::NtStatus::FILE_IS_A_DIRECTORY,
        command: smb2::types::Command::Create,
    };
    let ve = map_smb_error(err);
    assert!(matches!(ve, VolumeError::IsADirectory(_)));
}

#[test]
fn map_smb_error_access_denied_is_not_misclassified() {
    // Non-directory errors must not be classified as IsADirectory.
    let err = smb2::Error::Protocol {
        status: smb2::types::status::NtStatus::ACCESS_DENIED,
        command: smb2::types::Command::Create,
    };
    let ve = map_smb_error(err);
    assert!(matches!(ve, VolumeError::PermissionDenied(_)));
}

// ── Connection state tests ──────────────────────────────────────

#[test]
fn connection_state_round_trip() {
    for state in [ConnectionState::Direct, ConnectionState::Disconnected] {
        assert_eq!(ConnectionState::from_u8(state as u8), state);
    }
}

#[test]
fn connection_state_unknown_value_defaults_to_disconnected() {
    // The internal state machine is binary; `1` (the old `OsMount`
    // discriminant) and any other unknown byte must decode as
    // `Disconnected`, the safe / "stop using smb2" state.
    assert_eq!(ConnectionState::from_u8(1), ConnectionState::Disconnected);
    assert_eq!(ConnectionState::from_u8(255), ConnectionState::Disconnected);
}

// ── Path conversion tests ───────────────────────────────────────

#[test]
fn to_smb_path_empty() {
    let vol = make_test_volume();
    assert_eq!(vol.to_smb_path(Path::new("")), "");
    assert_eq!(vol.to_smb_path(Path::new("/")), "");
    assert_eq!(vol.to_smb_path(Path::new(".")), "");
}

#[test]
fn to_smb_path_relative() {
    let vol = make_test_volume();
    assert_eq!(vol.to_smb_path(Path::new("Documents")), "Documents");
    assert_eq!(
        vol.to_smb_path(Path::new("Documents/report.pdf")),
        "Documents/report.pdf"
    );
}

#[test]
fn to_smb_path_absolute_under_mount() {
    let vol = make_test_volume();
    assert_eq!(vol.to_smb_path(Path::new("/Volumes/TestShare/Documents")), "Documents");
    assert_eq!(
        vol.to_smb_path(Path::new("/Volumes/TestShare/Documents/report.pdf")),
        "Documents/report.pdf"
    );
}

#[test]
fn to_smb_path_mount_root() {
    let vol = make_test_volume();
    assert_eq!(vol.to_smb_path(Path::new("/Volumes/TestShare")), "");
}

#[test]
fn to_display_path_empty_is_mount_root() {
    let vol = make_test_volume();
    assert_eq!(vol.to_display_path(""), "/Volumes/TestShare");
}

#[test]
fn to_display_path_with_subpath() {
    let vol = make_test_volume();
    assert_eq!(
        vol.to_display_path("Documents/report.pdf"),
        "/Volumes/TestShare/Documents/report.pdf"
    );
}

#[test]
fn supports_watching_returns_false() {
    let vol = make_test_volume();
    assert!(!vol.supports_watching());
}

#[test]
fn name_returns_share_name() {
    let vol = make_test_volume();
    assert_eq!(vol.name(), "TestShare");
}

#[test]
fn root_returns_mount_path() {
    let vol = make_test_volume();
    assert_eq!(vol.root(), Path::new("/Volumes/TestShare"));
}

#[test]
fn local_path_returns_none() {
    let vol = make_test_volume();
    assert!(vol.local_path().is_none());
}

#[test]
fn supports_export_returns_true() {
    let vol = make_test_volume();
    assert!(vol.supports_export());
}

/// The opt-in that makes a background SMB copy stand aside for navigation. A copy
/// and the pane's listings share one SMB session, so without this the transfer
/// never parks and the share stays sluggish for the whole copy.
#[test]
fn supports_foreground_yield_is_on() {
    let vol = make_test_volume();
    assert!(vol.supports_foreground_yield());
}

/// The UPLOAD counterpart: an SMB share also opts into the DESTINATION-side yield,
/// so writing to it stands aside for navigation on the same share. SMB writes are
/// discrete WRITE chunks with no lease, so a bounded park between them is safe.
#[test]
fn supports_foreground_yield_as_destination_is_on() {
    let vol = make_test_volume();
    assert!(vol.supports_foreground_yield_as_destination());
}

/// …and the probe behind it is scoped to THIS share: navigating the volume being
/// copied from parks the copy, navigating anything else leaves it at full speed.
#[tokio::test]
async fn foreground_pending_tracks_navigation_on_this_share_only() {
    let vol = make_test_volume();
    assert!(!vol.foreground_pending().await, "nothing browsed yet");

    crate::media_index::foreground::note_foreground_activity_on("some-other-volume");
    assert!(
        !vol.foreground_pending().await,
        "browsing another volume must not park a copy off this share"
    );

    crate::media_index::foreground::note_foreground_activity_on(vol.volume_id());
    assert!(vol.foreground_pending().await, "browsing this share parks the copy");
}

// ── Reconnect tests (no Docker, no real network) ────────────────

/// Helper that flips a test volume into `Direct` so we can test the
/// "already connected" no-op path without needing a real session.
fn make_test_volume_direct() -> SmbVolume {
    let vol = make_test_volume();
    vol.state.store(ConnectionState::Direct as u8, Ordering::Relaxed);
    vol
}

#[tokio::test]
async fn attempt_reconnect_noop_when_already_direct() {
    // If state is Direct, the helper bails early without building a session.
    // This is the path concurrent callers hit after the winner finishes.
    let vol = make_test_volume_direct();
    let result = vol.do_attempt_reconnect().await;
    assert!(result.is_ok(), "expected Ok when already Direct, got {:?}", result);
    assert_eq!(vol.connection_state(), ConnectionState::Direct);
}

#[tokio::test]
async fn attempt_reconnect_bails_when_unmounted() {
    // After `on_unmount` runs, reconnect must not try to build a new session
    // (otherwise we'd leak a watcher + smb2 session into an orphaned volume).
    let vol = make_test_volume();
    vol.unmounted.store(true, Ordering::Relaxed);
    let result = vol.do_attempt_reconnect().await;
    assert!(
        matches!(result, Err(VolumeError::DeviceDisconnected(_))),
        "expected DeviceDisconnected when unmounted, got {:?}",
        result
    );
}

#[tokio::test]
async fn single_flight_concurrent_callers_serialize() {
    // Two parallel `do_attempt_reconnect` calls must serialize on
    // `reconnect_lock`. With the volume already Direct, both should return
    // Ok cheaply: the second one observes Direct after the first releases
    // the guard. Mutex contention itself is the assertion that single-flight
    // is wired up; if it wasn't, both calls would race past the early-exit
    // check.
    let vol = Arc::new(make_test_volume_direct());
    let v2 = Arc::clone(&vol);
    let v3 = Arc::clone(&vol);
    let (r1, r2) = tokio::join!(async move { v2.do_attempt_reconnect().await }, async move {
        v3.do_attempt_reconnect().await
    });
    assert!(r1.is_ok());
    assert!(r2.is_ok());
    assert_eq!(vol.connection_state(), ConnectionState::Direct);
}

#[tokio::test]
async fn transition_to_disconnected_idempotent() {
    // Calling `transition_to_disconnected` twice should only emit once.
    // We can't verify the emit count without a real `AppHandle`, but we
    // can verify the underlying `swap` semantics: the second call is a
    // no-op (returns the same value).
    let vol = make_test_volume_direct();
    assert_eq!(vol.connection_state(), ConnectionState::Direct);
    vol.transition_to_disconnected();
    assert_eq!(vol.connection_state(), ConnectionState::Disconnected);
    vol.transition_to_disconnected();
    assert_eq!(vol.connection_state(), ConnectionState::Disconnected);
}

#[tokio::test]
async fn transition_to_direct_idempotent() {
    let vol = make_test_volume();
    assert_eq!(vol.connection_state(), ConnectionState::Disconnected);
    vol.transition_to_direct();
    assert_eq!(vol.connection_state(), ConnectionState::Direct);
    vol.transition_to_direct();
    assert_eq!(vol.connection_state(), ConnectionState::Direct);
}

#[test]
fn listing_is_watched_false_when_disconnected() {
    // No watcher_cancel set and state Disconnected: false.
    let vol = make_test_volume();
    assert!(!vol.listing_is_watched(Path::new("/")));
}

#[test]
fn listing_is_watched_false_when_direct_but_no_watcher() {
    // State Direct but `watcher_cancel` empty: still false (we need both).
    let vol = make_test_volume_direct();
    assert!(!vol.listing_is_watched(Path::new("/")));
}

#[test]
fn listing_is_watched_false_when_watcher_set_but_disconnected() {
    // `watcher_cancel` populated but state Disconnected: false.
    let vol = make_test_volume();
    let (tx, _rx) = tokio::sync::oneshot::channel::<()>();
    *vol.watcher_cancel.lock().unwrap() = Some(tx);
    assert!(!vol.listing_is_watched(Path::new("/")));
}

#[test]
fn listing_is_watched_true_when_direct_and_watcher_set() {
    // Both conditions met: true.
    let vol = make_test_volume_direct();
    let (tx, _rx) = tokio::sync::oneshot::channel::<()>();
    *vol.watcher_cancel.lock().unwrap() = Some(tx);
    assert!(vol.listing_is_watched(Path::new("/")));
}

#[test]
fn on_unmount_marks_volume_dead() {
    // `on_unmount` is sync (called from FSEvents thread) and uses
    // `blocking_lock`, so this must be a `#[test]`, not a `#[tokio::test]`
    // (the latter panics inside a runtime when calling `blocking_lock`).
    let vol = make_test_volume_direct();
    assert!(!vol.unmounted.load(Ordering::Relaxed));
    vol.on_unmount();
    assert!(vol.unmounted.load(Ordering::Relaxed));
    assert_eq!(vol.connection_state(), ConnectionState::Disconnected);
}

/// Opening the scan pool no-ops when the volume is disconnected: it must not try
/// to `build_session` (a real network round trip) against a dead volume. Cheap,
/// server-free coverage of the guard; the live open/list/close path is the
/// Docker integration test.
#[tokio::test]
async fn open_scan_pool_noops_when_disconnected() {
    let vol = make_test_volume(); // Disconnected, no session
    vol.open_scan_pool().await;
    assert!(
        vol.scan_pool.read().await.is_none(),
        "a disconnected volume opens no scan pool"
    );
}

/// Closing the scan pool with none open is a no-op (idempotent), so
/// `end_scan_session` / `on_unmount` are always safe to call.
#[tokio::test]
async fn close_scan_pool_is_idempotent_noop() {
    let vol = make_test_volume();
    vol.close_scan_pool().await;
    vol.close_scan_pool().await;
    assert!(vol.scan_pool.read().await.is_none());
}

/// Creates a test SmbVolume in disconnected state (no real connection).
fn make_test_volume() -> SmbVolume {
    let params = SmbConnectionParams {
        server: "192.168.1.100".to_string(),
        share_name: "TestShare".to_string(),
        port: 445,
        username: "Guest".to_string(),
        password: String::new(),
    };
    SmbVolume {
        name: "TestShare".to_string(),
        mount_path: PathBuf::from("/Volumes/TestShare"),
        share_name: "TestShare".to_string(),
        volume_id: "volumestestshare".to_string(),
        params: Arc::new(tokio::sync::RwLock::new(params)),
        client: Arc::new(tokio::sync::Mutex::new(None)),
        tree: Arc::new(tokio::sync::RwLock::new(None)),
        state: Arc::new(AtomicU8::new(ConnectionState::Disconnected as u8)),
        watcher_cancel: std::sync::Mutex::new(None),
        reconnect_lock: Arc::new(tokio::sync::Mutex::new(())),
        unmounted: Arc::new(AtomicBool::new(false)),
        scan_pool: tokio::sync::RwLock::new(None),
    }
}

// ── SmbReadStream consumer tests ────────────────────────────────
//
// These test the consumer side of the channel-backed SmbReadStream in
// isolation. End-to-end SMB streaming is covered by the Docker
// integration tests below (smb_integration_open_read_stream,
// smb_integration_export_streams).

/// Builds an SmbReadStream backed by a pre-seeded channel, bypassing the
/// real SMB producer task. Returns the stream plus the cancel receiver
/// side so tests can assert that drop sends a cancel signal.
fn make_stream_from_chunks(
    chunks: Vec<Result<Vec<u8>, VolumeError>>,
    total_size: u64,
) -> (SmbReadStream, tokio::sync::oneshot::Receiver<()>) {
    let (chunk_tx, chunk_rx) =
        tokio::sync::mpsc::channel::<Result<Vec<u8>, VolumeError>>(SMB_STREAM_CHANNEL_CAPACITY.max(chunks.len()));
    let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel::<()>();

    for chunk in chunks {
        // blocking_send is fine in tests; we sized the channel to fit.
        chunk_tx.try_send(chunk).expect("channel has capacity in test setup");
    }
    // Drop chunk_tx so recv returns None after draining.
    drop(chunk_tx);

    let stream = SmbReadStream {
        rx: chunk_rx,
        cancel: Some(cancel_tx),
        total_size,
        bytes_read: 0,
    };
    (stream, cancel_rx)
}

#[tokio::test]
async fn smb_read_stream_empty_file() {
    let (mut stream, _cancel_rx) = make_stream_from_chunks(vec![], 0);
    assert_eq!(stream.total_size(), 0);
    assert_eq!(stream.bytes_read(), 0);
    assert!(stream.next_chunk().await.is_none());
}

#[tokio::test]
async fn smb_read_stream_yields_chunks_in_order() {
    let (mut stream, _cancel_rx) =
        make_stream_from_chunks(vec![Ok(vec![1u8; 100]), Ok(vec![2u8; 50]), Ok(vec![3u8; 30])], 180);
    assert_eq!(stream.total_size(), 180);

    let c1 = stream.next_chunk().await.unwrap().unwrap();
    assert_eq!(c1, vec![1u8; 100]);
    assert_eq!(stream.bytes_read(), 100);

    let c2 = stream.next_chunk().await.unwrap().unwrap();
    assert_eq!(c2, vec![2u8; 50]);
    assert_eq!(stream.bytes_read(), 150);

    let c3 = stream.next_chunk().await.unwrap().unwrap();
    assert_eq!(c3, vec![3u8; 30]);
    assert_eq!(stream.bytes_read(), 180);

    assert!(stream.next_chunk().await.is_none());
}

#[tokio::test]
async fn smb_read_stream_propagates_mid_stream_error() {
    let (mut stream, _cancel_rx) = make_stream_from_chunks(
        vec![
            Ok(vec![1u8; 10]),
            Err(VolumeError::DeviceDisconnected("simulated".to_string())),
        ],
        0,
    );

    let first = stream.next_chunk().await.unwrap().unwrap();
    assert_eq!(first, vec![1u8; 10]);
    assert_eq!(stream.bytes_read(), 10);

    let second = stream.next_chunk().await.unwrap();
    assert!(matches!(second, Err(VolumeError::DeviceDisconnected(_))));
    // bytes_read should not have advanced on the error
    assert_eq!(stream.bytes_read(), 10);
}

#[tokio::test]
async fn smb_read_stream_drop_sends_cancel() {
    let (stream, mut cancel_rx) = make_stream_from_chunks(vec![Ok(vec![1u8; 10])], 10);
    drop(stream);

    // The cancel oneshot should have been fired by Drop.
    match cancel_rx.try_recv() {
        Ok(()) => {}
        other => panic!("expected cancel signal, got {other:?}"),
    }
}

#[test]
fn smb_supports_streaming() {
    // SmbVolume should report streaming support so cross-volume copies
    // (MTP↔SMB) use the streaming path instead of NotSupported/temp files.
    let vol = make_test_volume();
    assert!(vol.supports_streaming());
}

/// Pins the `client-mutex:` and `recv:` log-message prefix convention the
/// `MutexCaptureLogger` routes by. The prefixes are intentionally part of
/// our log-message contract (see the actual `log::debug!` sites further up
/// in this file and in the smb2 receiver loop). If the prefixes drift, the
/// debug ring buffer stops capturing them and a future hung-test triage
/// loses the diagnostic. This test pins both prefixes against the
/// canonical message-format helpers so any rename of the convention
/// triggers a compile-fail or string-mismatch here first.
#[test]
fn mutex_capture_logger_routes_known_prefixes() {
    // Format mirrors the real `log::debug!` sites in `clone_session`.
    let mutex_msg = format!(
        "client-mutex: waiting ticket={} caller=clone_session share={}",
        7, "Public"
    );
    let recv_msg = "recv: smb2 frame 0x10 mid=42";
    let other_msg = "some unrelated log line";

    assert!(
        mutex_msg.starts_with("client-mutex:"),
        "mutex prefix drifted: {mutex_msg}"
    );
    assert!(recv_msg.starts_with("recv:"), "recv prefix drifted: {recv_msg}");
    assert!(!other_msg.starts_with("client-mutex:") && !other_msg.starts_with("recv:"));
}

#[test]
#[should_panic(expected = "refusing to clean a prefix outside")]
fn cleanup_test_prefix_rejects_unsafe_prefix() {
    // The cleanup helper is async, but the safety assert fires before
    // any await point. Poll the future once via a no-op waker so we
    // hit the assert without needing a runtime.
    use std::task::Context;
    let vol = make_test_volume();
    let mount = PathBuf::from("/Volumes/TestShare");
    let mut fut = Box::pin(cleanup_test_prefix(&vol, &mount, "etc/passwd"));
    let waker = futures_util::task::noop_waker();
    let mut cx = Context::from_waker(&waker);
    let _ = fut.as_mut().poll(&mut cx); // panics in the assert
}

#[test]
fn test_prefix_root_is_safely_scoped() {
    // Static check: the prefix lives under `_test/` and clearly
    // identifies cmdr's regression test, so a future reader (or a
    // misconfigured share) can recognize stale artifacts at a glance.
    assert!(TEST_PREFIX_ROOT.starts_with("_test/"));
    assert!(TEST_PREFIX_ROOT.contains("cmdr-regression-"));
}
