//! Integration tests for the SMB backend (require Docker SMB containers).
//!
//! Covers connection management (watcher/state gates, reconnect), core CRUD
//! (create/read/rename/delete/no-clobber/mtime), single-chunk streaming smoke
//! tests, and scan / conflict-preview / batch-scan behavior. The expanded
//! streaming tests live in `smb_streaming_integration_test`, the high-level
//! transfer-semantics tests in `smb_transfer_semantics_test`, and the
//! concurrency stress tests in `smb_stress_test`.
//!
//! Every test here is `#[ignore]`d so default runs skip it. Start the
//! containers with `./apps/desktop/test/smb-servers/start.sh`, then run
//! `cargo nextest run smb_integration --run-ignored all`. Declared as a
//! `#[cfg(test)]` submodule of `smb`; shared helpers come from
//! `super::smb_test_support`.

use super::smb_test_support::*;
use super::*;
use crate::file_system::volume::InMemoryVolume;

#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_listing_is_watched_flips_with_connection() {
    // End-to-end check against a live Docker SMB server: after
    // `connect_smb_volume`, the watcher is spawned and state is Direct,
    // so the oracle gate returns true. After flipping the state to
    // Disconnected (simulating a ConnectionLost event), the gate flips
    // false even though `watcher_cancel` is still set: the contract is
    // "watcher present AND Direct," and a half-broken volume must not be
    // treated as fresh.
    let vol = make_docker_volume().await;
    assert_eq!(vol.connection_state(), ConnectionState::Direct);
    assert!(
        vol.listing_is_watched(Path::new("/")),
        "expected true on a freshly-connected Docker volume"
    );

    vol.transition_to_disconnected();
    assert!(
        !vol.listing_is_watched(Path::new("/")),
        "expected false after transitioning to Disconnected"
    );
}

/// Regression: closing a pane's listing must NOT tear down the SMB watcher.
///
/// The watcher's lifetime is the VOLUME's (spawned at `connect_smb_volume`,
/// canceled only by `on_unmount` / reconnect), not a pane's. The index relies on
/// this: it must keep receiving change events while the volume's index is live,
/// even with no pane showing the share. `list_directory_end` (the pane-close IPC)
/// only drops a listing-cache entry and its FSEvents `WatchedDirectory` (SMB has
/// none), so it can't reach the watcher. This test pins that: after a pane close,
/// the volume is still watched.
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_pane_close_does_not_kill_index_watcher() {
    use crate::file_system::listing::operations::list_directory_end;

    let vol = make_docker_volume().await;
    assert_eq!(vol.connection_state(), ConnectionState::Direct);
    assert!(
        vol.listing_is_watched(Path::new("/")),
        "watcher must be alive right after connect",
    );

    // Simulate a pane closing its listing. Even for listing ids that were never
    // registered, this exercises the close path; the point is that NOTHING in it
    // cancels the volume-scoped SMB watcher.
    list_directory_end("some-pane-listing-id");
    list_directory_end("another-pane-listing-id");

    assert!(
        vol.listing_is_watched(Path::new("/")),
        "pane close must NOT tear down the volume's index watcher",
    );
}

#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_attempt_reconnect_rebuilds_session() {
    // Drives the full reconnect cycle against a real SMB server:
    // 1. Connect, verify Direct.
    // 2. Force-flip to Disconnected (simulating a ConnectionLost event). Drop the underlying client +
    //    tree to mimic a dead session.
    // 3. Verify hot-path ops fail with DeviceDisconnected.
    // 4. Call attempt_reconnect; verify it succeeds and state is Direct.
    // 5. Verify hot-path ops work again.
    let vol = make_docker_volume().await;
    assert_eq!(vol.connection_state(), ConnectionState::Direct);
    assert!(vol.list_directory_impl(Path::new("")).await.is_ok());

    // Simulate "the server hung up": drop the smb2 session and flip state.
    // We don't need to actually break the network; `attempt_reconnect`'s
    // job is to rebuild the session regardless of why state went down.
    {
        let mut client_guard = vol.client.lock().await;
        *client_guard = None;
    }
    {
        let mut tree_guard = vol.tree.write().await;
        *tree_guard = None;
    }
    vol.transition_to_disconnected();
    assert_eq!(vol.connection_state(), ConnectionState::Disconnected);

    // Hot-path op should fail: clone_session refuses while Disconnected.
    let result = vol.list_directory_impl(Path::new("")).await;
    assert!(
        matches!(result, Err(VolumeError::DeviceDisconnected(_))),
        "expected DeviceDisconnected before reconnect, got {:?}",
        result
    );

    // Reconnect should rebuild the session and flip back to Direct.
    vol.do_attempt_reconnect()
        .await
        .expect("attempt_reconnect should succeed against a live Docker SMB");
    assert_eq!(vol.connection_state(), ConnectionState::Direct);

    // And hot-path ops should work again.
    let entries = vol
        .list_directory_impl(Path::new(""))
        .await
        .expect("list_directory should succeed after reconnect");
    assert!(entries.iter().all(|e| e.name != "." && e.name != ".."));
}

