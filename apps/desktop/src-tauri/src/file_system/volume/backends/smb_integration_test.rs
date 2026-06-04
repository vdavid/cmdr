//! Integration tests for the SMB backend (require Docker SMB containers).
//!
//! Every test here is `#[ignore]`d so default runs skip it. Start the
//! containers with `./apps/desktop/test/smb-servers/start.sh`, then run
//! `cargo nextest run smb_integration --run-ignored all`. Declared as a
//! `#[cfg(test)]` submodule of `smb`; shared helpers come from
//! `super::smb_test_support`.

use super::smb_test_support::*;
use super::*;
use crate::file_system::volume::InMemoryVolume;
use crate::file_system::volume::smb_volume_id;

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

// ── SMB streaming integration tests (Docker) ───────────────────

#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_open_read_stream() {
    let vol = make_docker_volume().await;
    let dir = test_dir_name();
    ensure_clean(&vol, &dir).await;
    vol.create_directory(Path::new(&dir)).await.unwrap();

    let data = b"streaming read test content";
    vol.create_file(Path::new(&format!("{}/read.txt", dir)), data)
        .await
        .unwrap();

    let mut stream = vol
        .open_read_stream(Path::new(&format!("{}/read.txt", dir)))
        .await
        .unwrap();
    assert_eq!(stream.total_size(), data.len() as u64);

    let mut reassembled = Vec::new();
    while let Some(Ok(chunk)) = stream.next_chunk().await {
        reassembled.extend_from_slice(&chunk);
    }
    assert_eq!(reassembled, data);
    assert_eq!(stream.bytes_read(), data.len() as u64);

    ensure_clean(&vol, &dir).await;
}

#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_write_from_stream() {
    let vol = make_docker_volume().await;
    let dir = test_dir_name();
    ensure_clean(&vol, &dir).await;
    vol.create_directory(Path::new(&dir)).await.unwrap();

    // Create a source via InMemoryVolume
    let source = InMemoryVolume::new("Source");
    let data: Vec<u8> = (0..=255).cycle().take(50_000).collect();
    source.create_file(Path::new("/payload.bin"), &data).await.unwrap();

    let stream = source.open_read_stream(Path::new("/payload.bin")).await.unwrap();
    let no_progress = &|_: u64, _: u64| std::ops::ControlFlow::Continue(());
    let bytes = vol
        .write_from_stream(Path::new(&format!("{}/payload.bin", dir)), 50_000, stream, no_progress)
        .await
        .unwrap();
    assert_eq!(bytes, 50_000);

    // Read back and verify content integrity
    let mut verify = vol
        .open_read_stream(Path::new(&format!("{}/payload.bin", dir)))
        .await
        .unwrap();
    let mut readback = Vec::new();
    while let Some(Ok(chunk)) = verify.next_chunk().await {
        readback.extend_from_slice(&chunk);
    }
    assert_eq!(readback, data);

    ensure_clean(&vol, &dir).await;
}

#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_write_from_stream_with_progress() {
    let vol = make_docker_volume().await;
    let dir = test_dir_name();
    ensure_clean(&vol, &dir).await;
    vol.create_directory(Path::new(&dir)).await.unwrap();

    let source = InMemoryVolume::new("Source");
    let data = vec![0xCD; 200_000]; // ~200 KB
    source.create_file(Path::new("/big.bin"), &data).await.unwrap();

    use std::sync::atomic::{AtomicU64, AtomicUsize};

    let progress_calls = AtomicUsize::new(0);
    let last_bytes = AtomicU64::new(0);

    let stream = source.open_read_stream(Path::new("/big.bin")).await.unwrap();
    let bytes = vol
        .write_from_stream(
            Path::new(&format!("{}/big.bin", dir)),
            200_000,
            stream,
            &|bytes_done, total| {
                progress_calls.fetch_add(1, Ordering::Relaxed);
                last_bytes.store(bytes_done, Ordering::Relaxed);
                assert_eq!(total, 200_000);
                std::ops::ControlFlow::Continue(())
            },
        )
        .await
        .unwrap();

    assert_eq!(bytes, 200_000);
    assert!(
        progress_calls.load(Ordering::Relaxed) >= 1,
        "expected at least 1 progress call"
    );
    assert_eq!(last_bytes.load(Ordering::Relaxed), 200_000);

    // Byte-level integrity: a progress-reporting write that loses or
    // duplicates chunks would still satisfy the "progress_calls >= 1
    // and final bytes_done == 200_000" assertions; hash the destination
    // against the source to catch that.
    let mut verify = vol
        .open_read_stream(Path::new(&format!("{}/big.bin", dir)))
        .await
        .unwrap();
    let mut readback = Vec::with_capacity(200_000);
    while let Some(Ok(chunk)) = verify.next_chunk().await {
        readback.extend_from_slice(&chunk);
    }
    assert_eq!(readback, data, "destination bytes must match source bytes");

    ensure_clean(&vol, &dir).await;
}

