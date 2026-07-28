use super::*;

// ── Path reconstruction ──────────────────────────────────────────

#[test]
fn path_reconstruction() {
    let index = make_test_index();
    let path = reconstruct_path_from_index(&index, 4); // report.pdf
    assert_eq!(path, "/Users/alice/report.pdf");
}

#[test]
fn path_reconstruction_root() {
    let index = make_test_index();
    let path = reconstruct_path_from_index(&index, 1);
    assert_eq!(path, "/");
}

#[test]
fn path_reconstruction_top_level_dir() {
    let index = make_test_index();
    let path = reconstruct_path_from_index(&index, 2); // Users
    assert_eq!(path, "/Users");
}

/// The streamed hash is the ranking hot path's substitute for
/// `hash_path(reconstruct_path_from_index(..))`, so it has to agree with it for
/// EVERY entry — a drifted hash silently reads the wrong (or no) importance weight,
/// with no visible failure beyond subtly worse ranking. Covers the root sentinel, a
/// top-level dir, a nested dir, and files.
#[test]
fn streamed_hash_matches_whole_path_hash() {
    use crate::search::ranking::hash_path;

    let index = make_test_index();
    for entry in &index.entries {
        let path = reconstruct_path_from_index(&index, entry.id);
        assert_eq!(
            hash_path_from_index(&index, entry.id),
            hash_path(&path),
            "streamed hash differs for id {} ({path})",
            entry.id
        );
    }

    // An id absent from the index (an orphan) resolves to "/" both ways.
    assert_eq!(
        hash_path_from_index(&index, 9999),
        hash_path(&reconstruct_path_from_index(&index, 9999))
    );
}

// ── Icon ID derivation ───────────────────────────────────────────

#[test]
fn icon_id_directory() {
    assert_eq!(derive_icon_id("Documents", true), "dir");
}

#[test]
fn icon_id_file_with_extension() {
    assert_eq!(derive_icon_id("report.pdf", false), "ext:pdf");
}

#[test]
fn icon_id_file_without_extension() {
    assert_eq!(derive_icon_id("Makefile", false), "file");
}

#[test]
fn icon_id_uppercase_extension() {
    assert_eq!(derive_icon_id("Photo.JPG", false), "ext:jpg");
}