#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_attempt_reconnect_noop_when_already_direct() {
    // Call reconnect against a live, healthy session. Should be a fast no-op
    // (no extra round-trip to the server).
    let vol = make_docker_volume().await;
    assert_eq!(vol.connection_state(), ConnectionState::Direct);
    let start = std::time::Instant::now();
    vol.do_attempt_reconnect().await.unwrap();
    let elapsed = start.elapsed();
    assert_eq!(vol.connection_state(), ConnectionState::Direct);
    // No-op should be effectively instant. Any real session build would
    // take tens of ms minimum even against localhost. Pad the bound for
    // CI noise.
    assert!(
        elapsed < Duration::from_millis(50),
        "noop reconnect took {:?}; expected <50ms",
        elapsed
    );
}

#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_list_directory() {
    let vol = make_docker_volume().await;
    let entries = vol.list_directory_impl(Path::new("")).await.unwrap();
    // The public share should be listable (may have files from other tests)
    assert!(entries.iter().all(|e| e.name != "." && e.name != ".."));
}

#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_create_and_read_file() {
    let vol = make_docker_volume().await;
    let dir = test_dir_name();
    ensure_clean(&vol, &dir).await;

    // Create a directory
    vol.create_directory(Path::new(&dir)).await.unwrap();

    // Create a file inside it
    let file_path = format!("{}/test.txt", dir);
    let content = b"hello from cmdr integration test";
    vol.create_file(Path::new(&file_path), content).await.unwrap();

    // Verify it exists
    assert!(vol.exists(Path::new(&file_path)).await);
    assert!(!vol.is_directory(Path::new(&file_path)).await.unwrap());

    // Verify metadata
    let meta = vol.get_metadata(Path::new(&file_path)).await.unwrap();
    assert_eq!(meta.name, "test.txt");
    assert_eq!(meta.size, Some(content.len() as u64));
    assert!(!meta.is_directory);

    // Byte-level integrity: read the destination back and compare bytes.
    // Catches any pipeline bug that lets metadata say "N bytes" while the
    // wire payload is something other than the source.
    let mut readback_stream = vol.open_read_stream(Path::new(&file_path)).await.unwrap();
    let mut readback = Vec::new();
    while let Some(Ok(chunk)) = readback_stream.next_chunk().await {
        readback.extend_from_slice(&chunk);
    }
    assert_eq!(readback, content, "destination bytes must match source bytes");

    // List the directory and verify the file is there
    let entries = vol.list_directory_impl(Path::new(&dir)).await.unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "test.txt");

    // Clean up
    vol.delete(Path::new(&file_path)).await.unwrap();
    vol.delete(Path::new(&dir)).await.unwrap();
}

/// Regression for the high-severity audit finding: `create_file` is a
/// no-overwrite contract. Pre-fix, SMB delegated to `tree.write_file`
/// which uses `FileOverwriteIf` disposition and silently truncated.
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_create_file_does_not_clobber_existing() {
    let vol = make_docker_volume().await;
    let dir = test_dir_name();
    ensure_clean(&vol, &dir).await;

    vol.create_directory(Path::new(&dir)).await.unwrap();
    let file_path = format!("{}/notes.txt", dir);
    let original = b"important user data";
    vol.create_file(Path::new(&file_path), original).await.unwrap();

    // Second create on the same path must fail with AlreadyExists;
    // bytes on the wire must be unchanged.
    let result = vol.create_file(Path::new(&file_path), b"junk").await;
    assert!(
        matches!(result, Err(VolumeError::AlreadyExists(_))),
        "expected AlreadyExists, got {:?}",
        result
    );

    let mut readback = vol.open_read_stream(Path::new(&file_path)).await.unwrap();
    let mut bytes = Vec::new();
    while let Some(Ok(chunk)) = readback.next_chunk().await {
        bytes.extend_from_slice(&chunk);
    }
    assert_eq!(bytes, original, "original bytes must survive a colliding create_file");

    vol.delete(Path::new(&file_path)).await.unwrap();
    vol.delete(Path::new(&dir)).await.unwrap();
}

