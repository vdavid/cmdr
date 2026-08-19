//! Part of `conflict.rs`, split out as a `#[path]` child so the module itself
//! stays readable. `super::` here is `conflict`, exactly as when these lived
//! inline.
//!
//! Tests for `reduce_conditional_resolution` — the gate that decides whether
//! the user's "Overwrite all smaller" / "Overwrite all older" choice
//! actually overwrites this particular file, or skips it.
//!
//! **Data-safety contract pinned here**: a destination is overwritten ONLY
//! when strictly smaller (by byte count) or strictly older (by mtime) than
//! the source. Equal-or-bigger / equal-or-newer / unknown metadata always
//! reduces to `Skip`. Bugs in this function would silently overwrite files
//! the user expected to keep, so the tests below are exhaustive across the
//! comparison axes.
use super::*;
use std::time::{Duration, SystemTime};
use tempfile::TempDir;
use uuid::Uuid;

fn temp_with_size_and_mtime(dir: &Path, name: &str, size: usize, mtime_secs: i64) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, vec![0u8; size]).unwrap();
    let mtime = filetime::FileTime::from_unix_time(mtime_secs, 0);
    filetime::set_file_mtime(&path, mtime).unwrap();
    path
}

/// Snapshot the metadata once. `reduce_conditional_resolution` borrows
/// `&Metadata`, so the helper has to return owned values the caller
/// stores in locals.
fn meta(path: &Path) -> fs::Metadata {
    fs::metadata(path).unwrap()
}

fn unique_dir() -> TempDir {
    // Per-test unique dir keeps parallel runs from racing on identical
    // names. Default TempDir is already unique but the explicit prefix
    // makes failing-test debug output easier to interpret.
    tempfile::Builder::new()
        .prefix(&format!("cmdr-cond-resolve-{}", Uuid::new_v4()))
        .tempdir()
        .unwrap()
}

// ------------------------------------------------------------------
// OverwriteSmaller — strict by byte count
// ------------------------------------------------------------------

#[test]
fn smaller_overwrites_only_when_dest_strictly_smaller() {
    // Pin: dst (smaller) < src → Overwrite. Any other relation → Skip.
    let dir = unique_dir();
    let src = temp_with_size_and_mtime(dir.path(), "src", 1000, 1_600_000_000);
    let dst = temp_with_size_and_mtime(dir.path(), "dst", 500, 1_600_000_000);
    let src_m = meta(&src);
    let dst_m = meta(&dst);

    let resolved = reduce_conditional_resolution(ConflictResolution::OverwriteSmaller, Some(&src_m), Some(&dst_m));
    assert_eq!(
        resolved,
        ConflictResolution::Overwrite,
        "Strictly smaller dst must be overwritten under OverwriteSmaller"
    );
}

#[test]
fn smaller_skips_when_dest_equal_size() {
    // Pin: dst.len == src.len → Skip. Critical to data safety: a same-size
    // dst is NOT obviously stale; user picked "smaller" meaning STRICTLY.
    let dir = unique_dir();
    let src = temp_with_size_and_mtime(dir.path(), "src", 1000, 1_600_000_000);
    let dst = temp_with_size_and_mtime(dir.path(), "dst", 1000, 1_600_000_000);
    let src_m = meta(&src);
    let dst_m = meta(&dst);

    let resolved = reduce_conditional_resolution(ConflictResolution::OverwriteSmaller, Some(&src_m), Some(&dst_m));
    assert_eq!(
        resolved,
        ConflictResolution::Skip,
        "Equal-size dst must NOT be overwritten under OverwriteSmaller"
    );
}

#[test]
fn smaller_skips_when_dest_strictly_larger() {
    let dir = unique_dir();
    let src = temp_with_size_and_mtime(dir.path(), "src", 500, 1_600_000_000);
    let dst = temp_with_size_and_mtime(dir.path(), "dst", 1000, 1_600_000_000);
    let src_m = meta(&src);
    let dst_m = meta(&dst);

    let resolved = reduce_conditional_resolution(ConflictResolution::OverwriteSmaller, Some(&src_m), Some(&dst_m));
    assert_eq!(
        resolved,
        ConflictResolution::Skip,
        "Larger dst must NOT be overwritten under OverwriteSmaller — would corrupt the user's keeper file"
    );
}

#[test]
fn smaller_skips_on_zero_byte_dest_when_source_also_zero() {
    // Edge: 0 bytes vs 0 bytes is NOT strictly smaller; must skip.
    let dir = unique_dir();
    let src = temp_with_size_and_mtime(dir.path(), "src", 0, 1_600_000_000);
    let dst = temp_with_size_and_mtime(dir.path(), "dst", 0, 1_600_000_000);
    let src_m = meta(&src);
    let dst_m = meta(&dst);

    let resolved = reduce_conditional_resolution(ConflictResolution::OverwriteSmaller, Some(&src_m), Some(&dst_m));
    assert_eq!(resolved, ConflictResolution::Skip);
}