#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_write_from_stream_cancel() {
    let vol = make_docker_volume().await;
    let dir = test_dir_name();
    ensure_clean(&vol, &dir).await;
    vol.create_directory(Path::new(&dir)).await.unwrap();

    let source = InMemoryVolume::new("Source");
    let data = vec![0xEF; 500_000]; // ~500 KB, several chunks
    source.create_file(Path::new("/big.bin"), &data).await.unwrap();

    let call_count = std::sync::atomic::AtomicUsize::new(0);
    let stream = source.open_read_stream(Path::new("/big.bin")).await.unwrap();
    let result = vol
        .write_from_stream(Path::new(&format!("{}/big.bin", dir)), 500_000, stream, &|_, _| {
            let n = call_count.fetch_add(1, Ordering::Relaxed);
            if n >= 1 {
                std::ops::ControlFlow::Break(())
            } else {
                std::ops::ControlFlow::Continue(())
            }
        })
        .await;

    assert!(result.is_err(), "expected cancellation error");

    ensure_clean(&vol, &dir).await;
}

#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_cross_volume_streaming_copy() {
    // Full end-to-end: InMemoryVolume → SmbVolume via open_read_stream + write_from_stream.
    // Tests the same path that copy_single_path uses for non-local volumes.
    use std::sync::atomic::{AtomicUsize, Ordering};

    let smb_vol = make_docker_volume().await;
    let dir = test_dir_name();
    ensure_clean(&smb_vol, &dir).await;
    smb_vol.create_directory(Path::new(&dir)).await.unwrap();

    let source = InMemoryVolume::new("Source");
    let data: Vec<u8> = (0..=255).cycle().take(100_000).collect();
    source.create_file(Path::new("/photo.bin"), &data).await.unwrap();

    let progress_calls = AtomicUsize::new(0);

    // Read from InMemory, write to SMB (the same path copy_single_path takes)
    let stream = source.open_read_stream(Path::new("/photo.bin")).await.unwrap();
    let bytes = smb_vol
        .write_from_stream(Path::new(&format!("{}/photo.bin", dir)), 100_000, stream, &|_, _| {
            progress_calls.fetch_add(1, Ordering::Relaxed);
            std::ops::ControlFlow::Continue(())
        })
        .await
        .unwrap();

    assert_eq!(bytes, 100_000);
    assert!(progress_calls.load(Ordering::Relaxed) >= 1);

    // Verify content via read back
    let mut verify = smb_vol
        .open_read_stream(Path::new(&format!("{}/photo.bin", dir)))
        .await
        .unwrap();
    let mut readback = Vec::new();
    while let Some(Ok(chunk)) = verify.next_chunk().await {
        readback.extend_from_slice(&chunk);
    }
    assert_eq!(readback, data);

    ensure_clean(&smb_vol, &dir).await;
}

#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_open_read_stream_large_file_spans_many_chunks() {
    // Verifies the streaming reader delivers a multi-MB file correctly
    // across many chunk boundaries. Before the channel-backed rewrite, the
    // whole file was buffered in memory up front.
    //
    // The file has to exceed `max_read_size` (up to 8 MB on Samba) for
    // smb2 to split the read into more than one READ. 20 MB is a safe
    // multiple that stays under the single-chunk ceiling.
    let vol = make_docker_volume().await;
    let dir = test_dir_name();
    ensure_clean(&vol, &dir).await;
    vol.create_directory(Path::new(&dir)).await.unwrap();

    // 20 MB: guarantees multiple READs even at 8 MB max_read_size.
    let size = 20 * 1024 * 1024;
    let data: Vec<u8> = (0..size).map(|i| (i % 251) as u8).collect();
    let smb_path = format!("{}/big-stream.bin", dir);
    vol.create_file(Path::new(&smb_path), &data).await.unwrap();

    // Hash chunks as they arrive so a 20 MB mismatch produces a single
    // 32-byte hex pair instead of a 20 MB `Vec<u8>` diff. Also avoids
    // the 20 MB reassembly allocation.
    let mut stream = vol.open_read_stream(Path::new(&smb_path)).await.unwrap();
    assert_eq!(stream.total_size(), size as u64);

    let mut hasher = blake3::Hasher::new();
    let mut chunks_seen = 0usize;
    let mut total_read = 0usize;
    while let Some(result) = stream.next_chunk().await {
        let chunk = result.unwrap();
        assert!(!chunk.is_empty(), "should not yield empty chunks");
        hasher.update(&chunk);
        total_read += chunk.len();
        chunks_seen += 1;
    }
    assert_eq!(total_read, size, "total bytes streamed must equal source size");
    let readback_hash = *hasher.finalize().as_bytes();
    let expected_hash = hash_bytes(&data);
    assert_eq!(
        readback_hash, expected_hash,
        "streamed bytes must match source (expected blake3 {:x?}, got {:x?})",
        expected_hash, readback_hash
    );
    assert_eq!(stream.bytes_read(), size as u64);
    assert!(chunks_seen >= 2, "multi-MB file should span multiple chunks");

    ensure_clean(&vol, &dir).await;
}

