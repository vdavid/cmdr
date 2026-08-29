//! The local move's conflict × resolution matrix, swept across both engines.
//!
//! One invariant, every cell: the source survives IFF it was NOT actually moved
//! to the destination. Each resolution is run twice, once through the same-FS
//! rename engine and once through the cross-FS staging engine, so a data-safety
//! rule can't hold on one path and quietly break on the other. Engine-specific
//! behavior (progress, flushing, staging internals) is in `move_op_tests.rs`.
//!
//! What each cell asserts:
//!
//! - Skip, or a conditional resolution that doesn't meet its condition: nothing
//!   landed, so the source survives and the destination is unchanged.
//! - An Overwrite that landed: the destination holds the source's content and
//!   the source is gone.
//! - A Rename that landed: the original destination AND the renamed incoming
//!   both sit at the destination, and the source is gone.
//!
//! Single-file and dir-with-one-skipped-child shapes both appear. Cells a
//! tempdir can't produce (two real filesystems for a genuine cross-FS move) are
//! simulated by calling `move_with_staging` directly, the same seam the
//! engine's own tests use. The volume (MTP / SMB) axis of this matrix is the
//! `volume/move_*_tests.rs` suites, `volume/move_tests.rs` first.

use super::test_support::{run_cross_fs_move, run_same_fs_move};
use super::*;
use crate::file_system::write_operations::types::ConflictResolution;

/// One single-file matrix cell: a source file collides with a same-named dest
/// file, resolved by `resolution`. Asserts the survives-iff-not-moved
/// invariant for the given expected outcome.
struct SingleFileCell {
    resolution: ConflictResolution,
    /// `None` => Skip-equivalent (source survives, dest unchanged).
    /// `Some(true)` => Overwrite-equivalent (dest := source content, source gone).
    /// `Some(false)` => Rename-equivalent (dest kept + renamed incoming, source gone).
    landed_as_overwrite: Option<bool>,
}

fn assert_single_file_cell(
    cross_fs: bool,
    cell: &SingleFileCell,
    src_bytes: &[u8],
    dst_bytes: &[u8],
    src_mtime_newer: bool,
) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let src_dir = tmp.path().join("src");
    let dst_dir = tmp.path().join("dst");
    fs::create_dir_all(&src_dir).unwrap();
    fs::create_dir_all(&dst_dir).unwrap();

    let src_file = src_dir.join("f.bin");
    fs::write(&src_file, src_bytes).unwrap();
    let dst_file = dst_dir.join("f.bin");
    fs::write(&dst_file, dst_bytes).unwrap();

    // Force a deterministic mtime ordering for the conditional cells.
    set_mtimes(&src_file, &dst_file, src_mtime_newer);

    let op_id = format!(
        "matrix-single-{}-{:?}",
        if cross_fs { "xfs" } else { "samefs" },
        cell.resolution
    );
    let result = if cross_fs {
        run_cross_fs_move(std::slice::from_ref(&src_file), &dst_dir, cell.resolution, &op_id)
    } else {
        run_same_fs_move(std::slice::from_ref(&src_file), &dst_dir, cell.resolution, &op_id)
    };
    assert!(result.is_ok(), "{:?} on {}: {:?}", cell.resolution, op_id, result.err());

    match cell.landed_as_overwrite {
        None => {
            // Skip-equivalent: source survives, dest unchanged.
            assert!(src_file.exists(), "{op_id}: Skip-equivalent must preserve the source");
            assert_eq!(
                fs::read(&src_file).unwrap(),
                src_bytes,
                "{op_id}: source content intact"
            );
            assert_eq!(
                fs::read(&dst_file).unwrap(),
                dst_bytes,
                "{op_id}: dest must be unchanged"
            );
        }
        Some(true) => {
            // Overwrite: source moved, dest now holds the source's content.
            assert!(
                !src_file.exists(),
                "{op_id}: Overwrite that landed must delete the source"
            );
            assert_eq!(
                fs::read(&dst_file).unwrap(),
                src_bytes,
                "{op_id}: dest := source content"
            );
        }
        Some(false) => {
            // Rename: source moved, original dest kept, renamed incoming present.
            assert!(!src_file.exists(), "{op_id}: Rename that landed must delete the source");
            assert_eq!(fs::read(&dst_file).unwrap(), dst_bytes, "{op_id}: original dest kept");
            let renamed = dst_dir.join("f (1).bin");
            assert!(renamed.exists(), "{op_id}: renamed incoming must exist");
            assert_eq!(
                fs::read(&renamed).unwrap(),
                src_bytes,
                "{op_id}: renamed incoming has source bytes"
            );
        }
    }
}

/// Sets mtimes so the strict-comparison conditional cells are deterministic.
fn set_mtimes(src: &Path, dst: &Path, src_newer: bool) {
    let older = filetime::FileTime::from_unix_time(1_600_000_000, 0);
    let newer = filetime::FileTime::from_unix_time(1_700_000_000, 0);
    if src_newer {
        filetime::set_file_mtime(src, newer).unwrap();
        filetime::set_file_mtime(dst, older).unwrap();
    } else {
        filetime::set_file_mtime(src, older).unwrap();
        filetime::set_file_mtime(dst, newer).unwrap();
    }
}

#[test]
fn matrix_single_file_skip_preserves_source_both_move_kinds() {
    let cell = SingleFileCell {
        resolution: ConflictResolution::Skip,
        landed_as_overwrite: None,
    };
    assert_single_file_cell(false, &cell, b"SRC", b"DST", true);
    assert_single_file_cell(true, &cell, b"SRC", b"DST", true);
}