#[test]
fn smaller_skips_when_source_metadata_missing() {
    // Source meta missing means we can't prove dst is smaller; fail closed (Skip).
    let dir = unique_dir();
    let dst = temp_with_size_and_mtime(dir.path(), "dst", 500, 1_600_000_000);
    let dst_m = meta(&dst);

    let resolved = reduce_conditional_resolution(ConflictResolution::OverwriteSmaller, None, Some(&dst_m));
    assert_eq!(
        resolved,
        ConflictResolution::Skip,
        "Unknown source size must fail closed to Skip; we cannot prove dst is smaller"
    );
}

#[test]
fn smaller_skips_when_dest_metadata_missing() {
    // Dest meta missing: the dest exists (we're in conflict resolution
    // because of that) but we can't size it — skip rather than overwrite blindly.
    let dir = unique_dir();
    let src = temp_with_size_and_mtime(dir.path(), "src", 1000, 1_600_000_000);
    let src_m = meta(&src);

    let resolved = reduce_conditional_resolution(ConflictResolution::OverwriteSmaller, Some(&src_m), None);
    assert_eq!(resolved, ConflictResolution::Skip);
}

#[test]
fn smaller_skips_when_both_metadata_missing() {
    let resolved = reduce_conditional_resolution(ConflictResolution::OverwriteSmaller, None, None);
    assert_eq!(resolved, ConflictResolution::Skip);
}

// ------------------------------------------------------------------
// OverwriteOlder — strict by mtime
// ------------------------------------------------------------------

#[test]
fn older_overwrites_only_when_dest_strictly_older() {
    // Pin: dst mtime < src mtime → Overwrite. The user wants stale files replaced.
    let dir = unique_dir();
    // 2020 dst, 2024 src
    let src = temp_with_size_and_mtime(dir.path(), "src", 500, 1_700_000_000);
    let dst = temp_with_size_and_mtime(dir.path(), "dst", 500, 1_600_000_000);
    let src_m = meta(&src);
    let dst_m = meta(&dst);

    let resolved = reduce_conditional_resolution(ConflictResolution::OverwriteOlder, Some(&src_m), Some(&dst_m));
    assert_eq!(resolved, ConflictResolution::Overwrite);
}

#[test]
fn older_skips_when_dest_equal_mtime() {
    // Equal mtime is not strictly older — skip.
    let dir = unique_dir();
    let src = temp_with_size_and_mtime(dir.path(), "src", 500, 1_600_000_000);
    let dst = temp_with_size_and_mtime(dir.path(), "dst", 500, 1_600_000_000);
    let src_m = meta(&src);
    let dst_m = meta(&dst);

    let resolved = reduce_conditional_resolution(ConflictResolution::OverwriteOlder, Some(&src_m), Some(&dst_m));
    assert_eq!(
        resolved,
        ConflictResolution::Skip,
        "Equal-mtime dst must NOT be overwritten under OverwriteOlder"
    );
}

#[test]
fn older_skips_when_dest_strictly_newer() {
    let dir = unique_dir();
    let src = temp_with_size_and_mtime(dir.path(), "src", 500, 1_600_000_000);
    let dst = temp_with_size_and_mtime(dir.path(), "dst", 500, 1_700_000_000);
    let src_m = meta(&src);
    let dst_m = meta(&dst);

    let resolved = reduce_conditional_resolution(ConflictResolution::OverwriteOlder, Some(&src_m), Some(&dst_m));
    assert_eq!(
        resolved,
        ConflictResolution::Skip,
        "Newer dst must NOT be overwritten under OverwriteOlder — would clobber the user's fresher file"
    );
}

#[test]
fn older_one_second_difference_still_counts_as_older() {
    // Subsecond paranoia: cross the second boundary so the comparison is unambiguous.
    let dir = unique_dir();
    let src = temp_with_size_and_mtime(dir.path(), "src", 100, 1_600_000_001);
    let dst = temp_with_size_and_mtime(dir.path(), "dst", 100, 1_600_000_000);
    let src_m = meta(&src);
    let dst_m = meta(&dst);

    let resolved = reduce_conditional_resolution(ConflictResolution::OverwriteOlder, Some(&src_m), Some(&dst_m));
    assert_eq!(resolved, ConflictResolution::Overwrite);
}

