//! Unit tests for `compute_brief_column_text_widths`.
//!
//! These tests insert a listing into `LISTING_CACHE` and seed `font_metrics`
//! with deterministic per-codepoint widths so we can assert exact pixel sums.
//! Each font ID gets a fixed width-per-char so the widest filename's width
//! collapses to `width_per_char * name.chars().count()`.
//!
//! Both globals are keyed per test: the listing id comes from `TestListing`, and
//! the font id from `seed_font`'s own counter. `font_metrics::METRICS_CACHE` is
//! process-global too, so a shared font id let one test's `store_metrics` change
//! the width-per-char another test was mid-assertion on.

use std::collections::HashMap;

use super::brief_columns::{BriefColumnsError, compute_brief_column_text_widths};
use super::caching_test_support::{TestListing, TestListingGuard, unique_test_id};
use super::metadata::FileEntry;

/// Seeds `font_metrics` with a uniform `width_per_char` for ASCII printable code
/// points, under a font id unique to this call, so any filename of length N
/// measures to `N * width_per_char`. Returns the font id.
fn seed_font(width_per_char: f32) -> String {
    let font_id = unique_test_id("brief-columns-font");
    let mut widths = HashMap::new();
    // ASCII printable range covers all test filenames; the `..` literal too.
    for cp in 0x20u32..=0x7Eu32 {
        widths.insert(cp, width_per_char);
    }
    crate::font_metrics::store_metrics(font_id.clone(), widths).expect("store font metrics");
    font_id
}

fn make_entry(name: &str) -> FileEntry {
    FileEntry::new(name.to_string(), format!("/test/{}", name), false, false)
}

fn insert_listing(tag: &str, entries: Vec<FileEntry>) -> TestListingGuard {
    TestListing::new().entries(entries).insert(tag)
}

// ============================================================================
// Empty listing
// ============================================================================

#[test]
fn empty_listing_returns_empty_vec() {
    let font = seed_font(7.0);
    let listing = insert_listing("bc_empty", vec![]);

    let widths = compute_brief_column_text_widths(listing.id(), 5, false, &font, false).unwrap();
    assert!(widths.is_empty());
}

#[test]
fn empty_listing_with_has_parent_returns_one_column() {
    // Just ".." → 1 cell → 1 column.
    let font = seed_font(7.0);
    let listing = insert_listing("bc_empty_parent", vec![]);

    let widths = compute_brief_column_text_widths(listing.id(), 5, true, &font, false).unwrap();
    assert_eq!(widths.len(), 1);
    // Width of ".." (2 chars) * 7.0
    assert_eq!(widths[0], 14.0);
}

// ============================================================================
// Single column, single short name
// ============================================================================

#[test]
fn single_column_single_short_name() {
    let font = seed_font(10.0);
    let listing = insert_listing("bc_single", vec![make_entry("abc")]);

    let widths = compute_brief_column_text_widths(listing.id(), 5, false, &font, false).unwrap();
    assert_eq!(widths.len(), 1);
    assert_eq!(widths[0], 30.0);
}

// ============================================================================
// Long name produces a wide column
// ============================================================================

#[test]
fn long_name_unclamped_width() {
    // Backend doesn't clamp; FE owns the cap. Verify a very long name
    // measures to its full width.
    let font = seed_font(8.0);
    let long = "a".repeat(200);
    let listing = insert_listing("bc_long", vec![make_entry(&long)]);

    let widths = compute_brief_column_text_widths(listing.id(), 5, false, &font, false).unwrap();
    assert_eq!(widths.len(), 1);
    assert_eq!(widths[0], 8.0 * 200.0);
}

// ============================================================================
// Two columns, second shorter than first
// ============================================================================

