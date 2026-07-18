use std::collections::HashMap;

use super::*;
use crate::search::index::SearchEntry;

mod count;
mod filters;
mod matching;
mod reconstruction;
mod scope;

// ── Helper: build a small in-memory index ────────────────────────

/// Helper: push a name into the arena and return (offset, len).
pub(super) fn arena_push(names: &mut String, name: &str) -> (u32, u16) {
    let offset = names.len() as u32;
    let len = name.len() as u16;
    names.push_str(name);
    (offset, len)
}

pub(super) fn make_test_index() -> SearchIndex {
    let mut names = String::new();
    let test_names = [
        "",
        "Users",
        "alice",
        "report.pdf",
        "photo.jpg",
        "notes.txt",
        "Documents",
        "Q1-report.pdf",
    ];
    let offsets: Vec<(u32, u16)> = test_names.iter().map(|n| arena_push(&mut names, n)).collect();

    let entries = vec![
        SearchEntry {
            id: 1,
            parent_id: 0,
            name_offset: offsets[0].0,
            name_len: offsets[0].1,
            is_directory: true,
            size: None,
            modified_at: None,
        },
        SearchEntry {
            id: 2,
            parent_id: 1,
            name_offset: offsets[1].0,
            name_len: offsets[1].1,
            is_directory: true,
            size: None,
            modified_at: Some(1000),
        },
        SearchEntry {
            id: 3,
            parent_id: 2,
            name_offset: offsets[2].0,
            name_len: offsets[2].1,
            is_directory: true,
            size: None,
            modified_at: Some(2000),
        },
        SearchEntry {
            id: 4,
            parent_id: 3,
            name_offset: offsets[3].0,
            name_len: offsets[3].1,
            is_directory: false,
            size: Some(1_000_000),
            modified_at: Some(3000),
        },
        SearchEntry {
            id: 5,
            parent_id: 3,
            name_offset: offsets[4].0,
            name_len: offsets[4].1,
            is_directory: false,
            size: Some(5_000_000),
            modified_at: Some(4000),
        },
        SearchEntry {
            id: 6,
            parent_id: 3,
            name_offset: offsets[5].0,
            name_len: offsets[5].1,
            is_directory: false,
            size: Some(500),
            modified_at: Some(5000),
        },
        SearchEntry {
            id: 7,
            parent_id: 2,
            name_offset: offsets[6].0,
            name_len: offsets[6].1,
            is_directory: true,
            size: None,
            modified_at: Some(1500),
        },
        SearchEntry {
            id: 8,
            parent_id: 7,
            name_offset: offsets[7].0,
            name_len: offsets[7].1,
            is_directory: false,
            size: Some(2_000_000),
            modified_at: Some(6000),
        },
    ];
    let mut id_to_index = HashMap::new();
    for (i, e) in entries.iter().enumerate() {
        id_to_index.insert(e.id, i);
    }
    SearchIndex {
        names,
        entries,
        id_to_index,
        generation: 1,
    }
}
