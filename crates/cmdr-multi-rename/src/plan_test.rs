//! The preview: new names and their statuses over a folder's entries.

use std::cell::Cell;
use std::path::Path;

use super::mask::RowMedia;
use super::plan::{
    Compiled, FOLDS, InvalidNameReason, MediaLookup, MultiRenameSpec, NameEdits, NoMedia, RenameExample, RowStatus,
    SpecError, preview, preview_names, render_examples,
};
use super::transform::{CaseChange, SEARCH_BUILDS};
use cmdr_fs::entry::FileEntry;
use cmdr_media_meta::{MediaFacts, MediaField};

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
        greek_to_latin: false,
        normalize_unicode: false,
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
    preview(
        &compiled,
        Path::new(DIR),
        &rows,
        folder,
        &NameEdits::default(),
        &NoMedia,
    )
    .into_iter()
    .map(|p| (p.new_name, p.status))
    .collect()
}

/// The session and the MCP tool both preview by NAME: a name the folder no longer holds is
/// `Missing` at its own row, and the counter numbers only the files still there.
#[test]
fn previewing_by_name_marks_a_gone_file_missing_in_place() {
    let folder = [file("b.jpg"), file("a.jpg")];
    let names: Vec<String> = ["a.jpg", "gone.jpg", "b.jpg"]
        .iter()
        .map(|s| (*s).to_string())
        .collect();
    let compiled = Compiled::new(&spec("[C]")).expect("a valid spec");
    let rows = preview_names(
        &compiled,
        Path::new(DIR),
        &names,
        &folder,
        &NameEdits::default(),
        &NoMedia,
    );
    let out: Vec<_> = rows.iter().map(|r| (r.row, r.new_name.as_str(), &r.status)).collect();
    assert_eq!(
        out,
        vec![
            (0, "1.jpg", &RowStatus::Ready),
            (1, "gone.jpg", &RowStatus::Missing),
            (2, "2.jpg", &RowStatus::Ready),
        ]
    );
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
    let out = preview(
        &compiled,
        Path::new(DIR),
        &rows,
        &folder,
        &NameEdits::default(),
        &NoMedia,
    );
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

fn example(file_name: &str, spec: MultiRenameSpec) -> RenameExample {
    RenameExample {
        file_name: file_name.to_string(),
        spec,
    }
}

#[test]
fn examples_render_masks_for_a_made_up_file_in_a_made_up_folder_on_a_fixed_date() {
    let name_only = |mask: &str| MultiRenameSpec {
        extension_mask: String::new(),
        ..spec(mask)
    };
    let out = render_examples(&[
        example("Beach day.jpg", spec("[N]")),
        example("Beach day.jpg", name_only("[N1-2]<[N3]>[N4-]")),
        example("Beach day.jpg", name_only("[G]/[P]")),
        example("Beach day.jpg", name_only("[Y]-[M]-[D] [h]:[m]:[s]")),
        example("Beach day.jpg", name_only("[C10:3]")),
    ]);
    assert_eq!(
        out,
        vec![
            Some("Beach day.jpg".to_string()),
            Some("Be<a>ch day".to_string()),
            Some("Trips/Lisbon 2026".to_string()),
            Some("2026-07-14 09:05:30".to_string()),
            Some("010".to_string()),
        ]
    );
}

#[test]
fn examples_run_search_and_replace_as_a_rename_would() {
    let replacing = |search: &str, replace: &str| MultiRenameSpec {
        search: search.to_string(),
        replace: replace.to_string(),
        ..spec("[N]")
    };
    let out = render_examples(&[
        example("Photo photo.jpg", replacing("photo", "pic")),
        example(
            "Photo photo.jpg",
            MultiRenameSpec {
                case_sensitive: true,
                ..replacing("photo", "pic")
            },
        ),
        example(
            "IMG_0042.jpg",
            MultiRenameSpec {
                substitute: true,
                ..replacing("IMG", "Lisbon")
            },
        ),
    ]);
    assert_eq!(
        out,
        vec![
            Some("pic pic.jpg".to_string()),
            Some("Photo pic.jpg".to_string()),
            Some("Lisbon.jpg".to_string()),
        ]
    );
}

#[test]
fn examples_read_the_made_up_files_exif_and_tags() {
    let name_only = |mask: &str| MultiRenameSpec {
        extension_mask: String::new(),
        ..spec(mask)
    };
    let out = render_examples(&[
        example("IMG_0042.jpg", spec("[=exif.taken:d] [N]")),
        example(
            "IMG_0042.jpg",
            name_only("[=exif.camera], [=exif.width]×[=exif.height], ISO [=exif.iso]"),
        ),
        example("song.mp3", spec("[=tag.track:2] [=tag.artist] - [=tag.title]")),
        example("song.mp3", name_only("[=tag.album] ([=tag.year])")),
        example("song.mp3", spec("[=tag.comment]")),
    ]);
    assert_eq!(
        out,
        vec![
            Some("2026-07-14 IMG_0042.jpg".to_string()),
            Some("Apple iPhone 15 Pro, 4032×3024, ISO 64".to_string()),
            Some("03 Daft Punk - Digital Love.mp3".to_string()),
            Some("Discovery (2001)".to_string()),
            None,
        ],
        "a field the example file doesn't have renders no example"
    );
}

/// Media facts by file name; a file not listed is `NotMedia`.
struct MediaByName<'a>(Vec<(&'a str, RowMedia<'a>)>);

impl MediaLookup for MediaByName<'_> {
    fn media(&self, entry: &FileEntry) -> RowMedia<'_> {
        self.0
            .iter()
            .find(|(name, _)| *name == entry.name)
            .map_or(RowMedia::NotMedia, |(_, media)| *media)
    }
}