#[test]
fn older_skips_when_metadata_missing() {
    // Both sides missing: skip. Source missing: skip. Dest missing: skip.
    assert_eq!(
        reduce_conditional_resolution(ConflictResolution::OverwriteOlder, None, None),
        ConflictResolution::Skip
    );
    let dir = unique_dir();
    let some = temp_with_size_and_mtime(dir.path(), "f", 100, 1_600_000_000);
    let some_m = meta(&some);
    assert_eq!(
        reduce_conditional_resolution(ConflictResolution::OverwriteOlder, None, Some(&some_m)),
        ConflictResolution::Skip,
        "Missing source mtime → fail closed to Skip"
    );
    assert_eq!(
        reduce_conditional_resolution(ConflictResolution::OverwriteOlder, Some(&some_m), None),
        ConflictResolution::Skip,
        "Missing dest mtime → fail closed to Skip"
    );
}

// ------------------------------------------------------------------
// Pass-through variants
// ------------------------------------------------------------------

#[test]
fn non_conditional_variants_pass_through_unchanged() {
    // The reduction must be a no-op for the four pre-existing variants;
    // callers depend on this when they pass `response.resolution` blindly.
    let dir = unique_dir();
    let src = temp_with_size_and_mtime(dir.path(), "src", 500, 1_600_000_000);
    let dst = temp_with_size_and_mtime(dir.path(), "dst", 500, 1_600_000_000);
    let src_m = meta(&src);
    let dst_m = meta(&dst);

    for v in [
        ConflictResolution::Stop,
        ConflictResolution::Skip,
        ConflictResolution::Overwrite,
        ConflictResolution::Rename,
    ] {
        assert_eq!(
            reduce_conditional_resolution(v, Some(&src_m), Some(&dst_m)),
            v,
            "Variant {v:?} must pass through unchanged"
        );
    }
}

// ------------------------------------------------------------------
// Independence of axes
// ------------------------------------------------------------------

#[test]
fn smaller_ignores_mtime() {
    // A dst that's smaller AND newer must still be overwritten under
    // OverwriteSmaller — the user opted in to size-only comparison.
    let dir = unique_dir();
    let src = temp_with_size_and_mtime(dir.path(), "src", 1000, 1_600_000_000);
    let dst = temp_with_size_and_mtime(dir.path(), "dst", 100, 1_700_000_000); // newer + smaller
    let src_m = meta(&src);
    let dst_m = meta(&dst);

    assert_eq!(
        reduce_conditional_resolution(ConflictResolution::OverwriteSmaller, Some(&src_m), Some(&dst_m)),
        ConflictResolution::Overwrite
    );
}

#[test]
fn older_ignores_size() {
    // A dst that's older AND larger must still be overwritten under
    // OverwriteOlder. (Common case: a stub file replaced by a fuller version
    // — the user wants the newer file regardless of size.)
    let dir = unique_dir();
    let src = temp_with_size_and_mtime(dir.path(), "src", 100, 1_700_000_000);
    let dst = temp_with_size_and_mtime(dir.path(), "dst", 5000, 1_600_000_000); // older + larger
    let src_m = meta(&src);
    let dst_m = meta(&dst);

    assert_eq!(
        reduce_conditional_resolution(ConflictResolution::OverwriteOlder, Some(&src_m), Some(&dst_m)),
        ConflictResolution::Overwrite
    );
}

// ------------------------------------------------------------------
// Realistic mtime span via SystemTime (in case filetime crate ever drifts).
// ------------------------------------------------------------------

#[test]
fn older_works_with_systemtime_offsets_from_now() {
    // Belt-and-suspenders: don't depend on hardcoded epoch values.
    // Use "now" and "now - 1 hour" to prove the comparison works for
    // realistic timestamps the OS would produce live.
    let dir = unique_dir();
    let src = dir.path().join("src");
    let dst = dir.path().join("dst");
    fs::write(&src, b"src").unwrap();
    fs::write(&dst, b"dst").unwrap();

    let now = SystemTime::now();
    let an_hour_ago = now - Duration::from_secs(3600);
    filetime::set_file_mtime(&src, filetime::FileTime::from_system_time(now)).unwrap();
    filetime::set_file_mtime(&dst, filetime::FileTime::from_system_time(an_hour_ago)).unwrap();

    let src_m = meta(&src);
    let dst_m = meta(&dst);
    assert_eq!(
        reduce_conditional_resolution(ConflictResolution::OverwriteOlder, Some(&src_m), Some(&dst_m)),
        ConflictResolution::Overwrite,
        "dst from an hour ago must be older than src from now"
    );
    // And the inverse: dst newer than src → Skip.
    filetime::set_file_mtime(&src, filetime::FileTime::from_system_time(an_hour_ago)).unwrap();
    filetime::set_file_mtime(&dst, filetime::FileTime::from_system_time(now)).unwrap();
    let src_m = meta(&src);
    let dst_m = meta(&dst);
    assert_eq!(
        reduce_conditional_resolution(ConflictResolution::OverwriteOlder, Some(&src_m), Some(&dst_m)),
        ConflictResolution::Skip,
        "dst from now must NOT be overwritten by src from an hour ago"
    );
}
