//! The rename mask against Total Commander's own examples (help file and wiki).

use chrono::{NaiveDate, NaiveDateTime};
use cmdr_media_meta::{FieldKind, MediaFacts, MediaField};

use super::mask::{
    FieldGap, FieldText, MAX_COUNTER_DIGITS, Mask, MaskError, MediaToken, RangeToken, RowFacts, RowMedia,
    counter_token, media_token_parts, range_token, range_token_bounds,
};

/// A mask without media fields always renders.
trait Text {
    fn text(&self, row: &RowFacts<'_>) -> String;
}

impl Text for Mask {
    fn text(&self, row: &RowFacts<'_>) -> String {
        self.render(row).expect("no media field, so no gap")
    }
}

fn facts(name: &str) -> RowFacts<'_> {
    RowFacts {
        file_name: name,
        is_directory: false,
        parent: "Holiday 2026",
        grandparent: "Photos",
        modified: NaiveDate::from_ymd_opt(2026, 6, 15).and_then(|d| d.and_hms_opt(23, 10, 9)),
        position: 0,
        media: RowMedia::NotMedia,
    }
}

/// Renders `mask` as the NAME mask of `file` (the extension mask is a separate field).
fn name(mask: &str, file: &str) -> String {
    Mask::parse(mask).expect("a valid mask").text(&facts(file))
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

/// The `[N]` tooltip's forms (`placeholder-help.ts`), each between what the field keeps around it.
#[test]
fn the_name_tooltip_forms_take_what_their_help_says() {
    let file = "Beach day.jpg";
    assert_eq!(name("[N1-2]<[N3]>[N4-]", file), "Be<a>ch day");
    assert_eq!(name("[N1]<[N2-5]>[N6-]", file), "B<each> day");
    assert_eq!(name("<[N1--2]>[N-1-]", file), "<Beach da>y", "up to the second-to-last");
    assert_eq!(name("[N1-6]<[N7,3]>[N10-]", file), "Beach <day>", "three from the 7th");
    assert_eq!(name("[N1--4]<[N-3,3]>", file), "Beach <day>", "the last three");
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
    assert_eq!(mask.text(&folder), "archive.2024|");
}

#[test]
fn a_bare_counter_counts_one_two_three_and_each_counter_has_its_own_settings() {
    let mask = Mask::parse("[C]").expect("valid");
    let row = |position| RowFacts {
        position,
        ..facts("a.txt")
    };
    assert_eq!(mask.text(&row(0)), "1");
    assert_eq!(mask.text(&row(11)), "12", "no padding to the batch size");

    let inline = Mask::parse("[C10+5:3]").expect("valid");
    assert_eq!(inline.text(&row(0)), "010");
    assert_eq!(inline.text(&row(2)), "020");
    let two = Mask::parse("[C]-[C100-10:4]").expect("valid");
    assert_eq!(two.text(&row(2)), "3-0080");
    assert_eq!(Mask::parse("[C:2]").expect("valid").text(&row(8)), "09");
    assert_eq!(Mask::parse("[C100-10]").expect("valid").text(&row(3)), "70");
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
fn a_date_placeholder_keeps_dashes_dots_underscores_and_spaces_between_its_parts() {
    let file = "a.txt";
    assert_eq!(name("[Y-M-D h.m.s]", file), "2026-06-15 23.10.09");
    assert_eq!(name("[D.M.Y]", file), "15.06.2026");
    assert_eq!(name("[y_M_D]", file), "26_06_15");
    assert_eq!(name("[hm-]", file), "2310-", "a separator may end it");
    assert_eq!(name("[M-m]", file), "06-10", "case still tells month from minute");
}

#[test]
fn a_date_with_separators_leaves_the_rest_of_the_grammar_alone() {
    let file = "ABCDEFGHIJ.txt";
    assert_eq!(
        name("[2-5]", file),
        "BCDE",
        "a leading digit is still a full-name range"
    );
    assert_eq!(name("[-3-]", file), "txt", "a leading dash too");
    assert_eq!(name("[N2-5]", file), "BCDE");
    assert_eq!(name("[d]", file), "2026-06-15");
    assert_eq!(name("[t]", file), "23.10.09");
    for bad in ["[Y:M]", "[Y/M]", "[-Y]", "[.Y]", "[ Y]", "[Y-d]", "[t.Y]", "[Y-D-x]"] {
        assert!(
            matches!(Mask::parse(bad), Err(MaskError::Unknown { .. })),
            "{bad} isn't a date"
        );
    }
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
    assert_eq!(wide.text(&row).len(), 64);
    let huge = Mask::parse("[C9223372036854775807+9223372036854775807]").expect("valid");
    assert_eq!(huge.text(&row), i64::MAX.to_string());
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
    let render = |mask: &str, position| Mask::parse(mask).expect("valid").text(&row(position));
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
                typed.text(&row(position)),
                rewritten.text(&row(position)),
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

/// The range vectors the frontend's `range-token.ts` is tested against too, so the
/// two grammars can't drift, and the editor's tokens take what the user chose.
#[test]
fn the_range_grammar_agrees_with_the_shared_vectors() {
    let vectors: serde_json::Value =
        serde_json::from_str(include_str!("range_token_vectors.json")).expect("the vectors are JSON");
    let valid = vectors["valid"].as_array().expect("a valid list");
    assert!(!valid.is_empty());
    for vector in valid {
        let token = vector["token"].as_str().expect("a token");
        let expected = (
            vector["field"].as_str().expect("a field"),
            vector["kind"].as_str().expect("a kind"),
            vector["at"].as_i64(),
            vector["to"].as_i64(),
            vector["length"].as_u64(),
        );
        assert_eq!(range_token(token), Some(expected), "{token}");
        // The frontend's minimal rewrite takes the same characters, from names short and long.
        let minimal = vector["minimal"].as_str().expect("a minimal form");
        let (typed, rewritten) = (Mask::parse(token).expect(token), Mask::parse(minimal).expect(minimal));
        for file in ["a", "Beach day.jpg", "ABCDEFGHIJKLMNOP.txt", ""] {
            assert_eq!(
                typed.text(&facts(file)),
                rewritten.text(&facts(file)),
                "{token} vs {minimal} on {file:?}"
            );
        }
    }
    for token in vectors["invalid"].as_array().expect("an invalid list") {
        let token = token.as_str().expect("a token");
        assert!(
            matches!(Mask::parse(token), Err(MaskError::Unknown { .. })),
            "{token} must not parse"
        );
    }
    for token in vectors["notRanges"].as_array().expect("a not-ranges list") {
        let token = token.as_str().expect("a token");
        assert!(Mask::parse(token).is_ok(), "must parse: {token}");
        assert_eq!(range_token(token), None, "{token} isn't a range");
    }
    let selections = vectors["selections"].as_array().expect("a selections list");
    assert!(!selections.is_empty());
    for selection in selections {
        let token = selection["token"].as_str().expect("a token");
        let length = usize::try_from(selection["length"].as_u64().expect("a length")).expect("a usize");
        let first = usize::try_from(selection["first"].as_u64().expect("a first")).expect("a usize");
        let last = usize::try_from(selection["last"].as_u64().expect("a last")).expect("a usize");
        assert_eq!(
            range_token_bounds(token, length),
            Some((first, last)),
            "{token}, length {length}"
        );
    }
}

#[test]
fn a_range_token_reads_its_field_and_says_which_characters_it_takes() {
    let row = facts("Beach day.jpg");
    let take = |inner: &str| {
        let range = RangeToken::parse(inner).expect(inner);
        let source = range.source(&row);
        (source, range.bounds(source.chars().count()))
    };
    assert_eq!(take("N2-5"), ("Beach day", Some((1, 4))));
    assert_eq!(take("E"), ("jpg", Some((0, 2))));
    assert_eq!(take("P-4-"), ("Holiday 2026", Some((8, 11))));
    assert_eq!(take("G1"), ("Photos", Some((0, 0))));
    assert_eq!(take("-3-"), ("Beach day.jpg", Some((10, 12))));
    // Past the end, it takes nothing.
    assert_eq!(take("N20-25"), ("Beach day", None));
    assert_eq!(take("N0"), ("Beach day", None));
    // Characters, never bytes.
    let accented = facts("Érdekes.txt");
    let range = RangeToken::parse("N2-3").expect("a range");
    assert_eq!(range.source(&accented), "Érdekes");
    assert_eq!(range.bounds(7), Some((1, 2)));
    // Another placeholder isn't a range.
    assert_eq!(RangeToken::parse("C"), None);
    assert_eq!(RangeToken::parse("n"), None);
}

fn at(y: i32, mo: u32, d: u32, h: u32, mi: u32, s: u32) -> Option<NaiveDateTime> {
    NaiveDate::from_ymd_opt(y, mo, d).and_then(|date| date.and_hms_opt(h, mi, s))
}

/// A phone photo that's also, improbably, tagged like a song: every kind of field.
fn media() -> MediaFacts {
    MediaFacts {
        taken: at(2026, 7, 14, 9, 5, 30),
        make: Some("Apple".to_string()),
        model: Some("iPhone 15 Pro".to_string()),
        iso: Some(64),
        width: Some(4032),
        artist: Some("Daft Punk".to_string()),
        title: Some("Digital Love".to_string()),
        track: Some(3),
        ..MediaFacts::default()
    }
}

/// `mask` as a name mask for a file whose media facts are `media` (modified 2026-06-15 23:10:09).
fn render(mask: &str, media: RowMedia<'_>) -> Result<String, FieldGap> {
    Mask::parse(mask).expect("a valid mask").render(&RowFacts {
        media,
        ..facts("IMG_0042.jpg")
    })
}

#[test]
fn a_date_field_takes_the_date_letters_and_reads_as_date_and_time_without_them() {
    let photo = media();
    let read = RowMedia::Read(&photo);
    assert_eq!(render("[=exif.taken]", read).as_deref(), Ok("2026-07-14 09.05.30"));
    assert_eq!(render("[=exif.taken:d]", read).as_deref(), Ok("2026-07-14"));
    assert_eq!(render("[=exif.taken:t]", read).as_deref(), Ok("09.05.30"));
    assert_eq!(
        render("[=exif.taken:Y-M-D h.m.s]", read).as_deref(),
        Ok("2026-07-14 09.05.30")
    );
    assert_eq!(
        render("[=exif.taken:YMD_hms] [N]", read).as_deref(),
        Ok("20260714_090530 IMG_0042")
    );
}

#[test]
fn a_number_field_pads_to_its_digits_and_never_cuts() {
    let photo = media();
    let read = RowMedia::Read(&photo);
    assert_eq!(render("[=tag.track]", read).as_deref(), Ok("3"));
    assert_eq!(render("[=tag.track:2]", read).as_deref(), Ok("03"));
    assert_eq!(render("[=exif.iso:4]", read).as_deref(), Ok("0064"));
    assert_eq!(render("[=exif.width:2]", read).as_deref(), Ok("4032"));
}

#[test]
fn a_text_field_takes_a_range_like_the_name_does() {
    let photo = media();
    let read = RowMedia::Read(&photo);
    assert_eq!(render("[=tag.artist]", read).as_deref(), Ok("Daft Punk"));
    assert_eq!(render("[=tag.artist:1-4]", read).as_deref(), Ok("Daft"));
    assert_eq!(render("[=tag.artist:-4-]", read).as_deref(), Ok("Punk"));
    assert_eq!(
        render("[=tag.artist:20-30]", read).as_deref(),
        Ok(""),
        "past the end is empty"
    );
    assert_eq!(render("[=exif.camera]", read).as_deref(), Ok("Apple iPhone 15 Pro"));
    assert_eq!(
        render("[=tag.track:2] [=tag.artist] - [=tag.title]", read).as_deref(),
        Ok("03 Daft Punk - Digital Love")
    );
}

#[test]
fn a_field_goes_through_the_case_switches_like_any_text() {
    let photo = media();
    assert_eq!(
        render("[U][=tag.artist][n] [=tag.title]", RowMedia::Read(&photo)).as_deref(),
        Ok("DAFT PUNK Digital Love")
    );
}

#[test]
fn a_field_without_a_value_is_a_gap_that_names_it_never_an_empty_string() {
    let empty = MediaFacts::default();
    assert_eq!(
        render("[N] [=exif.taken:d]", RowMedia::Read(&empty)),
        Err(FieldGap::NoValue {
            field: MediaField::ExifTaken
        })
    );
    assert_eq!(
        render("[=tag.artist]", RowMedia::NotMedia),
        Err(FieldGap::NoValue {
            field: MediaField::TagArtist
        }),
        "a file we don't read has no tags"
    );
    assert_eq!(
        render("[=tag.artist]", RowMedia::Unreadable),
        Err(FieldGap::Unreadable {
            field: MediaField::TagArtist
        })
    );
    assert_eq!(render("[=tag.artist]", RowMedia::Reading), Err(FieldGap::Reading));
}

#[test]
fn the_first_gap_wins_unless_a_value_is_still_coming() {
    let photo = MediaFacts {
        taken: at(2026, 7, 14, 9, 5, 30),
        ..MediaFacts::default()
    };
    assert_eq!(
        render("[=exif.taken][=tag.title][=tag.album]", RowMedia::Read(&photo)),
        Err(FieldGap::NoValue {
            field: MediaField::TagTitle
        })
    );
    assert_eq!(
        render("[=exif.taken][=tag.title]", RowMedia::Reading),
        Err(FieldGap::Reading)
    );
}

#[test]
fn modified_stands_in_for_a_missing_date_but_not_for_one_still_being_read() {
    let empty = MediaFacts::default();
    let photo = media();
    let mask = "[=exif.taken|modified:d]";
    assert_eq!(
        render(mask, RowMedia::Read(&photo)).as_deref(),
        Ok("2026-07-14"),
        "its own date first"
    );
    assert_eq!(render(mask, RowMedia::Read(&empty)).as_deref(), Ok("2026-06-15"));
    assert_eq!(render(mask, RowMedia::NotMedia).as_deref(), Ok("2026-06-15"));
    assert_eq!(render(mask, RowMedia::Unreadable).as_deref(), Ok("2026-06-15"));
    assert_eq!(
        render(mask, RowMedia::Reading),
        Err(FieldGap::Reading),
        "its own date may yet come"
    );
    assert_eq!(
        render("[=exif.taken|modified]", RowMedia::NotMedia).as_deref(),
        Ok("2026-06-15 23.10.09")
    );
    let undated = RowFacts {
        modified: None,
        media: RowMedia::NotMedia,
        ..facts("a.txt")
    };
    assert_eq!(
        Mask::parse(mask).expect("valid").render(&undated),
        Err(FieldGap::NoValue {
            field: MediaField::ExifTaken
        })
    );
}

#[test]
fn a_media_token_alone_says_its_text_and_whether_it_fell_back() {
    let photo = media();
    let token = MediaToken::parse("=exif.taken|modified:d").expect("a media token");
    assert_eq!(token.field(), MediaField::ExifTaken);
    let row = |media| RowFacts {
        media,
        ..facts("IMG_0042.jpg")
    };
    assert_eq!(
        token.resolve(&row(RowMedia::Read(&photo))),
        Ok(FieldText::Value("2026-07-14".to_string()))
    );
    assert_eq!(
        token.resolve(&row(RowMedia::NotMedia)),
        Ok(FieldText::Fallback("2026-06-15".to_string()))
    );
    assert_eq!(token.resolve(&row(RowMedia::Reading)), Err(FieldGap::Reading));
    // The default format, for the picker's examples.
    assert_eq!(
        MediaToken::of(MediaField::ExifTaken).value_text(&photo).as_deref(),
        Some("2026-07-14 09.05.30")
    );
    assert_eq!(
        MediaToken::of(MediaField::TagTrack).value_text(&photo).as_deref(),
        Some("3")
    );
    assert_eq!(MediaToken::of(MediaField::TagAlbum).value_text(&photo), None);
    assert_eq!(MediaToken::parse("N2-5"), None);
    assert_eq!(MediaToken::parse("=exif.nope"), None);
}

/// The media vectors the frontend's `media-token.ts` is tested against too, so the
/// two grammars can't drift.
#[test]
fn the_media_grammar_agrees_with_the_shared_vectors() {
    let vectors: serde_json::Value =
        serde_json::from_str(include_str!("media_token_vectors.json")).expect("the vectors are JSON");
    assert_eq!(vectors["maxDigits"], u64::from(MAX_COUNTER_DIGITS));
    let fields: Vec<serde_json::Value> = MediaField::ALL
        .into_iter()
        .map(|field| {
            let kind = match field.kind() {
                FieldKind::Date => "date",
                FieldKind::Number => "number",
                FieldKind::Text => "text",
            };
            serde_json::json!({ "name": field.name(), "wire": field, "kind": kind })
        })
        .collect();
    assert_eq!(vectors["fields"], serde_json::Value::Array(fields));

    let photo = media();
    let sample_media = [
        RowMedia::Read(&photo),
        RowMedia::NotMedia,
        RowMedia::Unreadable,
        RowMedia::Reading,
    ];
    let valid = vectors["valid"].as_array().expect("a valid list");
    assert!(!valid.is_empty());
    for vector in valid {
        let token = vector["token"].as_str().expect("a token");
        let expected = serde_json::json!({
            "field": vector["field"],
            "fallback": vector["fallback"],
            "format": vector["format"],
        });
        assert_eq!(media_token_parts(token), Some(expected), "{token}");
        // The frontend's minimal rewrite renders the same.
        let minimal = vector["minimal"].as_str().expect("a minimal form");
        let (typed, rewritten) = (Mask::parse(token).expect(token), Mask::parse(minimal).expect(minimal));
        for media in sample_media {
            let row = RowFacts {
                media,
                ..facts("a.jpg")
            };
            assert_eq!(typed.render(&row), rewritten.render(&row), "{token} vs {minimal}");
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