#[test]
fn two_columns_second_shorter() {
    let font = seed_font(5.0);
    // 6 entries, items_per_column = 3 → 2 columns.
    // Col 0: longest is "wide-name" (9 chars)
    // Col 1: longest is "ok.txt" (6 chars)
    let entries = vec![
        make_entry("a.txt"),
        make_entry("bb.txt"),
        make_entry("wide-name"),
        make_entry("c.txt"),
        make_entry("ok.txt"),
        make_entry("d.txt"),
    ];
    let listing = insert_listing("bc_two_cols", entries);

    let widths = compute_brief_column_text_widths(listing.id(), 3, false, &font, false).unwrap();
    assert_eq!(widths.len(), 2);
    assert_eq!(widths[0], 9.0 * 5.0); // "wide-name"
    assert_eq!(widths[1], 6.0 * 5.0); // "ok.txt"
    assert!(widths[1] < widths[0]);
}

// ============================================================================
// items_per_column = 0 → InvalidItemsPerColumn
// ============================================================================

#[test]
fn items_per_column_zero_rejected() {
    let font = seed_font(5.0);
    let listing = insert_listing("bc_zero", vec![make_entry("a.txt")]);

    let result = compute_brief_column_text_widths(listing.id(), 0, false, &font, false);
    assert_eq!(result, Err(BriefColumnsError::InvalidItemsPerColumn));
}

// ============================================================================
// has_parent = true: offset math (Risk #6 in plan)
// ============================================================================

#[test]
fn has_parent_offset_math_items_per_column_5() {
    // 12 visible entries + ".." → 13 cells, items_per_column = 5.
    // total_cells = 13, columns = ceil(13 / 5) = 3.
    //
    // Col 0: ".." + entries[0..4) -> 5 cells, names: "..", "a", "b", "c", "d"
    //   widest = "..widest" only via the entry. Place a wide name in col 0.
    // Col 1: entries[4..9) -> 5 cells: "e", "f", "g", "h", "i"
    // Col 2: entries[9..14) clamped to [9..12) -> 3 cells: "j", "k", "l"
    //
    // We'll plant a known-width filename in each column to verify the slicing.
    let font = seed_font(3.0);

    let entries = vec![
        // entries[0..4) → column 0 (alongside "..")
        make_entry("a"),
        make_entry("bb"),
        make_entry("ccc"),
        make_entry("ddddddd"), // 7 chars, widest in col 0 except possibly ".."
        // entries[4..9) → column 1
        make_entry("e"),
        make_entry("ff"),
        make_entry("ggg"),
        make_entry("hhhh"),
        make_entry("iiiiiiiii"), // 9 chars, widest in col 1
        // entries[9..12) → column 2
        make_entry("j"),
        make_entry("kkkkk"), // 5 chars, widest in col 2
        make_entry("ll"),
    ];

    let listing = insert_listing("bc_parent", entries);

    let widths = compute_brief_column_text_widths(listing.id(), 5, true, &font, false).unwrap();
    assert_eq!(widths.len(), 3, "expected 3 columns, got {:?}", widths);
    assert_eq!(widths[0], 7.0 * 3.0, "col 0: widest is 'ddddddd' (7 chars)");
    assert_eq!(widths[1], 9.0 * 3.0, "col 1: widest is 'iiiiiiiii' (9 chars)");
    assert_eq!(widths[2], 5.0 * 3.0, "col 2: widest is 'kkkkk' (5 chars)");
}

#[test]
fn has_parent_parent_literal_counts_in_col0() {
    // Verify ".." (2 chars) is the widest if real entries are shorter.
    let font = seed_font(4.0);

    let listing = insert_listing("bc_parent_widest", vec![make_entry("a"), make_entry("b")]);

    let widths = compute_brief_column_text_widths(listing.id(), 5, true, &font, false).unwrap();
    assert_eq!(widths.len(), 1);
    assert_eq!(
        widths[0],
        2.0 * 4.0,
        "'..' (2 chars * 4.0) should beat single-char entries"
    );
}

