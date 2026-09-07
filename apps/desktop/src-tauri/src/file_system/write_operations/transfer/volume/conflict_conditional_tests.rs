//! Conditional-resolution tests for `conflict.rs`: `OverwriteSmaller` and
//! `OverwriteOlder` at the volume seam. A `#[path]` child of `conflict.rs`, so
//! `super::` is `conflict` and `super::super::` is `volume` — the same
//! one-level-shallower rule every `*_tests.rs` in this directory follows.

use super::*;
use crate::file_system::listing::FileEntry;
use crate::file_system::volume::InMemoryVolume;
use std::sync::Arc;

// ======================================================================
// Conditional resolution (OverwriteSmaller / OverwriteOlder)
// ======================================================================
//
// Same data-safety contract as the local-FS path: a destination is
// overwritten ONLY when strictly smaller / strictly older than the source.
// The volume side has two extra wrinkles the local side doesn't:
//   1. Size hints from the caller (preview scan) can short-circuit the `get_metadata` round-trip;
//      tests cover both hint-provided and hint-absent paths.
//   2. Volume backends may not surface `modified_at` (SMB servers vary). OverwriteOlder must Skip
//      rather than overwrite when mtime is unknown on either side.

/// Build an InMemoryVolume holding a single file at `path` with the given
/// `size` and `modified_at`. The volume's `get_metadata` will return
/// exactly these values, letting tests pin the comparison behavior
/// independent of clock drift.
fn volume_with_file(name: &str, path: &str, size: u64, modified_at: Option<u64>) -> Arc<InMemoryVolume> {
    let entry = FileEntry {
        size: Some(size),
        modified_at,
        created_at: modified_at,
        permissions: 0o644,
        owner: "testuser".to_string(),
        group: "staff".to_string(),
        extended_metadata_loaded: true,
        ..FileEntry::new(
            path.rsplit('/').next().unwrap_or(path).to_string(),
            path.to_string(),
            false,
            false,
        )
    };
    Arc::new(InMemoryVolume::with_entries(name, vec![entry]))
}

