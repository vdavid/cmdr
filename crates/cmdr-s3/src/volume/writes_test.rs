//! Which request shape a write takes, and what a HEAD after it proves.

use cmdr_fs::volume::{StreamLength, WriteMode};

use super::{Landed, Landing, UploadShape, judge_landing, shape_for};
use crate::multipart::{MIN_PART_SIZE, ShortTail};

const MIB: u64 = 1024 * 1024;

#[test]
fn a_write_that_fits_one_part_is_a_single_put() {
    assert!(matches!(
        shape_for(StreamLength::Known(0), MIN_PART_SIZE, ShortTail::Fold),
        Ok(UploadShape::Single(0))
    ));
    assert!(matches!(
        shape_for(StreamLength::Known(10 * MIB), MIN_PART_SIZE, ShortTail::Fold),
        Ok(UploadShape::Single(n)) if n == 10 * MIB
    ));
    // 65 MiB is one 64 MiB part plus a 1 MiB tail that folds into it: one
    // part, so one PUT rather than three multipart requests.
    // Where the provider keeps a short tail as its own part, it's still one
    // PUT: the tail only matters once there are parts.
    for tail in [ShortTail::Fold, ShortTail::Keep] {
        assert!(matches!(
            shape_for(StreamLength::Known(65 * MIB), MIN_PART_SIZE, tail),
            Ok(UploadShape::Single(_))
        ));
    }
}

#[test]
fn a_write_past_one_part_goes_in_parts() {
    let Ok(UploadShape::Parts(plan)) = shape_for(StreamLength::Known(140 * MIB), MIN_PART_SIZE, ShortTail::Fold) else {
        panic!("140 MiB is three parts");
    };
    assert_eq!((plan.part_size, plan.part_count), (64 * MIB, 3));
    // A 2 MiB tail folds into the part before it.
    let Ok(UploadShape::Parts(plan)) = shape_for(StreamLength::Known(130 * MIB), MIN_PART_SIZE, ShortTail::Fold) else {
        panic!("130 MiB is two parts");
    };
    assert_eq!(plan.part_count, 2);
    // Kept as its own part where a larger last part is refused (R2).
    let Ok(UploadShape::Parts(plan)) = shape_for(StreamLength::Known(130 * MIB), MIN_PART_SIZE, ShortTail::Keep) else {
        panic!("130 MiB is three parts");
    };
    assert_eq!(plan.part_count, 3);
}

#[test]
fn a_write_of_unknown_length_is_open() {
    assert!(matches!(
        shape_for(StreamLength::Unknown, MIN_PART_SIZE, ShortTail::Fold),
        Ok(UploadShape::Open)
    ));
}

#[test]
fn a_write_too_big_for_ten_thousand_parts_is_refused_before_anything_is_sent() {
    assert!(
        shape_for(
            StreamLength::Known(49 * 1024 * 1024 * MIB),
            MIN_PART_SIZE,
            ShortTail::Fold
        )
        .is_err()
    );
}

fn landed(size: u64, etag: &str) -> Landed {
    Landed {
        size: Some(size),
        etag: Some(etag.to_string()),
    }
}

#[test]
fn our_own_object_at_the_promised_size_is_verified() {
    let found = landed(12, "\"9e107d9d372bb6826bd81d3542a419d6\"");
    for mode in [WriteMode::CreateNew, WriteMode::CreateOrReplace] {
        // A server may quote the ETag in one answer and not the other.
        assert_eq!(
            judge_landing(12, Some("9E107D9D372BB6826BD81D3542A419D6"), Some(&found), mode),
            Landing::Verified
        );
    }
}

/// ❗ The race a check-then-write can notice: another writer's object took the
/// name after ours landed. Their file survived and ours is gone, which is the
/// outcome a refused `CreateNew` reports.
#[test]
fn another_objects_etag_after_a_create_new_is_a_name_taken_after_us() {
    let found = landed(99, "\"theirs\"");
    assert_eq!(
        judge_landing(12, Some("\"ours\""), Some(&found), WriteMode::CreateNew),
        Landing::TakenAfterUs
    );
}

/// The user asked to replace; a newer write winning afterwards is what any
/// filesystem does, so it's noted, not failed.
#[test]
fn another_objects_etag_after_a_replace_is_a_newer_write() {
    let found = landed(99, "\"theirs\"");
    assert_eq!(
        judge_landing(12, Some("\"ours\""), Some(&found), WriteMode::CreateOrReplace),
        Landing::ReplacedAfterUs
    );
}

#[test]
fn nothing_at_the_name_after_a_write_is_vanished() {
    assert_eq!(
        judge_landing(12, Some("\"ours\""), None, WriteMode::CreateNew),
        Landing::Vanished
    );
}

#[test]
fn our_etag_at_the_wrong_size_is_wrong_size() {
    let found = landed(11, "\"ours\"");
    assert_eq!(
        judge_landing(12, Some("\"ours\""), Some(&found), WriteMode::CreateOrReplace),
        Landing::WrongSize { found: Some(11) }
    );
}

/// A server that sent no ETag on the write leaves the size as the only proof.
#[test]
fn without_our_etag_the_size_decides() {
    let found = landed(12, "\"anything\"");
    assert_eq!(
        judge_landing(12, None, Some(&found), WriteMode::CreateNew),
        Landing::Verified
    );
    let short = landed(3, "\"anything\"");
    assert_eq!(
        judge_landing(12, None, Some(&short), WriteMode::CreateNew),
        Landing::WrongSize { found: Some(3) }
    );
}