/// Regression test for a unit-mismatch bug where SMB returned `modified_at` in
/// milliseconds while the rest of cmdr (and the frontend formatter) expects seconds.
/// That caused displayed years like 58247 on real shares. Asserts the mtime of a
/// just-created file lands near wall-clock `now`, in Unix seconds.
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_modified_at_is_unix_seconds() {
    use std::time::{SystemTime, UNIX_EPOCH};

    let vol = make_docker_volume().await;
    let dir = test_dir_name();
    ensure_clean(&vol, &dir).await;
    vol.create_directory(Path::new(&dir)).await.unwrap();
    let file_path = format!("{}/mtime.txt", dir);
    vol.create_file(Path::new(&file_path), b"mtime").await.unwrap();

    let now_secs = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
    let meta = vol.get_metadata(Path::new(&file_path)).await.unwrap();
    let mtime = meta.modified_at.expect("mtime should be populated");

    // Must be Unix seconds, not millis (*1000) or micros (*1_000_000).
    // Allow a 1 hour window for clock skew between host and container.
    let lower = now_secs.saturating_sub(3600);
    let upper = now_secs + 3600;
    assert!(
        mtime >= lower && mtime <= upper,
        "modified_at {mtime} out of range [{lower}, {upper}]; likely wrong unit (seconds expected)",
    );

    vol.delete(Path::new(&file_path)).await.unwrap();
    vol.delete(Path::new(&dir)).await.unwrap();
}

#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_rename() {
    let vol = make_docker_volume().await;
    let dir = test_dir_name();

    vol.create_directory(Path::new(&dir)).await.unwrap();
    let old_path = format!("{}/old.txt", dir);
    let new_path = format!("{}/new.txt", dir);

    vol.create_file(Path::new(&old_path), b"rename me").await.unwrap();

    // Rename
    vol.rename(Path::new(&old_path), Path::new(&new_path), false)
        .await
        .unwrap();

    // Old is gone, new exists
    assert!(!vol.exists(Path::new(&old_path)).await);
    assert!(vol.exists(Path::new(&new_path)).await);

    // Clean up
    vol.delete(Path::new(&new_path)).await.unwrap();
    vol.delete(Path::new(&dir)).await.unwrap();
}

#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_rename_force_overwrites() {
    let vol = make_docker_volume().await;
    let dir = test_dir_name();

    vol.create_directory(Path::new(&dir)).await.unwrap();
    let src = format!("{}/src.txt", dir);
    let dst = format!("{}/dst.txt", dir);

    vol.create_file(Path::new(&src), b"source content").await.unwrap();
    vol.create_file(Path::new(&dst), b"will be overwritten").await.unwrap();

    // Non-force should fail
    let err = vol.rename(Path::new(&src), Path::new(&dst), false).await;
    assert!(matches!(err, Err(VolumeError::AlreadyExists(_))));

    // Force should succeed
    vol.rename(Path::new(&src), Path::new(&dst), true).await.unwrap();
    assert!(!vol.exists(Path::new(&src)).await);
    assert!(vol.exists(Path::new(&dst)).await);

    // Clean up
    vol.delete(Path::new(&dst)).await.unwrap();
    vol.delete(Path::new(&dir)).await.unwrap();
}

#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_delete_directory() {
    let vol = make_docker_volume().await;
    let dir = test_dir_name();

    vol.create_directory(Path::new(&dir)).await.unwrap();
    assert!(vol.exists(Path::new(&dir)).await);
    assert!(vol.is_directory(Path::new(&dir)).await.unwrap());

    vol.delete(Path::new(&dir)).await.unwrap();
    assert!(!vol.exists(Path::new(&dir)).await);
}

#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_read_stream_single_file() {
    // Exercises the SMB → local byte path (now via open_read_stream) at
    // the simplest shape.
    let vol = make_docker_volume().await;
    let dir = test_dir_name();

    vol.create_directory(Path::new(&dir)).await.unwrap();
    let smb_file = format!("{}/export-test.txt", dir);
    let content = b"exported content";
    vol.create_file(Path::new(&smb_file), content).await.unwrap();

    let mut stream = vol.open_read_stream(Path::new(&smb_file)).await.unwrap();
    assert_eq!(stream.total_size(), content.len() as u64);
    let mut readback = Vec::new();
    while let Some(Ok(chunk)) = stream.next_chunk().await {
        readback.extend_from_slice(&chunk);
    }
    assert_eq!(readback, content);

    vol.delete(Path::new(&smb_file)).await.unwrap();
    vol.delete(Path::new(&dir)).await.unwrap();
}

