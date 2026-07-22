//! The throwaway directory tree behind Debug > Soft dialogs (dev builds only).
//!
//! Five soft dialogs do real work on mount: `delete-confirmation` and
//! `transfer-confirmation` run background scans, `mkdir-confirmation` /
//! `new-file-confirmation` check name conflicts against a live listing, and
//! `go-to-path` resolves paths. Faking that would fake the very numbers the
//! design displays, so the gallery points them at a real directory instead and
//! lets them behave for real.
//!
//! The tree lives under the app data dir (per-worktree in dev), so it never
//! touches the user's own files. `mkdir-confirmation` and
//! `new-file-confirmation` genuinely write when confirmed; this directory is
//! what makes that harmless.
//!
//! Idempotent by construction: a file is (re)written only when it's missing or
//! its length differs, so triggering a dialog twice creates nothing new and
//! never deletes anything a reviewer left behind.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// The directory name under the app data dir. Also the name a reviewer sees in
/// the pane's breadcrumb, so it says what it is.
pub const FIXTURE_DIR_NAME: &str = "dialog-gallery-fixtures";

/// Where the gallery's disk-backed dialogs point, and the landmarks inside it
/// they need by name. Returned rather than guessed on the frontend: the side
/// that CREATES the tree is the only side that can name its parts without
/// drifting from what's on disk.
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DialogGalleryFixtures {
    /// Absolute path of the fixture directory itself.
    pub root: String,
    /// Absolute path of the folder the copy / move states use as their destination.
    pub destination_dir: String,
    /// Name (not path) of a folder directly inside `root`, for the mkdir conflict state.
    pub existing_folder_name: String,
    /// Name (not path) of a file directly inside `root`, for the mkfile conflict state.
    pub existing_file_name: String,
    /// A deep path inside `root`, for the "Go to path" preview.
    pub nested_path: String,
}

/// Name of the folder the transfer states copy / move into.
const DESTINATION_DIR: &str = "Backup destination";
/// Name of the folder the mkdir conflict state collides with.
const EXISTING_FOLDER: &str = "Photos";
/// Name of the file the mkfile conflict state collides with.
const EXISTING_FILE: &str = "Invoice 2026-07.pdf";
/// Deep-ish path (relative to the root) the go-to-path state offers.
const NESTED_DIR: &str = "Projects/cmdr/src-tauri/src/file_system";

/// A 195-character name. Every dialog that shows a filename gets to prove what it
/// does when one never fits: the delete list, the transfer source line, the
/// conflict warnings.
const VERY_LONG_NAME: &str = "A deliberately very long file name that exists only so the dialogs get to show what they do when a name never fits on one line, including where they truncate it and where they simply overflow.txt";