fn run_media(spec: &MultiRenameSpec, folder: &[FileEntry], media: &MediaByName<'_>) -> Vec<(String, RowStatus)> {
    let compiled = Compiled::new(spec).expect("a valid spec");
    let rows: Vec<(usize, &FileEntry)> = folder.iter().enumerate().collect();
    preview(&compiled, Path::new(DIR), &rows, folder, &NameEdits::default(), media)
        .into_iter()
        .map(|p| (p.new_name, p.status))
        .collect()
}

#[test]
fn a_row_a_media_field_has_no_text_for_keeps_its_name_and_says_why() {
    let song = MediaFacts {
        artist: Some("Daft Punk".to_string()),
        ..MediaFacts::default()
    };
    let untitled = MediaFacts::default();
    let folder = [
        file("a.mp3"),
        file("b.mp3"),
        file("c.mp3"),
        file("d.mp3"),
        file("e.txt"),
    ];
    let media = MediaByName(vec![
        ("a.mp3", RowMedia::Read(&song)),
        ("b.mp3", RowMedia::Read(&untitled)),
        ("c.mp3", RowMedia::Unreadable),
        ("d.mp3", RowMedia::Reading),
    ]);
    let artist = RowStatus::NoValue {
        field: MediaField::TagArtist,
    };
    assert_eq!(
        run_media(&spec("[=tag.artist] [N]"), &folder, &media),
        vec![
            ("Daft Punk a.mp3".to_string(), RowStatus::Ready),
            ("b.mp3".to_string(), artist.clone()),
            (
                "c.mp3".to_string(),
                RowStatus::Unreadable {
                    field: MediaField::TagArtist
                }
            ),
            ("d.mp3".to_string(), RowStatus::Reading),
            ("e.txt".to_string(), artist.clone()),
        ]
    );
    assert!(artist.is_problem() && !artist.is_ready());
    assert!(
        !RowStatus::Reading.is_problem() && !RowStatus::Reading.is_ready(),
        "a row still reading may yet turn ready: not a problem, not ready"
    );
}

#[test]
fn still_reading_in_either_mask_wins_over_a_missing_value_in_the_other() {
    let folder = [file("a.mp3")];
    let media = MediaByName(vec![("a.mp3", RowMedia::Reading)]);
    let both = MultiRenameSpec {
        extension_mask: "[=tag.genre]".to_string(),
        ..spec("[=tag.title]")
    };
    assert_eq!(run_media(&both, &folder, &media)[0].1, RowStatus::Reading);
    let untitled = MediaFacts::default();
    let media = MediaByName(vec![("a.mp3", RowMedia::Read(&untitled))]);
    assert_eq!(
        run_media(&both, &folder, &media)[0].1,
        RowStatus::NoValue {
            field: MediaField::TagTitle
        },
        "the name mask's gap comes first"
    );
}