#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_write_from_stream_single_file() {
    // Exercises the local → SMB byte path (now via write_from_stream) at
    // the simplest shape. Uses InMemoryVolume as the source stream.
    let vol = make_docker_volume().await;
    let dir = test_dir_name();

    vol.create_directory(Path::new(&dir)).await.unwrap();
    let content = b"imported content";
    let source = InMemoryVolume::new("Source");
    source
        .create_file(Path::new("/import-test.txt"), content)
        .await
        .unwrap();

    let smb_file = format!("{}/import-test.txt", dir);
    let stream = source.open_read_stream(Path::new("/import-test.txt")).await.unwrap();
    let size = stream.total_size();
    let bytes = vol
        .write_from_stream(Path::new(&smb_file), size, stream, &|_, _| {
            std::ops::ControlFlow::Continue(())
        })
        .await
        .unwrap();
    assert_eq!(bytes, content.len() as u64);

    assert!(vol.exists(Path::new(&smb_file)).await);
    let meta = vol.get_metadata(Path::new(&smb_file)).await.unwrap();
    assert_eq!(meta.size, Some(content.len() as u64));

    // Byte-level integrity: the bytes that landed on the SMB share must
    // be the same bytes the source stream produced. A bug in the write
    // pipeline (wrong chunk reused, compound-write fast-path mis-splitting
    // the buffer) would leave size correct but content wrong.
    let mut verify = vol.open_read_stream(Path::new(&smb_file)).await.unwrap();
    let mut readback = Vec::new();
    while let Some(Ok(chunk)) = verify.next_chunk().await {
        readback.extend_from_slice(&chunk);
    }
    assert_eq!(readback, content, "SMB destination bytes must match source bytes");

    vol.delete(Path::new(&smb_file)).await.unwrap();
    vol.delete(Path::new(&dir)).await.unwrap();
}

#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_scan_for_copy() {
    let vol = make_docker_volume().await;
    let dir = test_dir_name();

    // Create a small tree
    vol.create_directory(Path::new(&dir)).await.unwrap();
    let sub = format!("{}/inner", dir);
    vol.create_directory(Path::new(&sub)).await.unwrap();
    vol.create_file(Path::new(&format!("{}/f1.txt", dir)), b"aaa")
        .await
        .unwrap();
    vol.create_file(Path::new(&format!("{}/inner/f2.txt", dir)), b"bbbbbb")
        .await
        .unwrap();

    let result = vol.scan_for_copy(Path::new(&dir)).await.unwrap();
    assert_eq!(result.file_count, 2);
    assert_eq!(result.dir_count, 2); // dir + inner
    assert_eq!(result.total_bytes, 9); // 3 + 6

    // Clean up
    vol.delete(Path::new(&format!("{}/inner/f2.txt", dir))).await.unwrap();
    vol.delete(Path::new(&format!("{}/f1.txt", dir))).await.unwrap();
    vol.delete(Path::new(&sub)).await.unwrap();
    vol.delete(Path::new(&dir)).await.unwrap();
}