#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_read_stream_large_file_multi_chunk() {
    // SMB → local byte path now goes through `open_read_stream`, then the
    // caller writes into whatever destination. Verify that the streaming
    // reader yields multiple chunks for a multi-MB file.
    //
    // `max_read_size` negotiation can go up to 8 MB on modern Samba, so
    // the file has to be >8 MB to guarantee multiple READs.
    let vol = make_docker_volume().await;
    let dir = test_dir_name();
    ensure_clean(&vol, &dir).await;
    vol.create_directory(Path::new(&dir)).await.unwrap();

    let size = 20 * 1024 * 1024; // 20 MB, exceeds 8 MB max_read_size
    let data: Vec<u8> = (0..size).map(|i| ((i * 7) % 251) as u8).collect();
    let smb_path = format!("{}/export-large.bin", dir);
    vol.create_file(Path::new(&smb_path), &data).await.unwrap();

    // Hash chunks as they arrive (see the sibling large-file test for
    // why we avoid `assert_eq!` on 20 MB `Vec<u8>`s).
    let mut stream = vol.open_read_stream(Path::new(&smb_path)).await.unwrap();
    assert_eq!(stream.total_size(), size as u64);

    let mut chunks_seen = 0usize;
    let mut hasher = blake3::Hasher::new();
    let mut total_read = 0usize;
    while let Some(Ok(chunk)) = stream.next_chunk().await {
        chunks_seen += 1;
        hasher.update(&chunk);
        total_read += chunk.len();
    }
    assert!(
        chunks_seen >= 2,
        "streaming should yield multiple chunks for a multi-MB file"
    );
    assert_eq!(total_read, size, "total bytes streamed must equal source size");
    let readback_hash = *hasher.finalize().as_bytes();
    let expected_hash = hash_bytes(&data);
    assert_eq!(
        readback_hash, expected_hash,
        "streamed bytes must match source (expected blake3 {:x?}, got {:x?})",
        expected_hash, readback_hash
    );

    ensure_clean(&vol, &dir).await;
}

#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_open_read_stream_cancel_by_drop() {
    // Drop the stream mid-way and verify that subsequent SMB operations
    // on the same volume still work (producer task released the mutex).
    let vol = make_docker_volume().await;
    let dir = test_dir_name();
    ensure_clean(&vol, &dir).await;
    vol.create_directory(Path::new(&dir)).await.unwrap();

    let data = vec![0xAA; 2 * 1024 * 1024]; // 2 MB
    let smb_path = format!("{}/cancel-me.bin", dir);
    vol.create_file(Path::new(&smb_path), &data).await.unwrap();

    let mut stream = vol.open_read_stream(Path::new(&smb_path)).await.unwrap();
    // Read exactly one chunk then drop
    let _first = stream.next_chunk().await.unwrap().unwrap();
    drop(stream);

    // Subsequent op on the volume should succeed; the producer task
    // must have released the session mutex on cancel.
    let entries = vol.list_directory(Path::new(&dir), None).await.unwrap();
    assert!(entries.iter().any(|e| e.name == "cancel-me.bin"));

    ensure_clean(&vol, &dir).await;
}

#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_write_from_stream_local_source_large_file() {
    // Local → SMB byte path now goes through LocalPosixVolume's
    // `open_read_stream` + SmbVolume's `write_from_stream`. Verify that
    // multi-MB input triggers multiple progress callbacks and round-trips.
    use std::sync::atomic::{AtomicU64, AtomicUsize};

    let vol = make_docker_volume().await;
    let dir = test_dir_name();
    ensure_clean(&vol, &dir).await;
    vol.create_directory(Path::new(&dir)).await.unwrap();

    let size = 4 * 1024 * 1024; // 4 MB, spans multiple import chunks
    let data: Vec<u8> = (0..size).map(|i| ((i * 13) % 251) as u8).collect();

    let local_tmp = std::env::temp_dir().join(format!("cmdr-smb-import-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&local_tmp);
    std::fs::create_dir_all(&local_tmp).unwrap();
    std::fs::write(local_tmp.join("import-large.bin"), &data).unwrap();

    let local_vol = crate::file_system::volume::LocalPosixVolume::new("local-src", local_tmp.clone());

    let smb_path = format!("{}/import-large.bin", dir);
    let progress_calls = AtomicUsize::new(0);
    let last_bytes = AtomicU64::new(0);

    let stream = local_vol.open_read_stream(Path::new("import-large.bin")).await.unwrap();
    assert_eq!(stream.total_size(), size as u64);

    let bytes = vol
        .write_from_stream(Path::new(&smb_path), size as u64, stream, &|done, total| {
            progress_calls.fetch_add(1, Ordering::Relaxed);
            last_bytes.store(done, Ordering::Relaxed);
            assert_eq!(total, size as u64);
            std::ops::ControlFlow::Continue(())
        })
        .await
        .unwrap();

    assert_eq!(bytes, size as u64);
    assert!(
        progress_calls.load(Ordering::Relaxed) >= 2,
        "streaming write should call progress multiple times for a multi-chunk source"
    );
    assert_eq!(last_bytes.load(Ordering::Relaxed), size as u64);

    // Byte-level integrity: hash the source and the destination and
    // compare. Streaming hash avoids materializing a 4 MB `Vec<u8>`
    // just to `assert_eq!` it, and on mismatch we get a legible hex
    // dump instead of a multi-megabyte diff.
    let expected_hash = hash_bytes(&data);
    let actual_hash = hash_volume_file(&vol as &dyn Volume, Path::new(&smb_path)).await;
    assert_eq!(
        actual_hash, expected_hash,
        "SMB destination bytes must match source (expected blake3 {:x?}, got {:x?})",
        expected_hash, actual_hash
    );

    let _ = std::fs::remove_dir_all(&local_tmp);
    ensure_clean(&vol, &dir).await;
}

#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_write_from_stream_streams_large_file() {
    // InMemoryVolume → SmbVolume via write_from_stream with a multi-chunk
    // source. Verifies the SMB write path now pulls chunks on demand
    // rather than collecting the full source into a Vec<u8>.
    use std::sync::atomic::{AtomicU64, AtomicUsize};

    let vol = make_docker_volume().await;
    let dir = test_dir_name();
    ensure_clean(&vol, &dir).await;
    vol.create_directory(Path::new(&dir)).await.unwrap();

    let size: usize = 4 * 1024 * 1024; // 4 MB
    let data: Vec<u8> = (0..size).map(|i| ((i * 11) % 251) as u8).collect();

    let source = InMemoryVolume::new("Source");
    source.create_file(Path::new("/big-stream.bin"), &data).await.unwrap();

    let smb_path = format!("{}/big-stream.bin", dir);
    let progress_calls = AtomicUsize::new(0);
    let last_bytes = AtomicU64::new(0);

    let stream = source.open_read_stream(Path::new("/big-stream.bin")).await.unwrap();
    let bytes = vol
        .write_from_stream(Path::new(&smb_path), size as u64, stream, &|done, total| {
            progress_calls.fetch_add(1, Ordering::Relaxed);
            last_bytes.store(done, Ordering::Relaxed);
            assert_eq!(total, size as u64);
            std::ops::ControlFlow::Continue(())
        })
        .await
        .unwrap();

    assert_eq!(bytes, size as u64);
    assert!(
        progress_calls.load(Ordering::Relaxed) >= 2,
        "streaming write should call progress multiple times for a multi-chunk source"
    );
    assert_eq!(last_bytes.load(Ordering::Relaxed), size as u64);

    // Byte-level integrity: streaming hash over the destination catches
    // any chunk drop/duplicate/reuse that "bytes_written == expected"
    // on its own can't see. See the sibling local-source test for the
    // rationale on hashing vs. `assert_eq!` on a 4 MB buffer.
    let expected_hash = hash_bytes(&data);
    let actual_hash = hash_volume_file(&vol as &dyn Volume, Path::new(&smb_path)).await;
    assert_eq!(
        actual_hash, expected_hash,
        "SMB destination bytes must match source (expected blake3 {:x?}, got {:x?})",
        expected_hash, actual_hash
    );

    ensure_clean(&vol, &dir).await;
}

