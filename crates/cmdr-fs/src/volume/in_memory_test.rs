//! `InMemoryVolume`: construction, the CRUD surface, listing, rename, and
//! concurrent access. The scan surface is `in_memory_scan_test.rs`; streaming and
//! the test-only fault knobs are `in_memory_stream_test.rs`.

use super::*;
use crate::entry::FileEntry;
use std::path::Path;

#[tokio::test]
async fn test_new_creates_empty_volume() {
    let volume = InMemoryVolume::new("Test");
    assert_eq!(volume.name(), "Test");
    assert_eq!(volume.root(), Path::new("/"));

    let entries = volume.list_directory(Path::new(""), None).await.unwrap();
    assert!(entries.is_empty());
}

#[tokio::test]
async fn test_with_entries_populates_volume() {
    let entries = vec![
        FileEntry {
            size: Some(1024),
            modified_at: Some(1_640_000_000),
            created_at: Some(1_639_000_000),
            permissions: 0o644,
            owner: "testuser".to_string(),
            group: "staff".to_string(),
            extended_metadata_loaded: true,
            ..FileEntry::new("test.txt".to_string(), "/test.txt".to_string(), false, false)
        },
        FileEntry {
            modified_at: Some(1_640_000_000),
            created_at: Some(1_639_000_000),
            permissions: 0o755,
            owner: "testuser".to_string(),
            group: "staff".to_string(),
            extended_metadata_loaded: true,
            ..FileEntry::new("folder".to_string(), "/folder".to_string(), true, false)
        },
    ];

    let volume = InMemoryVolume::with_entries("Test", entries);
    let result = volume.list_directory(Path::new(""), None).await.unwrap();

    assert_eq!(result.len(), 2);
    // Directories should be first (sorted)
    assert_eq!(result[0].name, "folder");
    assert!(result[0].is_directory);
    assert_eq!(result[1].name, "test.txt");
    assert!(!result[1].is_directory);
}

#[tokio::test]
async fn test_with_file_count_creates_correct_number() {
    let volume = InMemoryVolume::with_file_count("Test", 100);
    let entries = volume.list_directory(Path::new(""), None).await.unwrap();

    assert_eq!(entries.len(), 100);
    assert!(entries[0].name.starts_with("file_"));
}

#[tokio::test]
async fn test_with_file_count_stress_test() {
    // Verify we can handle large file counts for stress testing
    let volume = InMemoryVolume::with_file_count("Test", 50_000);
    let entries = volume.list_directory(Path::new(""), None).await.unwrap();

    assert_eq!(entries.len(), 50_000);
}

#[tokio::test]
async fn test_exists_returns_true_for_existing() {
    let entries = vec![FileEntry {
        size: Some(100),
        permissions: 0o644,
        owner: "user".to_string(),
        group: "group".to_string(),
        extended_metadata_loaded: true,
        ..FileEntry::new("test.txt".to_string(), "/test.txt".to_string(), false, false)
    }];

    let volume = InMemoryVolume::with_entries("Test", entries);

    assert!(volume.exists(Path::new("/test.txt")).await);
    assert!(volume.exists(Path::new("test.txt")).await); // Relative path
}

#[tokio::test]
async fn test_exists_returns_false_for_nonexistent() {
    let volume = InMemoryVolume::new("Test");
    assert!(!volume.exists(Path::new("/nonexistent.txt")).await);
}

#[tokio::test]
async fn test_get_metadata_returns_correct_entry() {
    let entries = vec![FileEntry {
        size: Some(1024),
        modified_at: Some(1_640_000_000),
        permissions: 0o644,
        owner: "user".to_string(),
        group: "group".to_string(),
        extended_metadata_loaded: true,
        ..FileEntry::new("test.txt".to_string(), "/test.txt".to_string(), false, false)
    }];

    let volume = InMemoryVolume::with_entries("Test", entries);
    let result = volume.get_metadata(Path::new("/test.txt")).await.unwrap();

    assert_eq!(result.name, "test.txt");
    assert_eq!(result.size, Some(1024));
}

