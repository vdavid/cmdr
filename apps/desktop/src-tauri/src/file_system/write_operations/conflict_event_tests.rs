//! Part of `conflict.rs`, split out as a `#[path]` child so the module itself
//! stays readable. `super::` here is `conflict`, exactly as when these lived
//! inline.
//!
//! Regression for the low-severity audit finding: the Stop-mode
//! conflict event used to carry no `is_directory` flags, so the FE
//! dialog rendered a generic "file already exists" prompt even when
//! the collision was a type mismatch (file → directory or vice versa).
//! User clicked "Overwrite" thinking they were replacing a file, ended
//! up dropping a whole directory tree without warning.
use super::*;
use tempfile::TempDir;

#[test]
fn file_over_directory_marks_destination_is_directory() {
    let temp = TempDir::new().unwrap();
    let source = temp.path().join("notes.txt");
    let dest = temp.path().join("conflicting");
    fs::write(&source, b"a file").unwrap();
    fs::create_dir(&dest).unwrap();

    let source_meta = fs::metadata(&source).unwrap();
    let dest_meta = fs::metadata(&dest).unwrap();

    let event = build_conflict_event(
        "op-1",
        ConflictId(1),
        &source,
        &dest,
        Some(&source_meta),
        Some(&dest_meta),
        None,
        Some(12345),
    );

    assert!(!event.source_is_directory, "source is a file");
    assert!(event.destination_is_directory, "destination is a directory");
}

#[test]
fn directory_over_file_marks_source_is_directory() {
    let temp = TempDir::new().unwrap();
    let source = temp.path().join("conflicting");
    let dest = temp.path().join("notes.txt");
    fs::create_dir(&source).unwrap();
    fs::write(&dest, b"a file").unwrap();

    let source_meta = fs::metadata(&source).unwrap();
    let dest_meta = fs::metadata(&dest).unwrap();

    let event = build_conflict_event(
        "op-2",
        ConflictId(1),
        &source,
        &dest,
        Some(&source_meta),
        Some(&dest_meta),
        Some(67890),
        None,
    );

    assert!(event.source_is_directory, "source is a directory");
    assert!(!event.destination_is_directory, "destination is a file");
}

#[test]
fn file_over_file_flags_both_false() {
    let temp = TempDir::new().unwrap();
    let source = temp.path().join("a.txt");
    let dest = temp.path().join("b.txt");
    fs::write(&source, b"a").unwrap();
    fs::write(&dest, b"b").unwrap();

    let source_meta = fs::metadata(&source).unwrap();
    let dest_meta = fs::metadata(&dest).unwrap();

    let event = build_conflict_event(
        "op-3",
        ConflictId(1),
        &source,
        &dest,
        Some(&source_meta),
        Some(&dest_meta),
        None,
        None,
    );

    assert!(!event.source_is_directory);
    assert!(!event.destination_is_directory);
}

#[test]
fn file_dest_uses_metadata_len_ignoring_override() {
    // Files always have a known size via metadata. The override exists
    // only for directories (where metadata.len() is the inode entry
    // size, not the recursive content size).
    let temp = TempDir::new().unwrap();
    let source = temp.path().join("a.txt");
    let dest = temp.path().join("b.txt");
    fs::write(&source, b"hello").unwrap();
    fs::write(&dest, b"world!").unwrap();

    let source_meta = fs::metadata(&source).unwrap();
    let dest_meta = fs::metadata(&dest).unwrap();

    let event = build_conflict_event(
        "op",
        ConflictId(1),
        &source,
        &dest,
        Some(&source_meta),
        Some(&dest_meta),
        Some(99999),
        Some(99999),
    );

    assert_eq!(event.source_size, Some(5));
    assert_eq!(event.destination_size, Some(6));
    assert_eq!(event.size_difference, Some(1));
}

#[test]
fn folder_dest_uses_override_size() {
    // For dir destinations the recursive size lives in the drive index;
    // the caller fetches it (or `None` when the index doesn't cover the
    // path) and hands it to us.
    let temp = TempDir::new().unwrap();
    let source = temp.path().join("notes.txt");
    let dest = temp.path().join("conflicting");
    fs::write(&source, b"a").unwrap();
    fs::create_dir(&dest).unwrap();

    let source_meta = fs::metadata(&source).unwrap();
    let dest_meta = fs::metadata(&dest).unwrap();

    let event = build_conflict_event(
        "op",
        ConflictId(1),
        &source,
        &dest,
        Some(&source_meta),
        Some(&dest_meta),
        None,
        Some(4_096_000),
    );

    assert_eq!(event.source_size, Some(1));
    assert_eq!(event.destination_size, Some(4_096_000));
    assert_eq!(event.size_difference, Some(4_095_999));
}

#[test]
fn folder_dest_with_unknown_size_surfaces_none() {
    // The index doesn't cover the destination (network mount, MTP, …).
    // Report `(unknown)` on the wire as `None`; size_difference also
    // collapses to `None` since one side is unknown.
    let temp = TempDir::new().unwrap();
    let source = temp.path().join("notes.txt");
    let dest = temp.path().join("conflicting");
    fs::write(&source, b"a").unwrap();
    fs::create_dir(&dest).unwrap();

    let source_meta = fs::metadata(&source).unwrap();
    let dest_meta = fs::metadata(&dest).unwrap();

    let event = build_conflict_event(
        "op",
        ConflictId(1),
        &source,
        &dest,
        Some(&source_meta),
        Some(&dest_meta),
        None,
        None,
    );

    assert_eq!(event.source_size, Some(1));
    assert_eq!(event.destination_size, None);
    assert_eq!(event.size_difference, None);
}

#[test]
fn folder_source_uses_override_size() {
    // Folder-source sizes come from the pre-flight scan's per-source-root
    // total. The override is always Some for source-folder clashes.
    let temp = TempDir::new().unwrap();
    let source = temp.path().join("payload");
    let dest = temp.path().join("notes.txt");
    fs::create_dir(&source).unwrap();
    fs::write(&dest, b"hi").unwrap();

    let source_meta = fs::metadata(&source).unwrap();
    let dest_meta = fs::metadata(&dest).unwrap();

    let event = build_conflict_event(
        "op",
        ConflictId(1),
        &source,
        &dest,
        Some(&source_meta),
        Some(&dest_meta),
        Some(123_456),
        None,
    );

    assert_eq!(event.source_size, Some(123_456));
    assert_eq!(event.destination_size, Some(2));
    assert_eq!(event.size_difference, Some(2 - 123_456));
}

#[test]
fn folder_source_with_unknown_size_surfaces_none() {
    // A folder source with no pre-flight scan total (the skip-preflight /
    // fast-path case) surfaces `source_size: None`, and `size_difference`
    // collapses to `None` just as it does when the destination is unknown.
    let temp = TempDir::new().unwrap();
    let source = temp.path().join("payload");
    let dest = temp.path().join("notes.txt");
    fs::create_dir(&source).unwrap();
    fs::write(&dest, b"hi").unwrap();

    let source_meta = fs::metadata(&source).unwrap();
    let dest_meta = fs::metadata(&dest).unwrap();

    let event = build_conflict_event(
        "op",
        ConflictId(1),
        &source,
        &dest,
        Some(&source_meta),
        Some(&dest_meta),
        None,
        None,
    );

    assert_eq!(event.source_size, None);
    assert_eq!(event.destination_size, Some(2));
    assert_eq!(event.size_difference, None);
}