#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_write_from_stream_cancel_mid_write() {
    // Cancel partway through a multi-chunk write via progress-break.
    // Verifies Cancelled is returned and that the SMB session is still
    // usable for subsequent ops (writer.abort() drains in-flight WRITE
    // responses cleanly on cancel, best-effort-deletes the partial file).
    let vol = make_docker_volume().await;
    let dir = test_dir_name();
    ensure_clean(&vol, &dir).await;
    vol.create_directory(Path::new(&dir)).await.unwrap();

    let size = 4 * 1024 * 1024; // 4 MB, several write chunks
    let data = vec![0xC3u8; size];

    let source = InMemoryVolume::new("Source");
    source.create_file(Path::new("/cancel-me.bin"), &data).await.unwrap();

    let smb_path = format!("{}/cancel-me.bin", dir);
    let call_count = std::sync::atomic::AtomicUsize::new(0);

    let stream = source.open_read_stream(Path::new("/cancel-me.bin")).await.unwrap();
    let result = vol
        .write_from_stream(Path::new(&smb_path), size as u64, stream, &|_, _| {
            let n = call_count.fetch_add(1, Ordering::Relaxed);
            if n >= 1 {
                std::ops::ControlFlow::Break(())
            } else {
                std::ops::ControlFlow::Continue(())
            }
        })
        .await;

    assert!(
        matches!(result, Err(VolumeError::Cancelled(_))),
        "expected Cancelled, got {result:?}"
    );

    // The session must still work after cancel.
    let _ = vol.list_directory(Path::new(&dir), None).await.unwrap();

    ensure_clean(&vol, &dir).await;
}

/// A read stream that yields a fixed number of good chunks, then a source
/// read error on the next pull. Used to exercise the partial-file cleanup
/// on the write_from_stream ERROR path: once the SMB `FileWriter` is open
/// and a chunk has streamed into it, the source error must propagate AND
/// the half-written file must be deleted from the destination.
struct ErroringReadStream {
    good_chunks: usize,
    chunk: Vec<u8>,
    total_size: u64,
    bytes_read: u64,
    yielded: usize,
}

impl VolumeReadStream for ErroringReadStream {
    fn next_chunk(&mut self) -> Pin<Box<dyn Future<Output = Option<Result<Vec<u8>, VolumeError>>> + Send + '_>> {
        Box::pin(async move {
            if self.yielded < self.good_chunks {
                self.yielded += 1;
                self.bytes_read += self.chunk.len() as u64;
                Some(Ok(self.chunk.clone()))
            } else {
                Some(Err(VolumeError::IoError {
                    message: "Injected source read error".to_string(),
                    raw_os_error: None,
                }))
            }
        })
    }

    fn total_size(&self) -> u64 {
        self.total_size
    }

    fn bytes_read(&self) -> u64 {
        self.bytes_read
    }
}

