use std::ffi::OsStr;

use super::{MacOnlyFiles, is_mac_only_file_name};

#[test]
fn finder_metadata_and_apple_double_names_match() {
    for name in [".DS_Store", "._photo.jpg", "._x", "._.DS_Store"] {
        assert!(is_mac_only_file_name(OsStr::new(name)), "{name} is Mac-only");
    }
}

#[test]
fn look_alikes_stay_ordinary_files() {
    for name in [
        "._",
        ".DS_Store.bak",
        ".ds_store",
        "DS_Store",
        "_photo.jpg",
        ".gitignore",
        "a._b",
    ] {
        assert!(!is_mac_only_file_name(OsStr::new(name)), "{name} is an ordinary file");
    }
}

#[test]
fn only_skip_leaves_anything_out() {
    assert!(MacOnlyFiles::Skip.leaves_out_str(".DS_Store", false));
    assert!(MacOnlyFiles::Skip.leaves_out_str("._a", false));
    assert!(!MacOnlyFiles::Include.leaves_out_str(".DS_Store", false));
    assert!(!MacOnlyFiles::Skip.leaves_out_str("notes.txt", false));
}

#[test]
fn a_folder_never_matches() {
    assert!(!MacOnlyFiles::Skip.leaves_out_str("._folder", true));
    assert!(!MacOnlyFiles::Skip.leaves_out_str(".DS_Store", true));
}

#[test]
fn travels_over_ipc_in_camel_case() {
    assert_eq!(serde_json::to_string(&MacOnlyFiles::Skip).unwrap(), "\"skip\"");
    assert_eq!(
        serde_json::from_str::<MacOnlyFiles>("\"include\"").unwrap(),
        MacOnlyFiles::Include
    );
}