#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_scan_for_copy_batch_mixed() {
    // Pipelined batch scan on the SMB hot copy path.
    // Mixed batch of files + a directory: aggregate counts should match
    // what the per-path scan_for_copy loop would produce, and the
    // per_path vec should carry correct top_level_is_directory / size.
    let vol = make_docker_volume().await;
    let dir = test_dir_name();

    vol.create_directory(Path::new(&dir)).await.unwrap();
    vol.create_file(Path::new(&format!("{}/a.txt", dir)), b"aaa")
        .await
        .unwrap();
    vol.create_file(Path::new(&format!("{}/b.txt", dir)), b"bbbb")
        .await
        .unwrap();
    vol.create_file(Path::new(&format!("{}/c.txt", dir)), b"ccccc")
        .await
        .unwrap();
    let subdir = format!("{}/nested", dir);
    vol.create_directory(Path::new(&subdir)).await.unwrap();
    vol.create_file(Path::new(&format!("{}/nested/d.txt", dir)), b"dddddd")
        .await
        .unwrap();

    let paths: Vec<PathBuf> = vec![
        PathBuf::from(format!("{}/a.txt", dir)),
        PathBuf::from(format!("{}/b.txt", dir)),
        PathBuf::from(format!("{}/c.txt", dir)),
        PathBuf::from(format!("{}/nested", dir)),
        PathBuf::from(format!("{}/nested/d.txt", dir)),
    ];

    let batch = vol.scan_for_copy_batch(&paths).await.unwrap();

    // Compare against per-path scan_for_copy to ensure parity.
    let mut expected_files = 0usize;
    let mut expected_dirs = 0usize;
    let mut expected_bytes = 0u64;
    for p in &paths {
        let r = vol.scan_for_copy(p).await.unwrap();
        expected_files += r.file_count;
        expected_dirs += r.dir_count;
        expected_bytes += r.total_bytes;
    }
    assert_eq!(batch.aggregate.file_count, expected_files);
    assert_eq!(batch.aggregate.dir_count, expected_dirs);
    assert_eq!(batch.aggregate.total_bytes, expected_bytes);

    // per_path preserves input order and type info.
    assert_eq!(batch.per_path.len(), paths.len());
    for (i, (path, scan)) in batch.per_path.iter().enumerate() {
        assert_eq!(path, &paths[i]);
        let is_dir_name = path.to_string_lossy().ends_with("/nested");
        assert_eq!(scan.top_level_is_directory, is_dir_name, "path #{} type mismatch", i);
    }

    // The top-level files' per_path entries carry the file size.
    let a = batch
        .per_path
        .iter()
        .find(|(p, _)| p.to_string_lossy().ends_with("/a.txt"))
        .unwrap();
    assert_eq!(a.1.total_bytes, 3);

    // Cleanup.
    for entry in &["nested/d.txt", "a.txt", "b.txt", "c.txt"] {
        vol.delete(Path::new(&format!("{}/{}", dir, entry))).await.unwrap();
    }
    vol.delete(Path::new(&subdir)).await.unwrap();
    vol.delete(Path::new(&dir)).await.unwrap();
}

#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_scan_for_copy_batch_single_path() {
    // Regression guard for the N=1 fast-path: should behave exactly like
    // scan_for_copy and handle the empty-root case naturally.
    let vol = make_docker_volume().await;
    let dir = test_dir_name();

    vol.create_directory(Path::new(&dir)).await.unwrap();
    vol.create_file(Path::new(&format!("{}/only.txt", dir)), b"single")
        .await
        .unwrap();

    let path = PathBuf::from(format!("{}/only.txt", dir));
    let batch = vol.scan_for_copy_batch(std::slice::from_ref(&path)).await.unwrap();
    let single = vol.scan_for_copy(&path).await.unwrap();

    assert_eq!(batch.aggregate.file_count, single.file_count);
    assert_eq!(batch.aggregate.dir_count, single.dir_count);
    assert_eq!(batch.aggregate.total_bytes, single.total_bytes);
    assert_eq!(batch.per_path.len(), 1);
    assert_eq!(batch.per_path[0].0, path);
    assert!(!batch.per_path[0].1.top_level_is_directory);
    assert_eq!(batch.per_path[0].1.total_bytes, 6);

    vol.delete(&path).await.unwrap();
    vol.delete(Path::new(&dir)).await.unwrap();
}

#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_scan_for_copy_batch_propagates_missing_path_error() {
    // If one path in the batch doesn't exist, the whole batch must
    // surface an error (callers treat scan as a pre-flight gate: a
    // missing source is a user-visible problem, not a silent drop).
    let vol = make_docker_volume().await;
    let dir = test_dir_name();

    vol.create_directory(Path::new(&dir)).await.unwrap();
    vol.create_file(Path::new(&format!("{}/real.txt", dir)), b"data")
        .await
        .unwrap();

    let paths: Vec<PathBuf> = vec![
        PathBuf::from(format!("{}/real.txt", dir)),
        PathBuf::from(format!("{}/does-not-exist.txt", dir)),
        PathBuf::from(format!("{}/also-real-but-missing.txt", dir)),
    ];

    let result = vol.scan_for_copy_batch(&paths).await;
    assert!(matches!(result, Err(VolumeError::NotFound(_))));

    // Cleanup.
    vol.delete(Path::new(&format!("{}/real.txt", dir))).await.unwrap();
    vol.delete(Path::new(&dir)).await.unwrap();
}