#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_write_from_stream_source_error_deletes_partial() {
    // Mid-stream source read error on the streaming path leaves a partial
    // file open on the server. The write_from_stream ERROR path must
    // delete that partial (mirroring the cancel branch) and propagate the
    // ORIGINAL error (NOT Cancelled). Without the fix, the destination
    // keeps a half-written file under the user's intended name.
    let vol = make_docker_volume().await;
    let dir = test_dir_name();
    ensure_clean(&vol, &dir).await;
    vol.create_directory(Path::new(&dir)).await.unwrap();

    // `size` larger than any plausible max_write_size forces the streaming
    // writer path (not the compound fast-path), so the writer is genuinely
    // open when the source errors on the second pull.
    let chunk = vec![0xA7u8; 256 * 1024]; // 256 KB good chunk
    let size = 64 * 1024 * 1024u64; // 64 MB promised, far above max_write
    let stream = ErroringReadStream {
        good_chunks: 1,
        chunk,
        total_size: size,
        bytes_read: 0,
        yielded: 0,
    };

    let smb_path = format!("{}/partial-on-error.bin", dir);
    let result = vol
        .write_from_stream(Path::new(&smb_path), size, Box::new(stream), &|_, _| {
            std::ops::ControlFlow::Continue(())
        })
        .await;

    // The original IoError must propagate, NOT Cancelled.
    assert!(
        matches!(result, Err(VolumeError::IoError { .. })),
        "expected the source IoError to propagate, got {result:?}"
    );

    // The partial must be gone: cleanup deleted it on a fresh session.
    assert!(
        !vol.exists(Path::new(&smb_path)).await,
        "partial file was left at the destination after a source-read error"
    );

    // The session must still be usable for subsequent ops.
    let _ = vol.list_directory(Path::new(&dir), None).await.unwrap();

    ensure_clean(&vol, &dir).await;
}

/// Cross-task content integrity: 100 concurrent SMB → local copies, each file
/// with unique deterministic content. After the batch completes, every
/// destination's blake3 hash must match the hash of the source it claims to
/// come from: catches buffer reuse across tasks, wrong-buffer-to-wrong-path
/// routing, races in the `Arc<Mutex<Option<SmbClient>>>` +
/// `Arc<RwLock<Option<Arc<Tree>>>>` split-session (Fix 2), and
/// cross-MessageId wire demux mistakes on cloned `Connection`s.
///
/// Identical-content tests can't see any of these; every file would hash
/// the same, so a "swapped slice mid-file" or "task B's buffer landed under
/// task A's path" bug would pass trivially. Unique per-file content makes
/// any cross-contamination flip at least one destination's hash.
///
/// Runs the real copy pipeline (`copy_volumes_with_progress`, the same
/// function `copy_between_volumes` calls) so `FuturesUnordered` + Fix 2's
/// split session + Fix 3's compound fast-path + Fix 4's pipelined scan all
/// execute together, the way a user's "copy 100 files" action does.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_copy_100_unique_files_no_cross_contamination() {
    use crate::file_system::write_operations::{
        CollectorEventSink, VolumeCopyConfig, WriteOperationState, copy_volumes_with_progress,
    };
    use std::time::{Duration, Instant};

    // Content scheme: `blake3(b"cmdr-fix8-" || index_le) .as_bytes() repeated 320 times`
    // = 10_240 bytes per file, truly unique per index, every byte position varies
    // between files. Any cross-task slice swap (even a 32-byte block in the
    // middle of one file coming from a neighbor's buffer) flips blake3.
    // 10 KB keeps fixture setup cheap and stays inside the SMB compound
    // fast-path (Fix 3) so we're exercising it, not the streaming fallback.
    fn expected_content(index: usize) -> Vec<u8> {
        let mut seed = Vec::with_capacity(10 + 8);
        seed.extend_from_slice(b"cmdr-fix8-");
        seed.extend_from_slice(&(index as u64).to_le_bytes());
        let block = *blake3::hash(&seed).as_bytes(); // 32 bytes
        let mut out = Vec::with_capacity(32 * 320);
        for _ in 0..320 {
            out.extend_from_slice(&block);
        }
        out
    }

    const FILE_COUNT: usize = 100;

    // Hold the concrete `SmbVolume` for `ensure_clean` (which takes
    // `&SmbVolume`) and clone an `Arc<dyn Volume>` view of the same
    // session for the copy pipeline.
    let smb_vol = Arc::new(make_docker_volume().await);
    let src_dir = test_dir_name();
    ensure_clean(&smb_vol, &src_dir).await;
    smb_vol.create_directory(Path::new(&src_dir)).await.unwrap();
    let vol: Arc<dyn Volume> = smb_vol.clone();

    // Fixture: create 100 files on the SMB source, serially. Parallel
    // `create_file` on a single SMB session wouldn't speed this up
    // (creates are 1 RTT each), and keeping setup simple keeps any bug
    // the test catches unambiguously a read/copy-path bug, not a
    // write-path races-with-itself bug.
    let fixture_start = Instant::now();
    let mut source_paths: Vec<PathBuf> = Vec::with_capacity(FILE_COUNT);
    for i in 0..FILE_COUNT {
        let name = format!("f_{:03}.bin", i);
        let smb_path = format!("{}/{}", src_dir, name);
        vol.create_file(Path::new(&smb_path), &expected_content(i))
            .await
            .unwrap();
        source_paths.push(PathBuf::from(smb_path));
    }
    log::info!(
        "smb_integration_copy_100_unique_files: fixture setup took {:?}",
        fixture_start.elapsed()
    );

    // Destination: local TempDir wrapped in a LocalPosixVolume. We feed the
    // copy pipeline the same way production does (SMB volume → Local
    // volume → `copy_volumes_with_progress`). `dest_path` is "/" relative to
    // the local volume root (i.e. the TempDir itself).
    let local_dir = tempfile::TempDir::new().expect("create TempDir");
    let dest_vol: Arc<dyn Volume> = Arc::new(crate::file_system::volume::LocalPosixVolume::new(
        "dest",
        local_dir.path().to_path_buf(),
    ));

    let state = Arc::new(WriteOperationState::new(Duration::from_millis(200)));
    let events = Arc::new(CollectorEventSink::new());
    let config = VolumeCopyConfig::default();

    let copy_start = Instant::now();
    let result = copy_volumes_with_progress(
        events.clone(),
        "test-op-100-unique",
        &state,
        Arc::clone(&vol),
        &source_paths,
        Arc::clone(&dest_vol),
        Path::new("/"),
        &config,
    )
    .await;
    log::info!(
        "smb_integration_copy_100_unique_files: copy pipeline took {:?}",
        copy_start.elapsed()
    );
    assert!(result.is_ok(), "copy should succeed: {:?}", result);

    // Count landed files: cheap aggregate sanity check before per-index
    // verification. A cross-contamination bug that swapped two destinations
    // would still show 100 files here, so this is not the real check.
    let entries = std::fs::read_dir(local_dir.path())
        .expect("read dest dir")
        .filter_map(|e| e.ok())
        .count();
    assert_eq!(entries, FILE_COUNT, "expected {} files at destination", FILE_COUNT);

    // Per-index integrity: for each source index, read its destination file
    // and compare blake3 against the expected hash derived from the same
    // index. Assert each one individually so a swap of two destinations
    // fails loudly with both offending indices, not a vague aggregate.
    let mut mismatches: Vec<String> = Vec::new();
    for i in 0..FILE_COUNT {
        let name = format!("f_{:03}.bin", i);
        let dest_path = local_dir.path().join(&name);
        let actual_bytes = match std::fs::read(&dest_path) {
            Ok(b) => b,
            Err(e) => {
                mismatches.push(format!("{}: couldn't read destination: {}", name, e));
                continue;
            }
        };
        let expected_bytes = expected_content(i);
        let expected_hash = hash_bytes(&expected_bytes);
        let actual_hash = hash_bytes(&actual_bytes);
        if actual_hash != expected_hash {
            // Find the first diff position and a small slice of context;
            // a 10 KB diff dump would drown the terminal on any failure.
            let first_diff = expected_bytes.iter().zip(actual_bytes.iter()).position(|(a, b)| a != b);
            let diff_detail = match first_diff {
                Some(pos) => {
                    let end_exp = pos.saturating_add(16).min(expected_bytes.len());
                    let end_act = pos.saturating_add(16).min(actual_bytes.len());
                    format!(
                        "first diff at byte {}: expected {:02x?}, got {:02x?}",
                        pos,
                        &expected_bytes[pos..end_exp],
                        &actual_bytes[pos..end_act]
                    )
                }
                None => {
                    // Same bytes but different length (hashes differ so
                    // there must be a difference somewhere).
                    format!(
                        "byte-for-byte equal in overlap but lengths differ: expected {}, got {}",
                        expected_bytes.len(),
                        actual_bytes.len()
                    )
                }
            };
            mismatches.push(format!(
                "{}: expected blake3 {} ({} bytes), got blake3 {} ({} bytes); {}",
                name,
                hex_of(&expected_hash),
                expected_bytes.len(),
                hex_of(&actual_hash),
                actual_bytes.len(),
                diff_detail,
            ));
        }
    }
    assert!(
        mismatches.is_empty(),
        "{} of {} destinations failed content check:\n  - {}",
        mismatches.len(),
        FILE_COUNT,
        mismatches.join("\n  - "),
    );

    // Cleanup the SMB source. The TempDir cleans itself on drop.
    ensure_clean(&smb_vol, &src_dir).await;
}