#[test]
fn matrix_single_file_overwrite_replaces_dest_both_move_kinds() {
    let cell = SingleFileCell {
        resolution: ConflictResolution::Overwrite,
        landed_as_overwrite: Some(true),
    };
    assert_single_file_cell(false, &cell, b"SRC-NEW", b"DST-OLD", true);
    assert_single_file_cell(true, &cell, b"SRC-NEW", b"DST-OLD", true);
}

#[test]
fn matrix_single_file_rename_keeps_both_both_move_kinds() {
    let cell = SingleFileCell {
        resolution: ConflictResolution::Rename,
        landed_as_overwrite: Some(false),
    };
    assert_single_file_cell(false, &cell, b"SRC", b"DST", true);
    assert_single_file_cell(true, &cell, b"SRC", b"DST", true);
}

#[test]
fn matrix_single_file_overwrite_smaller_strict_semantics() {
    // dest strictly smaller than source => overwrite lands.
    let landed = SingleFileCell {
        resolution: ConflictResolution::OverwriteSmaller,
        landed_as_overwrite: Some(true),
    };
    assert_single_file_cell(false, &landed, b"source-is-bigger", b"dst", true);
    assert_single_file_cell(true, &landed, b"source-is-bigger", b"dst", true);

    // dest NOT strictly smaller (equal size) => Skip-equivalent, source survives.
    let skipped = SingleFileCell {
        resolution: ConflictResolution::OverwriteSmaller,
        landed_as_overwrite: None,
    };
    assert_single_file_cell(false, &skipped, b"AAAA", b"BBBB", true);
    assert_single_file_cell(true, &skipped, b"AAAA", b"BBBB", true);
}

#[test]
fn matrix_single_file_overwrite_older_strict_semantics() {
    // dest strictly older than source => overwrite lands (src_newer = true).
    let landed = SingleFileCell {
        resolution: ConflictResolution::OverwriteOlder,
        landed_as_overwrite: Some(true),
    };
    assert_single_file_cell(false, &landed, b"SRC", b"DST", true);
    assert_single_file_cell(true, &landed, b"SRC", b"DST", true);

    // dest newer than source => Skip-equivalent, source survives (src_newer = false).
    let skipped = SingleFileCell {
        resolution: ConflictResolution::OverwriteOlder,
        landed_as_overwrite: None,
    };
    assert_single_file_cell(false, &skipped, b"SRC", b"DST", false);
    assert_single_file_cell(true, &skipped, b"SRC", b"DST", false);
}

/// Directory-with-one-skipped-child cell for the conditional policies: the
/// colliding child does NOT meet the condition (Skip-equivalent), the other
/// child is new. The skipped child's source must survive; the new child moves.
/// This is the conditional sibling of `cross_fs_move_dir_merge_skip_child_*`.
fn assert_dir_one_skipped_child(cross_fs: bool, resolution: ConflictResolution, src_newer_for_collide: bool) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let src_root = tmp.path().join("src");
    let dst_root = tmp.path().join("dst");
    fs::create_dir_all(&src_root).unwrap();
    fs::create_dir_all(&dst_root).unwrap();

    let src_dir = src_root.join("d");
    fs::create_dir_all(&src_dir).unwrap();
    fs::write(src_dir.join("keep.bin"), b"new child").unwrap();
    fs::write(src_dir.join("collide.bin"), b"AAAA").unwrap();

    let dst_dir = dst_root.join("d");
    fs::create_dir_all(&dst_dir).unwrap();
    // Equal size keeps OverwriteSmaller Skip-equivalent; mtime drives OverwriteOlder.
    fs::write(dst_dir.join("collide.bin"), b"BBBB").unwrap();
    set_mtimes(
        &src_dir.join("collide.bin"),
        &dst_dir.join("collide.bin"),
        src_newer_for_collide,
    );

    let op_id = format!(
        "matrix-dir-{}-{:?}",
        if cross_fs { "xfs" } else { "samefs" },
        resolution
    );
    let result = if cross_fs {
        run_cross_fs_move(std::slice::from_ref(&src_dir), &dst_root, resolution, &op_id)
    } else {
        run_same_fs_move(std::slice::from_ref(&src_dir), &dst_root, resolution, &op_id)
    };
    assert!(result.is_ok(), "{op_id}: {:?}", result.err());

    // The non-colliding child moved.
    assert!(
        dst_dir.join("keep.bin").exists(),
        "{op_id}: new child should land at dest"
    );
    assert!(
        !src_dir.join("keep.bin").exists(),
        "{op_id}: new child should leave the source"
    );

    // The colliding child is Skip-equivalent: dest unchanged, source survives.
    assert_eq!(
        fs::read(dst_dir.join("collide.bin")).unwrap(),
        b"BBBB",
        "{op_id}: Skip-equivalent leaves the dest child unchanged"
    );
    assert!(
        src_dir.exists() && src_dir.join("collide.bin").exists(),
        "{op_id}: the skipped child's source must survive (data loss otherwise)"
    );
    assert_eq!(fs::read(src_dir.join("collide.bin")).unwrap(), b"AAAA");
}

#[test]
fn matrix_dir_overwrite_smaller_skips_equal_child_preserves_source() {
    // Equal-size collide child => not strictly smaller => Skip-equivalent.
    assert_dir_one_skipped_child(false, ConflictResolution::OverwriteSmaller, true);
    assert_dir_one_skipped_child(true, ConflictResolution::OverwriteSmaller, true);
}

#[test]
fn matrix_dir_overwrite_older_skips_newer_child_preserves_source() {
    // Collide child where source is NOT newer (dest newer) => not strictly
    // older => Skip-equivalent.
    assert_dir_one_skipped_child(false, ConflictResolution::OverwriteOlder, false);
    assert_dir_one_skipped_child(true, ConflictResolution::OverwriteOlder, false);
}
