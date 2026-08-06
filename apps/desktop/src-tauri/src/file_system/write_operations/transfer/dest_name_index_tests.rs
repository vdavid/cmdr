//! The matching rule, on its own.
//!
//! `volume/copy_precheck_tests.rs` proves the driver honours these
//! answers end to end against a case- and normalization-insensitive
//! destination; this file pins the rule itself, including the cases that must
//! concede a probe rather than claim a name is free.

use super::*;

fn listing(names: &[&str]) -> DestNameIndex {
    DestNameIndex::build(
        names
            .iter()
            .map(|name| FileEntry::new((*name).to_string(), format!("/inbox/{name}"), false, false))
            .collect(),
    )
}

fn lookup(index: &DestNameIndex, name: &str) -> DestLookup {
    index.lookup(Some(OsStr::new(name)))
}

#[test]
fn an_exact_name_is_answered_from_the_listing() {
    let index = listing(&["notes.txt", "photo.jpg"]);
    let DestLookup::Present(entry) = lookup(&index, "notes.txt") else {
        panic!("a name the listing holds exactly needs no round trip");
    };
    assert_eq!(entry.name, "notes.txt");
    assert!(!entry.is_directory, "and it carries what the probe would have said");
}

#[test]
fn a_name_nothing_can_resolve_to_is_free() {
    let index = listing(&["notes.txt", "photo.jpg"]);
    assert!(
        matches!(lookup(&index, "fresh.txt"), DestLookup::Absent),
        "this is the whole win: no round trip for a name nothing collides with"
    );
}

/// ❌ SMB shares and macOS volumes are typically case-INsensitive, so
/// `get_metadata("notes.txt")` finds a stored `Notes.TXT`. Claiming the name is
/// free here overwrites a file the user would have been prompted about.
#[test]
fn a_case_difference_is_the_backends_call_not_ours() {
    let index = listing(&["Notes.TXT"]);
    assert!(matches!(lookup(&index, "notes.txt"), DestLookup::Unknown));
    assert!(matches!(lookup(&index, "NOTES.txt"), DestLookup::Unknown));
}

/// ❌ macOS and SMB move paths between NFC and NFD, so one user-visible name is
/// two byte strings. A byte-exact key misses what the backend would find.
#[test]
fn a_normalization_difference_is_the_backends_call_not_ours() {
    // Stored composed, asked decomposed, and the other way round.
    assert!(matches!(
        lookup(&listing(&["caf\u{e9}.txt"]), "cafe\u{301}.txt"),
        DestLookup::Unknown
    ));
    assert!(matches!(
        lookup(&listing(&["cafe\u{301}.txt"]), "caf\u{e9}.txt"),
        DestLookup::Unknown
    ));
}

#[test]
fn case_and_normalization_differences_compound() {
    let index = listing(&["CAF\u{c9}.TXT"]);
    assert!(matches!(lookup(&index, "cafe\u{301}.txt"), DestLookup::Unknown));
}

/// A byte-exact hit still wins inside a bucket that also holds a fold-only
/// sibling — a case-SENSITIVE destination legitimately holds both spellings,
/// and the exact one is the file this copy is about to land on.
#[test]
fn an_exact_hit_wins_over_a_fold_only_sibling_in_the_same_bucket() {
    let index = listing(&["Notes.TXT", "notes.txt"]);
    let DestLookup::Present(entry) = lookup(&index, "notes.txt") else {
        panic!("the exact name is in the listing");
    };
    assert_eq!(entry.name, "notes.txt");
}

/// Win32 path canonicalization strips trailing dots and spaces from the
/// request, so a Windows-hosted share resolves `report.` onto a stored
/// `report`.
#[test]
fn a_trailing_dot_or_space_may_canonicalize_onto_a_stored_name() {
    let index = listing(&["report", "notes.txt"]);
    assert!(matches!(lookup(&index, "report."), DestLookup::Unknown));
    assert!(matches!(lookup(&index, "report "), DestLookup::Unknown));
    assert!(matches!(lookup(&index, "REPORT.."), DestLookup::Unknown));
    // Nothing to canonicalize onto: still free.
    assert!(matches!(lookup(&index, "summary."), DestLookup::Absent));
    // A name that merely CONTAINS a dot is the ordinary case and must stay fast.
    assert!(matches!(lookup(&index, "other.txt"), DestLookup::Absent));
}

/// 8.3 short names are a generated second name for an entry the listing reports
/// under its real one, and they can't be enumerated — so a `~` name can't be
/// proven free.
#[test]
fn a_name_that_could_be_an_8_3_alias_concedes_the_probe() {
    let index = listing(&["Program Files"]);
    assert!(matches!(lookup(&index, "PROGRA~1"), DestLookup::Unknown));
}

/// A source path with no final component targets the destination directory
/// itself, which this index doesn't describe.
#[test]
fn a_name_this_index_cannot_describe_is_unknown() {
    let index = listing(&["notes.txt"]);
    assert!(matches!(index.lookup(None), DestLookup::Unknown));
}

#[test]
fn an_empty_destination_still_answers_absent() {
    let index = listing(&[]);
    assert!(matches!(lookup(&index, "notes.txt"), DestLookup::Absent));
}

#[test]
fn folding_ascii_and_folding_through_the_normalizer_agree() {
    // The ASCII fast path is an optimization, not a second rule.
    for name in ["Notes.TXT", "a-b_c 1.txt", "UPPER", ""] {
        assert_eq!(
            fold(name),
            name.nfc().flat_map(char::to_lowercase).collect::<String>(),
            "the ASCII shortcut must answer what the general path answers"
        );
    }
}
