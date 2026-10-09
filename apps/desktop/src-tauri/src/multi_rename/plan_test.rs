//! The preview: new names and their statuses over a folder's entries.

use std::cell::Cell;
use std::path::Path;

use super::plan::{
    Compiled, FOLDS, InvalidNameReason, MaskExamples, MultiRenameSpec, RowStatus, SpecError, mask_examples, preview,
};
use super::transform::{CaseChange, SEARCH_BUILDS};
use crate::file_system::listing::metadata::FileEntry;

const DIR: &str = "/Users/me/Photos/Holiday 2026";

fn file(name: &str) -> FileEntry {
    FileEntry {
        modified_at: Some(1_781_557_809), // 2026-06-15 21:10:09 UTC
        ..FileEntry::new(name.to_string(), format!("{DIR}/{name}"), false, false)
    }
}

fn spec(name_mask: &str) -> MultiRenameSpec {
    MultiRenameSpec {
        name_mask: name_mask.to_string(),
        extension_mask: "[E]".to_string(),
        search: String::new(),
        replace: String::new(),
        case_sensitive: false,
        first_only: false,
        include_extension: false,
        regex: false,
        substitute: false,
        case: CaseChange::Unchanged,
        remove_diacritics: false,
    }
}

/// Previews renaming `batch` (all of `folder` that's named) with `spec`.
fn run(spec: &MultiRenameSpec, folder: &[FileEntry], batch: &[&str]) -> Vec<(String, RowStatus)> {
    let compiled = Compiled::new(spec).expect("a valid spec");
    let rows: Vec<(usize, &FileEntry)> = folder
        .iter()
        .enumerate()
        .filter(|(_, e)| batch.contains(&e.name.as_str()))
        .collect();
    preview(&compiled, Path::new(DIR), &rows, folder)
        .into_iter()
        .map(|p| (p.new_name, p.status))
        .collect()
}

#[test]
fn a_counter_and_the_parent_number_the_batch_in_order() {
    let folder = [file("IMG_0003.jpg"), file("IMG_0001.jpg"), file("notes.txt")];
    let out = run(&spec("[P] [C:2]"), &folder, &["IMG_0003.jpg", "IMG_0001.jpg"]);
    assert_eq!(
        out,
        vec![
            ("Holiday 2026 01.jpg".to_string(), RowStatus::Ready),
            ("Holiday 2026 02.jpg".to_string(), RowStatus::Ready),
        ]
    );
}

#[test]
fn the_users_everyday_preset_strips_diacritics() {
    let s = MultiRenameSpec {
        remove_diacritics: true,
        ..spec("[N]")
    };
    let folder = [file("Žádost o přezkum.pdf"), file("plain.pdf")];
    let out = run(&s, &folder, &["Žádost o přezkum.pdf", "plain.pdf"]);
    assert_eq!(out[0], ("Zadost o prezkum.pdf".to_string(), RowStatus::Ready));
    assert_eq!(out[1], ("plain.pdf".to_string(), RowStatus::Unchanged));
}

#[test]
fn two_rows_getting_one_name_are_both_flagged() {
    let folder = [file("a1.txt"), file("a2.txt")];
    let out = run(&spec("a"), &folder, &["a1.txt", "a2.txt"]);
    assert_eq!(
        out.iter().map(|(_, s)| s.clone()).collect::<Vec<_>>(),
        vec![RowStatus::Duplicate, RowStatus::Duplicate]
    );
}

#[test]
fn a_name_held_by_a_file_that_stays_is_taken_but_one_leaving_is_free() {
    let folder = [file("a.txt"), file("b.txt"), file("c.txt")];
    // a → b while b stays: taken (case-insensitively, as the Mac sees names).
    let s = spec("B");
    assert_eq!(run(&s, &folder, &["a.txt"])[0].1, RowStatus::TargetExists);
    // a → b while b → c, and no c: b is free because b leaves (one pass, so a
    // becomes b, not c).
    let two = [file("a.txt"), file("b.txt")];
    let chain = MultiRenameSpec {
        search: "a|b".to_string(),
        replace: "b|c".to_string(),
        ..spec("[N]")
    };
    let out = run(&chain, &two, &["a.txt", "b.txt"]);
    assert_eq!(out[0], ("b.txt".to_string(), RowStatus::Ready));
    assert_eq!(out[1], ("c.txt".to_string(), RowStatus::Ready));
}