/// The tree: `(path relative to the root, byte length)`. Sizes are spread over
/// four orders of magnitude so the scan tallies, the size column, and the
/// thousands separators all get something real to render. Directories come from
/// the paths, so an entry is the only thing that creates one.
///
/// Files are sparse above the first line of content (see `write_file`), so the
/// whole tree costs a few kilobytes of actual disk while reporting ~24 MB.
const FILES: &[(&str, u64)] = &[
    ("README.txt", 412),
    (EXISTING_FILE, 184_320),
    // A folder of photos: the "many files, uniform size" shape.
    ("Photos/2026-06 Stockholm/IMG_2201.jpg", 3_214_592),
    ("Photos/2026-06 Stockholm/IMG_2202.jpg", 2_981_888),
    ("Photos/2026-06 Stockholm/IMG_2203.jpg", 3_450_112),
    ("Photos/2026-06 Stockholm/IMG_2204.jpg", 2_772_992),
    ("Photos/2026-06 Stockholm/IMG_2205.jpg", 3_106_816),
    ("Photos/2026-06 Stockholm/IMG_2206.jpg", 2_899_968),
    // Non-ASCII, both in the folder name and in the file names.
    ("Photos/2026-07 Åre skidresa/DSC00417.arw", 1_258_291),
    ("Photos/2026-07 Åre skidresa/DSC00418.arw", 1_310_720),
    ("Photos/2026-07 Åre skidresa/Färdplan för veckan.md", 3_820),
    ("Photos/exported/preview-01.webp", 96_256),
    ("Photos/exported/preview-02.webp", 88_064),
    // A source tree: the "many tiny files, deep nesting" shape.
    ("Projects/cmdr/README.md", 8_192),
    ("Projects/cmdr/Cargo.toml", 2_048),
    ("Projects/cmdr/src-tauri/src/file_system/listing.rs", 41_984),
    ("Projects/cmdr/src-tauri/src/file_system/write_ops.rs", 63_488),
    ("Projects/cmdr/src-tauri/src/file_system/mod.rs", 7_168),
    ("Projects/cmdr/src-tauri/src/main.rs", 1_024),
    ("Projects/cmdr/src/app.css", 24_576),
    ("Projects/notes/backlog.md", 5_120),
    ("Projects/notes/2026-07-14 retro.md", 3_072),
    // Documents, including the name that never fits.
    ("Documents/Contracts/lease-2026.pdf", 742_400),
    ("Documents/Contracts/insurance-2026.pdf", 512_000),
    ("Documents/Receipts/2026-07-02 hardware.pdf", 128_000),
    ("Documents/Receipts/2026-07-11 groceries.pdf", 96_000),
    ("Documents/Receipts/2026-07-19 fuel.pdf", 74_752),
    ("Documents/Notes/meeting-notes.md", 12_288),
    ("Documents/Notes/ideas.md", 6_144),
    (VERY_LONG_NAME, 21_504),
    // One big file on its own, so a single-item delete has a real size to show.
    ("Videos/2026-06-21 midsommar.mov", 486_539_264),
    // The destination folder isn't empty: it already holds entries named like
    // top-level sources, so the transfer dialog's conflict pre-check finds real
    // conflicts instead of an always-clean destination.
    ("Backup destination/README.txt", 412),
    ("Backup destination/Documents/Notes/ideas.md", 6_144),
    ("Backup destination/Photos/exported/preview-01.webp", 96_256),
];

/// Creates (or completes) the fixture tree at `root` and returns its landmarks.
///
/// Safe to call repeatedly: existing files of the right length are left alone,
/// and nothing is ever deleted.
pub fn ensure_dialog_gallery_fixtures(root: &Path) -> Result<DialogGalleryFixtures, String> {
    fs::create_dir_all(root).map_err(|e| format!("Failed to create {}: {e}", root.display()))?;

    for (relative, size) in FILES {
        let path = root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("Failed to create {}: {e}", parent.display()))?;
        }
        write_file(&path, *size)?;
    }

    // An empty folder the tree wouldn't otherwise have: the delete and transfer
    // dialogs count folders separately from files, and an empty one is the case
    // where a recursive scan finds nothing to add.
    let empty_dir = root.join("Documents/Empty folder");
    fs::create_dir_all(&empty_dir).map_err(|e| format!("Failed to create {}: {e}", empty_dir.display()))?;

    Ok(DialogGalleryFixtures {
        root: root.to_string_lossy().into_owned(),
        destination_dir: path_string(root, DESTINATION_DIR),
        existing_folder_name: EXISTING_FOLDER.to_string(),
        existing_file_name: EXISTING_FILE.to_string(),
        nested_path: path_string(root, NESTED_DIR),
    })
}

fn path_string(root: &Path, relative: &str) -> String {
    root.join(relative).to_string_lossy().into_owned()
}

