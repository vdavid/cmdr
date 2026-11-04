//! The width math over one store: measured, estimated-and-reported, filled in,
//! and borrowed from a neighboring size.

use std::collections::{BTreeSet, HashMap};

use crate::metrics::{FontMetrics, FontMetricsError, FontMetricsStore, widths_from_parallel};

fn seed(store: &FontMetricsStore, widths: &[(u32, f32)]) -> String {
    let font_id = "system-400-12".to_string();
    let map: HashMap<u32, f32> = widths.iter().copied().collect();
    store.insert(font_id.clone(), FontMetrics::new(font_id.clone(), map));
    font_id
}

#[test]
fn measured_code_points_report_nothing_missing() {
    let store = FontMetricsStore::new();
    let font_id = seed(&store, &[('a' as u32, 7.0), ('b' as u32, 3.0)]);
    let mut missing = BTreeSet::new();

    let width = store.max_width_with_suffixes(&[("ab", 0.0)], &font_id, &mut missing);

    assert_eq!(width, Some(10.0));
    assert!(missing.is_empty(), "every code point was measured");
}

#[test]
fn unmeasured_code_points_use_the_average_and_are_reported() {
    // Average of 7 and 3 is 5, so the unmeasured '猫' counts as 5.
    let store = FontMetricsStore::new();
    let font_id = seed(&store, &[('a' as u32, 7.0), ('b' as u32, 3.0)]);
    let mut missing = BTreeSet::new();

    let width = store.max_width_with_suffixes(&[("a猫", 0.0)], &font_id, &mut missing);

    assert_eq!(width, Some(12.0), "7 for 'a' + the 5.0 average for '猫'");
    assert_eq!(missing.into_iter().collect::<Vec<_>>(), vec!['猫' as u32]);
}

#[test]
fn extending_replaces_the_average_with_the_real_width() {
    let store = FontMetricsStore::new();
    let font_id = seed(&store, &[('a' as u32, 7.0), ('b' as u32, 3.0)]);
    let mut missing = BTreeSet::new();
    store.max_width_with_suffixes(&[("a猫", 0.0)], &font_id, &mut missing);
    assert!(!missing.is_empty(), "the fill-in request starts non-empty");

    let merged = store
        .extend_with(&font_id, HashMap::from([('猫' as u32, 20.0)]), |_| "ran under the lock")
        .expect("the seeded font is stored");
    assert_eq!(merged, "ran under the lock");

    let mut still_missing = BTreeSet::new();
    let width = store.max_width_with_suffixes(&[("a猫", 0.0)], &font_id, &mut still_missing);

    assert_eq!(width, Some(27.0), "7 for 'a' + the measured 20 for '猫'");
    assert!(still_missing.is_empty(), "nothing is missing once filled");
}

#[test]
fn extending_an_unstored_font_is_refused() {
    let store = FontMetricsStore::new();
    let result = store.extend_with("never-measured", HashMap::from([(1, 1.0)]), |_| ());
    assert!(result.is_err());
    assert!(!store.contains("never-measured"), "a refused extend stores nothing");
}

#[test]
fn unknown_font_id_reports_no_width() {
    let store = FontMetricsStore::new();
    let mut missing = BTreeSet::new();
    assert_eq!(
        store.max_width_with_suffixes(&[("a", 0.0)], "never-measured", &mut missing),
        None
    );
}

fn seed_size(store: &FontMetricsStore, family: &str, size: f32, width_per_char: f32) {
    let map: HashMap<u32, f32> = ('a'..='z').map(|c| (c as u32, width_per_char)).collect();
    let font_id = format!("{family}-400-{size}");
    store.insert(font_id.clone(), FontMetrics::new(font_id, map));
}

#[test]
fn an_unmeasured_size_borrows_the_nearest_measured_one_scaled() {
    let store = FontMetricsStore::new();
    seed_size(&store, "system", 12.0, 6.0);
    seed_size(&store, "system", 20.0, 10.0);

    // 19 px is nearest the measured 20 px: 10 × 19/20 = 9.5 per character.
    let width = store.max_width_scaled_from_nearest(&[("ab", 0.0)], "system-400-19");

    assert_eq!(width, Some(19.0));
}

#[test]
fn a_fractional_size_scales_too() {
    let store = FontMetricsStore::new();
    seed_size(&store, "system", 12.0, 6.0);

    // 14.4 / 12 = 1.2, the 120% text size.
    let width = store
        .max_width_scaled_from_nearest(&[("ab", 1.0)], "system-400-14.4")
        .expect("a size of this family is measured");

    assert!((width - (12.0 * 1.2 + 1.0)).abs() < 1e-4, "got {width}");
}

#[test]
fn another_family_or_weight_is_never_borrowed() {
    let store = FontMetricsStore::new();
    seed_size(&store, "system", 12.0, 6.0);

    assert_eq!(
        store.max_width_scaled_from_nearest(&[("ab", 0.0)], "systemx-400-14"),
        None
    );
    assert_eq!(
        store.max_width_scaled_from_nearest(&[("ab", 0.0)], "system-600-12"),
        None
    );
    assert_eq!(
        store.max_width_scaled_from_nearest(&[("ab", 0.0)], "never-measured"),
        None
    );
}

#[test]
fn parallel_arrays_zip_into_one_map() {
    let widths = widths_from_parallel(vec![97, 98], vec![7.0, 3.0]).expect("lengths match");
    assert_eq!(widths, HashMap::from([(97, 7.0), (98, 3.0)]));
}

#[test]
fn parallel_arrays_of_different_lengths_are_rejected() {
    let result = widths_from_parallel(vec![97, 98], vec![7.0]);
    assert!(matches!(
        result,
        Err(FontMetricsError::MismatchedWidths {
            code_points: 2,
            widths: 1
        })
    ));
}
