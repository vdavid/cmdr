//! What a share does with FILES, against a real server: core CRUD
//! (create / read / rename / delete / no-clobber / mtime), single-chunk
//! streaming smoke tests, the copy and conflict scans, and space info.
//!
//! The session's own behavior (connection gates, reconnect, the scan pool,
//! supersede) is `session_integration_test.rs`; the byte path is
//! `read_stream_integration_test.rs`, `write_stream_integration_test.rs`, and
//! `wire_shape_integration_test.rs`.
//!
//! Every test here is `#[ignore]`d so default runs skip it. Start the containers
//! with `apps/desktop/test/smb-servers/start.sh`, then run
//! `cargo nextest run smb_integration --run-ignored all`.

use super::test_support::*;
use super::*;
use cmdr_fs::volume::WriteMode;
use cmdr_fs::volume::{InMemoryVolume, SpaceInfo, StreamLength};

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

/// Fills `dir` with `count` files whose long names spread the listing over
/// several server batches (a QUERY_DIRECTORY answer holds about 64 KiB of
/// entries, so 600 names of ~100 characters take three or more).
async fn seed_a_multi_batch_directory(vol: &SmbVolume, dir: &str, count: usize) {
    use futures_util::StreamExt;
    vol.create_directory(Path::new(dir)).await.unwrap();
    futures_util::stream::iter(0..count)
        .for_each_concurrent(32, |i| async move {
            let name = format!("{dir}/{i:04}-{}.txt", "a-long-name-that-fills-the-batch".repeat(3));
            vol.create_file(Path::new(&name), b"x").await.unwrap();
        })
        .await;
}

/// A big folder reports its progress as each server batch arrives, so the
/// pane's "Loaded N entries" line climbs while a slow share lists instead of
/// jumping from nothing to everything at the end.
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_a_listing_reports_progress_per_server_batch() {
    let vol = make_docker_volume().await;
    let dir = test_dir_name();
    seed_a_multi_batch_directory(&vol, &dir, 600).await;

    let seen = std::sync::Mutex::new(Vec::new());
    let on_progress = |p: cmdr_fs::volume::ListingProgress| seen.lock().unwrap().push(p.entries());
    let entries = vol
        .list_directory_with_cancel(Path::new(&dir), Some(&on_progress), None)
        .await
        .unwrap();

    assert_eq!(entries.len(), 600);
    let seen = seen.into_inner().unwrap();
    assert!(seen.len() >= 2, "one tick per batch, got {seen:?}");
    assert!(seen.windows(2).all(|w| w[0] < w[1]), "counts only climb: {seen:?}");
    assert_eq!(seen.last(), Some(&600), "the last tick is the whole folder");

    ensure_clean(&vol, &dir).await;
}

/// A cancelled listing stops between server batches and says so, rather than
/// reading the rest of a folder nobody is waiting for. The pane cancels when
/// the user navigates away; the share is free for the next listing right after.
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_a_cancelled_listing_stops_between_batches() {
    let vol = make_docker_volume().await;
    let dir = test_dir_name();
    seed_a_multi_batch_directory(&vol, &dir, 600).await;

    let cancel = tokio_util::sync::CancellationToken::new();
    let on_progress = |_: cmdr_fs::volume::ListingProgress| cancel.cancel();
    let result = vol
        .list_directory_with_cancel(Path::new(&dir), Some(&on_progress), Some(&cancel))
        .await;
    assert!(matches!(result, Err(VolumeError::Cancelled(_))), "got {result:?}");

    let again = vol.list_directory_impl(Path::new(&dir)).await.unwrap();
    assert_eq!(again.len(), 600, "the next listing of the folder is whole");

    ensure_clean(&vol, &dir).await;
}

/// `(requests_sent, compound_requests_sent)` on the volume's main connection.
async fn request_counts(vol: &SmbVolume) -> (u64, u64) {
    let d = vol.diagnostics().await.expect("a connected volume has diagnostics");
    (
        d.primary.metrics.requests_sent,
        d.primary.metrics.compound_requests_sent,
    )
}

