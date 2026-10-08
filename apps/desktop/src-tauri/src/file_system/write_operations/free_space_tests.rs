//! The pre-copy free-space check (`free_space.rs`): which free-space figure it
//! asks for, what a copy onto existing files can need, and when it refuses.

use std::cell::Cell;
use std::fs;
use std::path::{Path, PathBuf};

use super::free_space::{SpaceNeed, available_space_from, check_copy_space, local_copy_need};
use super::scan_cache::FileInfo;
use super::types::{ConflictResolution, SpaceShortfall, WriteOperationError};
use crate::test_support::TestDir;

// ============================================================================
// Which figure: statvfs first, the slow "important usage" figure only when short
// ============================================================================

#[test]
fn a_fast_figure_that_fits_never_asks_the_slow_one() {
    let slow_asked = Cell::new(false);
    let available = available_space_from(
        100,
        || Some(500),
        || {
            slow_asked.set(true);
            Some(900)
        },
    );
    assert_eq!(available, Some(500));
    assert!(
        !slow_asked.get(),
        "statvfs said it fits, so the slow figure must not be asked"
    );
}

#[test]
fn a_fast_figure_that_falls_short_asks_the_slow_one_which_counts_purgeable_space() {
    assert_eq!(available_space_from(1_000, || Some(500), || Some(2_000)), Some(2_000));
}

#[test]
fn the_larger_figure_wins_when_both_fall_short() {
    // Free blocks are free whatever the important-usage figure says.
    assert_eq!(available_space_from(1_000, || Some(500), || Some(300)), Some(500));
}

#[test]
fn a_silent_fast_figure_falls_back_to_the_slow_one() {
    assert_eq!(available_space_from(1_000, || None, || Some(2_000)), Some(2_000));
}

#[test]
fn a_short_fast_figure_stands_when_the_slow_one_is_silent() {
    assert_eq!(available_space_from(1_000, || Some(500), || None), Some(500));
}

#[test]
fn no_figure_at_all_is_none() {
    assert_eq!(available_space_from(1_000, || None, || None), None);
}

#[cfg(unix)]
#[test]
fn a_real_folder_reports_some_free_space() {
    let dir = TestDir::new("free_space_real");
    let available = super::free_space::available_space(&dir, 1);
    assert!(available.is_some_and(|bytes| bytes > 0), "got {available:?}");
}

// ============================================================================
// What a copy can need
// ============================================================================

#[test]
fn a_file_with_nothing_in_the_way_counts_in_full() {
    let mut need = SpaceNeed::default();
    need.add_new(700);
    assert_eq!(need.bytes(), 700);
}

#[test]
fn an_overwrite_counts_only_the_growth_plus_room_to_stage_the_largest_replacement() {
    let mut need = SpaceNeed::default();
    // 1,000 over 900: grows by 100, and stages 1,000 beside the 900 while landing.
    need.add_clash(1_000, 900, ConflictResolution::Overwrite);
    // 300 over 500: shrinks, so adds nothing beyond its 300 staged bytes.
    need.add_clash(300, 500, ConflictResolution::Overwrite);
    assert_eq!(need.bytes(), 100 + 900);
}

#[test]
fn the_conditional_overwrites_count_like_an_overwrite() {
    for policy in [ConflictResolution::OverwriteSmaller, ConflictResolution::OverwriteOlder] {
        let mut need = SpaceNeed::default();
        need.add_clash(1_000, 900, policy);
        need.add_clash(1_000, 1_000, policy);
        assert_eq!(need.bytes(), 100 + 1_000, "{policy:?}");
    }
}

#[test]
fn a_skip_adds_nothing() {
    let mut need = SpaceNeed::default();
    need.add_clash(1_000, 10, ConflictResolution::Skip);
    assert_eq!(need.bytes(), 0);
}

#[test]
fn rename_and_stop_count_in_full_because_both_copies_can_stay() {
    for policy in [ConflictResolution::Rename, ConflictResolution::Stop] {
        let mut need = SpaceNeed::default();
        need.add_clash(1_000, 900, policy);
        assert_eq!(need.bytes(), 1_000, "{policy:?}");
    }
}

/// A source tree under `<root>/src/docs` and its scan entries, as the copy's
/// scan hands them over (each `source_root` is the parent of the top-level
/// source).
fn scanned(root: &Path, files: &[(&str, usize)]) -> (PathBuf, Vec<FileInfo>) {
    let source = root.join("src").join("docs");
    let mut infos = Vec::new();
    for (name, size) in files {
        let path = source.join(name);
        fs::create_dir_all(path.parent().expect("has a parent")).expect("mkdir");
        fs::write(&path, vec![b'x'; *size]).expect("write source");
        let metadata = fs::metadata(&path).expect("stat source");
        infos.push(FileInfo::new(path, root.join("src"), &metadata));
    }
    (source, infos)
}