/// Hex formatter for blake3 hashes in failure messages. Avoids a hex-crate
/// dep just for test diagnostics.
fn hex_of(bytes: &[u8; 32]) -> String {
    let mut s = String::with_capacity(64);
    for b in bytes {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

/// Captures `client-mutex:` (cmdr) and `recv:` (smb2 receiver loop)
/// debug lines into bounded ring buffers so a hung test's panic message
/// can include the last ~30 lines from each stream. That's invaluable
/// for diagnosing a future regression. Installed via `log::set_logger`
/// once per process; subsequent installs are no-ops.
struct MutexCaptureLogger {
    mutex_lines: std::sync::Mutex<std::collections::VecDeque<String>>,
    recv_lines: std::sync::Mutex<std::collections::VecDeque<String>>,
}

impl log::Log for MutexCaptureLogger {
    fn enabled(&self, _md: &log::Metadata) -> bool {
        true
    }
    fn log(&self, record: &log::Record) {
        let msg = format!("{}", record.args());
        let target = record.target();
        // `client-mutex:` lines come from smb.rs via `log::debug!` with
        // the module-path target (`cmdr_lib::file_system::volume::smb`).
        // `recv:` lines come from the smb2 receiver loop with an `smb2::*`
        // target.
        // allowed-error-string-match: routes log records into ring buffers by our own `log::debug!` message-prefix convention (`client-mutex:` from this file, `recv:` from the smb2 crate's receiver loop). Not error/state classification; we own both prefixes and `cleanup_test_prefix` would notice drift. Pinned by `mutex_capture_logger_routes_known_prefixes`.
        if msg.starts_with("client-mutex:") {
            let mut q = self.mutex_lines.lock().unwrap();
            if q.len() >= 200 {
                q.pop_front();
            }
            q.push_back(format!("[{}] {}", target, msg));
            // allowed-error-string-match: same convention as the `client-mutex:` branch above — routes smb2 receiver-loop log records by message prefix, not error/state classification. Pinned by `mutex_capture_logger_routes_known_prefixes`.
        } else if msg.starts_with("recv:") || (target.starts_with("smb2") && msg.contains("recv")) {
            let mut q = self.recv_lines.lock().unwrap();
            if q.len() >= 200 {
                q.pop_front();
            }
            q.push_back(format!("[{}] {}", target, msg));
        }
        // The captured ring buffers are the diagnostic. We deliberately
        // skip mirroring to stderr: `eprintln!` is denied crate-wide,
        // and re-emitting through `log::*` would recurse into this same
        // logger (and the mutex above) on every call.
    }
    fn flush(&self) {}
}

static MUTEX_CAPTURE_LOGGER: OnceLock<&'static MutexCaptureLogger> = OnceLock::new();

fn install_mutex_capture_logger() -> &'static MutexCaptureLogger {
    if let Some(l) = MUTEX_CAPTURE_LOGGER.get() {
        return l;
    }
    let leaked: &'static MutexCaptureLogger = Box::leak(Box::new(MutexCaptureLogger {
        mutex_lines: std::sync::Mutex::new(std::collections::VecDeque::with_capacity(200)),
        recv_lines: std::sync::Mutex::new(std::collections::VecDeque::with_capacity(200)),
    }));
    // Best-effort: if another logger is already installed, ignore.
    let _ = log::set_logger(leaked);
    log::set_max_level(log::LevelFilter::Debug);
    let _ = MUTEX_CAPTURE_LOGGER.set(leaked);
    leaked
}

