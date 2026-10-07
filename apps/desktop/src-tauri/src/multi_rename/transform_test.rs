//! Search & replace, the case step, and removing diacritics.

use super::transform::{CaseChange, Replace, ReplaceError, Transform, remove_diacritics};

fn replace(search: &str, with: &str) -> Replace {
    Replace {
        search: search.to_string(),
        replace: with.to_string(),
        ..Replace::default()
    }
}

fn run(transform: &Transform, name: &str, ext: &str) -> (String, String) {
    transform.apply(name, ext).expect("a valid transform")
}

fn only(replace: Replace) -> Transform {
    Transform {
        replace: Some(replace),
        ..Transform::default()
    }
}

#[test]
fn plain_search_is_case_insensitive_and_replaces_every_match() {
    let t = only(replace("photo", "img"));
    assert_eq!(run(&t, "Photo photo PHOTO", "jpg").0, "img img img");
}

#[test]
fn case_sensitive_and_first_only() {
    let t = only(Replace {
        case_sensitive: true,
        first_only: true,
        ..replace("a", "_")
    });
    assert_eq!(run(&t, "aAaA", "txt").0, "_AaA");
}

#[test]
fn wildcards_match_any_run_and_one_character() {
    assert_eq!(run(&only(replace("IMG_*_", "")), "IMG_2026_0001", "jpg").0, "0001");
    assert_eq!(
        run(&only(replace("v?", "v2")), "report v1 final", "pdf").0,
        "report v2 final"
    );
}

#[test]
fn parallel_lists_replace_each_with_its_pair() {
    let t = only(replace("ä|ö|ü", "ae|oe|ue"));
    assert_eq!(run(&t, "Grüße aus Köln", "txt").0, "Grueße aus Koeln");
}

#[test]
fn parallel_lists_replace_in_one_pass() {
    assert_eq!(run(&only(replace("a|b", "b|c")), "ab", "txt").0, "bc", "not cc");
    assert_eq!(run(&only(replace("one|two", "two|one")), "one two", "txt").0, "two one");
}

#[test]
fn the_extension_is_left_alone_unless_asked() {
    let t = only(replace("jpeg", "jpg"));
    assert_eq!(run(&t, "photo", "jpeg"), ("photo".to_string(), "jpeg".to_string()));
    let with_ext = only(Replace {
        include_extension: true,
        ..replace("jpeg", "jpg")
    });
    assert_eq!(
        run(&with_ext, "photo", "jpeg"),
        ("photo".to_string(), "jpg".to_string())
    );
}

#[test]
fn regex_with_groups() {
    let t = only(Replace {
        regex: true,
        ..replace(r"(\d{4})-(\d{2})-(\d{2})", "$3.$2.$1")
    });
    assert_eq!(run(&t, "scan 2026-06-15", "pdf").0, "scan 15.06.2026");
}

#[test]
fn substitute_replaces_the_whole_name_with_the_replacement() {
    let t = only(Replace {
        regex: true,
        substitute: true,
        ..replace(r"^IMG_(\d+).*$", "photo $1")
    });
    assert_eq!(run(&t, "IMG_0042 edited copy", "jpg").0, "photo 0042");
    assert_eq!(run(&t, "holiday", "jpg").0, "holiday", "no match leaves the name");
}

#[test]
fn a_broken_regex_is_an_error_not_a_panic() {
    let t = only(Replace {
        regex: true,
        ..replace("(unclosed", "x")
    });
    assert!(matches!(t.apply("a", "b"), Err(ReplaceError::BadRegex { .. })));
}

#[test]
fn the_case_step_runs_after_replace() {
    let t = Transform {
        replace: Some(replace("draft", "final")),
        case: CaseChange::Upper,
        ..Transform::default()
    };
    assert_eq!(run(&t, "report draft", "pdf").0, "REPORT FINAL");

    let words = Transform {
        case: CaseChange::Words,
        ..Transform::default()
    };
    assert_eq!(run(&words, "the QUICK brown-fox", "txt").0, "The Quick Brown-Fox");

    let lower = Transform {
        case: CaseChange::Lower,
        ..Transform::default()
    };
    assert_eq!(
        run(&lower, "MIXED Case", "JPG"),
        ("mixed case".to_string(), "jpg".to_string())
    );

    let first = Transform {
        case: CaseChange::FirstUpper,
        ..Transform::default()
    };
    assert_eq!(run(&first, "hELLO world", "txt").0, "Hello world");
}

#[test]
fn diacritics_go_in_every_language() {
    assert_eq!(
        remove_diacritics("Příliš žluťoučký kůň úpěl ďábelské ódy"),
        "Prilis zlutoucky kun upel dabelske ody"
    );
    assert_eq!(remove_diacritics("Ľubomír Šťastný"), "Lubomir Stastny");
    assert_eq!(
        remove_diacritics("Łódź Straße Ærøskøbing Œuvre Đorđe"),
        "Lodz Strasse Aeroskobing Oeuvre Dorde"
    );
    assert_eq!(
        remove_diacritics("Crème brûlée naïve façade"),
        "Creme brulee naive facade"
    );
    assert_eq!(remove_diacritics("plain ascii 123"), "plain ascii 123");
}

#[test]
fn removing_diacritics_touches_name_and_extension() {
    let t = Transform {
        remove_diacritics: true,
        ..Transform::default()
    };
    assert_eq!(
        run(&t, "Žádost", "přílohá"),
        ("Zadost".to_string(), "priloha".to_string())
    );
}

#[test]
fn a_single_search_takes_its_replacement_literally() {
    assert_eq!(run(&only(replace("x", "a|b")), "x", "txt").0, "a|b");
}

#[test]
fn a_list_of_nothing_searches_for_nothing() {
    let t = only(Replace {
        substitute: true,
        ..replace("|", "z")
    });
    assert_eq!(run(&t, "keep", "txt").0, "keep");
}

#[test]
fn a_trailing_star_runs_to_the_end() {
    assert_eq!(
        run(&only(replace("draft*", "final")), "report draft v2", "pdf").0,
        "report final"
    );
}

#[test]
fn a_composed_search_finds_a_decomposed_name() {
    assert_eq!(run(&only(replace("\u{e9}", "e")), "cafe\u{301}", "txt").0, "cafe");
}

#[test]
fn other_scripts_keep_their_marks() {
    assert_eq!(remove_diacritics("がぱ"), "がぱ", "kana dakuten stay");
    assert_eq!(
        remove_diacritics("Йошкар-Ола ёлка"),
        "Йошкар-Ола ёлка",
        "Cyrillic й and ё stay"
    );
    assert_eq!(remove_diacritics("हिन्दी"), "हिन्दी", "Devanagari vowel signs stay");
    assert_eq!(remove_diacritics("Ελληνικά άέ"), "Ελληνικα αε", "Greek accents go");
}
