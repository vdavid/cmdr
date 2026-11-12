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
//!
//! One sibling tree breaks that last rule on purpose: the conflict preview's
//! (`CONFLICT_FIXTURE_DIR_NAME`). The `operation-conflict` row starts a REAL
//! copy into it that parks on a real clash, and whatever the reviewer answers
//! really happens, so every trigger puts the destination side back the way the
//! clash needs it. It's a sibling, not a subfolder, so the disk-backed dialogs'
//! listing never shows it. The `transfer-progress` row's tree is a third sibling
//! with the same reset rule for its destination (`transfer_preview.rs`).

mod transfer_preview;

use std::collections::HashSet;
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
    /// The conflict preview's clash pairs, in their own sibling tree.
    pub conflict_preview: ConflictPreviewFixtures,
    /// The transfer preview's shapes, in their own sibling tree.
    pub transfer_preview: TransferPreviewFixtures,
}

pub use transfer_preview::{TRANSFER_FIXTURE_DIR_NAME, TransferPreviewFixtures};

/// Where the `operation-conflict` preview copies from and to. Every name below
/// exists in BOTH folders, as the kinds its field names, so copying `from_dir/<name>`
/// into `to_dir` parks on exactly that clash.
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ConflictPreviewFixtures {
    /// Absolute path of the folder holding the incoming side of every clash.
    pub from_dir: String,
    /// Absolute path of the folder the preview copies into.
    pub to_dir: String,
    /// Name of a folder in `from_dir` that is a FILE in `to_dir`.
    pub folder_over_file: String,
    /// Name of a file in `from_dir` that is a FOLDER in `to_dir`.
    pub file_over_folder: String,
    /// Name of a file in both.
    pub file_over_file: String,
}

/// The conflict preview's own directory name under the app data dir.
pub const CONFLICT_FIXTURE_DIR_NAME: &str = "dialog-gallery-conflict-fixtures";

const CONFLICT_FOLDER_OVER_FILE: &str = "Website redesign";
const CONFLICT_FILE_OVER_FOLDER: &str = "Quarterly report.pdf";
const CONFLICT_FILE_OVER_FILE: &str = "Budget 2026.xlsx";

/// The incoming side, under `From/`. Same shape as `FILES`.
const CONFLICT_SOURCES: &[(&str, u64)] = &[
    ("Website redesign/index.html", 18_432),
    ("Website redesign/styles/site.css", 9_216),
    ("Website redesign/images/hero.jpg", 1_482_752),
    ("Website redesign/images/team.jpg", 964_608),
    (CONFLICT_FILE_OVER_FOLDER, 2_359_296),
    (CONFLICT_FILE_OVER_FILE, 48_128),
];

/// What stands in the way, under `To/`: each top-level name is the other kind
/// (or, for the plain clash, a same-named file of another size).
const CONFLICT_BLOCKERS: &[(&str, u64)] = &[
    (CONFLICT_FOLDER_OVER_FILE, 7_340),
    ("Quarterly report.pdf/draft-1.pdf", 1_048_576),
    ("Quarterly report.pdf/draft-2.pdf", 1_310_720),
    (CONFLICT_FILE_OVER_FILE, 52_224),
];

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
    ("Photos/2026-06 Testville/IMG_2201.jpg", 3_214_592),
    ("Photos/2026-06 Testville/IMG_2202.jpg", 2_981_888),
    ("Photos/2026-06 Testville/IMG_2203.jpg", 3_450_112),
    ("Photos/2026-06 Testville/IMG_2204.jpg", 2_772_992),
    ("Photos/2026-06 Testville/IMG_2205.jpg", 3_106_816),
    ("Photos/2026-06 Testville/IMG_2206.jpg", 2_899_968),
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

