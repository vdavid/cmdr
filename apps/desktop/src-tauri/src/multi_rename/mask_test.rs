//! The rename mask against Total Commander's own examples (help file and wiki).

use chrono::NaiveDate;

use super::mask::{MAX_COUNTER_DIGITS, Mask, MaskError, RowFacts, counter_token};

fn facts(name: &str) -> RowFacts<'_> {
    RowFacts {
        file_name: name,
        is_directory: false,
        parent: "Holiday 2026",
        grandparent: "Photos",
        modified: NaiveDate::from_ymd_opt(2026, 6, 15).and_then(|d| d.and_hms_opt(23, 10, 9)),
        position: 0,
    }
}

/// Renders `mask` as the NAME mask of `file` (the extension mask is a separate field).
fn name(mask: &str, file: &str) -> String {
    Mask::parse(mask).expect("a valid mask").render(&facts(file))
}

#[test]
fn plain_text_stays_and_n_is_the_name_without_extension() {
    assert_eq!(name("[N]", "report.final.pdf"), "report.final");
    assert_eq!(name("copy of [N]", "a.txt"), "copy of a");
    assert_eq!(name("[N]", ".bashrc"), ".bashrc", "a leading dot is part of the name");
}

#[test]
fn ranges_count_from_one_and_from_the_end() {
    let file = "ABCDEFGHIJ.txt";
    assert_eq!(name("[N1]", file), "A");
    assert_eq!(name("[N2-5]", file), "BCDE");
    assert_eq!(name("[N2,5]", file), "BCDEF");
    assert_eq!(name("[N2-]", file), "BCDEFGHIJ");
    assert_eq!(name("[N-8,5]", file), "CDEFG");
    assert_eq!(name("[N-8-5]", file), "CDEF", "8th-last to 5th-last");
    assert_eq!(name("[N2--5]", file), "BCDEF", "2nd to 5th-last");
    assert_eq!(name("[N-5-]", file), "FGHIJ");
    assert_eq!(name("[N20-30]", file), "", "past the end is empty, not an error");
}

#[test]
fn ranges_count_characters_not_bytes() {
    assert_eq!(name("[N1-3]", "žluťoučký.txt"), "žlu");
}

#[test]
fn extension_parent_and_grandparent() {
    assert_eq!(name("[E]", "song.mp3"), "mp3");
    assert_eq!(name("[E1-2]", "song.mp3"), "mp");
    assert_eq!(name("[P] [N]", "img.jpg"), "Holiday 2026 img");
    assert_eq!(name("[P1-7]", "img.jpg"), "Holiday");
    assert_eq!(name("[G]", "img.jpg"), "Photos");
}

#[test]
fn a_folder_has_no_extension() {
    let folder = RowFacts {
        is_directory: true,
        ..facts("archive.2024")
    };
    let mask = Mask::parse("[N]|[E]").expect("valid");
    assert_eq!(mask.render(&folder), "archive.2024|");
}

#[test]
fn a_bare_counter_counts_one_two_three_and_each_counter_has_its_own_settings() {
    let mask = Mask::parse("[C]").expect("valid");
    let row = |position| RowFacts {
        position,
        ..facts("a.txt")
    };
    assert_eq!(mask.render(&row(0)), "1");
    assert_eq!(mask.render(&row(11)), "12", "no padding to the batch size");

    let inline = Mask::parse("[C10+5:3]").expect("valid");
    assert_eq!(inline.render(&row(0)), "010");
    assert_eq!(inline.render(&row(2)), "020");
    let two = Mask::parse("[C]-[C100-10:4]").expect("valid");
    assert_eq!(two.render(&row(2)), "3-0080");
    assert_eq!(Mask::parse("[C:2]").expect("valid").render(&row(8)), "09");
    assert_eq!(Mask::parse("[C100-10]").expect("valid").render(&row(3)), "70");
}

#[test]
fn date_and_time_from_the_modified_time() {
    let file = "a.txt";
    assert_eq!(name("[YMD]", file), "20260615");
    assert_eq!(name("[Y]-[M]-[D]", file), "2026-06-15");
    assert_eq!(name("[hms]", file), "231009");
    assert_eq!(name("[y]", file), "26");
    assert_eq!(name("[d]", file), "2026-06-15");
    assert_eq!(name("[t]", file), "23.10.09", "no colons: macOS shows them as slashes");
}

