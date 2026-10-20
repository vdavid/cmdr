use super::*;
use crate::test_support::TestDir;

fn tag(name: &str, color: u8) -> TagRef {
    TagRef {
        name: name.to_string(),
        color,
    }
}

fn temp_dir(name: &str) -> TestDir {
    TestDir::new(&format!("tags_write_{}_{name}", std::process::id()))
}

fn path_str(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

#[test]
fn set_tags_round_trips_through_read_tags() {
    let dir = temp_dir("round");
    let file = dir.join("f.txt");
    std::fs::write(&file, b"x").unwrap();

    // Single color, and the raw xattr really is a binary plist.
    set_tags(&file, &[tag("Red", 6)]).unwrap();
    assert_eq!(read_tags(&file), vec![tag("Red", 6)]);
    let raw = xattr::get(&file, TAGS_XATTR).unwrap().expect("xattr present");
    assert!(raw.starts_with(b"bplist00"), "raw xattr must decode as binary plist");

    // Multiple, including a colorless named tag.
    let many = vec![tag("Important", 0), tag("Red", 6), tag("Blue", 4)];
    set_tags(&file, &many).unwrap();
    assert_eq!(read_tags(&file), many);

    // Clear removes the xattr entirely (matches Finder).
    set_tags(&file, &[]).unwrap();
    assert_eq!(read_tags(&file), Vec::<TagRef>::new());
    assert!(
        xattr::get(&file, TAGS_XATTR).unwrap().is_none(),
        "clearing all tags removes the xattr"
    );

    // Clearing an already-untagged file is a clean no-op (no spurious ENOATTR).
    set_tags(&file, &[]).unwrap();

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn toggle_adds_color_preserving_existing_unrelated_tags() {
    let dir = temp_dir("add");
    let file = dir.join("f.txt");
    std::fs::write(&file, b"x").unwrap();
    set_tags(&file, &[tag("Important", 0)]).unwrap();

    let result = toggle_color(&[path_str(&file)], 6).unwrap();
    let expected = vec![tag("Important", 0), tag("Red", 6)];
    assert_eq!(read_tags(&file), expected, "red added, colorless tag preserved");
    assert_eq!(result.updates, vec![(path_str(&file), expected)]);
    assert!(result.skipped.is_empty());

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn toggle_removes_only_the_targeted_color() {
    let dir = temp_dir("remove");
    let file = dir.join("f.txt");
    std::fs::write(&file, b"x").unwrap();
    set_tags(&file, &[tag("Blue", 4), tag("Red", 6)]).unwrap();

    // File already carries red → toggling red strips it, keeps blue.
    toggle_color(&[path_str(&file)], 6).unwrap();
    assert_eq!(read_tags(&file), vec![tag("Blue", 4)]);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn toggle_multi_file_all_have_removes_from_all() {
    let dir = temp_dir("multi_all");
    let a = dir.join("a.txt");
    let b = dir.join("b.txt");
    std::fs::write(&a, b"x").unwrap();
    std::fs::write(&b, b"x").unwrap();
    set_tags(&a, &[tag("Red", 6)]).unwrap();
    set_tags(&b, &[tag("Green", 2), tag("Red", 6)]).unwrap();

    // Both already red → remove from both; b keeps green.
    toggle_color(&[path_str(&a), path_str(&b)], 6).unwrap();
    assert_eq!(read_tags(&a), Vec::<TagRef>::new());
    assert_eq!(read_tags(&b), vec![tag("Green", 2)]);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn toggle_multi_file_some_have_adds_to_all_without_duplicating() {
    let dir = temp_dir("multi_some");
    let a = dir.join("a.txt");
    let b = dir.join("b.txt");
    std::fs::write(&a, b"x").unwrap();
    std::fs::write(&b, b"x").unwrap();
    // `a` already has a CUSTOM red tag; `b` has none.
    set_tags(&a, &[tag("Important", 6)]).unwrap();

    // Not all have red → add to all. `a` keeps its custom red (no system "Red"
    // duplicate); `b` gains the canonical "Red".
    toggle_color(&[path_str(&a), path_str(&b)], 6).unwrap();
    assert_eq!(read_tags(&a), vec![tag("Important", 6)], "no duplicate same-color tag");
    assert_eq!(read_tags(&b), vec![tag("Red", 6)]);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn toggle_rejects_colorless_index() {
    let dir = temp_dir("reject");
    let file = dir.join("f.txt");
    std::fs::write(&file, b"x").unwrap();
    assert!(toggle_color(&[path_str(&file)], 0).is_err());
    assert!(toggle_color(&[path_str(&file)], 8).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

/// A binary plist array holding one tag string and one integer: Finder never
/// writes this, but another tool can, and the lenient reader drops the integer.
fn plist_with_non_string_element() -> Vec<u8> {
    let mut buf = Vec::new();
    plist::Value::Array(vec![
        plist::Value::String("Work\n0".into()),
        plist::Value::Integer(42.into()),
    ])
    .to_writer_binary(&mut buf)
    .expect("an in-memory plist write can't fail");
    buf
}

/// #541 regression: a value Cmdr can't fully decode must never read as "no tags"
/// on the write path, or the toggle replaces every existing tag with one color.
#[test]
fn toggle_skips_a_file_whose_tags_are_malformed_and_leaves_its_bytes_alone() {
    let dir = temp_dir("malformed");
    let file = dir.join("f.txt");
    std::fs::write(&file, b"x").unwrap();
    let garbage = b"not a plist at all".to_vec();
    xattr::set(&file, TAGS_XATTR, &garbage).unwrap();

    let outcome = toggle_color(&[path_str(&file)], 6).unwrap();

    let raw = xattr::get(&file, TAGS_XATTR).unwrap().expect("xattr still present");
    assert_eq!(raw, garbage, "the original xattr bytes must be unchanged");
    assert!(outcome.updates.is_empty());
    assert_eq!(outcome.skipped.len(), 1);
    assert_eq!(outcome.skipped[0].path, path_str(&file));
    assert!(matches!(outcome.skipped[0].reason, TagReadError::Undecodable));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn toggle_skips_a_file_with_a_non_string_tag_element() {
    let dir = temp_dir("non_string");
    let file = dir.join("f.txt");
    std::fs::write(&file, b"x").unwrap();
    let original = plist_with_non_string_element();
    xattr::set(&file, TAGS_XATTR, &original).unwrap();

    let outcome = toggle_color(&[path_str(&file)], 6).unwrap();

    let raw = xattr::get(&file, TAGS_XATTR).unwrap().expect("xattr still present");
    assert_eq!(raw, original, "the non-string element must survive");
    assert_eq!(outcome.skipped.len(), 1);
    assert!(matches!(outcome.skipped[0].reason, TagReadError::Undecodable));
    let _ = std::fs::remove_dir_all(&dir);
}

/// Write permission without read: `getxattr` fails, `setxattr` would succeed.
#[test]
fn toggle_skips_a_write_only_file() {
    use std::os::unix::fs::PermissionsExt;
    let dir = temp_dir("write_only");
    let file = dir.join("f.txt");
    std::fs::write(&file, b"x").unwrap();
    set_tags(&file, &[tag("Important", 0), tag("Blue", 4)]).unwrap();
    let original = xattr::get(&file, TAGS_XATTR).unwrap().expect("xattr present");
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o200)).unwrap();
    if xattr::get(&file, TAGS_XATTR).is_ok() {
        // Running as root (or a filesystem that ignores the mode): no unreadable state to test.
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        return;
    }

    let outcome = toggle_color(&[path_str(&file)], 6).unwrap();

    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
    let raw = xattr::get(&file, TAGS_XATTR).unwrap().expect("xattr still present");
    assert_eq!(raw, original, "an unreadable file's tags must not be overwritten");
    assert_eq!(outcome.skipped.len(), 1);
    assert!(matches!(outcome.skipped[0].reason, TagReadError::Unreadable(_)));
    let _ = std::fs::remove_dir_all(&dir);
}

/// The skip is per file: the readable rest of the selection still gets the color,
/// and the skipped file doesn't sway the all-have decision.
#[test]
fn toggle_tags_the_readable_files_and_skips_only_the_malformed_one() {
    let dir = temp_dir("mixed");
    let good = dir.join("good.txt");
    let bad = dir.join("bad.txt");
    std::fs::write(&good, b"x").unwrap();
    std::fs::write(&bad, b"x").unwrap();
    set_tags(&good, &[tag("Work", 0)]).unwrap();
    xattr::set(&bad, TAGS_XATTR, b"garbage").unwrap();

    let outcome = toggle_color(&[path_str(&good), path_str(&bad)], 6).unwrap();

    let expected = vec![tag("Work", 0), tag("Red", 6)];
    assert_eq!(read_tags(&good), expected);
    assert_eq!(outcome.updates, vec![(path_str(&good), expected)]);
    assert_eq!(xattr::get(&bad, TAGS_XATTR).unwrap().as_deref(), Some(&b"garbage"[..]));
    assert_eq!(outcome.skipped.len(), 1);
    assert_eq!(outcome.skipped[0].path, path_str(&bad));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn strict_reader_tells_absent_apart_from_empty() {
    let dir = temp_dir("strict_absent");
    let file = dir.join("f.txt");
    std::fs::write(&file, b"x").unwrap();
    assert!(matches!(read_tags_strict(&file), Ok(None)));
    xattr::set(&file, TAGS_XATTR, &encode_tags_plist(&[]).unwrap()).unwrap();
    assert!(matches!(read_tags_strict(&file), Ok(Some(ref t)) if t.is_empty()));
    let _ = std::fs::remove_dir_all(&dir);
}

/// D11 regression: tagging must NOT clobber a folder's custom-icon FinderInfo.
/// Zeroing `com.apple.FinderInfo` would wipe `kHasCustomIcon` (0x0400 at offset 8)
/// and break both the custom icon and Cmdr's own `has_custom_folder_icon`.
#[test]
fn tagging_preserves_finder_info_custom_icon_flag() {
    let dir = temp_dir("d11");
    let folder = dir.join("CustomIconFolder");
    std::fs::create_dir(&folder).unwrap();

    // FinderInfo with kHasCustomIcon set (big-endian 0x0400 at byte offset 8).
    let mut finder_info = vec![0u8; 32];
    finder_info[8] = 0x04;
    finder_info[9] = 0x00;
    xattr::set(&folder, "com.apple.FinderInfo", &finder_info).unwrap();

    toggle_color(&[path_str(&folder)], 6).unwrap();

    // The tag landed…
    assert_eq!(read_tags(&folder), vec![tag("Red", 6)]);
    // …and the FinderInfo blob (with the custom-icon flag) survived untouched.
    let fi = xattr::get(&folder, "com.apple.FinderInfo")
        .unwrap()
        .expect("FinderInfo must still be present after tagging");
    assert_eq!(fi.len(), 32, "FinderInfo must not be truncated");
    let flags = u16::from_be_bytes([fi[8], fi[9]]);
    assert!(flags & 0x0400 != 0, "kHasCustomIcon must survive a tag write");

    let _ = std::fs::remove_dir_all(&dir);
}