#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_scan_for_conflicts() {
    let vol = make_docker_volume().await;
    let dir = test_dir_name();

    vol.create_directory(Path::new(&dir)).await.unwrap();
    vol.create_file(Path::new(&format!("{}/exists.txt", dir)), b"data")
        .await
        .unwrap();

    let source_items = vec![
        SourceItemInfo {
            name: "exists.txt".to_string(),
            size: 100,
            modified: Some(0),
            is_directory: false,
        },
        SourceItemInfo {
            name: "missing.txt".to_string(),
            size: 200,
            modified: Some(0),
            is_directory: false,
        },
    ];

    let conflicts = vol.scan_for_conflicts(&source_items, Path::new(&dir)).await.unwrap();
    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].source_path, "exists.txt");

    // Clean up
    vol.delete(Path::new(&format!("{}/exists.txt", dir))).await.unwrap();
    vol.delete(Path::new(&dir)).await.unwrap();
}

#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_space_info() {
    let vol = make_docker_volume().await;
    let space = vol.get_space_info().await.unwrap();
    assert!(space.total_bytes > 0);
    assert!(space.available_bytes > 0);
    assert!(space.used_bytes <= space.total_bytes);
}

/// End-to-end proof that a zip living on a REAL SMB share browses and extracts
/// through `SmbVolume::read_range` (backed by `smb2::FileReader`) — the remote
/// counterpart to the in-memory `remote_backed_archive_*` unit tests in
/// `archive/volume_test.rs`. This is the integration link the ranged-read
/// primitive exists for.
///
/// Writes a small zip to the share, wraps the live `SmbVolume` as an
/// `ArchiveVolume` parent (a direct-SMB volume reports
/// `supports_local_fs_access() == false`, so the archive takes the ranged-read
/// path, not a local `pread`), then lists the root and extracts a STORED and a
/// DEFLATED entry, checking the decompressed bytes.
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_archive_browse_and_extract_via_read_range() {
    use crate::file_system::volume::backends::archive::ArchiveVolume;
    use std::io::Write as _;

    async fn drain_archive(archive: &ArchiveVolume, inner: &str) -> Vec<u8> {
        let mut stream = archive.open_read_stream(Path::new(inner)).await.unwrap();
        let mut out = Vec::new();
        while let Some(chunk) = stream.next_chunk().await {
            out.extend_from_slice(&chunk.expect("archive extract chunk"));
        }
        out
    }

    let vol = Arc::new(make_docker_volume().await);

    // Build a small zip: one STORED entry at the root, one DEFLATED entry in a
    // subdirectory (so the synthetic-directory browse path runs too).
    let zip_bytes = {
        let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let stored = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        let deflated = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        w.start_file("a.txt", stored).unwrap();
        w.write_all(b"hello").unwrap();
        w.start_file("dir/b.txt", deflated).unwrap();
        w.write_all(b"world from a deflated entry").unwrap();
        w.finish().unwrap().into_inner()
    };

    // Unique root-level name so the no-clobber `create_file` never collides and
    // no directory setup is needed.
    let zip_path = PathBuf::from(format!("/{}.zip", test_dir_name()));
    vol.create_file(&zip_path, &zip_bytes).await.unwrap();

    // Wrap the live SMB volume as the archive's parent. Direct SMB has no local
    // FS access, so every archive read flows through `SmbVolume::read_range`.
    assert!(!vol.supports_local_fs_access());
    let archive = ArchiveVolume::new(Arc::clone(&vol) as Arc<dyn Volume>, zip_path.clone());

    // Browse: root shows the synthetic `dir` first, then the file.
    let root = archive.list_directory(Path::new(""), None).await.unwrap();
    let names: Vec<String> = root.iter().map(|e| e.name.clone()).collect();
    assert_eq!(names, vec!["dir", "a.txt"], "unexpected archive root listing");

    // Extract both entries, pulling every byte through the ranged-read seam.
    assert_eq!(drain_archive(&archive, "a.txt").await, b"hello");
    assert_eq!(
        drain_archive(&archive, "dir/b.txt").await,
        b"world from a deflated entry"
    );

    // Cleanup: remove the zip from the share.
    let _ = vol.delete(&zip_path).await;
}

// ── Remote-backed archive write-routing + edit (M5) ─────────────────
//
// The read-path counterpart above (`smb_integration_archive_browse_and_extract_via_read_range`)
// proves a zip on a share BROWSES and EXTRACTS. These prove the WRITE side end
// to end on a real share: the async parent-aware routing predicate detects a
// zip-inner path over SMB, an extract-out materializes to local disk, and a
// remote EDIT (pull → apply → upload → swap) commits — while a cancel before the
// swap leaves the remote original byte-for-byte intact. The data-safety contract
// unit-tested with an `InMemoryVolume` in `archive_remote_edit_tests`, now over
// real SMB.

