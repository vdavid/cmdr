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
