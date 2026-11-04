//! Tests for the listing operations: cancel and the volume-backed start.

// ============================================================================
// Tests for streaming directory listing
// ============================================================================

#[test]
fn test_cancel_listing_sets_flag() {
    use super::testing::TestEngine;
    use std::sync::Arc;

    let engine = TestEngine::new();
    // Create a test listing ID and state
    let listing_id = "test-cancel-listing-12345";
    let state = engine.streaming_state();

    // Store it in the streaming state cache
    {
        let mut cache = engine.inner.streaming.write().unwrap();
        cache.insert(listing_id.to_string(), Arc::clone(&state));
    }

    // Verify the token starts un-cancelled
    assert!(!state.cancel.is_cancelled());

    // Call cancel_listing
    engine.inner.cancel_listing(listing_id);

    // Verify the token is now cancelled
    assert!(state.cancel.is_cancelled());
}

#[test]
fn test_cancel_listing_nonexistent_does_not_panic() {
    use super::testing::TestEngine;

    // Should not panic when listing doesn't exist
    TestEngine::new().inner.cancel_listing("nonexistent-listing-id");
}

#[test]
fn test_streaming_state_lifecycle() {
    use super::testing::TestEngine;
    use std::sync::Arc;

    let engine = TestEngine::new();
    // Create and register a streaming state
    let listing_id = "integration-test-lifecycle";
    let state = engine.streaming_state();

    // Insert into cache
    {
        let mut cache = engine.inner.streaming.write().unwrap();
        cache.insert(listing_id.to_string(), Arc::clone(&state));
    }

    // Verify it exists in cache
    {
        let cache = engine.inner.streaming.read().unwrap();
        assert!(cache.contains_key(listing_id));
    }

    // Cancel it
    engine.inner.cancel_listing(listing_id);
    assert!(state.cancel.is_cancelled());

    // Cleanup (simulate what the streaming task does)
    {
        let mut cache = engine.inner.streaming.write().unwrap();
        cache.remove(listing_id);
    }

    // Verify it's gone
    {
        let cache = engine.inner.streaming.read().unwrap();
        assert!(!cache.contains_key(listing_id));
    }
}

#[test]
fn test_multiple_concurrent_streaming_states() {
    use super::streaming::StreamingListingState;
    use super::testing::TestEngine;
    use std::sync::Arc;

    let engine = TestEngine::new();
    // Create multiple streaming states
    let ids = ["stream-1", "stream-2", "stream-3"];
    let states: Vec<Arc<StreamingListingState>> = ids.iter().map(|_| engine.streaming_state()).collect();

    // Insert all into cache
    {
        let mut cache = engine.inner.streaming.write().unwrap();
        for (id, state) in ids.iter().zip(states.iter()) {
            cache.insert(id.to_string(), Arc::clone(state));
        }
    }

    // Verify all exist
    {
        let cache = engine.inner.streaming.read().unwrap();
        assert_eq!(cache.len(), 3);
    }

    // Cancel only the second one
    engine.inner.cancel_listing("stream-2");

    // Verify only second is cancelled
    assert!(!states[0].cancel.is_cancelled());
    assert!(states[1].cancel.is_cancelled());
    assert!(!states[2].cancel.is_cancelled());
}

// ============================================================================
// Tests for list_directory_start_with_volume
// ============================================================================

/// Tests that `list_directory_start_with_volume` reads entries from an InMemoryVolume,
/// caches them in the listing cache, and returns the correct count.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_list_directory_start_with_volume_caches_entries() {
    use crate::testing::{TestEngine, TestListingGuard};
    use cmdr_fs::entry::FileEntry;
    use cmdr_fs::volume::InMemoryVolume;
    use cmdr_sort::{DirectorySortMode, SortColumn, SortOrder};
    use std::path::PathBuf;
    use std::sync::Arc;

    let volume_id = format!("test-vol-ldswv-{}", uuid::Uuid::new_v4());
    let dir_path = PathBuf::from("/testdir");

    // Create volume with three files
    let volume = Arc::new(InMemoryVolume::with_entries(
        "TestLDSWV",
        vec![
            FileEntry {
                size: Some(100),
                permissions: 0o644,
                owner: "test".to_string(),
                group: "staff".to_string(),
                ..FileEntry::new("p.txt".to_string(), "/testdir/p.txt".to_string(), false, false)
            },
            FileEntry {
                size: Some(200),
                permissions: 0o644,
                owner: "test".to_string(),
                group: "staff".to_string(),
                ..FileEntry::new("q.txt".to_string(), "/testdir/q.txt".to_string(), false, false)
            },
            FileEntry {
                size: Some(300),
                permissions: 0o644,
                owner: "test".to_string(),
                group: "staff".to_string(),
                ..FileEntry::new("r.txt".to_string(), "/testdir/r.txt".to_string(), false, false)
            },
        ],
    ));

    let engine = TestEngine::builder().volume(&volume_id, volume).build();

    let result = engine
        .inner
        .list_directory_start_with_volume(
            &volume_id,
            &dir_path,
            true,
            SortColumn::Name,
            SortOrder::Ascending,
            DirectorySortMode::LikeFiles,
        )
        .await;

    assert!(
        result.is_ok(),
        "list_directory_start_with_volume failed: {:?}",
        result.err()
    );
    let result = result.unwrap();
    assert_eq!(result.total_count, 3);

    let listing = TestListingGuard::adopt(&engine, &result.listing_id);

    let names = listing.entry_names();
    assert_eq!(names.len(), 3);
    assert!(names.iter().any(|n| n == "p.txt"));
    assert!(names.iter().any(|n| n == "q.txt"));
    assert!(names.iter().any(|n| n == "r.txt"));
    listing.with_listing(|cached| {
        assert_eq!(cached.volume_id, volume_id);
        assert_eq!(cached.path.as_path(), std::path::Path::new(&dir_path));
    });
}

/// Tests that `list_directory_start_with_volume` returns an error for a nonexistent volume.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_list_directory_start_with_volume_unknown_volume() {
    use crate::testing::TestEngine;
    use cmdr_sort::{DirectorySortMode, SortColumn, SortOrder};
    use std::path::PathBuf;

    let result = TestEngine::new()
        .inner
        .list_directory_start_with_volume(
            "nonexistent-volume-id",
            &PathBuf::from("/some/path"),
            true,
            SortColumn::Name,
            SortOrder::Ascending,
            DirectorySortMode::LikeFiles,
        )
        .await;

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not found"));
}