/// A no-op `MutationHooks` for the mutator (never pauses/cancels here).
struct RemoteEditNoHooks;
impl crate::file_system::volume::backends::archive::mutator::MutationHooks for RemoteEditNoHooks {}

/// Streams a zip back off the share via `open_read_stream` and parses it into a
/// `name -> contents` map, so assertions re-verify the archive THROUGH THE SHARE
/// (not the local working copy). A corrupt swap fails loudly here.
async fn read_share_zip(vol: &SmbVolume, path: &Path) -> std::collections::HashMap<String, Vec<u8>> {
    use std::io::Read as _;
    let mut stream = vol.open_read_stream(path).await.expect("open remote archive");
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next_chunk().await {
        bytes.extend_from_slice(&chunk.expect("read archive chunk"));
    }
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).expect("share archive parses");
    let mut out = std::collections::HashMap::new();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).expect("entry");
        let name = entry.name().to_string();
        let mut buf = Vec::new();
        entry.read_to_end(&mut buf).expect("entry bytes");
        out.insert(name, buf);
    }
    out
}

/// True if a `<zip>.cmdr-tmp-*` upload temp for `zip_path` lingers in the share
/// root (a debris check scoped to this test's unique zip name, so it ignores
/// other parallel tests' artifacts).
async fn upload_temp_lingers(vol: &SmbVolume, zip_path: &Path) -> bool {
    let temp_prefix = format!(
        "{}.cmdr-tmp-",
        zip_path.file_name().expect("zip name").to_string_lossy()
    );
    vol.list_directory_impl(Path::new("/"))
        .await
        .map(|entries| entries.iter().any(|e| e.name.starts_with(&temp_prefix)))
        .unwrap_or(false)
}

/// Two entries: a stored one to keep and a deflated one to drop, so a delete edit
/// has something to remove and something to retain (verbatim raw-copy).
fn two_entry_zip() -> Vec<u8> {
    use std::io::Write as _;
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let stored = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let deflated = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    w.start_file("keep.txt", stored).unwrap();
    w.write_all(b"keep me").unwrap();
    w.start_file("drop.txt", deflated).unwrap();
    w.write_all(b"delete me from the share").unwrap();
    w.finish().unwrap().into_inner()
}

/// The async, parent-aware write-routing predicate detects a zip-INNER path on a
/// real SMB share (the `std::fs`-only sync predicate would wrongly return false),
/// and an extract-out streams an entry to LOCAL disk through the ranged reads.
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_archive_routing_detection_and_extract_out() {
    use crate::file_system::get_volume_manager;
    use crate::file_system::volume::backends::archive::ArchiveVolume;
    use std::io::Write as _;

    let vol = Arc::new(make_docker_volume().await);
    let zip_path = PathBuf::from(format!("/{}.zip", test_dir_name()));
    vol.create_file(&zip_path, &two_entry_zip()).await.unwrap();

    // Register the live SMB volume so the parent-aware predicate confirms the
    // boundary through the SMB volume's OWN `get_metadata` + `read_range`.
    let vol_id = "smb-archive-routing-test";
    get_volume_manager().register(vol_id, Arc::clone(&vol) as Arc<dyn Volume>);
    assert!(!vol.supports_local_fs_access(), "direct SMB is not local-FS-backed");

    // A genuinely-inner path routes; the `.zip` file itself is a plain file.
    assert!(
        get_volume_manager()
            .path_is_inside_archive(vol_id, &zip_path.join("drop.txt"))
            .await,
        "a zip-inner path on a real SMB share must be detected (write-routing reaches the edit driver)"
    );
    assert!(
        !get_volume_manager().path_is_inside_archive(vol_id, &zip_path).await,
        "the `.zip` file itself is a plain file, not archive-inner"
    );

    // Extract-out: stream a DEFLATED entry through the archive (over `read_range`)
    // and materialize it to LOCAL disk, then read it back.
    let archive = ArchiveVolume::new(Arc::clone(&vol) as Arc<dyn Volume>, zip_path.clone());
    let local_dir = tempfile::tempdir().unwrap();
    let out_path = local_dir.path().join("drop.txt");
    {
        let mut stream = archive.open_read_stream(Path::new("drop.txt")).await.unwrap();
        let mut f = std::fs::File::create(&out_path).unwrap();
        while let Some(chunk) = stream.next_chunk().await {
            f.write_all(&chunk.expect("extract chunk")).unwrap();
        }
        f.flush().unwrap();
    }
    assert_eq!(std::fs::read(&out_path).unwrap(), b"delete me from the share");

    get_volume_manager().unregister(vol_id);
    let _ = vol.delete(&zip_path).await;
}