#[test]
fn a_swap_inside_the_batch_is_fine() {
    let folder = [file("one.txt"), file("two.txt")];
    let swap = MultiRenameSpec {
        search: "one|two".to_string(),
        replace: "two|one".to_string(),
        regex: false,
        ..spec("[N]")
    };
    // One pass swaps the names; the executor runs the cycle through a temp name.
    let out = run(&swap, &folder, &["one.txt", "two.txt"]);
    assert_eq!(
        out,
        vec![
            ("two.txt".to_string(), RowStatus::Ready),
            ("one.txt".to_string(), RowStatus::Ready),
        ]
    );
}

#[test]
fn a_case_only_rename_of_the_same_file_is_ready() {
    let folder = [file("report.pdf")];
    let s = MultiRenameSpec {
        case: CaseChange::Upper,
        ..spec("[N]")
    };
    assert_eq!(
        run(&s, &folder, &["report.pdf"])[0],
        ("REPORT.PDF".to_string(), RowStatus::Ready)
    );
}

#[test]
fn invalid_names_say_why() {
    let folder = [file("a.txt")];
    assert_eq!(
        run(&spec("x/y"), &folder, &["a.txt"])[0].1,
        RowStatus::InvalidName {
            reason: InvalidNameReason::DisallowedCharacter {
                character: "/".to_string()
            }
        }
    );
    let empty = MultiRenameSpec {
        extension_mask: String::new(),
        ..spec("")
    };
    assert_eq!(
        run(&empty, &folder, &["a.txt"])[0].1,
        RowStatus::InvalidName {
            reason: InvalidNameReason::Empty
        }
    );
}

#[test]
fn a_hidden_sibling_still_holds_its_name() {
    let folder = [file("a.txt"), file(".b.txt")];
    assert_eq!(run(&spec(".b"), &folder, &["a.txt"])[0].1, RowStatus::TargetExists);
}

#[test]
fn a_bad_spec_is_one_error_for_the_sheet() {
    assert!(matches!(Compiled::new(&spec("[N")), Err(SpecError::NameMask { .. })));
    let bad_regex = MultiRenameSpec {
        search: "(".to_string(),
        regex: true,
        ..spec("[N]")
    };
    assert!(matches!(Compiled::new(&bad_regex), Err(SpecError::BadRegex { .. })));
}

#[test]
fn a_row_renaming_into_a_blocked_one_is_blocked_too() {
    // a → b, b → c, and c stays: b→c is taken, so b stays, so a→b is taken too.
    let folder = [file("a.txt"), file("b.txt"), file("c.txt")];
    let chain = MultiRenameSpec {
        search: "a|b".to_string(),
        replace: "b|c".to_string(),
        ..spec("[N]")
    };
    let out = run(&chain, &folder, &["a.txt", "b.txt"]);
    assert_eq!(out[0], ("b.txt".to_string(), RowStatus::TargetExists));
    assert_eq!(out[1], ("c.txt".to_string(), RowStatus::TargetExists));
}

#[test]
fn a_decomposed_name_is_unchanged_when_only_its_form_would_change() {
    let folder = [file("Cafe\u{301}.txt")];
    assert_eq!(
        run(&spec("[N]"), &folder, &["Cafe\u{301}.txt"])[0].1,
        RowStatus::Unchanged
    );
    // And a range never splits the accent off its letter.
    assert_eq!(
        run(&spec("[N1-4]"), &folder, &["Cafe\u{301}.txt"])[0].0,
        "Caf\u{e9}.txt"
    );
}

#[test]
fn a_huge_counter_width_is_capped() {
    let folder = [file("a.txt")];
    let out = run(&spec("[C:4000000000]"), &folder, &["a.txt"]);
    assert_eq!(out[0].0.len(), 64 + ".txt".len());
}