// ----- OverwriteSmaller -----

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn volume_smaller_overwrites_when_dest_strictly_smaller_via_hints() {
    let src: Arc<dyn Volume> = volume_with_file("src", "/f.bin", 1000, Some(100));
    let dst: Arc<dyn Volume> = volume_with_file("dst", "/f.bin", 500, Some(100));

    let resolved = reduce_volume_conditional_resolution(
        ConflictResolution::OverwriteSmaller,
        &src,
        Path::new("/f.bin"),
        &dst,
        Path::new("/f.bin"),
        Some(1000),
        Some(500),
    )
    .await;

    assert_eq!(resolved, ConflictResolution::Overwrite);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn volume_smaller_skips_when_dest_equal_size() {
    let src: Arc<dyn Volume> = volume_with_file("src", "/f.bin", 500, Some(100));
    let dst: Arc<dyn Volume> = volume_with_file("dst", "/f.bin", 500, Some(100));

    let resolved = reduce_volume_conditional_resolution(
        ConflictResolution::OverwriteSmaller,
        &src,
        Path::new("/f.bin"),
        &dst,
        Path::new("/f.bin"),
        Some(500),
        Some(500),
    )
    .await;

    assert_eq!(
        resolved,
        ConflictResolution::Skip,
        "Equal-size dst must NOT be overwritten on a volume any more than on local FS"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn volume_smaller_skips_when_dest_larger() {
    let src: Arc<dyn Volume> = volume_with_file("src", "/f.bin", 100, Some(100));
    let dst: Arc<dyn Volume> = volume_with_file("dst", "/f.bin", 9999, Some(100));

    let resolved = reduce_volume_conditional_resolution(
        ConflictResolution::OverwriteSmaller,
        &src,
        Path::new("/f.bin"),
        &dst,
        Path::new("/f.bin"),
        Some(100),
        Some(9999),
    )
    .await;

    assert_eq!(
        resolved,
        ConflictResolution::Skip,
        "Larger dst must NOT be overwritten — would clobber the user's keeper file"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn volume_smaller_falls_back_to_get_metadata_when_hints_missing() {
    // Critical: when the caller (move path, no scan phase) passes no hints,
    // the reducer must `get_metadata` from each volume rather than
    // defaulting to Skip on absent hints. Otherwise OverwriteSmaller would
    // never actually overwrite on moves.
    let src: Arc<dyn Volume> = volume_with_file("src", "/f.bin", 1000, Some(100));
    let dst: Arc<dyn Volume> = volume_with_file("dst", "/f.bin", 500, Some(100));

    let resolved = reduce_volume_conditional_resolution(
        ConflictResolution::OverwriteSmaller,
        &src,
        Path::new("/f.bin"),
        &dst,
        Path::new("/f.bin"),
        None,
        None,
    )
    .await;

    assert_eq!(
        resolved,
        ConflictResolution::Overwrite,
        "With no hints, the reducer should still get_metadata and compare correctly"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn volume_smaller_skips_when_dest_metadata_unavailable() {
    // Source is fine but dest get_metadata fails (path missing). Reducer
    // must Skip — we can't prove the destination is smaller, so we never
    // touch it.
    let src: Arc<dyn Volume> = volume_with_file("src", "/f.bin", 1000, Some(100));
    let dst: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("dst")); // empty

    let resolved = reduce_volume_conditional_resolution(
        ConflictResolution::OverwriteSmaller,
        &src,
        Path::new("/f.bin"),
        &dst,
        Path::new("/f.bin"),
        None,
        None,
    )
    .await;

    assert_eq!(resolved, ConflictResolution::Skip);
}

// ----- OverwriteOlder -----

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn volume_older_overwrites_when_dest_strictly_older() {
    let src: Arc<dyn Volume> = volume_with_file("src", "/f.bin", 100, Some(1_700_000_000));
    let dst: Arc<dyn Volume> = volume_with_file("dst", "/f.bin", 100, Some(1_600_000_000));

    let resolved = reduce_volume_conditional_resolution(
        ConflictResolution::OverwriteOlder,
        &src,
        Path::new("/f.bin"),
        &dst,
        Path::new("/f.bin"),
        None,
        None,
    )
    .await;

    assert_eq!(resolved, ConflictResolution::Overwrite);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn volume_older_skips_when_dest_equal_mtime() {
    let src: Arc<dyn Volume> = volume_with_file("src", "/f.bin", 100, Some(1_600_000_000));
    let dst: Arc<dyn Volume> = volume_with_file("dst", "/f.bin", 100, Some(1_600_000_000));

    let resolved = reduce_volume_conditional_resolution(
        ConflictResolution::OverwriteOlder,
        &src,
        Path::new("/f.bin"),
        &dst,
        Path::new("/f.bin"),
        None,
        None,
    )
    .await;

    assert_eq!(resolved, ConflictResolution::Skip);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn volume_older_skips_when_dest_strictly_newer() {
    let src: Arc<dyn Volume> = volume_with_file("src", "/f.bin", 100, Some(1_600_000_000));
    let dst: Arc<dyn Volume> = volume_with_file("dst", "/f.bin", 100, Some(1_700_000_000));

    let resolved = reduce_volume_conditional_resolution(
        ConflictResolution::OverwriteOlder,
        &src,
        Path::new("/f.bin"),
        &dst,
        Path::new("/f.bin"),
        None,
        None,
    )
    .await;

    assert_eq!(
        resolved,
        ConflictResolution::Skip,
        "Newer dst must NOT be overwritten — would clobber the user's fresher file"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn volume_older_skips_when_source_mtime_unknown() {
    // Many SMB servers don't surface modified_at reliably. The reducer
    // must fail closed to Skip rather than defaulting to overwrite.
    let src: Arc<dyn Volume> = volume_with_file("src", "/f.bin", 100, None);
    let dst: Arc<dyn Volume> = volume_with_file("dst", "/f.bin", 100, Some(1_600_000_000));

    let resolved = reduce_volume_conditional_resolution(
        ConflictResolution::OverwriteOlder,
        &src,
        Path::new("/f.bin"),
        &dst,
        Path::new("/f.bin"),
        None,
        None,
    )
    .await;

    assert_eq!(
        resolved,
        ConflictResolution::Skip,
        "Unknown source mtime must fail closed; we cannot prove dst is older"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn volume_older_skips_when_dest_mtime_unknown() {
    let src: Arc<dyn Volume> = volume_with_file("src", "/f.bin", 100, Some(1_700_000_000));
    let dst: Arc<dyn Volume> = volume_with_file("dst", "/f.bin", 100, None);

    let resolved = reduce_volume_conditional_resolution(
        ConflictResolution::OverwriteOlder,
        &src,
        Path::new("/f.bin"),
        &dst,
        Path::new("/f.bin"),
        None,
        None,
    )
    .await;

    assert_eq!(
        resolved,
        ConflictResolution::Skip,
        "Unknown dest mtime must fail closed; we cannot prove it's older"
    );
}

// ----- Pass-through -----

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn volume_non_conditional_variants_pass_through_unchanged() {
    let src: Arc<dyn Volume> = volume_with_file("src", "/f.bin", 100, Some(1_600_000_000));
    let dst: Arc<dyn Volume> = volume_with_file("dst", "/f.bin", 100, Some(1_600_000_000));

    for v in [
        ConflictResolution::Stop,
        ConflictResolution::Skip,
        ConflictResolution::Overwrite,
        ConflictResolution::Rename,
    ] {
        let resolved = reduce_volume_conditional_resolution(
            v,
            &src,
            Path::new("/f.bin"),
            &dst,
            Path::new("/f.bin"),
            Some(100),
            Some(100),
        )
        .await;
        assert_eq!(resolved, v, "Variant {v:?} must pass through unchanged");
    }
}

// ----- Axis independence -----

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn volume_smaller_ignores_mtime() {
    // Smaller AND newer dst: still overwrite under OverwriteSmaller.
    let src: Arc<dyn Volume> = volume_with_file("src", "/f.bin", 1000, Some(1_600_000_000));
    let dst: Arc<dyn Volume> = volume_with_file("dst", "/f.bin", 100, Some(1_700_000_000));

    let resolved = reduce_volume_conditional_resolution(
        ConflictResolution::OverwriteSmaller,
        &src,
        Path::new("/f.bin"),
        &dst,
        Path::new("/f.bin"),
        Some(1000),
        Some(100),
    )
    .await;

    assert_eq!(resolved, ConflictResolution::Overwrite);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn volume_older_ignores_size() {
    // Older AND larger dst: still overwrite under OverwriteOlder.
    let src: Arc<dyn Volume> = volume_with_file("src", "/f.bin", 100, Some(1_700_000_000));
    let dst: Arc<dyn Volume> = volume_with_file("dst", "/f.bin", 9999, Some(1_600_000_000));

    let resolved = reduce_volume_conditional_resolution(
        ConflictResolution::OverwriteOlder,
        &src,
        Path::new("/f.bin"),
        &dst,
        Path::new("/f.bin"),
        None,
        None,
    )
    .await;

    assert_eq!(resolved, ConflictResolution::Overwrite);
}
