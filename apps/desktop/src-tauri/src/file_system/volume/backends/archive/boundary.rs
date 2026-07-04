//! Archive-boundary detection: does a path cross into a supported archive?
//!
//! The transparent-path model presents a `.zip` as a folder: navigating to
//! `/path/to/foo.zip/inner` should route to the read-only `ArchiveVolume` for
//! `foo.zip` and list `inner` inside it. This module owns the ONE detector both
//! `VolumeManager::resolve` and `commands/volumes.rs` share, so the pane label
//! and the I/O target can never disagree (two drifting detectors is the failure
//! this centralizes away).
//!
//! Two tiers, by cost:
//!
//! - [`archive_boundary_candidate`] / [`has_supported_archive_extension`] are
//!   pure string checks (no I/O). Extension-only is deliberate: it's what
//!   `FileEntry.is_archive` uses at listing time, where a magic-byte sniff per
//!   entry would be a round-trip-per-file on a remote backend (principles 3/5).
//! - [`confirm_archive_boundary`] adds the I/O the string check can't do: it
//!   confirms the candidate component is a real FILE (a directory literally
//!   named `foo.zip` must lose to normal directory navigation) and that its
//!   first bytes are a zip signature (a mislabeled file isn't routed). This runs
//!   once per navigation, only when a component carries an archive extension.

use std::path::{Component, Path, PathBuf};

/// Archive file extensions this build routes INTO as browsable folders.
///
/// Extension-only, lowercased, no leading dot. ZIP is the only browsable format
/// for now; the later tar/7z read milestone extends this ONE set (it's the
/// single source of truth shared by `FileEntry.is_archive` and boundary
/// detection). Magic-byte confirmation ([`confirm_archive_boundary`]) is
/// zip-specific and gains sibling checks when that set grows.
pub const SUPPORTED_ARCHIVE_EXTENSIONS: &[&str] = &["zip"];

/// True if `name`'s extension is a supported archive format (case-insensitive).
///
/// Extension-only, no I/O. A name with no extension (`zip`) or a dotfile with no
/// stem (`.zip`) is not an archive. Drives `FileEntry.is_archive` at listing
/// time and the cheap pre-filter in [`archive_boundary_candidate`].
pub fn has_supported_archive_extension(name: &str) -> bool {
    Path::new(name)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .is_some_and(|ext| SUPPORTED_ARCHIVE_EXTENSIONS.contains(&ext.as_str()))
}

/// The extension-only boundary candidate (no I/O). If a path component (scanning
/// from the root) carries a supported archive extension, split there: return
/// `(archive_path, inner_path)` where `archive_path` is the path up to and
/// including the FIRST such component and `inner_path` is the remainder (empty
/// when the archive component is the last one).
///
/// Leftmost match wins, so a nested `a.zip/b.zip/…` routes into `a.zip` and
/// treats `b.zip` as a plain inner entry — nested archives are out of scope
/// (an inner archive is a file, not a recursively-browsable volume).
///
/// This only looks at the path string; it does NOT confirm the component is a
/// real archive file. Use [`confirm_archive_boundary`] at navigation time.
pub fn archive_boundary_candidate(path: &Path) -> Option<(PathBuf, PathBuf)> {
    let mut archive_path = PathBuf::new();
    let mut components = path.components();
    while let Some(component) = components.next() {
        archive_path.push(component.as_os_str());
        if let Component::Normal(name) = component
            && let Some(name) = name.to_str()
            && has_supported_archive_extension(name)
        {
            // `components` has already advanced past the matched component, so
            // its remaining path is exactly the inner path.
            return Some((archive_path, components.as_path().to_path_buf()));
        }
    }
    None
}