/// Connects to a Docker SMB fixture's `public` share at `127.0.0.1:port`
/// as guest. `mount_label` becomes the synthetic mount path
/// (`/Volumes/<label>`); no real OS mount is needed because the test
/// only drives the smb2 path.
async fn connect_docker_smb_volume(port: u16, mount_label: &str) -> SmbVolume {
    let mount_path = format!("/Volumes/{mount_label}");
    let volume_id = smb_volume_id("127.0.0.1", port, "public");
    let params = SmbConnectionParams::new("127.0.0.1", "public", port, None, None);
    connect_smb_volume("public", &mount_path, &volume_id, params)
        .await
        .unwrap_or_else(|e| panic!("connect to 127.0.0.1:{port} failed: {e:?}"))
}

/// One pass of the concurrent-streaming-write scenario:
/// - generate `n_files` source files of `file_size` bytes in a tempdir,
/// - pre-upload `n_conflicts` of them to the destination at the same size so `OverwriteSmaller`
///   resolves them as Skip,
/// - run `copy_volumes_with_progress` over all `n_files` with a timeout,
/// - on timeout, panic with the last 30 mutex/recv lines as a diagnostic dump,
/// - clean up the unique prefix directory either way.
async fn run_concurrent_write_pass(
    vol: Arc<SmbVolume>,
    mount_path: &Path,
    logger: &'static MutexCaptureLogger,
    n_files: usize,
    n_conflicts: usize,
    file_size: usize,
    timeout_secs: u64,
) -> Duration {
    use crate::file_system::write_operations::{
        CollectorEventSink, VolumeCopyConfig, WriteOperationState, copy_volumes_with_progress,
    };

    assert!(n_conflicts <= n_files);

    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let unique_prefix = format!("{TEST_PREFIX_ROOT}{ts}-n{n_files}");

    let dest_dir_abs = mount_path.join(unique_prefix.trim_start_matches('/'));
    let _ = vol.create_directory(&mount_path.join("_test")).await;
    vol.create_directory(&dest_dir_abs)
        .await
        .expect("create unique dest dir");

    let local_dir = tempfile::TempDir::new().expect("tempdir");
    for i in 0..n_files {
        let name = format!("f_{i:04}.bin");
        let path = local_dir.path().join(&name);
        // Distinct content per file (byte = i % 251) + an 8-byte seed
        // prefix, so identical-size pre-uploads still hash-differ from
        // their sources should we ever want to verify content.
        let mut buf = vec![0u8; file_size];
        buf[..8].copy_from_slice(&(i as u64).to_le_bytes());
        for b in buf.iter_mut().skip(8) {
            *b = (i % 251) as u8;
        }
        std::fs::write(&path, &buf).expect("write source");
    }

    log::info!(
        "regression: pre-uploading {} to {unique_prefix}",
        crate::pluralize::pluralize(n_conflicts as u64, "conflicting file")
    );
    for i in 0..n_conflicts {
        let name = format!("f_{i:04}.bin");
        let dest_abs = dest_dir_abs.join(&name);
        let buf = std::fs::read(local_dir.path().join(&name)).unwrap();
        let stream: Box<dyn VolumeReadStream> = Box::new(InlineReadStream::new(buf.clone()));
        let size = buf.len() as u64;
        let progress = |_a: u64, _b: u64| -> std::ops::ControlFlow<()> { std::ops::ControlFlow::Continue(()) };
        let bytes = vol
            .write_from_stream(&dest_abs, size, stream, &progress)
            .await
            .unwrap_or_else(|e| panic!("pre-upload {name} failed: {e:?}"));
        assert_eq!(bytes, size, "pre-upload size mismatch");
    }
    log::info!("regression: pre-upload done");

    let src_vol: Arc<dyn Volume> = Arc::new(crate::file_system::volume::LocalPosixVolume::new(
        "regression-src",
        local_dir.path().to_path_buf(),
    ));
    let dst_vol: Arc<dyn Volume> = vol.clone() as Arc<dyn Volume>;
    let source_rel_paths: Vec<PathBuf> = (0..n_files).map(|i| PathBuf::from(format!("f_{i:04}.bin"))).collect();

    let state = Arc::new(WriteOperationState::new(Duration::from_millis(200)));
    let events = Arc::new(CollectorEventSink::new());
    let config = VolumeCopyConfig {
        conflict_resolution: crate::file_system::write_operations::ConflictResolution::OverwriteSmaller,
        ..VolumeCopyConfig::default()
    };

    let start = std::time::Instant::now();
    log::info!(
        "regression: spawning copy n_files={n_files} n_conflicts={n_conflicts} size={file_size} timeout={timeout_secs}s"
    );

    let res = tokio::time::timeout(
        Duration::from_secs(timeout_secs),
        copy_volumes_with_progress(
            events.clone(),
            "regression-op",
            &state,
            Arc::clone(&src_vol),
            &source_rel_paths,
            Arc::clone(&dst_vol),
            &dest_dir_abs,
            &config,
        ),
    )
    .await;

    let elapsed = start.elapsed();

    let panic_msg: Option<String> = match res {
        Ok(Ok(())) => {
            log::info!("regression: copy completed in {elapsed:?}");
            None
        }
        Ok(Err(e)) => Some(format!("regression: copy failed in {elapsed:?}: {e:?}")),
        Err(_) => {
            let tail = |q: &std::sync::Mutex<std::collections::VecDeque<String>>| -> Vec<String> {
                let q = q.lock().unwrap();
                let n = q.len().min(30);
                q.iter().skip(q.len() - n).cloned().collect()
            };
            let mutex_dump = tail(&logger.mutex_lines);
            let recv_dump = tail(&logger.recv_lines);
            let last_ticket = CLIENT_LOCK_TICKET.load(Ordering::Relaxed);
            Some(format!(
                "regression: HANG after {:?} (timeout={}s) n_files={} n_conflicts={} last_ticket={}\n\
                 ── last {} client-mutex lines ──\n{}\n── last {} recv lines ──\n{}\n",
                elapsed,
                timeout_secs,
                n_files,
                n_conflicts,
                last_ticket,
                mutex_dump.len(),
                mutex_dump.join("\n"),
                recv_dump.len(),
                recv_dump.join("\n"),
            ))
        }
    };

    cleanup_test_prefix(&vol, mount_path, &unique_prefix).await;

    if let Some(m) = panic_msg {
        panic!("{m}");
    }
    elapsed
}