#[test]
fn a_resync_onto_a_folder_that_holds_most_of_it_needs_only_what_changed() {
    let dir = TestDir::new("free_space_resync");
    let (source, files) = scanned(&dir, &[("a.bin", 4_000), ("sub/b.bin", 3_000), ("new.bin", 500)]);
    let dest = dir.join("dest");
    fs::create_dir_all(dest.join("docs/sub")).expect("mkdir dest");
    fs::write(dest.join("docs/a.bin"), vec![b'y'; 4_000]).expect("write dest a");
    fs::write(dest.join("docs/sub/b.bin"), vec![b'y'; 2_000]).expect("write dest b");

    let need = local_copy_need(&dest, &[source], &files, ConflictResolution::OverwriteSmaller, || false)
        .expect("not cancelled");

    // new.bin in full, b.bin's 1,000 growth, and 4,000 to stage a.bin.
    assert_eq!(need.bytes(), 500 + 1_000 + 4_000);
}

#[test]
fn a_folder_in_the_way_of_a_file_is_not_a_clash() {
    let dir = TestDir::new("free_space_dir_in_way");
    let (source, files) = scanned(&dir, &[("a.bin", 1_000)]);
    let dest = dir.join("dest");
    fs::create_dir_all(dest.join("docs/a.bin")).expect("mkdir folder at the file's name");

    let need =
        local_copy_need(&dest, &[source], &files, ConflictResolution::Overwrite, || false).expect("not cancelled");
    assert_eq!(need.bytes(), 1_000);
}

#[test]
fn a_duplicate_in_place_counts_in_full_rather_than_as_a_clash_with_itself() {
    let dir = TestDir::new("free_space_duplicate");
    let (source, files) = scanned(&dir, &[("a.bin", 1_000)]);
    // Copying `src/docs` into `src` duplicates it as `docs (1)`.
    let need = local_copy_need(&dir.join("src"), &[source], &files, ConflictResolution::Skip, || false)
        .expect("not cancelled");
    assert_eq!(need.bytes(), 1_000);
}

#[test]
fn a_cancel_during_the_probe_answers_nothing() {
    let dir = TestDir::new("free_space_cancel");
    let (source, files) = scanned(&dir, &[("a.bin", 10)]);
    assert!(local_copy_need(&dir.join("dest"), &[source], &files, ConflictResolution::Skip, || true).is_none());
}

// ============================================================================
// The verdict
// ============================================================================

#[test]
fn a_copy_that_fits_never_refines() {
    let result = check_copy_space(Path::new("/tmp"), 100, Some(200), SpaceShortfall::Refuse, || {
        panic!("refine must not run when the full size fits")
    });
    assert!(result.is_ok());
}

#[test]
fn a_shortfall_refuses_with_the_refined_figure() {
    let result = check_copy_space(
        Path::new("/Volumes/Backup/docs"),
        1_000,
        Some(100),
        SpaceShortfall::Refuse,
        || Ok(Some(600)),
    );
    assert!(
        matches!(
            &result,
            Err(WriteOperationError::InsufficientSpace { required: 600, available: 100, volume_name: Some(name) })
                if name == "Backup"
        ),
        "got {result:?}"
    );
}

#[test]
fn a_shortfall_that_files_already_there_cover_goes_ahead() {
    let result = check_copy_space(Path::new("/tmp"), 1_000, Some(700), SpaceShortfall::Refuse, || {
        Ok(Some(600))
    });
    assert!(result.is_ok(), "got {result:?}");
}

#[test]
fn a_shortfall_with_no_refined_figure_refuses_with_the_full_size() {
    let result = check_copy_space(Path::new("/tmp"), 1_000, Some(100), SpaceShortfall::Refuse, || Ok(None));
    assert!(
        matches!(
            result,
            Err(WriteOperationError::InsufficientSpace {
                required: 1_000,
                available: 100,
                ..
            })
        ),
        "got {result:?}"
    );
}

#[test]
fn copy_anyway_goes_ahead_without_measuring() {
    let result = check_copy_space(Path::new("/tmp"), u64::MAX, Some(0), SpaceShortfall::Proceed, || {
        panic!("a copy the person chose to run anyway is not measured")
    });
    assert!(result.is_ok());
}

#[test]
fn a_destination_that_cant_tell_goes_ahead() {
    let result = check_copy_space(Path::new("/tmp"), u64::MAX, None, SpaceShortfall::Refuse, || Ok(None));
    assert!(result.is_ok());
}

#[cfg(unix)]
#[test]
fn a_real_folder_refuses_an_impossible_copy() {
    let dir = TestDir::new("free_space_impossible");
    let available = super::free_space::available_space(&dir, u64::MAX);
    let result = check_copy_space(&dir, u64::MAX, available, SpaceShortfall::Refuse, || Ok(None));
    assert!(
        matches!(result, Err(WriteOperationError::InsufficientSpace { .. })),
        "got {result:?}"
    );
}