#[test]
fn case_switches_apply_from_where_they_stand() {
    assert_eq!(name("[U][N]", "Annual report.pdf"), "ANNUAL REPORT");
    assert_eq!(name("[N1-6][U][N7-]", "Annual report.pdf"), "Annual REPORT");
    assert_eq!(name("[F][N]", "aNNUAL rEPORT.pdf"), "Annual Report");
    assert_eq!(name("[L][N][n] X", "ABC.pdf"), "abc X");
}

#[test]
fn literal_brackets() {
    assert_eq!(name("[[[N]]", "a.txt"), "[a]");
}

#[test]
fn a_bad_mask_says_what_is_wrong() {
    assert!(matches!(Mask::parse("[N"), Err(MaskError::Unclosed { .. })));
    assert!(matches!(Mask::parse("[Q]"), Err(MaskError::Unknown { .. })));
    assert!(matches!(Mask::parse("[N2-x]"), Err(MaskError::Unknown { .. })));
}

#[test]
fn a_trailing_dot_belongs_to_the_name() {
    assert_eq!(name("[N]", "a."), "a.");
}

#[test]
fn an_inline_width_is_capped_and_the_counter_never_overflows() {
    let row = RowFacts {
        position: 3,
        ..facts("a.txt")
    };
    let wide = Mask::parse("[C:4000000000]").expect("valid");
    assert_eq!(wide.render(&row).len(), 64);
    let huge = Mask::parse("[C9223372036854775807+9223372036854775807]").expect("valid");
    assert_eq!(huge.render(&row), i64::MAX.to_string());
}

#[test]
fn a_huge_range_length_runs_to_the_end_without_overflowing() {
    let file = "ABCDEFGHIJ.txt";
    assert_eq!(name("[N5,9223372036854775807]", file), "EFGHIJ");
    assert_eq!(
        name("[N5,18446744073709551615]", file),
        "EFGHIJ",
        "a usize past i64::MAX"
    );
    assert_eq!(name("[N-3,9223372036854775807]", file), "HIJ");
}

#[test]
fn a_negative_start_is_followed_by_its_step() {
    let row = |position| RowFacts {
        position,
        ..facts("a.txt")
    };
    let render = |mask: &str, position| Mask::parse(mask).expect("valid").render(&row(position));
    assert_eq!(render("[C-5+1]", 0), "-5");
    assert_eq!(render("[C-5+1]", 6), "1");
    assert_eq!(render("[C-5-2:3]", 1), "-07");
    // A lone leading sign is the step, as in TC.
    assert_eq!(render("[C-5]", 1), "-4");
    assert_eq!(render("[C+5]", 1), "6");
}

/// The `[C…]` vectors the frontend's `counter-token.ts` is tested against too, so
/// the two parsers can't drift.
#[test]
fn the_counter_grammar_agrees_with_the_shared_vectors() {
    let vectors: serde_json::Value =
        serde_json::from_str(include_str!("counter_token_vectors.json")).expect("the vectors are JSON");
    assert_eq!(vectors["maxDigits"], u64::from(MAX_COUNTER_DIGITS));
    let row = |position| RowFacts {
        position,
        ..facts("a.txt")
    };
    let valid = vectors["valid"].as_array().expect("a valid list");
    assert!(!valid.is_empty());
    for vector in valid {
        let token = vector["token"].as_str().expect("a token");
        let expected = (
            vector["start"].as_i64(),
            vector["step"].as_i64(),
            vector["digits"]
                .as_u64()
                .map(|d| u32::try_from(d).expect("a u32 width")),
        );
        assert_eq!(counter_token(token), Some(expected), "{token}");
        // The frontend's minimal rewrite counts the same.
        let minimal = vector["minimal"].as_str().expect("a minimal form");
        let (typed, rewritten) = (Mask::parse(token).expect(token), Mask::parse(minimal).expect(minimal));
        for position in [0, 1, 7] {
            assert_eq!(
                typed.render(&row(position)),
                rewritten.render(&row(position)),
                "{token} vs {minimal}"
            );
        }
    }
    let invalid = vectors["invalid"].as_array().expect("an invalid list");
    assert!(!invalid.is_empty());
    for token in invalid {
        let token = token.as_str().expect("a token");
        assert!(
            matches!(Mask::parse(token), Err(MaskError::Unknown { .. })),
            "{token} must not parse"
        );
    }
}
