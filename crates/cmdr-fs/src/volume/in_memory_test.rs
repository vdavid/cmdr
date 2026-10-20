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

/// The batch delete a move's source sweep uses, over the double whose batches
/// the engine's own cells count.
#[tokio::test]
async fn delete_files_honors_the_shared_batch_contract() {
    let volume = InMemoryVolume::new("Test");
    volume.create_directory(Path::new("/level")).await.unwrap();
    for name in ["a.txt", "b.txt", "kept.txt"] {
        volume
            .create_file(&Path::new("/level").join(name), b"bytes")
            .await
            .unwrap();
    }

    conformance::assert_delete_files_removes_exactly_what_it_names(
        &volume,
        [Path::new("/level/a.txt"), Path::new("/level/b.txt")],
        Path::new("/level/kept.txt"),
    )
    .await;
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

/// The double's node identity behaves like an inode: a rename carries it along, and a
/// new entry under a freed name gets its own. A rollback test leans on both to stand a
/// stranger in for a trashed folder.
#[tokio::test]
async fn node_identity_follows_the_entry_not_the_name() {
    let volume = InMemoryVolume::new("Test");
    volume.create_directory(Path::new("/Invoices")).await.unwrap();
    let original = volume.node_identity(Path::new("/Invoices")).await.unwrap();
    assert!(original.is_some());

    volume
        .rename(Path::new("/Invoices"), Path::new("/.Trash/Invoices"), false)
        .await
        .unwrap();
    assert_eq!(
        volume.node_identity(Path::new("/.Trash/Invoices")).await.unwrap(),
        original
    );

    volume.delete(Path::new("/.Trash/Invoices")).await.unwrap();
    volume.create_directory(Path::new("/.Trash/Invoices")).await.unwrap();
    assert_ne!(
        volume.node_identity(Path::new("/.Trash/Invoices")).await.unwrap(),
        original
    );
    assert!(matches!(
        volume.node_identity(Path::new("/Invoices")).await,
        Err(VolumeError::NotFound(_))
    ));
}

/// A confirmed replace never takes a folder that holds something, from a file
/// or from a folder.
#[tokio::test]
async fn forced_rename_honors_the_shared_non_empty_folder_contract() {
    let volume = InMemoryVolume::new("Test");
    volume.create_file(Path::new("/source.txt"), b"source").await.unwrap();
    volume.create_directory(Path::new("/source-dir")).await.unwrap();
    volume.create_directory(Path::new("/album")).await.unwrap();
    volume
        .create_file(Path::new("/album/source.txt"), b"the user's photo")
        .await
        .unwrap();

    for from in ["/source.txt", "/source-dir"] {
        conformance::assert_forced_rename_keeps_a_non_empty_dir(&volume, Path::new(from), Path::new("/album")).await;
    }
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
async fn write_from_stream_create_new_honors_the_shared_no_clobber_contract() {
    let volume = InMemoryVolume::new("Test");
    volume.create_file(Path::new("/notes.txt"), b"original").await.unwrap();

    conformance::assert_write_from_stream_create_new_refuses_to_clobber(
        &volume,
        Path::new("/notes.txt"),
        Path::new("/fresh.txt"),
        b"new",
    )
    .await;
}

/// The double keeps the source's date too, so an engine test copying onto it
/// sees what a real destination does.
#[tokio::test]
async fn a_copy_keeps_the_source_date_per_the_shared_contract() {
    let volume = InMemoryVolume::new("Test");

    conformance::assert_write_from_stream_keeps_the_source_date(
        &volume,
        Path::new("/dated.txt"),
        std::time::Duration::ZERO,
    )
    .await;
    conformance::assert_read_stream_reports_the_listed_date(&volume, Path::new("/dated.txt")).await;
}

/// And dates a folder, so an engine test can see a copied folder keep its date.
#[tokio::test]
async fn apply_metadata_honors_the_shared_folder_date_contract() {
    let volume = InMemoryVolume::new("Test");

    conformance::assert_apply_metadata_dates_a_folder(&volume, Path::new("/dated"), std::time::Duration::ZERO).await;
}

/// The reference keeps every class a listing shows, as a destination, on a file
/// and on a folder, and reports them as a source, as its caps claim.
#[tokio::test]
async fn the_double_honors_the_shared_metadata_contract() {
    let volume = InMemoryVolume::new("Test");

    conformance::assert_write_from_stream_keeps_metadata(&volume, Path::new("/born.txt")).await;
    conformance::assert_read_stream_reports_the_listed_metadata(&volume, Path::new("/born.txt")).await;
    conformance::assert_apply_metadata_keeps(&volume, Path::new("/born")).await;
}

/// The double stands in for a backend's caps exactly: configured to keep and
/// report only a creation date (an SMB share's), it still passes, so the
/// loss-note matrix can trust it to drop what such a backend drops.
#[tokio::test]
async fn a_double_configured_to_a_backends_caps_honors_them() {
    use crate::metadata::{MetadataCaps, MetadataClasses};
    let volume = InMemoryVolume::new("Test").with_metadata_caps(MetadataCaps {
        stores: MetadataClasses::CREATED,
        reports: MetadataClasses::CREATED,
        folder_stores: MetadataClasses::CREATED,
        folder_dates: true,
        staged_flags: crate::metadata::UserFlags::NONE,
    });

    conformance::assert_write_from_stream_keeps_metadata(&volume, Path::new("/born.txt")).await;
    conformance::assert_read_stream_reports_the_listed_metadata(&volume, Path::new("/born.txt")).await;
    conformance::assert_apply_metadata_keeps(&volume, Path::new("/born")).await;
    let carried = volume.carried_metadata(Path::new("/born.txt")).expect("landed");
    // The conformance source's mode is `0600`; a double that kept it would list it.
    assert_ne!(carried.mode, Some(0o600), "a class outside `stores` never lands");
}

/// The point of deriving the assertions from the caps: a backend claiming a
/// class it drops fails them.
#[tokio::test]
#[should_panic(expected = "must keep the source's creation date")]
async fn a_backend_claiming_a_class_it_drops_fails_the_contract() {
    use crate::metadata::MetadataCaps;
    // Honors nothing, the way an S3 bucket doesn't.
    let honors_nothing = InMemoryVolume::new("Liar").with_metadata_caps(MetadataCaps::NONE);
    let liar = ClaimsCreationDates(honors_nothing);

    conformance::assert_write_from_stream_keeps_metadata(&liar, Path::new("/born.txt")).await;
}

/// The flags and xattrs have no listing field, so the reference pins them here:
/// a write keeps the xattrs and leaves the flags for after the landing, and
/// `apply_metadata` adds the flags.
#[tokio::test]
async fn the_double_keeps_xattrs_on_the_write_and_flags_after_it() {
    use crate::metadata::{CarriedMetadata, MetadataClasses, UserFlags, Xattr};
    let source = InMemoryVolume::new("Source");
    source
        .create_file(Path::new("/tagged.txt"), b"tagged")
        .await
        .expect("seed");
    let tag = Xattr {
        name: "user.tag".into(),
        value: b"red".to_vec(),
    };
    source.set_carried_metadata(
        Path::new("/tagged.txt"),
        CarriedMetadata {
            flags: UserFlags::LOCKED,
            xattrs: vec![tag.clone()],
            ..CarriedMetadata::default()
        },
    );
    let dest = InMemoryVolume::new("Dest");
    let stream = source.open_read_stream(Path::new("/tagged.txt")).await.expect("open");
    let after_landing = stream
        .metadata(MetadataClasses::ALL)
        .after_landing(InMemoryVolume::REFERENCE_CAPS.staged_flags);
    dest.write_from_stream(
        Path::new("/tagged.txt"),
        WriteMode::CreateNew,
        StreamLength::Known(6),
        stream,
        &|_| std::ops::ControlFlow::Continue(()),
    )
    .await
    .expect("write");

    let landed = dest.carried_metadata(Path::new("/tagged.txt")).expect("landed");
    assert_eq!(landed.xattrs, vec![tag]);
    assert_eq!(landed.flags, UserFlags::NONE, "flags wait for the landing");

    dest.apply_metadata(Path::new("/tagged.txt"), &after_landing, None)
        .await;
    let landed = dest.carried_metadata(Path::new("/tagged.txt")).expect("landed");
    assert_eq!(landed.flags, UserFlags::LOCKED);
}

#[tokio::test]
async fn unknown_write_honors_the_shared_early_refusal_contract() {
    let volume = InMemoryVolume::new("Test");
    let original = b"the user's original bytes";
    volume.create_file(Path::new("/notes.txt"), original).await.unwrap();

    conformance::assert_unknown_write_is_refused_before_io(&volume, Path::new("/notes.txt"), original).await;
}

#[tokio::test]
async fn create_directory_all_honors_the_shared_honesty_contract() {
    let volume = InMemoryVolume::new("Test");
    volume.create_directory(Path::new("/album")).await.unwrap();

    conformance::assert_create_directory_all_reports_an_existing_dir_honestly(&volume, Path::new("/album")).await;
}

/// The shared file-in-the-way assertion. The double runs the trait's default
/// walk, which every backend without a walk of its own inherits, so this is the
/// cell that pins the default for all of them.
#[tokio::test]
async fn create_directory_all_honors_the_shared_file_in_the_way_contract() {
    let volume = InMemoryVolume::new("Test");
    volume.create_directory(Path::new("/album")).await.unwrap();
    volume
        .create_file(Path::new("/album/notes"), b"the user's notes")
        .await
        .unwrap();

    conformance::assert_create_directory_all_refuses_a_file_in_the_way(&volume, Path::new("/album/notes")).await;
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

/// The shared server-side-copy handshake: the double copies nothing on a
/// server, so it claims nothing and answers `NotSupported`.
#[tokio::test]
async fn copy_within_honors_the_shared_handshake_contract() {
    let volume = InMemoryVolume::new("Test");
    volume.create_file(Path::new("/original.txt"), b"bytes").await.unwrap();

    conformance::assert_copy_within_matches_what_is_offered(
        &volume,
        Path::new("/original.txt"),
        Path::new("/duplicate.txt"),
    )
    .await;
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

/// The shared stop assertion, over the double every other suite stands on.
///
/// ❗ Per SOURCE PATH here, ❌ not per entry, and that's honest: this backend's
/// scan is a prefix match over an in-memory map with no I/O in it, so a path is
/// already the smallest unit of waiting there is. A backend that walks a real
/// tree owes `assert_batch_scan_asks_inside_the_walk` as well.
#[tokio::test]
async fn a_batch_scan_stops_when_it_is_told_to() {
    let volume = InMemoryVolume::new("Test");
    volume.create_directory(Path::new("/album")).await.unwrap();
    volume
        .create_file(Path::new("/album/keep.txt"), b"content")
        .await
        .unwrap();

    conformance::assert_batch_scan_stops_when_told(&volume, Path::new("/album")).await;
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

/// A volume that keeps what `InMemoryVolume` keeps under its caps, and claims a
/// creation date on top: the shape of a backend whose caps drifted from its
/// code.
struct ClaimsCreationDates(InMemoryVolume);

impl Volume for ClaimsCreationDates {
    fn name(&self) -> &str {
        self.0.name()
    }
    fn root(&self) -> &Path {
        self.0.root()
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn list_directory<'a>(
        &'a self,
        path: &'a Path,
        on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        self.0.list_directory(path, on_progress)
    }
    fn get_metadata<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
        self.0.get_metadata(path)
    }
    fn exists<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        self.0.exists(path)
    }
    fn is_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<bool, VolumeError>> + Send + 'a>> {
        self.0.is_directory(path)
    }
    fn write_from_stream<'a>(
        &'a self,
        dest: &'a Path,
        mode: WriteMode,
        length: StreamLength,
        stream: Box<dyn VolumeReadStream>,
        on_progress: &'a (dyn Fn(StreamWriteProgress) -> std::ops::ControlFlow<()> + Sync),
    ) -> Pin<Box<dyn Future<Output = Result<StreamWritten, VolumeError>> + Send + 'a>> {
        self.0.write_from_stream(dest, mode, length, stream, on_progress)
    }
    fn metadata_caps(&self) -> crate::metadata::MetadataCaps {
        crate::metadata::MetadataCaps {
            stores: MetadataClasses::CREATED,
            ..self.0.metadata_caps()
        }
    }
}