/// A forced rename over a file is ONE compound frame (CREATE + SET_INFO with
/// `ReplaceIfExists` + CLOSE) on the wire: the server swaps the file in under the name, so
/// the destination never goes missing the way it does between a delete and a
/// rename. That gap is what kept `supports_atomic_replace_rename` off.
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_a_forced_rename_replaces_the_file_in_one_frame() {
    let vol = make_docker_volume().await;
    let dir = test_dir_name();
    ensure_clean(&vol, &dir).await;
    vol.create_directory(Path::new(&dir)).await.unwrap();
    let src = format!("{dir}/notes.txt.tmp");
    let dst = format!("{dir}/notes.txt");
    vol.create_file(Path::new(&src), b"new contents").await.unwrap();
    vol.create_file(Path::new(&dst), b"old contents").await.unwrap();

    let (requests_before, compounds_before) = request_counts(&vol).await;
    vol.rename(Path::new(&src), Path::new(&dst), true).await.unwrap();
    let (requests_after, compounds_after) = request_counts(&vol).await;

    // The rename is one CREATE + SET_INFO + CLOSE frame; the second frame is the
    // stat `notify_mutation` patches the listing with (CREATE + three
    // QUERY_INFOs + CLOSE, the third asking the reparse tag since smb2 0.34).
    // Before, a stat and a delete went out ahead of the rename.
    assert_eq!(
        (compounds_after - compounds_before, requests_after - requests_before),
        (2, 8),
        "the replacing rename plus the listing patch's stat, and no delete"
    );
    assert!(!vol.exists(Path::new(&src)).await);
    let landed = drain(vol.open_read_stream(Path::new(&dst)).await.unwrap()).await;
    assert_eq!(landed, b"new contents");

    vol.delete(Path::new(&dst)).await.unwrap();
    vol.delete(Path::new(&dir)).await.unwrap();
}

/// A forced rename that only changes case renames the file in place. Regression
/// anchor: the force path used to stat the destination, find the SOURCE under
/// its other spelling on a case-insensitive share, and delete it before the
/// rename, so the file was gone and the rename then failed.
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_a_forced_case_only_rename_keeps_the_file() {
    let vol = make_docker_volume().await;
    let dir = test_dir_name();
    ensure_clean(&vol, &dir).await;
    vol.create_directory(Path::new(&dir)).await.unwrap();
    let lower = format!("{dir}/report.txt");
    let upper = format!("{dir}/Report.txt");
    vol.create_file(Path::new(&lower), b"keep me").await.unwrap();

    vol.rename(Path::new(&lower), Path::new(&upper), true).await.unwrap();

    let names: Vec<String> = vol
        .list_directory_impl(Path::new(&dir))
        .await
        .unwrap()
        .into_iter()
        .map(|e| e.name)
        .collect();
    assert_eq!(names, vec!["Report.txt".to_string()]);
    let kept = drain(vol.open_read_stream(Path::new(&upper)).await.unwrap()).await;
    assert_eq!(kept, b"keep me");

    // The same for an (empty) directory, where a refusal would reach the
    // fallback that clears the destination: the source must survive it.
    let album = format!("{dir}/album");
    let album_upper = format!("{dir}/Album");
    vol.create_directory(Path::new(&album)).await.unwrap();
    vol.rename(Path::new(&album), Path::new(&album_upper), true)
        .await
        .unwrap();
    assert!(vol.is_directory(Path::new(&album_upper)).await.unwrap());

    vol.delete(Path::new(&album_upper)).await.unwrap();
    vol.delete(Path::new(&upper)).await.unwrap();
    vol.delete(Path::new(&dir)).await.unwrap();
}

/// Renaming a file to the same name in another case is what the pane does on
/// a case-only edit (without force, since nothing else is in the way). On a
/// case-insensitive share the destination "exists" (it's the source), and that
/// must not read as a taken name.
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_a_case_only_rename_isnt_refused_as_taken() {
    let vol = make_docker_volume().await;
    let dir = test_dir_name();
    ensure_clean(&vol, &dir).await;
    vol.create_directory(Path::new(&dir)).await.unwrap();
    let lower = format!("{dir}/report.txt");
    let upper = format!("{dir}/Report.txt");
    vol.create_file(Path::new(&lower), b"keep me").await.unwrap();

    vol.rename(Path::new(&lower), Path::new(&upper), false).await.unwrap();

    let names: Vec<String> = vol
        .list_directory_impl(Path::new(&dir))
        .await
        .unwrap()
        .into_iter()
        .map(|e| e.name)
        .collect();
    assert_eq!(names, vec!["Report.txt".to_string()]);

    vol.delete(Path::new(&upper)).await.unwrap();
    vol.delete(Path::new(&dir)).await.unwrap();
}