/// The I/O-backed boundary check used at navigation / resolve time.
///
/// Runs [`archive_boundary_candidate`], then confirms the candidate is a real
/// archive: (a) the component is a FILE — a directory literally named `foo.zip`
/// must lose to normal directory navigation — and (b) its first bytes are a zip
/// signature, so a mislabeled non-archive isn't routed. Returns the split only
/// when both hold.
///
/// Blocking (a local stat + a few-byte read). Callers on the async executor
/// accept it because it runs once per navigation and only when a path component
/// carries an archive extension (the pure candidate check gates the I/O).
/// Remote-backed archives (a later milestone) revisit the sync sniff.
pub fn confirm_archive_boundary(path: &Path) -> Option<(PathBuf, PathBuf)> {
    let (archive_path, inner_path) = archive_boundary_candidate(path)?;
    let metadata = std::fs::metadata(&archive_path).ok()?;
    if !metadata.is_file() {
        return None;
    }
    if !file_starts_with_zip_signature(&archive_path) {
        return None;
    }
    Some((archive_path, inner_path))
}

/// Whether `path` crosses into a confirmed supported archive — the `.zip` file
/// ITSELF (empty inner) OR a path inside it. This is the "enter the archive"
/// predicate: navigation and listing route the `.zip` path here to browse the
/// archive root.
///
/// For write/copy/viewer sites that operate ON a path, prefer
/// [`path_is_inside_archive`]: those must treat the `.zip` file itself as a
/// normal file (copy/move/rename/view it), and only refuse or route paths
/// genuinely INSIDE the archive.
pub fn path_crosses_archive_boundary(path: &Path) -> bool {
    confirm_archive_boundary(path).is_some()
}

/// Whether `path` points at something INSIDE a confirmed archive (a non-empty
/// inner path like `foo.zip/entry`), as opposed to the `.zip` file itself.
///
/// This is the predicate for sites that operate ON a path rather than navigate
/// INTO it: copy/move/delete/rename guards, source routing, drag locality, and
/// the viewer. The `.zip` file itself is a regular file — copying, moving,
/// renaming, or viewing it must behave exactly like any other file — so those
/// sites must NOT treat it as archive-internal. Only a genuinely-inner path is
/// read-only / extract-routed / non-materializable.
pub fn path_is_inside_archive(path: &Path) -> bool {
    confirm_archive_boundary(path).is_some_and(|(_zip, inner)| !inner.as_os_str().is_empty())
}