#[tokio::test]
async fn test_get_metadata_nonexistent_returns_error() {
    let volume = InMemoryVolume::new("Test");
    let result = volume.get_metadata(Path::new("/nonexistent.txt")).await;

    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), VolumeError::NotFound(_)));
}

#[tokio::test]
async fn test_create_file_then_exists() {
    let volume = InMemoryVolume::new("Test");

    volume
        .create_file(Path::new("/test.txt"), b"Hello, World!")
        .await
        .unwrap();

    assert!(volume.exists(Path::new("/test.txt")).await);

    let metadata = volume.get_metadata(Path::new("/test.txt")).await.unwrap();
    assert_eq!(metadata.name, "test.txt");
    assert_eq!(metadata.size, Some(13)); // "Hello, World!" is 13 bytes
    assert!(!metadata.is_directory);
}

#[tokio::test]
async fn test_create_file_does_not_clobber_existing() {
    // Regression for the high-severity audit finding: `create_file` is a
    // no-overwrite contract. The InMemoryVolume must reject collisions so
    // tests that stand in for real backends don't mask the bug.
    let volume = InMemoryVolume::new("Test");

    volume
        .create_file(Path::new("/notes.txt"), b"important user data")
        .await
        .unwrap();

    let result = volume.create_file(Path::new("/notes.txt"), b"").await;

    assert!(
        matches!(result, Err(VolumeError::AlreadyExists(_))),
        "expected AlreadyExists, got {:?}",
        result
    );
    let metadata = volume.get_metadata(Path::new("/notes.txt")).await.unwrap();
    assert_eq!(metadata.size, Some(19), "original file bytes must survive");
}

#[tokio::test]
async fn test_create_directory_then_exists() {
    let volume = InMemoryVolume::new("Test");

    volume.create_directory(Path::new("/mydir")).await.unwrap();

    assert!(volume.exists(Path::new("/mydir")).await);

    let metadata = volume.get_metadata(Path::new("/mydir")).await.unwrap();
    assert_eq!(metadata.name, "mydir");
    assert!(metadata.is_directory);
}

#[tokio::test]
async fn test_delete_removes_entry() {
    let volume = InMemoryVolume::new("Test");

    volume.create_file(Path::new("/test.txt"), b"content").await.unwrap();
    assert!(volume.exists(Path::new("/test.txt")).await);

    volume.delete(Path::new("/test.txt")).await.unwrap();
    assert!(!volume.exists(Path::new("/test.txt")).await);
}

/// The shared `Volume::delete` non-recursion assertion, over the test double
/// that every other suite's fixtures stand on. If `InMemoryVolume` ever stops
/// honoring the contract, hundreds of tests keep passing while the thing they
/// were proving quietly stops being true — so it runs the same assertion the
/// real backends do.
#[tokio::test]
async fn delete_honors_the_shared_non_recursion_contract() {
    let volume = InMemoryVolume::new("Test");
    volume.create_directory(Path::new("/album")).await.unwrap();
    volume
        .create_file(Path::new("/album/keep.txt"), b"content")
        .await
        .unwrap();

    conformance::assert_delete_leaves_a_non_empty_dir_intact(&volume, Path::new("/album"), "keep.txt").await;
}

/// The shared no-clobber assertions, over the double every other suite's
/// fixtures stand on. Same reasoning as the delete one above: if the double
/// stops honoring a contract, hundreds of tests keep passing while the thing
/// they were proving quietly stops being true.
#[tokio::test]
async fn rename_honors_the_shared_no_clobber_contract() {
    let volume = InMemoryVolume::new("Test");
    volume.create_file(Path::new("/source.txt"), b"source").await.unwrap();
    volume
        .create_file(Path::new("/target.txt"), b"the user's target file")
        .await
        .unwrap();

    conformance::assert_rename_refuses_an_existing_destination(
        &volume,
        Path::new("/source.txt"),
        Path::new("/target.txt"),
    )
    .await;
}

