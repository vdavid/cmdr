//! Unit tests for `unique_name.rs`, split out as a `#[path]` child so the module
//! itself stays readable. `super::` here is `unique_name`, exactly as when these
//! lived inline.

use super::*;
use tempfile::TempDir;

// ============================================================================
// find_unique_name
// ============================================================================
//
// Regression for the low-severity audit finding: pre-fix `find_unique_name`
// picked a name by exists()-checking each candidate and returning the first
// miss. Between the check and the caller's write, a concurrent process (backup
// tool, cloud-sync agent, second Cmdr op) could land a file at the same name and
// the next copy / rename would silently clobber it. The fix atomically reserves
// the chosen name via O_EXCL.

#[test]
fn reserves_the_chosen_name_on_disk() {
    let temp = TempDir::new().unwrap();
    let target = temp.path().join("notes.txt");
    fs::write(&target, b"original").unwrap();

    let unique = find_unique_name(&target);

    assert_eq!(unique.file_name().unwrap().to_string_lossy(), "notes (1).txt");
    // O_EXCL placeholder must already exist after the call.
    assert!(unique.exists(), "reservation must create the placeholder");
    // Second call goes to (2), proving the first reservation persisted.
    let next = find_unique_name(&target);
    assert_eq!(next.file_name().unwrap().to_string_lossy(), "notes (2).txt");
}

#[test]
fn keeps_extension_in_the_right_place() {
    let temp = TempDir::new().unwrap();
    let target = temp.path().join("report.pdf");
    fs::write(&target, b"x").unwrap();
    let unique = find_unique_name(&target);
    assert_eq!(unique.file_name().unwrap().to_string_lossy(), "report (1).pdf");
}

#[test]
fn handles_extensionless_filenames() {
    let temp = TempDir::new().unwrap();
    let target = temp.path().join("README");
    fs::write(&target, b"x").unwrap();
    let unique = find_unique_name(&target);
    assert_eq!(unique.file_name().unwrap().to_string_lossy(), "README (1)");
}

#[test]
fn continues_a_trailing_sequence_instead_of_nesting() {
    let temp = TempDir::new().unwrap();
    let target = temp.path().join("photo (1).jpg");
    fs::write(&target, b"x").unwrap();
    let unique = find_unique_name(&target);
    assert_eq!(unique.file_name().unwrap().to_string_lossy(), "photo (2).jpg");
}

#[test]
fn a_sequence_skips_past_every_taken_number() {
    let temp = TempDir::new().unwrap();
    let target = temp.path().join("photo (1).jpg");
    fs::write(&target, b"x").unwrap();
    fs::write(temp.path().join("photo (2).jpg"), b"x").unwrap();
    fs::write(temp.path().join("photo (3).jpg"), b"x").unwrap();
    let unique = find_unique_name(&target);
    assert_eq!(unique.file_name().unwrap().to_string_lossy(), "photo (4).jpg");
}

#[test]
fn a_non_numeric_parenthetical_is_not_a_sequence() {
    let temp = TempDir::new().unwrap();
    let target = temp.path().join("Report (final).pdf");
    fs::write(&target, b"x").unwrap();
    let unique = find_unique_name(&target);
    assert_eq!(unique.file_name().unwrap().to_string_lossy(), "Report (final) (1).pdf");
}

#[test]
fn a_number_too_big_for_u32_is_literal_text() {
    let temp = TempDir::new().unwrap();
    let target = temp.path().join("photo (99999999999).jpg");
    fs::write(&target, b"x").unwrap();
    let unique = find_unique_name(&target);
    assert_eq!(
        unique.file_name().unwrap().to_string_lossy(),
        "photo (99999999999) (1).jpg"
    );
}

#[test]
fn zero_padding_is_not_preserved() {
    let temp = TempDir::new().unwrap();
    let target = temp.path().join("photo (007).jpg");
    fs::write(&target, b"x").unwrap();
    let unique = find_unique_name(&target);
    assert_eq!(unique.file_name().unwrap().to_string_lossy(), "photo (8).jpg");
}

#[test]
fn a_zero_sequence_advances_to_one() {
    let temp = TempDir::new().unwrap();
    let target = temp.path().join("photo (0).jpg");
    fs::write(&target, b"x").unwrap();
    let unique = find_unique_name(&target);
    assert_eq!(unique.file_name().unwrap().to_string_lossy(), "photo (1).jpg");
}

// ============================================================================
// split_sequence
// ============================================================================
//
// The pure half of the ` (N)` convention: what counts as a trailing sequence and
// what is ordinary text that happens to hold parentheses.

#[test]
fn a_bare_stem_starts_at_one() {
    assert_eq!(split_sequence("photo"), ("photo", 1));
}

#[test]
fn a_trailing_number_continues_from_itself() {
    assert_eq!(split_sequence("photo (4)"), ("photo", 5));
    assert_eq!(split_sequence("photo (0)"), ("photo", 1));
    assert_eq!(split_sequence("photo (007)"), ("photo", 8));
}

#[test]
fn only_the_last_parenthetical_counts() {
    assert_eq!(split_sequence("photo (2) (3)"), ("photo (2)", 4));
    assert_eq!(split_sequence("photo (2) (draft)"), ("photo (2) (draft)", 1));
}