#[test]
fn a_row_waiting_on_its_tags_still_holds_its_name() {
    // `b.mp3` keeps its name while it's read, so `a.mp3` can't take it.
    let titled_b = MediaFacts {
        title: Some("b".to_string()),
        ..MediaFacts::default()
    };
    let folder = [file("a.mp3"), file("b.mp3")];
    let media = MediaByName(vec![("a.mp3", RowMedia::Read(&titled_b)), ("b.mp3", RowMedia::Reading)]);
    assert_eq!(
        run_media(&spec("[=tag.title]"), &folder, &media),
        vec![
            ("b.mp3".to_string(), RowStatus::TargetExists),
            ("b.mp3".to_string(), RowStatus::Reading),
        ]
    );
}

#[test]
fn an_example_whose_spec_doesnt_run_renders_nothing_rather_than_failing_the_rest() {
    let bad_regex = MultiRenameSpec {
        search: "(".to_string(),
        regex: true,
        ..spec("[N]")
    };
    let out = render_examples(&[
        example("a.txt", spec("[Q]")),
        example("a.txt", bad_regex),
        example("a.txt", spec("[E]")),
    ]);
    assert_eq!(out, vec![None, None, Some("txt.txt".to_string())]);
}

#[test]
fn a_decomposed_name_counts_as_unchanged_unless_the_spec_normalizes() {
    let decomposed = "Z\u{030C}adost.pdf"; // Ž as Z + combining caron, as macOS and SMB hand it over
    let folder = [file(decomposed)];
    assert_eq!(run(&spec("[N]"), &folder, &[decomposed])[0].1, RowStatus::Unchanged);

    let normalize = MultiRenameSpec {
        normalize_unicode: true,
        ..spec("[N]")
    };
    let renamed = run(&normalize, &folder, &[decomposed]);
    assert_eq!(renamed[0], ("\u{017D}adost.pdf".to_string(), RowStatus::Ready));

    let composed = [file("\u{017D}adost.pdf")];
    assert_eq!(
        run(&normalize, &composed, &["\u{017D}adost.pdf"])[0].1,
        RowStatus::Unchanged,
        "an already composed name has nothing to do"
    );
}

#[test]
fn greek_to_latin_runs_through_the_spec() {
    let s = MultiRenameSpec {
        greek_to_latin: true,
        ..spec("[N]")
    };
    let folder = [file("Ρόδος 01.jpg")];
    assert_eq!(run(&s, &folder, &["Ρόδος 01.jpg"])[0].0, "Rodos 01.jpg");
}

/// Previews all of `folder` with names the user typed in Results.
fn run_edited(spec: &MultiRenameSpec, folder: &[FileEntry], edits: &[(&str, &str)]) -> Vec<(String, RowStatus, bool)> {
    let compiled = Compiled::new(spec).expect("a valid spec");
    let rows: Vec<(usize, &FileEntry)> = folder.iter().enumerate().collect();
    preview(
        &compiled,
        Path::new(DIR),
        &rows,
        folder,
        &NameEdits::new(edits),
        &NoMedia,
    )
    .into_iter()
    .map(|p| (p.new_name, p.status, p.edited))
    .collect()
}

#[test]
fn a_typed_name_replaces_the_computed_one_by_old_name() {
    let folder = [file("a.txt"), file("b.txt"), file("c.txt")];
    let out = run_edited(&spec("[N]-x"), &folder, &[("b.txt", "bee.txt"), ("gone.txt", "z.txt")]);
    assert_eq!(out[0], ("a-x.txt".to_string(), RowStatus::Ready, false));
    assert_eq!(out[1], ("bee.txt".to_string(), RowStatus::Ready, true));
    assert_eq!(out[2].0, "c-x.txt", "a line for a name that isn't here touches nothing");
}

#[test]
fn a_typed_name_is_checked_like_any_other() {
    let folder = [file("a.txt"), file("b.txt"), file("keep.txt")];
    let out = run_edited(
        &spec("[N]"),
        &folder,
        &[
            ("a.txt", "same.txt"),
            ("b.txt", "same.txt"),
            ("keep.txt", "bad/name.txt"),
        ],
    );
    assert_eq!(out[0].1, RowStatus::Duplicate);
    assert_eq!(out[1].1, RowStatus::Duplicate);
    assert!(matches!(out[2].1, RowStatus::InvalidName { .. }));
}

#[test]
fn a_typed_name_matches_an_old_name_in_either_unicode_form() {
    let folder = [file("Z\u{030C}adost.pdf")];
    let out = run_edited(&spec("[N]"), &folder, &[("\u{017D}adost.pdf", "Zadost.pdf")]);
    assert_eq!(out[0], ("Zadost.pdf".to_string(), RowStatus::Ready, true));
}