#[tokio::test]
async fn create_file_honors_the_shared_no_clobber_contract() {
    let volume = InMemoryVolume::new("Test");
    volume
        .create_file(Path::new("/notes.txt"), b"the user's notes")
        .await
        .unwrap();

    conformance::assert_create_file_refuses_to_clobber(&volume, Path::new("/notes.txt"), b"new").await;
}

#[tokio::test]
async fn create_directory_all_honors_the_shared_honesty_contract() {
    let volume = InMemoryVolume::new("Test");
    volume.create_directory(Path::new("/album")).await.unwrap();

    conformance::assert_create_directory_all_reports_an_existing_dir_honestly(&volume, Path::new("/album")).await;
}

/// The shared export-handshake assertion, over the double every other suite's
/// fixtures stand on: it streams bytes, so it must claim export.
#[tokio::test]
async fn export_honors_the_shared_handshake_contract() {
    let volume = InMemoryVolume::new("Test");
    let content = b"the bytes a copy would move";
    volume.create_file(Path::new("/exported.txt"), content).await.unwrap();

    conformance::assert_export_matches_the_bytes_offered(&volume, Path::new("/exported.txt"), content).await;
}

/// The shared `NotFound`-payload assertion. The double is the oracle every other
/// backend's fixtures are compared against, so it owes the payload too.
#[tokio::test]
async fn not_found_honors_the_shared_path_payload_contract() {
    let volume = InMemoryVolume::new("Test");

    conformance::assert_not_found_carries_the_path(&volume, Path::new("/no-such-file.txt")).await;
}

/// The shared conflict-scan assertion. The double lists a directory that isn't
/// there as an empty one, so it keeps this contract by construction; pinning it
/// is what stops a future `NotFound` on that path silently teaching every
/// fixture in the suite the wrong answer.
#[tokio::test]
async fn conflict_scan_honors_the_shared_missing_destination_contract() {
    let volume = InMemoryVolume::new("Test");

    conformance::assert_conflict_scan_reads_a_missing_destination_as_empty(&volume, Path::new("/not-created-yet"))
        .await;
}

#[tokio::test]
async fn test_delete_nonexistent_returns_error() {
    let volume = InMemoryVolume::new("Test");

    let result = volume.delete(Path::new("/nonexistent.txt")).await;
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), VolumeError::NotFound(_)));
}

#[tokio::test]
async fn test_list_directory_sorts_correctly() {
    let entries = vec![
        FileEntry {
            size: Some(100),
            permissions: 0o644,
            owner: "user".to_string(),
            group: "group".to_string(),
            extended_metadata_loaded: true,
            ..FileEntry::new("zebra.txt".to_string(), "/zebra.txt".to_string(), false, false)
        },
        FileEntry {
            permissions: 0o755,
            owner: "user".to_string(),
            group: "group".to_string(),
            extended_metadata_loaded: true,
            ..FileEntry::new("alpha".to_string(), "/alpha".to_string(), true, false)
        },
        FileEntry {
            size: Some(50),
            permissions: 0o644,
            owner: "user".to_string(),
            group: "group".to_string(),
            extended_metadata_loaded: true,
            ..FileEntry::new("apple.txt".to_string(), "/apple.txt".to_string(), false, false)
        },
        FileEntry {
            permissions: 0o755,
            owner: "user".to_string(),
            group: "group".to_string(),
            extended_metadata_loaded: true,
            ..FileEntry::new("beta".to_string(), "/beta".to_string(), true, false)
        },
    ];

    let volume = InMemoryVolume::with_entries("Test", entries);
    let result = volume.list_directory(Path::new(""), None).await.unwrap();

    // Expected order: directories first (alpha, beta), then files (apple.txt, zebra.txt)
    assert_eq!(result[0].name, "alpha");
    assert!(result[0].is_directory);
    assert_eq!(result[1].name, "beta");
    assert!(result[1].is_directory);
    assert_eq!(result[2].name, "apple.txt");
    assert!(!result[2].is_directory);
    assert_eq!(result[3].name, "zebra.txt");
    assert!(!result[3].is_directory);
}

