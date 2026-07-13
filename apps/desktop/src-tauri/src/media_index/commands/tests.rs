//! Command-layer tests. The seeded-`media.db` → `OcrHit`-with-snippet round-trip the
//! command delegates to is covered end-to-end in `read/tests.rs`
//! (`search_finds_the_image_by_ocr_text_and_survives_unmount`); here we cover the
//! command-specific limit resolution.

use super::{DEFAULT_LIMIT, MAX_LIMIT, resolve_limit};

#[test]
fn a_missing_limit_takes_the_default() {
    assert_eq!(resolve_limit(None), DEFAULT_LIMIT as usize);
}

#[test]
fn a_supplied_limit_is_honored_below_the_ceiling() {
    assert_eq!(resolve_limit(Some(25)), 25);
}

#[test]
fn an_oversized_limit_is_clamped_to_the_ceiling() {
    assert_eq!(resolve_limit(Some(100_000)), MAX_LIMIT as usize);
}