#[test]
fn has_parent_items_per_column_1() {
    // Edge: items_per_column = 1 with has_parent.
    // Col 0: ".." only (entries[0..0)).
    // Col 1: entries[0..1) -> first entry.
    // Col 2: entries[1..2) -> second entry.
    let font = seed_font(2.0);

    let listing = insert_listing("bc_parent_ipc1", vec![make_entry("foo"), make_entry("longername")]);

    let widths = compute_brief_column_text_widths(listing.id(), 1, true, &font, false).unwrap();
    assert_eq!(widths.len(), 3);
    assert_eq!(widths[0], 2.0 * 2.0); // ".."
    assert_eq!(widths[1], 3.0 * 2.0); // "foo"
    assert_eq!(widths[2], 10.0 * 2.0); // "longername"
}

// ============================================================================
// Hidden-files inclusion
// ============================================================================

#[test]
fn include_hidden_false_filters_dotfiles() {
    let font = seed_font(5.0);
    let listing = insert_listing(
        "bc_hidden_off",
        vec![make_entry(".hidden-very-long-name"), make_entry("v")],
    );

    let widths = compute_brief_column_text_widths(listing.id(), 5, false, &font, false).unwrap();
    assert_eq!(widths.len(), 1);
    // Only "v" remains; ".hidden..." is filtered out.
    assert_eq!(widths[0], 1.0 * 5.0);
}

#[test]
fn include_hidden_true_includes_dotfiles() {
    let font = seed_font(5.0);
    let listing = insert_listing(
        "bc_hidden_on",
        vec![
            make_entry(".hidden-very-long-name"), // 22 chars
            make_entry("v"),
        ],
    );

    let widths = compute_brief_column_text_widths(listing.id(), 5, false, &font, true).unwrap();
    assert_eq!(widths.len(), 1);
    assert_eq!(widths[0], 22.0 * 5.0);
}

// ============================================================================
// Non-default font_id produces different widths
// ============================================================================

#[test]
fn non_default_font_id_uses_alt_metrics() {
    let font = seed_font(5.0);
    let alt_font = seed_font(8.0);

    let listing = insert_listing("bc_alt_font", vec![make_entry("hello")]);

    let widths_default = compute_brief_column_text_widths(listing.id(), 5, false, &font, false).unwrap();
    let widths_alt = compute_brief_column_text_widths(listing.id(), 5, false, &alt_font, false).unwrap();

    assert_eq!(widths_default[0], 5.0 * 5.0);
    assert_eq!(widths_alt[0], 5.0 * 8.0);
    assert_ne!(widths_default[0], widths_alt[0]);
}

// ============================================================================
// Font ID not in cache → FontMetricsNotReady
// ============================================================================

#[test]
fn missing_font_id_returns_font_metrics_not_ready() {
    let listing = insert_listing("bc_no_font", vec![make_entry("a.txt")]);

    let result = compute_brief_column_text_widths(listing.id(), 5, false, "definitely-not-cached-font-id-xyz", false);
    assert_eq!(result, Err(BriefColumnsError::FontMetricsNotReady));
}

// ============================================================================
// Listing not in cache → ListingNotFound
// ============================================================================

#[test]
fn missing_listing_returns_listing_not_found() {
    let font = seed_font(5.0);
    let absent = "bc_listing_definitely_not_there";

    let result = compute_brief_column_text_widths(absent, 5, false, &font, false);
    match result {
        Err(BriefColumnsError::ListingNotFound(id)) => {
            assert_eq!(id, absent);
        }
        other => panic!("expected ListingNotFound, got {:?}", other),
    }
}

// ============================================================================
// All values are finite (no NaN/Inf): Risk #11 guard
// ============================================================================

#[test]
fn all_returned_widths_are_finite() {
    let font = seed_font(6.0);
    let entries: Vec<FileEntry> = (0..50).map(|i| make_entry(&format!("entry-{:03}", i))).collect();
    let listing = insert_listing("bc_finite", entries);

    let widths = compute_brief_column_text_widths(listing.id(), 7, true, &font, false).unwrap();

    assert!(!widths.is_empty());
    for (i, w) in widths.iter().enumerate() {
        assert!(w.is_finite(), "column {} returned non-finite width {}", i, w);
        assert!(*w >= 0.0, "column {} returned negative width {}", i, w);
    }
}