#[tokio::test]
async fn test_list_subdirectory() {
    let entries = vec![
        FileEntry {
            permissions: 0o755,
            owner: "user".to_string(),
            group: "group".to_string(),
            extended_metadata_loaded: true,
            ..FileEntry::new("subdir".to_string(), "/subdir".to_string(), true, false)
        },
        FileEntry {
            size: Some(100),
            permissions: 0o644,
            owner: "user".to_string(),
            group: "group".to_string(),
            extended_metadata_loaded: true,
            ..FileEntry::new(
                "file_in_subdir.txt".to_string(),
                "/subdir/file_in_subdir.txt".to_string(),
                false,
                false,
            )
        },
        FileEntry {
            size: Some(50),
            permissions: 0o644,
            owner: "user".to_string(),
            group: "group".to_string(),
            extended_metadata_loaded: true,
            ..FileEntry::new("root_file.txt".to_string(), "/root_file.txt".to_string(), false, false)
        },
    ];

    let volume = InMemoryVolume::with_entries("Test", entries);

    // List root - should only show subdir and root_file.txt
    let root_entries = volume.list_directory(Path::new(""), None).await.unwrap();
    assert_eq!(root_entries.len(), 2);

    // List subdir - should only show file_in_subdir.txt
    let subdir_entries = volume.list_directory(Path::new("/subdir"), None).await.unwrap();
    assert_eq!(subdir_entries.len(), 1);
    assert_eq!(subdir_entries[0].name, "file_in_subdir.txt");
}

#[test]
fn test_can_watch_listings_returns_false() {
    let volume = InMemoryVolume::new("Test");
    assert!(!volume.can_watch_listings());
}

// ============================================================================
// Rename tests
// ============================================================================

#[tokio::test]
async fn test_rename_success() {
    let volume = InMemoryVolume::new("Test");
    volume.create_file(Path::new("/old.txt"), b"content").await.unwrap();

    let result = volume.rename(Path::new("/old.txt"), Path::new("/new.txt"), false).await;
    assert!(result.is_ok());
    assert!(!volume.exists(Path::new("/old.txt")).await);
    assert!(volume.exists(Path::new("/new.txt")).await);

    let metadata = volume.get_metadata(Path::new("/new.txt")).await.unwrap();
    assert_eq!(metadata.name, "new.txt");
    assert_eq!(metadata.path, "/new.txt");
}

#[tokio::test]
async fn test_rename_conflict_no_force() {
    let volume = InMemoryVolume::new("Test");
    volume.create_file(Path::new("/source.txt"), b"source").await.unwrap();
    volume.create_file(Path::new("/target.txt"), b"target").await.unwrap();

    let result = volume
        .rename(Path::new("/source.txt"), Path::new("/target.txt"), false)
        .await;
    assert!(matches!(result, Err(VolumeError::AlreadyExists(_))));
    // Both entries still exist
    assert!(volume.exists(Path::new("/source.txt")).await);
    assert!(volume.exists(Path::new("/target.txt")).await);
}

#[tokio::test]
async fn test_rename_force_overwrites() {
    let volume = InMemoryVolume::new("Test");
    volume.create_file(Path::new("/source.txt"), b"new").await.unwrap();
    volume.create_file(Path::new("/target.txt"), b"old").await.unwrap();

    let result = volume
        .rename(Path::new("/source.txt"), Path::new("/target.txt"), true)
        .await;
    assert!(result.is_ok());
    assert!(!volume.exists(Path::new("/source.txt")).await);
    assert!(volume.exists(Path::new("/target.txt")).await);

    let metadata = volume.get_metadata(Path::new("/target.txt")).await.unwrap();
    assert_eq!(metadata.name, "target.txt");
}