#[test]
fn a_preview_compiles_the_search_once_not_per_row() {
    let folder: Vec<FileEntry> = (0..200).map(|i| file(&format!("IMG_{i:04}.jpg"))).collect();
    let batch: Vec<&str> = folder.iter().map(|e| e.name.as_str()).collect();
    let s = MultiRenameSpec {
        search: r"IMG_(\d+)".to_string(),
        replace: "photo $1".to_string(),
        regex: true,
        ..spec("[N]")
    };
    SEARCH_BUILDS.with(|n| n.set(0));
    let out = run(&s, &folder, &batch);
    assert_eq!(out[7].0, "photo 0007.jpg");
    assert_eq!(SEARCH_BUILDS.with(Cell::get), 1);
}

#[test]
fn a_long_chain_blocked_at_its_end_settles_in_linear_work() {
    // f0001 → f0002 → … → f1000 → f1001, and f1001 stays: the block runs down
    // the whole chain, one row at a time from the end.
    const N: usize = 1000;
    let folder: Vec<FileEntry> = (1..=N + 1).map(|i| file(&format!("f{i:04}.txt"))).collect();
    let batch: Vec<&str> = folder[..N].iter().map(|e| e.name.as_str()).collect();
    let s = spec("f[C2:4]");
    FOLDS.with(|n| n.set(0));
    let out = run(&s, &folder, &batch);
    assert_eq!(out[0].0, "f0002.txt");
    assert!(out.iter().all(|(_, status)| *status == RowStatus::TargetExists));
    // Each sibling once, and each row's old and new name once.
    let folds = FOLDS.with(Cell::get);
    assert!(folds <= folder.len() + 2 * N, "fold count: {folds}");
}

#[test]
fn a_row_carries_its_file_icon_and_whether_it_is_a_folder() {
    let folder = [
        file("notes.txt"),
        FileEntry::new("Trips".to_string(), format!("{DIR}/Trips"), true, false),
    ];
    let compiled = Compiled::new(&spec("[N]")).expect("a valid spec");
    let rows: Vec<(usize, &FileEntry)> = folder.iter().enumerate().collect();
    let out = preview(&compiled, Path::new(DIR), &rows, &folder);
    assert_eq!(out[0].icon_id.as_deref(), Some(folder[0].icon_id.as_str()));
    assert!(!out[0].is_directory);
    assert_eq!(out[1].icon_id.as_deref(), Some(folder[1].icon_id.as_str()));
    assert!(out[1].is_directory);
}

#[test]
fn the_extension_mask_counts_too() {
    let folder = [file("a.txt"), file("b.txt")];
    let s = MultiRenameSpec {
        extension_mask: "[E][C:2]".to_string(),
        ..spec("[N]")
    };
    let out = run(&s, &folder, &["a.txt", "b.txt"]);
    assert_eq!(out[0].0, "a.txt01");
    assert_eq!(out[1].0, "b.txt02");
}

fn masks(list: &[&str]) -> Vec<String> {
    list.iter().map(|m| (*m).to_string()).collect()
}

#[test]
fn examples_render_each_mask_for_the_file_as_the_preview_would() {
    let entry = file("report.pdf");
    let out = mask_examples(
        &entry,
        Path::new(DIR),
        &masks(&["[N]", "[E1]", "[E2-]", "[N1--4]", "[N-3-]", "[P]", "[G]"]),
    );
    assert_eq!(
        out.rendered,
        vec![
            Some("report".to_string()),
            Some("p".to_string()),
            Some("df".to_string()),
            Some("rep".to_string()),
            Some("ort".to_string()),
            Some("Holiday 2026".to_string()),
            Some("Photos".to_string()),
        ]
    );
    assert!(!out.sample_date);
}

#[test]
fn an_example_mask_that_doesnt_parse_renders_nothing_rather_than_failing_the_rest() {
    let out = mask_examples(&file("a.txt"), Path::new(DIR), &masks(&["[Q]", "[E]"]));
    assert_eq!(out.rendered, vec![None, Some("txt".to_string())]);
}

#[test]
fn a_file_with_no_modified_time_shows_dates_for_a_sample_one_and_says_so() {
    let entry = FileEntry {
        modified_at: None,
        ..file("a.txt")
    };
    let MaskExamples { rendered, sample_date } = mask_examples(&entry, Path::new(DIR), &masks(&["[YMD]", "[t]"]));
    assert_eq!(
        rendered,
        vec![Some("20260615".to_string()), Some("23.10.09".to_string())]
    );
    assert!(sample_date);
}