/// Guards the invariant that concurrent streaming writes through
/// `SmbVolume::write_from_stream` complete without deadlocking.
///
/// Uses the consumer-class `smb-consumer-maxreadsize` fixture
/// (`smb2 max read = smb2 max write = 65536`) so every 1 MB write exceeds
/// the server's max_write and is forced through the streaming-fallback
/// (FileWriter) path. That's the path that historically nested a
/// per-write lock under the client mutex and could starve the receiver
/// task to a halt.
///
/// Shape (200 files, 140 OverwriteSmaller conflicts + 60 actual copies,
/// concurrency=8) mirrors the production workload that originally
/// surfaced the bug, where mixed conflict-skip / write iterations on a
/// shared SmbClient stressed the lock-ordering pattern hardest.
///
/// Run with `./apps/desktop/test/smb-servers/start.sh core` (CI does
/// this) or `start.sh all`, then either `./scripts/check.sh --rust` or
/// `cargo nextest run -p cmdr smb_integration_concurrent_streaming_writes_no_deadlock
/// --run-ignored all`.
///
/// Originally hung at a QNAP NAS for >5 minutes before the fix in smb2
/// 0.9.0 (`FileWriter` owns its `Connection`) and the matching
/// `write_from_stream` rewrite. On post-fix code each pass completes in
/// roughly 5–15 s.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Requires docker-compose smb-consumer-maxreadsize on port 10494 (started by start.sh core)"]
async fn smb_integration_concurrent_streaming_writes_no_deadlock() {
    use futures_util::FutureExt;

    // 10494 matches smb2's smb-consumer-maxreadsize container; override
    // with `SMB_CONSUMER_MAXREADSIZE_PORT` to match
    // `smb2::testing::maxreadsize_port()` (requires the `smb-e2e`
    // feature; bare integration tests hardcode the default).
    let port: u16 = std::env::var("SMB_CONSUMER_MAXREADSIZE_PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10494);
    let logger = install_mutex_capture_logger();
    let prior_concurrency = crate::file_system::smb_concurrency();
    crate::file_system::set_smb_concurrency(8);

    let vol = Arc::new(connect_docker_smb_volume(port, "cmdr-regression-maxreadsize").await);
    let mount_path = vol.mount_path.clone();

    let result = std::panic::AssertUnwindSafe(run_concurrent_write_pass(
        Arc::clone(&vol),
        &mount_path,
        logger,
        /* n_files = */ 200,
        /* n_conflicts = */ 140,
        /* file_size = */ 1024 * 1024,
        /* timeout_secs = */ 120,
    ))
    .catch_unwind()
    .await;

    // Always restore concurrency, even on panic, before resuming the unwind.
    crate::file_system::set_smb_concurrency(prior_concurrency);
    if let Err(p) = result {
        std::panic::resume_unwind(p);
    }
}