#[tokio::test]
async fn test_rename_nonexistent_source() {
    let volume = InMemoryVolume::new("Test");
    let result = volume
        .rename(Path::new("/missing.txt"), Path::new("/new.txt"), false)
        .await;
    assert!(matches!(result, Err(VolumeError::NotFound(_))));
}

// ============================================================================
// Concurrency tests
// ============================================================================

#[tokio::test]
async fn test_concurrent_reads() {
    use std::sync::Arc;

    // 10 concurrent tasks × 25 iterations each = 250 interleaved reads. The
    // earlier shape (100 iters, ~1 M entry materialisations across threads)
    // passed in ~2 s in isolation but timed out at 8 s under `check.sh`'s
    // parallel-check load — per `.config/nextest.toml` we trim the workload
    // rather than bump the cap. Concurrency races surface at much smaller
    // scale than this; 250 interleavings is plenty of pressure on the
    // `RwLock<HashMap>`.
    let volume = Arc::new(InMemoryVolume::with_file_count("Test", 1000));
    let mut handles = vec![];

    for _ in 0..10 {
        let vol = Arc::clone(&volume);
        handles.push(tokio::spawn(async move {
            for _ in 0..25 {
                let _ = vol.list_directory(Path::new(""), None).await;
                let _ = vol.exists(Path::new("/file_000001.txt")).await;
                let _ = vol.get_metadata(Path::new("/file_000010.txt")).await;
            }
        }));
    }

    for handle in handles {
        handle.await.unwrap();
    }

    // Volume should still be intact
    assert_eq!(volume.list_directory(Path::new(""), None).await.unwrap().len(), 1000);
}

#[tokio::test]
async fn test_concurrent_writes() {
    use std::sync::Arc;

    let volume = Arc::new(InMemoryVolume::new("Test"));
    let mut handles = vec![];

    // Spawn 10 tasks each creating 10 files
    for i in 0..10 {
        let vol = Arc::clone(&volume);
        handles.push(tokio::spawn(async move {
            for j in 0..10 {
                let path = format!("/file_{}_{}.txt", i, j);
                vol.create_file(Path::new(&path), b"content").await.unwrap();
            }
        }));
    }

    for handle in handles {
        handle.await.unwrap();
    }

    // Should have all 100 files
    let entries = volume.list_directory(Path::new(""), None).await.unwrap();
    assert_eq!(entries.len(), 100);
}

#[tokio::test]
async fn test_concurrent_create_delete() {
    use std::sync::Arc;

    let volume = Arc::new(InMemoryVolume::new("Test"));
    // Create a permanent file
    volume.create_file(Path::new("/permanent.txt"), b"keep").await.unwrap();

    let mut handles = vec![];

    // Readers
    for _ in 0..5 {
        let vol = Arc::clone(&volume);
        handles.push(tokio::spawn(async move {
            for _ in 0..50 {
                let _ = vol.list_directory(Path::new(""), None).await;
                let _ = vol.exists(Path::new("/permanent.txt")).await;
                tokio::task::yield_now().await;
            }
        }));
    }

    // Writers: create and delete temporary files
    for i in 0..5 {
        let vol = Arc::clone(&volume);
        handles.push(tokio::spawn(async move {
            for j in 0..10 {
                let path = format!("/temp_{}_{}.txt", i, j);
                let p = Path::new(&path);
                vol.create_file(p, b"temp").await.unwrap();
                tokio::task::yield_now().await;
                let _ = vol.delete(p).await; // May fail if another task already deleted
            }
        }));
    }

    for handle in handles {
        handle.await.unwrap();
    }

    // Permanent file should still exist
    assert!(volume.exists(Path::new("/permanent.txt")).await);
}