/// Creates (or completes) both fixture trees under `data_dir` and returns their
/// landmarks.
///
/// Safe to call repeatedly: in the main tree, existing files of the right length
/// are left alone and nothing is ever deleted. The conflict preview's
/// destination is the one exception (`ensure_conflict_preview_fixtures`).
pub fn ensure_dialog_gallery_fixtures(data_dir: &Path) -> Result<DialogGalleryFixtures, String> {
    let root = &data_dir.join(FIXTURE_DIR_NAME);
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
        conflict_preview: ensure_conflict_preview_fixtures(&data_dir.join(CONFLICT_FIXTURE_DIR_NAME))?,
        transfer_preview: transfer_preview::ensure_transfer_preview_fixtures(
            &data_dir.join(TRANSFER_FIXTURE_DIR_NAME),
        )?,
    })
}

/// Creates the conflict preview's two folders, and puts `To/` back to exactly
/// the blockers the clashes need.
///
/// The previous preview's answer really happened: an Overwrite left the other
/// KIND of entry at a blocker's name, and a Rename left a ` (1)` sibling. So in
/// `To/` (and only there) anything that isn't a blocker is removed, and a
/// blocker of the wrong kind is replaced. `From/` is only ever completed, like
/// the main tree, so the drive index keeps knowing its folder's size.
fn ensure_conflict_preview_fixtures(root: &Path) -> Result<ConflictPreviewFixtures, String> {
    let from = root.join("From");
    let to = root.join("To");

    for (relative, size) in CONFLICT_SOURCES {
        let path = from.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("Failed to create {}: {e}", parent.display()))?;
        }
        write_file(&path, *size)?;
    }

    fs::create_dir_all(&to).map_err(|e| format!("Failed to create {}: {e}", to.display()))?;
    let blocker_names: HashSet<&str> = CONFLICT_BLOCKERS
        .iter()
        .filter_map(|(relative, _)| relative.split('/').next())
        .collect();
    for entry in fs::read_dir(&to).map_err(|e| format!("Failed to read {}: {e}", to.display()))? {
        let path = entry
            .map_err(|e| format!("Failed to read {}: {e}", to.display()))?
            .path();
        let is_blocker = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| blocker_names.contains(name));
        if !is_blocker {
            remove_entry(&path)?;
        }
    }
    for (relative, size) in CONFLICT_BLOCKERS {
        let path = to.join(relative);
        // Every level above the file has to be a folder, and the file itself a
        // file: an answered Overwrite swaps exactly one of them.
        for ancestor in path.ancestors().skip(1).take_while(|a| a.starts_with(&to) && *a != to) {
            if fs::symlink_metadata(ancestor).is_ok_and(|m| !m.is_dir()) {
                remove_entry(ancestor)?;
            }
        }
        if fs::symlink_metadata(&path).is_ok_and(|m| m.is_dir()) {
            remove_entry(&path)?;
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("Failed to create {}: {e}", parent.display()))?;
        }
        write_file(&path, *size)?;
    }

    Ok(ConflictPreviewFixtures {
        from_dir: from.to_string_lossy().into_owned(),
        to_dir: to.to_string_lossy().into_owned(),
        folder_over_file: CONFLICT_FOLDER_OVER_FILE.to_string(),
        file_over_folder: CONFLICT_FILE_OVER_FOLDER.to_string(),
        file_over_file: CONFLICT_FILE_OVER_FILE.to_string(),
    })
}

