//! Part of `conflict.rs`, split out as a `#[path]` child so the module itself
//! stays readable. `super::` here is `conflict`, exactly as when these lived
//! inline.
//!
//! The event's shape: sizes, the size difference, and the type flags. Whether
//! the flags describe what the engines really do on each clash shape is pinned
//! end to end in `transfer/conflict_prompt_sides_tests.rs`.
use super::*;
use tempfile::TempDir;

/// One side of a clash, typed from the entry on disk (`symlink_metadata`, so
/// the tests never stat through a link).
fn side<'a>(path: &'a Path, meta: &'a fs::Metadata, size_for_dir: Option<KnownSize>) -> ClashSide<'a> {
    ClashSide {
        path,
        meta: Some(meta),
        is_directory: fs::symlink_metadata(path).unwrap().is_dir(),
        size_for_dir,
    }
}

#[test]
fn file_over_directory_marks_destination_is_directory() {
    let temp = TempDir::new().unwrap();
    let source = temp.path().join("notes.txt");
    let dest = temp.path().join("conflicting");
    fs::write(&source, b"a file").unwrap();
    fs::create_dir(&dest).unwrap();
    let (source_meta, dest_meta) = (fs::metadata(&source).unwrap(), fs::metadata(&dest).unwrap());

    let event = build_conflict_event(
        "op-1",
        ConflictId(1),
        &side(&source, &source_meta, None),
        &side(&dest, &dest_meta, Some(KnownSize::Exact { bytes: 12345 })),
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
    let (source_meta, dest_meta) = (fs::metadata(&source).unwrap(), fs::metadata(&dest).unwrap());

    let event = build_conflict_event(
        "op-2",
        ConflictId(1),
        &side(&source, &source_meta, Some(KnownSize::Exact { bytes: 67890 })),
        &side(&dest, &dest_meta, None),
    );

    assert!(event.source_is_directory, "source is a directory");
    assert!(!event.destination_is_directory, "destination is a file");
}

/// The flags are the caller's classification, never the stat's: `meta` follows
/// links, and a link to a folder is a leaf to every local engine.
#[test]
fn type_flags_come_from_the_side_not_from_a_link_following_stat() {
    let temp = TempDir::new().unwrap();
    let target = temp.path().join("target");
    let source = temp.path().join("link");
    let dest = temp.path().join("folder");
    fs::create_dir(&target).unwrap();
    std::os::unix::fs::symlink(&target, &source).unwrap();
    fs::create_dir(&dest).unwrap();
    // Following the link, as `resolve_conflict`'s stat does: this says "folder".
    let (source_meta, dest_meta) = (fs::metadata(&source).unwrap(), fs::metadata(&dest).unwrap());
    assert!(source_meta.is_dir(), "precondition: the stat follows the link");

    let event = build_conflict_event(
        "op-link",
        ConflictId(1),
        &side(&source, &source_meta, None),
        &side(&dest, &dest_meta, None),
    );

    assert!(!event.source_is_directory, "a link is a leaf");
    assert!(event.destination_is_directory);
}

#[test]
fn file_over_file_flags_both_false() {
    let temp = TempDir::new().unwrap();
    let source = temp.path().join("a.txt");
    let dest = temp.path().join("b.txt");
    fs::write(&source, b"a").unwrap();
    fs::write(&dest, b"b").unwrap();
    let (source_meta, dest_meta) = (fs::metadata(&source).unwrap(), fs::metadata(&dest).unwrap());

    let event = build_conflict_event(
        "op-3",
        ConflictId(1),
        &side(&source, &source_meta, None),
        &side(&dest, &dest_meta, None),
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
    let (source_meta, dest_meta) = (fs::metadata(&source).unwrap(), fs::metadata(&dest).unwrap());

    let event = build_conflict_event(
        "op",
        ConflictId(1),
        &side(&source, &source_meta, Some(KnownSize::Exact { bytes: 99999 })),
        &side(&dest, &dest_meta, Some(KnownSize::Exact { bytes: 99999 })),
    );

    assert_eq!(event.source_size, Some(KnownSize::Exact { bytes: 5 }));
    assert_eq!(event.destination_size, Some(KnownSize::Exact { bytes: 6 }));
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
    let (source_meta, dest_meta) = (fs::metadata(&source).unwrap(), fs::metadata(&dest).unwrap());

    let event = build_conflict_event(
        "op",
        ConflictId(1),
        &side(&source, &source_meta, None),
        &side(&dest, &dest_meta, Some(KnownSize::Exact { bytes: 4_096_000 })),
    );

    assert_eq!(event.source_size, Some(KnownSize::Exact { bytes: 1 }));
    assert_eq!(event.destination_size, Some(KnownSize::Exact { bytes: 4_096_000 }));
    assert_eq!(event.size_difference, Some(4_095_999));
}

/// A folder the index has only partly counted reaches the prompt as a floor,
/// and no difference is computed against it: the incoming 1-byte file must not
/// read as the smaller side of a folder that may hold 40 GB (#528).
#[test]
fn folder_dest_with_a_lower_bound_size_computes_no_difference() {
    let temp = TempDir::new().unwrap();
    let source = temp.path().join("notes.txt");
    let dest = temp.path().join("conflicting");
    fs::write(&source, b"a").unwrap();
    fs::create_dir(&dest).unwrap();
    let (source_meta, dest_meta) = (fs::metadata(&source).unwrap(), fs::metadata(&dest).unwrap());

    let event = build_conflict_event(
        "op",
        ConflictId(1),
        &side(&source, &source_meta, None),
        &side(&dest, &dest_meta, Some(KnownSize::AtLeast { bytes: 12_288 })),
    );

    assert_eq!(event.destination_size, Some(KnownSize::AtLeast { bytes: 12_288 }));
    assert_eq!(event.size_difference, None);
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
    let (source_meta, dest_meta) = (fs::metadata(&source).unwrap(), fs::metadata(&dest).unwrap());

    let event = build_conflict_event(
        "op",
        ConflictId(1),
        &side(&source, &source_meta, None),
        &side(&dest, &dest_meta, None),
    );

    assert_eq!(event.source_size, Some(KnownSize::Exact { bytes: 1 }));
    assert_eq!(event.destination_size, None);
    assert_eq!(event.size_difference, None);
}

#[test]
fn folder_source_uses_override_size() {
    // A folder source's recursive size comes from the drive index, the same
    // lookup a folder destination uses.
    let temp = TempDir::new().unwrap();
    let source = temp.path().join("payload");
    let dest = temp.path().join("notes.txt");
    fs::create_dir(&source).unwrap();
    fs::write(&dest, b"hi").unwrap();
    let (source_meta, dest_meta) = (fs::metadata(&source).unwrap(), fs::metadata(&dest).unwrap());

    let event = build_conflict_event(
        "op",
        ConflictId(1),
        &side(&source, &source_meta, Some(KnownSize::Exact { bytes: 123_456 })),
        &side(&dest, &dest_meta, None),
    );

    assert_eq!(event.source_size, Some(KnownSize::Exact { bytes: 123_456 }));
    assert_eq!(event.destination_size, Some(KnownSize::Exact { bytes: 2 }));
    assert_eq!(event.size_difference, Some(2 - 123_456));
}

#[test]
fn folder_source_with_unknown_size_surfaces_none() {
    // A folder source the index doesn't cover surfaces `source_size: None`,
    // and `size_difference` collapses to `None` just as it does when the
    // destination is unknown.
    let temp = TempDir::new().unwrap();
    let source = temp.path().join("payload");
    let dest = temp.path().join("notes.txt");
    fs::create_dir(&source).unwrap();
    fs::write(&dest, b"hi").unwrap();
    let (source_meta, dest_meta) = (fs::metadata(&source).unwrap(), fs::metadata(&dest).unwrap());

    let event = build_conflict_event(
        "op",
        ConflictId(1),
        &side(&source, &source_meta, None),
        &side(&dest, &dest_meta, None),
    );

    assert_eq!(event.source_size, None);
    assert_eq!(event.destination_size, Some(KnownSize::Exact { bytes: 2 }));
    assert_eq!(event.size_difference, None);
}