/// Writes one fixture file, unless it already has the right length.
///
/// The first line is real text (so anything that peeks at the bytes sees what
/// this is), and the rest is a sparse tail via `set_len`: the tree reports
/// hundreds of megabytes to the scan while costing kilobytes of real disk, and
/// creating it stays instant on every trigger.
fn write_file(path: &PathBuf, size: u64) -> Result<(), String> {
    if let Ok(metadata) = fs::metadata(path)
        && metadata.is_file()
        && metadata.len() == size
    {
        return Ok(());
    }

    let mut file = fs::File::create(path).map_err(|e| format!("Failed to create {}: {e}", path.display()))?;
    let header = b"Cmdr dialog-gallery fixture. Safe to delete.\n";
    if size >= header.len() as u64 {
        file.write_all(header)
            .map_err(|e| format!("Failed to write {}: {e}", path.display()))?;
    }
    file.set_len(size)
        .map_err(|e| format!("Failed to size {}: {e}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every file the tree promises, with the length it promises. Walked rather
    /// than spot-checked so a bad `FILES` row can't hide behind its neighbours.
    fn assert_tree_matches(root: &Path) {
        for (relative, size) in FILES {
            let path = root.join(relative);
            let metadata = fs::metadata(&path).unwrap_or_else(|e| panic!("{} missing: {e}", path.display()));
            assert!(metadata.is_file(), "{} should be a file", path.display());
            assert_eq!(metadata.len(), *size, "{} has the wrong length", path.display());
        }
    }

    fn count_entries(dir: &Path) -> usize {
        fs::read_dir(dir)
            .expect("fixture dir should be readable")
            .filter_map(Result::ok)
            .map(|entry| {
                let path = entry.path();
                if path.is_dir() { 1 + count_entries(&path) } else { 1 }
            })
            .sum()
    }

    #[test]
    fn creates_the_whole_tree() {
        let temp = tempfile::tempdir().expect("temp dir");
        let root = temp.path().join(FIXTURE_DIR_NAME);

        let landmarks = ensure_dialog_gallery_fixtures(&root).expect("first run should succeed");

        assert_tree_matches(&root);
        assert_eq!(landmarks.root, root.to_string_lossy());
        assert!(Path::new(&landmarks.destination_dir).is_dir());
        assert!(Path::new(&landmarks.nested_path).is_dir());
        assert!(root.join(&landmarks.existing_folder_name).is_dir());
        assert!(root.join(&landmarks.existing_file_name).is_file());
    }

    /// The data-safety property: the gallery calls this on every trigger, so a
    /// second run must add nothing, change nothing, and remove nothing.
    #[test]
    fn is_idempotent_across_runs() {
        let temp = tempfile::tempdir().expect("temp dir");
        let root = temp.path().join(FIXTURE_DIR_NAME);

        let first = ensure_dialog_gallery_fixtures(&root).expect("first run should succeed");
        let entries_after_first = count_entries(&root);
        let sample = root.join("README.txt");
        let created_at = fs::metadata(&sample).expect("sample file").modified().ok();

        let second = ensure_dialog_gallery_fixtures(&root).expect("second run should succeed");

        assert_eq!(
            count_entries(&root),
            entries_after_first,
            "a second run duplicated entries"
        );
        assert_tree_matches(&root);
        assert_eq!(first.root, second.root);
        assert_eq!(first.destination_dir, second.destination_dir);
        assert_eq!(
            fs::metadata(&sample).expect("sample file").modified().ok(),
            created_at,
            "a second run rewrote a file that was already correct",
        );
    }

    /// A file a reviewer created inside the tree (the mkdir / mkfile dialogs
    /// write for real) must survive the next trigger.
    #[test]
    fn leaves_foreign_entries_alone() {
        let temp = tempfile::tempdir().expect("temp dir");
        let root = temp.path().join(FIXTURE_DIR_NAME);
        ensure_dialog_gallery_fixtures(&root).expect("first run should succeed");

        let reviewer_folder = root.join("Folder the reviewer made");
        fs::create_dir(&reviewer_folder).expect("create reviewer folder");
        let reviewer_file = root.join("Photos/reviewer.txt");
        fs::write(&reviewer_file, b"kept").expect("write reviewer file");

        ensure_dialog_gallery_fixtures(&root).expect("second run should succeed");

        assert!(reviewer_folder.is_dir(), "a reviewer-created folder was removed");
        assert_eq!(fs::read(&reviewer_file).expect("reviewer file"), b"kept");
    }

    /// A truncated (interrupted) fixture file is repaired rather than left short.
    #[test]
    fn repairs_a_file_with_the_wrong_length() {
        let temp = tempfile::tempdir().expect("temp dir");
        let root = temp.path().join(FIXTURE_DIR_NAME);
        ensure_dialog_gallery_fixtures(&root).expect("first run should succeed");

        let damaged = root.join("Photos/2026-06 Stockholm/IMG_2201.jpg");
        fs::write(&damaged, b"truncated").expect("truncate fixture file");

        ensure_dialog_gallery_fixtures(&root).expect("second run should succeed");

        assert_tree_matches(&root);
    }
}