/// Reads the first four bytes and checks them against the three zip start-of-file
/// signatures: a local file header (`PK\x03\x04`), an empty archive's end-of-
/// central-directory (`PK\x05\x06`), or the spanned-archive marker (`PK\x07\x08`).
/// A file shorter than four bytes, or one that can't be opened, isn't a zip.
fn file_starts_with_zip_signature(path: &Path) -> bool {
    use std::io::Read;
    let Ok(mut file) = std::fs::File::open(path) else {
        return false;
    };
    let mut buf = [0u8; 4];
    if file.read_exact(&mut buf).is_err() {
        return false;
    }
    matches!(&buf, b"PK\x03\x04" | b"PK\x05\x06" | b"PK\x07\x08")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extension_check_is_case_insensitive_and_needs_a_real_stem() {
        assert!(has_supported_archive_extension("foo.zip"));
        assert!(has_supported_archive_extension("foo.ZIP"));
        assert!(has_supported_archive_extension("archive.name.zip"));
        // Not archives:
        assert!(!has_supported_archive_extension("foo.txt"));
        // A name that IS the word "zip" with no dot is not an archive.
        assert!(!has_supported_archive_extension("zip"));
        // A dotfile with no stem (`.zip`) has no extension.
        assert!(!has_supported_archive_extension(".zip"));
        // The archive extension must be the LAST one.
        assert!(!has_supported_archive_extension("foo.zip.txt"));
    }

    #[test]
    fn candidate_splits_at_the_archive_component() {
        let (zip, inner) = archive_boundary_candidate(Path::new("/a/foo.zip/inner/b.txt")).expect("boundary");
        assert_eq!(zip, PathBuf::from("/a/foo.zip"));
        assert_eq!(inner, PathBuf::from("inner/b.txt"));
    }

    #[test]
    fn candidate_matches_a_trailing_archive_with_empty_inner() {
        let (zip, inner) = archive_boundary_candidate(Path::new("/a/foo.zip")).expect("boundary");
        assert_eq!(zip, PathBuf::from("/a/foo.zip"));
        assert_eq!(inner, PathBuf::from(""));
    }

    #[test]
    fn candidate_is_none_without_an_archive_component() {
        assert!(archive_boundary_candidate(Path::new("/a/b/c.txt")).is_none());
        assert!(archive_boundary_candidate(Path::new("/a/b/c")).is_none());
    }

    #[test]
    fn candidate_is_case_insensitive_mid_path() {
        let (zip, inner) = archive_boundary_candidate(Path::new("/a/FOO.ZIP/x")).expect("boundary");
        assert_eq!(zip, PathBuf::from("/a/FOO.ZIP"));
        assert_eq!(inner, PathBuf::from("x"));
    }

    #[test]
    fn candidate_leftmost_wins_so_nested_archives_are_not_recursed() {
        // The inner `b.zip` is a plain entry inside `a.zip`, not a second
        // boundary: the leftmost archive component wins.
        let (zip, inner) = archive_boundary_candidate(Path::new("/a.zip/b.zip/x")).expect("boundary");
        assert_eq!(zip, PathBuf::from("/a.zip"));
        assert_eq!(inner, PathBuf::from("b.zip/x"));
    }

    #[test]
    fn confirm_accepts_a_real_zip_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let zip = dir.path().join("real.zip");
        // A minimal but valid start-of-file zip signature is enough for the
        // magic check (which reads only the first four bytes).
        std::fs::write(&zip, b"PK\x03\x04rest-of-archive").expect("write zip");

        let inner = zip.join("inner/a.txt");
        let (got_zip, got_inner) = confirm_archive_boundary(&inner).expect("confirmed boundary");
        assert_eq!(got_zip, zip);
        assert_eq!(got_inner, PathBuf::from("inner/a.txt"));
    }

    #[test]
    fn confirm_rejects_a_real_directory_named_like_an_archive() {
        // A directory literally named `foo.zip` must lose to normal directory
        // navigation, never route into an archive volume.
        let dir = tempfile::tempdir().expect("tempdir");
        let fake = dir.path().join("foo.zip");
        std::fs::create_dir(&fake).expect("create dir named foo.zip");
        std::fs::create_dir(fake.join("sub")).expect("create child");

        assert!(confirm_archive_boundary(&fake.join("sub")).is_none());
        assert!(!path_crosses_archive_boundary(&fake.join("sub")));
    }

    #[test]
    fn confirm_rejects_a_mislabeled_non_zip_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mislabeled = dir.path().join("notreally.zip");
        std::fs::write(&mislabeled, b"this is plain text, not a zip").expect("write file");

        assert!(confirm_archive_boundary(&mislabeled.join("inner")).is_none());
    }

    #[test]
    fn inside_archive_distinguishes_the_zip_file_itself_from_its_contents() {
        let dir = tempfile::tempdir().expect("tempdir");
        let zip = dir.path().join("real.zip");
        std::fs::write(&zip, b"PK\x03\x04rest-of-archive").expect("write zip");

        // A path INSIDE the archive is "inside"...
        assert!(path_is_inside_archive(&zip.join("inner/a.txt")));
        // ...but the `.zip` file ITSELF is NOT inside — it's a regular file, even
        // though it DOES "cross" a boundary (navigation enters it).
        assert!(!path_is_inside_archive(&zip));
        assert!(path_crosses_archive_boundary(&zip));
        // A plain non-archive path is neither.
        assert!(!path_is_inside_archive(dir.path()));
        assert!(!path_crosses_archive_boundary(dir.path()));
    }

    #[test]
    fn confirm_is_none_when_the_archive_component_does_not_exist() {
        let dir = tempfile::tempdir().expect("tempdir");
        let missing = dir.path().join("ghost.zip").join("inner");
        assert!(confirm_archive_boundary(&missing).is_none());
    }
}
