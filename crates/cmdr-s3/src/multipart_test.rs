//! The part plan at its edges.

use super::{
    MAX_PART_SIZE, MAX_PARTS, MIB, MIN_PART_SIZE, PartPlan, ShortTail, TooLarge, plan_parts, plan_parts_with_floor,
};

const GIB: u64 = 1024 * MIB;
const TIB: u64 = 1024 * GIB;

#[test]
fn small_uploads_use_the_minimum_part_size() {
    assert_eq!(
        plan_parts(100 * MIB).unwrap(),
        PartPlan {
            part_size: MIN_PART_SIZE,
            part_count: 2,
            total: 100 * MIB
        }
    );
    assert_eq!(plan_parts(MIN_PART_SIZE).unwrap().part_count, 1);
    assert_eq!(plan_parts(MIN_PART_SIZE + 5 * MIB).unwrap().part_count, 2);
    assert_eq!(plan_parts(1).unwrap().part_count, 1);
}

#[test]
fn an_empty_upload_is_one_empty_part() {
    let plan = plan_parts(0).unwrap();
    assert_eq!(plan.part_count, 1);
    assert_eq!(plan.part_size, MIN_PART_SIZE);
}

#[test]
fn the_minimum_holds_up_to_625_gib() {
    // 10,000 × 64 MiB = 625 GiB.
    let plan = plan_parts(625 * GIB).unwrap();
    assert_eq!(plan.part_size, MIN_PART_SIZE);
    assert_eq!(u64::from(plan.part_count), MAX_PARTS);
}

#[test]
fn past_that_parts_grow_to_whole_mebibytes_and_never_exceed_10000() {
    let plan = plan_parts(625 * GIB + 1).unwrap();
    assert_eq!(plan.part_size, 65 * MIB);
    assert!(u64::from(plan.part_count) <= MAX_PARTS);

    for total in [TIB, 3 * TIB + 12_345, 10 * TIB, 48 * TIB] {
        let plan = plan_parts(total).unwrap();
        assert_eq!(plan.part_size % MIB, 0, "{total}");
        assert!(u64::from(plan.part_count) <= MAX_PARTS, "{total}");
        assert!(plan.part_size * u64::from(plan.part_count) >= total, "{total}");
        assert!(plan.part_size <= MAX_PART_SIZE, "{total}");
    }
}

#[test]
fn beyond_10000_parts_of_5_gib_is_too_large() {
    assert!(plan_parts(MAX_PARTS * MAX_PART_SIZE).is_ok());
    assert_eq!(plan_parts(MAX_PARTS * MAX_PART_SIZE + 1), Err(TooLarge));
}

#[test]
fn ranges_tile_the_object_with_a_short_last_part() {
    let plan = plan_parts(150 * MIB).unwrap();
    assert_eq!(plan.part_count, 3);
    assert_eq!(plan.range(1), (0, 64 * MIB - 1));
    assert_eq!(plan.range(2), (64 * MIB, 128 * MIB - 1));
    assert_eq!(plan.range(3), (128 * MIB, 150 * MIB - 1));
}

#[test]
fn a_tail_under_5_mib_folds_into_the_part_before_it() {
    // Garage refuses an `UploadPartCopy` source under 5 MiB even as the last
    // part (`apps/desktop/test/s3-servers/README.md`).
    let plan = plan_parts(130 * MIB).unwrap();
    assert_eq!(plan.part_count, 2);
    assert_eq!(plan.range(1), (0, 64 * MIB - 1));
    assert_eq!(plan.range(2), (64 * MIB, 130 * MIB - 1));

    assert_eq!(plan_parts(MIN_PART_SIZE + 1).unwrap().part_count, 1);
    assert_eq!(plan_parts(MIN_PART_SIZE + 1).unwrap().range(1), (0, MIN_PART_SIZE));
    // Exactly 5 MiB stands on its own.
    assert_eq!(
        plan_parts(MIN_PART_SIZE + 5 * MIB).unwrap().range(2),
        (MIN_PART_SIZE, MIN_PART_SIZE + 5 * MIB - 1)
    );
}

#[test]
fn a_tail_stays_separate_when_folding_would_pass_5_gib() {
    let total = 9_999 * MAX_PART_SIZE + 1;
    let plan = plan_parts(total).unwrap();
    assert_eq!(plan.part_size, MAX_PART_SIZE);
    assert_eq!(u64::from(plan.part_count), MAX_PARTS);
    assert_eq!(plan.range(10_000), (total - 1, total - 1));
}

#[test]
fn a_short_tail_stays_its_own_last_part_where_the_provider_refuses_a_larger_one() {
    // R2 answers `InvalidPart` when the last part is larger than the rest
    // (live, 2026-10-02), and takes a small last part fine.
    let plan = plan_parts_with_floor(130 * MIB, MIN_PART_SIZE, ShortTail::Keep).unwrap();
    assert_eq!((plan.part_size, plan.part_count), (64 * MIB, 3));
    assert_eq!(plan.range(3), (128 * MIB, 130 * MIB - 1));
    assert_eq!(
        plan_parts_with_floor(MIN_PART_SIZE + 1, MIN_PART_SIZE, ShortTail::Keep)
            .unwrap()
            .part_count,
        2
    );
}