/// Removes one entry of a preview's destination, whatever kind it is.
fn remove_entry(path: &Path) -> Result<(), String> {
    let is_dir = fs::symlink_metadata(path).is_ok_and(|m| m.is_dir());
    let removed = if is_dir {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    };
    removed.map_err(|e| format!("Failed to remove {}: {e}", path.display()))
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

        let landmarks = ensure_dialog_gallery_fixtures(temp.path()).expect("first run should succeed");

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

        let first = ensure_dialog_gallery_fixtures(temp.path()).expect("first run should succeed");
        let entries_after_first = count_entries(&root);
        let sample = root.join("README.txt");
        let created_at = fs::metadata(&sample).expect("sample file").modified().ok();

        let second = ensure_dialog_gallery_fixtures(temp.path()).expect("second run should succeed");

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
        ensure_dialog_gallery_fixtures(temp.path()).expect("first run should succeed");

        let reviewer_folder = root.join("Folder the reviewer made");
        fs::create_dir(&reviewer_folder).expect("create reviewer folder");
        let reviewer_file = root.join("Photos/reviewer.txt");
        fs::write(&reviewer_file, b"kept").expect("write reviewer file");

        ensure_dialog_gallery_fixtures(temp.path()).expect("second run should succeed");

        assert!(reviewer_folder.is_dir(), "a reviewer-created folder was removed");
        assert_eq!(fs::read(&reviewer_file).expect("reviewer file"), b"kept");
    }

    /// Each conflict-preview name really clashes, as the kind the field names.
    #[test]
    fn the_conflict_preview_names_real_clashes() {
        let temp = tempfile::tempdir().expect("temp dir");
        let conflict = ensure_dialog_gallery_fixtures(temp.path())
            .expect("first run should succeed")
            .conflict_preview;
        let (from, to) = (Path::new(&conflict.from_dir), Path::new(&conflict.to_dir));

        assert!(from.join(&conflict.folder_over_file).is_dir());
        assert!(to.join(&conflict.folder_over_file).is_file());
        assert!(from.join(&conflict.file_over_folder).is_file());
        assert!(to.join(&conflict.file_over_folder).is_dir());
        assert!(from.join(&conflict.file_over_file).is_file());
        assert!(to.join(&conflict.file_over_file).is_file());
        assert!(
            !temp.path().join(FIXTURE_DIR_NAME).join("To").exists(),
            "the preview lives beside the main tree, so its listing never shows it"
        );
    }

    /// Whatever the last preview's answer did to the destination, the next
    /// trigger puts the clashes back: an Overwrite swapped a blocker's kind, and a
    /// Rename left a ` (1)` sibling.
    #[test]
    fn the_conflict_preview_destination_is_restored_after_an_answer() {
        let temp = tempfile::tempdir().expect("temp dir");
        let conflict = ensure_dialog_gallery_fixtures(temp.path())
            .expect("first run should succeed")
            .conflict_preview;
        let to = Path::new(&conflict.to_dir);
        // Overwrite answered on both cross-type clashes, and a Rename on the plain one.
        let folder_over_file = to.join(&conflict.folder_over_file);
        fs::remove_file(&folder_over_file).unwrap();
        fs::create_dir_all(folder_over_file.join("images")).unwrap();
        let file_over_folder = to.join(&conflict.file_over_folder);
        fs::remove_dir_all(&file_over_folder).unwrap();
        fs::write(&file_over_folder, b"the incoming file").unwrap();
        fs::write(to.join("Budget 2026 (1).xlsx"), b"renamed aside").unwrap();

        ensure_dialog_gallery_fixtures(temp.path()).expect("second run should succeed");

        for (relative, size) in CONFLICT_BLOCKERS {
            let path = to.join(relative);
            let metadata = fs::symlink_metadata(&path).unwrap_or_else(|e| panic!("{} missing: {e}", path.display()));
            assert!(metadata.is_file(), "{} should be a file again", path.display());
            assert_eq!(metadata.len(), *size);
        }
        let mut names: Vec<String> = fs::read_dir(to)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(
            names,
            [
                CONFLICT_FILE_OVER_FILE,
                CONFLICT_FILE_OVER_FOLDER,
                CONFLICT_FOLDER_OVER_FILE
            ],
            "only the blockers are left"
        );
    }

    /// A truncated (interrupted) fixture file is repaired rather than left short.
    #[test]
    fn repairs_a_file_with_the_wrong_length() {
        let temp = tempfile::tempdir().expect("temp dir");
        let root = temp.path().join(FIXTURE_DIR_NAME);
        ensure_dialog_gallery_fixtures(temp.path()).expect("first run should succeed");

        let damaged = root.join("Photos/2026-06 Testville/IMG_2201.jpg");
        fs::write(&damaged, b"truncated").expect("truncate fixture file");

        ensure_dialog_gallery_fixtures(temp.path()).expect("second run should succeed");

        assert_tree_matches(&root);
    }
}