/// A remote EDIT (delete an inner entry) commits through pull → apply → upload →
/// swap against a real SMB share, and the result is re-verified BY RE-READING the
/// zip off the share. No upload temp lingers.
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_remote_zip_edit_deletes_an_entry_through_the_share() {
    use crate::file_system::volume::backends::archive::mutator::{self, Changeset};
    use crate::file_system::write_operations::{RemoteEditError, WriteOperationState, pull_apply_upload_swap};
    use std::time::Duration;

    let vol = Arc::new(make_docker_volume().await);
    let zip_path = PathBuf::from(format!("/{}.zip", test_dir_name()));
    vol.create_file(&zip_path, &two_entry_zip()).await.unwrap();

    let state = Arc::new(WriteOperationState::new(Duration::from_millis(50)));
    let result = pull_apply_upload_swap(
        Arc::clone(&vol) as Arc<dyn Volume>,
        zip_path.clone(),
        state,
        move |working: &Path| -> Result<(), RemoteEditError> {
            let changeset = Changeset {
                deletes: vec!["drop.txt".to_string()],
                ..Default::default()
            };
            mutator::apply(working, &changeset, &RemoteEditNoHooks).expect("local mutator apply");
            Ok(())
        },
    )
    .await;
    assert!(result.is_ok(), "a remote SMB zip edit should commit");

    // Re-verify through the share: the deleted entry is gone, the kept one survives.
    let back = read_share_zip(vol.as_ref(), &zip_path).await;
    assert!(
        back.contains_key("keep.txt"),
        "the retained entry survives on the share"
    );
    assert!(
        !back.contains_key("drop.txt"),
        "the deleted entry is gone on the share, got: {:?}",
        back.keys().collect::<Vec<_>>()
    );
    assert_eq!(back.get("keep.txt").map(Vec::as_slice), Some(b"keep me".as_slice()));

    assert!(
        !upload_temp_lingers(vol.as_ref(), &zip_path).await,
        "no leftover upload temp after the swap"
    );

    let _ = vol.delete(&zip_path).await;
}

/// A cancel landing AFTER the local apply but BEFORE the remote swap leaves the
/// share's original byte-for-byte intact (both entries still present) and drops no
/// upload temp — the core M5 data-safety guarantee, over real SMB.
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_remote_zip_edit_cancel_before_swap_keeps_original() {
    use crate::file_system::volume::backends::archive::mutator::{self, Changeset};
    use crate::file_system::write_operations::{
        OperationIntent, RemoteEditError, WriteOperationState, pull_apply_upload_swap,
    };
    use std::sync::atomic::Ordering;
    use std::time::Duration;

    let vol = Arc::new(make_docker_volume().await);
    let zip_path = PathBuf::from(format!("/{}.zip", test_dir_name()));
    vol.create_file(&zip_path, &two_entry_zip()).await.unwrap();

    let state = Arc::new(WriteOperationState::new(Duration::from_millis(50)));
    let state_for_closure = Arc::clone(&state);
    // Apply the delete to the LOCAL working copy, then flip the op to cancelled so
    // the orchestrator's pre-upload cancel check trips before touching the share.
    let result = pull_apply_upload_swap(
        Arc::clone(&vol) as Arc<dyn Volume>,
        zip_path.clone(),
        state,
        move |working: &Path| -> Result<(), RemoteEditError> {
            let changeset = Changeset {
                deletes: vec!["drop.txt".to_string()],
                ..Default::default()
            };
            mutator::apply(working, &changeset, &RemoteEditNoHooks).expect("local mutator apply");
            state_for_closure
                .intent
                .store(OperationIntent::Stopped as u8, Ordering::Relaxed);
            Ok(())
        },
    )
    .await;
    assert!(
        matches!(result, Err(RemoteEditError::Cancelled)),
        "a cancel before the swap must report Cancelled"
    );

    // The share's original is intact: BOTH entries still present, no temp debris.
    let back = read_share_zip(vol.as_ref(), &zip_path).await;
    assert!(back.contains_key("keep.txt"), "the original keep.txt survives");
    assert!(
        back.contains_key("drop.txt"),
        "the cancelled delete never reached the share"
    );
    assert!(
        !upload_temp_lingers(vol.as_ref(), &zip_path).await,
        "a cancelled edit leaves no upload temp on the share"
    );

    let _ = vol.delete(&zip_path).await;
}