/// smb2's replacing rename refuses a directory at either end, so a forced
/// rename onto an empty directory still clears it first and lands, as it did
/// before the rename could replace in one step.
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_a_forced_rename_onto_an_empty_directory_still_lands() {
    let vol = make_docker_volume().await;
    let dir = test_dir_name();
    ensure_clean(&vol, &dir).await;
    vol.create_directory(Path::new(&dir)).await.unwrap();
    let src = format!("{dir}/photos-new");
    let dst = format!("{dir}/photos");
    vol.create_directory(Path::new(&src)).await.unwrap();
    vol.create_file(Path::new(&format!("{src}/a.jpg")), b"a").await.unwrap();
    vol.create_directory(Path::new(&dst)).await.unwrap();

    vol.rename(Path::new(&src), Path::new(&dst), true).await.unwrap();

    assert!(!vol.exists(Path::new(&src)).await);
    assert!(vol.exists(Path::new(&format!("{dst}/a.jpg"))).await);

    vol.delete(Path::new(&format!("{dst}/a.jpg"))).await.unwrap();
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
    assert_eq!(stream.total_size(), StreamLength::Known(content.len() as u64));
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
        .write_from_stream(Path::new(&smb_file), WriteMode::CreateOrReplace, size, stream, &|_| {
            std::ops::ControlFlow::Continue(())
        })
        .await
        .unwrap();
    assert_eq!(bytes.bytes, content.len() as u64);

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
    let SpaceInfo::Bounded {
        total_bytes,
        available_bytes,
        used_bytes,
    } = space
    else {
        panic!("a share always has a size, got {space:?}");
    };
    assert!(total_bytes > 0);
    assert!(available_bytes > 0);
    assert!(used_bytes <= total_bytes);
}

// ── A mount anchored inside the share ─────────────────────────────────────────
//
// macOS follows a DFS referral by mounting the target underneath the namespace
// root, and a subdirectory mount looks identical: the volume root is a DIRECTORY
// in the share. Everything below proves the anchor against a real server, because
// the failure mode it guards is a real request landing at a real, wrong place —
// which unit tests over a pure function can pin but not disprove. Reported as
// ERR-48RZX.

/// A mount anchored inside the share lists what's at its own root, not the
/// share's, and the paths it hands back are under its own mount.
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_anchored_mount_lists_its_own_root() {
    const ANCHORED_MOUNT: &str = "/tmp/smb-anchored-mount";

    let share = make_docker_volume().await;
    let dir = test_dir_name();
    ensure_clean(&share, &dir).await;
    share.create_directory(Path::new(&dir)).await.unwrap();
    share
        .create_directory(Path::new(&format!("{dir}/inside")))
        .await
        .unwrap();
    share
        .create_file(Path::new(&format!("{dir}/marker.txt")), b"anchored")
        .await
        .unwrap();

    let anchored = make_docker_volume_anchored(&dir, ANCHORED_MOUNT).await;

    let entries = anchored.list_directory(Path::new(ANCHORED_MOUNT), None).await.unwrap();
    let mut names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    names.sort_unstable();
    assert_eq!(
        names,
        ["inside", "marker.txt"],
        "the anchored mount lists the directory it is anchored at, not the whole share"
    );
    assert!(
        entries.iter().all(|e| e.path.starts_with(ANCHORED_MOUNT)),
        "every path it hands back is addressable under its own mount root: {entries:#?}"
    );

    let meta = anchored
        .get_metadata(Path::new(&format!("{ANCHORED_MOUNT}/marker.txt")))
        .await
        .unwrap();
    assert_eq!(meta.size, Some(8), "it stats the file inside the anchor");

    ensure_clean(&share, &dir).await;
}

/// The bytes an anchored mount writes land inside its anchor, which only the
/// share-rooted volume can confirm: a missing join would have created
/// `/marker.txt` at the top of the share instead, and the write would still have
/// reported success.
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_an_anchored_write_lands_inside_the_anchor() {
    const ANCHORED_MOUNT: &str = "/tmp/smb-anchored-write-mount";

    let share = make_docker_volume().await;
    let dir = test_dir_name();
    ensure_clean(&share, &dir).await;
    share.create_directory(Path::new(&dir)).await.unwrap();

    // Unique inside the anchor too, so the "not at the top of the share" checks
    // below can't be answered by some other test's leftovers on this fixture.
    let inner = format!("inner-{dir}");

    let anchored = make_docker_volume_anchored(&dir, ANCHORED_MOUNT).await;
    anchored
        .create_directory(Path::new(&format!("{ANCHORED_MOUNT}/{inner}")))
        .await
        .unwrap();
    anchored
        .create_file(
            Path::new(&format!("{ANCHORED_MOUNT}/{inner}/written.txt")),
            b"landed here",
        )
        .await
        .unwrap();

    // Read back through the SHARE-rooted volume, at the full share-relative path.
    let landed = share
        .open_read_stream(Path::new(&format!("{dir}/{inner}/written.txt")))
        .await
        .expect("the file is inside the anchor, where the anchored mount put it");
    assert_eq!(drain(landed).await, b"landed here", "and it holds what was written");

    assert!(
        !share.exists(Path::new(&inner)).await,
        "the anchor was applied, not dropped: nothing was created at the top of the share"
    );

    ensure_clean(&share, &dir).await;
}

