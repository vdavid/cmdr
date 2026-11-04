use std::collections::HashMap;

use super::*;
use crate::search::index::{ArenaBuilder, arena_of};
use crate::search::ranking::ImportanceWeights;

mod allocations;
mod count;
mod filters;
mod matching;
mod reconstruction;
mod scope;

// ── Helper: build a small in-memory index ────────────────────────

pub(super) fn make_test_index() -> SearchIndex {
    const NAMES: [&str; 8] = [
        "",
        "Users",
        "alice",
        "report.pdf",
        "photo.jpg",
        "notes.txt",
        "Documents",
        "Q1-report.pdf",
    ];
    arena_of(
        &[
            (1, 0, NAMES[0], true, None, None),
            (2, 1, NAMES[1], true, None, Some(1000)),
            (3, 2, NAMES[2], true, None, Some(2000)),
            (4, 3, NAMES[3], false, Some(1_000_000), Some(3000)),
            (5, 3, NAMES[4], false, Some(5_000_000), Some(4000)),
            (6, 3, NAMES[5], false, Some(500), Some(5000)),
            (7, 2, NAMES[6], true, None, Some(1500)),
            (8, 7, NAMES[7], false, Some(2_000_000), Some(6000)),
        ],
        1,
    )
}