#[test]
fn text_in_parentheses_is_not_a_sequence() {
    assert_eq!(split_sequence("Report (final)"), ("Report (final)", 1));
    assert_eq!(split_sequence("photo ()"), ("photo ()", 1));
    assert_eq!(split_sequence("photo (12a)"), ("photo (12a)", 1));
    // `u32::from_str` accepts a leading `+`; the convention never writes one.
    assert_eq!(split_sequence("photo (+1)"), ("photo (+1)", 1));
    // Non-ASCII digits parse in no locale we generate.
    assert_eq!(split_sequence("photo (١)"), ("photo (١)", 1));
}

#[test]
fn the_separating_space_is_required() {
    assert_eq!(split_sequence("photo(1)"), ("photo(1)", 1));
    assert_eq!(split_sequence("(1)"), ("(1)", 1));
}

#[test]
fn a_number_that_cannot_advance_is_literal_text() {
    // Doesn't fit `u32` at all.
    assert_eq!(split_sequence("photo (99999999999)"), ("photo (99999999999)", 1));
    // Fits, but has no successor, so continuing the sequence is impossible.
    let at_max = format!("photo ({})", u32::MAX);
    assert_eq!(split_sequence(&at_max), (at_max.as_str(), 1));
}

// ============================================================================
// next_available_name
// ============================================================================
//
// The non-reserving sibling of `find_unique_name`: same convention, same
// sequence rule, but it only probes and never creates. Callers that reserve the
// name themselves (a directory claiming its own name with `create_dir`) need the
// probe without the placeholder a file reservation leaves behind.

#[test]
fn picks_the_next_free_name_without_creating_anything() {
    let temp = TempDir::new().unwrap();
    let target = temp.path().join("notes.txt");
    fs::write(&target, b"original").unwrap();

    let picked = next_available_name(&target, &ClaimedNames::default());

    assert_eq!(picked.file_name().unwrap().to_string_lossy(), "notes (1).txt");
    assert!(!picked.exists(), "the probe must not reserve the name");
    // Nothing was reserved on disk, so another operation answers the same.
    assert_eq!(next_available_name(&target, &ClaimedNames::default()), picked);
}

/// The ledger is what a probe can't have: nothing lands on disk between the two
/// picks, so only the record of the first keeps the second off it.
#[test]
fn a_second_pick_in_one_operation_walks_past_the_first() {
    let temp = TempDir::new().unwrap();
    let target = temp.path().join("notes.txt");
    fs::write(&target, b"original").unwrap();
    let claimed = ClaimedNames::default();

    let first = next_available_name(&target, &claimed);
    let second = next_available_name(&target, &claimed);

    assert_eq!(first.file_name().unwrap().to_string_lossy(), "notes (1).txt");
    assert_eq!(second.file_name().unwrap().to_string_lossy(), "notes (2).txt");
}

/// Two sources of one ` (N)` family, the shape that made this a bug: the second
/// source's sequence starts exactly where the first source's pick landed.
#[test]
fn two_sources_of_one_family_never_pick_the_same_name() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("photo.jpg"), b"x").unwrap();
    fs::write(temp.path().join("photo (1).jpg"), b"x").unwrap();
    let claimed = ClaimedNames::default();

    let from_plain = next_available_name(&temp.path().join("photo.jpg"), &claimed);
    let from_first = next_available_name(&temp.path().join("photo (1).jpg"), &claimed);

    assert_eq!(from_plain.file_name().unwrap().to_string_lossy(), "photo (2).jpg");
    assert_eq!(from_first.file_name().unwrap().to_string_lossy(), "photo (3).jpg");
}

/// A directory claims with `mkdir(2)`, which can't see a name only spoken for,
/// so it consults the ledger as well.
#[test]
fn a_directory_claim_walks_past_a_name_a_file_pick_spoke_for() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("notes"), b"x").unwrap();
    let claimed = ClaimedNames::default();

    let spoken_for = next_available_name(&temp.path().join("notes"), &claimed);
    let created = create_unique_dir(&temp.path().join("notes"), &claimed).unwrap();

    assert_eq!(spoken_for.file_name().unwrap().to_string_lossy(), "notes (1)");
    assert_eq!(created.file_name().unwrap().to_string_lossy(), "notes (2)");
    assert!(created.is_dir(), "the directory claim creates what it returns");
}

#[test]
fn skips_names_already_taken_and_continues_a_sequence() {
    let temp = TempDir::new().unwrap();
    let target = temp.path().join("photo (1).jpg");
    fs::write(&target, b"x").unwrap();
    fs::write(temp.path().join("photo (2).jpg"), b"x").unwrap();

    let picked = next_available_name(&target, &ClaimedNames::default());
    assert_eq!(picked.file_name().unwrap().to_string_lossy(), "photo (3).jpg");
}

#[test]
fn works_for_a_directory_source() {
    let temp = TempDir::new().unwrap();
    let target = temp.path().join("docs");
    fs::create_dir(&target).unwrap();

    let picked = next_available_name(&target, &ClaimedNames::default());
    assert_eq!(picked.file_name().unwrap().to_string_lossy(), "docs (1)");
    assert!(!picked.exists());
}

#[test]
fn a_dangling_symlink_counts_as_taken() {
    // `Path::exists()` follows symlinks and reports `false` for a broken
    // one; handing that name back would let the caller's write follow the
    // symlink to wherever it points.
    let temp = TempDir::new().unwrap();
    let target = temp.path().join("notes.txt");
    fs::write(&target, b"x").unwrap();
    std::os::unix::fs::symlink(temp.path().join("gone"), temp.path().join("notes (1).txt")).unwrap();

    let picked = next_available_name(&target, &ClaimedNames::default());
    assert_eq!(picked.file_name().unwrap().to_string_lossy(), "notes (2).txt");
}