/// Deleting through an anchored mount reaches the file inside the anchor. The
/// dangerous inverse of the write case: a dropped anchor would have aimed a
/// delete at a same-named path at the top of the share.
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_an_anchored_delete_stays_inside_the_anchor() {
    const ANCHORED_MOUNT: &str = "/tmp/smb-anchored-delete-mount";

    let share = make_docker_volume().await;
    let dir = test_dir_name();
    let decoy = format!("{dir}-decoy.txt");
    ensure_clean(&share, &dir).await;
    share.create_directory(Path::new(&dir)).await.unwrap();
    share
        .create_file(Path::new(&format!("{dir}/doomed.txt")), b"delete me")
        .await
        .unwrap();
    // A same-named file one level up is what a dropped anchor would hit instead.
    share.create_file(Path::new(&decoy), b"leave me alone").await.unwrap();

    let anchored = make_docker_volume_anchored(&dir, ANCHORED_MOUNT).await;
    anchored
        .delete(Path::new(&format!("{ANCHORED_MOUNT}/doomed.txt")))
        .await
        .unwrap();

    assert!(
        !share.exists(Path::new(&format!("{dir}/doomed.txt"))).await,
        "the file inside the anchor is gone"
    );
    assert!(
        share.exists(Path::new(&decoy)).await,
        "and nothing outside the anchor was touched"
    );

    let _ = share.delete(Path::new(&decoy)).await;
    ensure_clean(&share, &dir).await;
}

/// cmdr-reports#17: a delete succeeds while another open handle keeps the file
/// alive, and every new CREATE on that name answers `STATUS_DELETE_PENDING` until
/// the handle closes. A move's destination probe (`get_metadata`) hit that and
/// ended the whole move, for a status that clears on its own.
///
/// A handle released a moment later must read as "nothing there".
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_get_metadata_waits_out_a_brief_delete_pending() {
    let vol = make_docker_volume().await;
    let dir = test_dir_name();
    ensure_clean(&vol, &dir).await;
    vol.create_directory(Path::new(&dir)).await.unwrap();
    let file_path = format!("{dir}/pending.jpg");
    vol.create_file(Path::new(&file_path), b"photo").await.unwrap();

    // Someone else holds it open (sharing delete, as SMB clients do), so the
    // delete only marks it.
    let smb_path = vol.to_smb_path(Path::new(&file_path)).unwrap();
    let SessionLease { tree, mut conn, .. } = vol.clone_session().await.unwrap();
    let (file_id, _) = tree.open_file(&mut conn, &smb_path).await.unwrap();
    vol.delete(Path::new(&file_path)).await.unwrap();

    let release = tokio::spawn(async move {
        // allowed-test-sleep: the delay IS the subject, a handle released while the probe is retrying.
        tokio::time::sleep(Duration::from_millis(300)).await;
        tree.close_handle(&mut conn, file_id).await.unwrap();
    });

    let probed = vol.get_metadata(Path::new(&file_path)).await;
    release.await.unwrap();
    assert!(
        matches!(probed, Err(VolumeError::NotFound(_))),
        "a delete-pending that clears within the retry window reads as gone, got {probed:?}"
    );

    vol.delete(Path::new(&dir)).await.unwrap();
}

/// The other half: a handle that outlives the retries still surfaces as
/// `DeletePending`, so the user gets the transient message and a Retry, never a
/// hang.
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_get_metadata_reports_a_delete_pending_that_persists() {
    let vol = make_docker_volume().await;
    let dir = test_dir_name();
    ensure_clean(&vol, &dir).await;
    vol.create_directory(Path::new(&dir)).await.unwrap();
    let file_path = format!("{dir}/held.jpg");
    vol.create_file(Path::new(&file_path), b"photo").await.unwrap();

    let smb_path = vol.to_smb_path(Path::new(&file_path)).unwrap();
    let SessionLease { tree, mut conn, .. } = vol.clone_session().await.unwrap();
    let (file_id, _) = tree.open_file(&mut conn, &smb_path).await.unwrap();
    vol.delete(Path::new(&file_path)).await.unwrap();

    let probed = vol.get_metadata(Path::new(&file_path)).await;
    assert!(
        matches!(probed, Err(VolumeError::DeletePending(_))),
        "a handle that outlives the retries still reads as DeletePending, got {probed:?}"
    );

    tree.close_handle(&mut conn, file_id).await.unwrap();
    vol.delete(Path::new(&dir)).await.unwrap();
}
