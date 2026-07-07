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

use super::read::{ArchiveFormat, TarCodec, format_for_name, format_for_path};

/// Longest header prefix any format's magic check needs. A plain `.tar`'s ustar
/// magic sits at offset 257, so one 512-byte tar block covers every format
/// (zip/gzip/bzip2/xz/zstd/7z magics all live at offset 0).
pub const ARCHIVE_MAGIC_PREFIX_LEN: usize = 512;

/// True if `name`'s SUFFIX denotes a supported archive format (case-insensitive).
///
/// Extension-only, no I/O. Suffix-based (not just the last `.ext`) so `.tar.gz`
/// counts while a bare `.gz` doesn't — see [`format_for_name`], the single source
/// of truth this delegates to. Drives `FileEntry.is_archive` at listing time and
/// the cheap pre-filter in [`archive_boundary_candidate`].
pub fn has_supported_archive_extension(name: &str) -> bool {
    format_for_name(name).is_some()
}

/// Whether `header` (a file's first bytes, ideally [`ARCHIVE_MAGIC_PREFIX_LEN`]
/// long) carries the magic signature for `format`. Short slices simply fail to
/// match (never panic), so a truncated read declines the route rather than
/// mis-detecting. Shared by the local sniff and the REMOTE confirm in
/// `VolumeManager`, so both agree on what "is a `<format>`" means.
pub fn bytes_match_archive_magic(format: ArchiveFormat, header: &[u8]) -> bool {
    match format {
        ArchiveFormat::Zip => bytes_start_with_zip_signature(header),
        // A plain tar has no signature at offset 0; the ustar magic sits at 257.
        // (A pre-POSIX v7 tar has none at all — those are accepted by extension +
        // a successful parse rather than magic, but modern tars are ustar/GNU/pax.)
        ArchiveFormat::Tar(TarCodec::Plain) => matches!(header.get(257..262), Some(b"ustar")),
        ArchiveFormat::Tar(TarCodec::Gzip) => header.starts_with(&[0x1f, 0x8b]),
        ArchiveFormat::Tar(TarCodec::Bzip2) => header.starts_with(b"BZh"),
        ArchiveFormat::Tar(TarCodec::Xz) => header.starts_with(&[0xfd, b'7', b'z', b'X', b'Z', 0x00]),
        ArchiveFormat::Tar(TarCodec::Zstd) => header.starts_with(&[0x28, 0xb5, 0x2f, 0xfd]),
        ArchiveFormat::SevenZ => header.starts_with(&[0x37, 0x7a, 0xbc, 0xaf, 0x27, 0x1c]),
    }
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
/// must lose to normal directory navigation — and (b) its bytes carry the
/// format's magic ([`bytes_match_archive_magic`]), so a mislabeled non-archive
/// isn't routed. Returns the split only when both hold.
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
    let format = format_for_path(&archive_path)?;
    if !file_matches_archive_magic(&archive_path, format) {
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

/// True when a path targets an archive FILE Cmdr tried to browse — the archive
/// component exists and is a regular file (not a directory that merely shares the
/// name). Path + a cheap stat, no magic check: the caller pairs it with the
/// listing error kind to decide whether a FAILED listing was a damaged/mislabeled
/// archive (`NotSupported`/`IoError`) versus a valid archive with a missing inner
/// path (`NotFound`). A real directory named `foo.zip` returns false — it lists
/// like any folder.
pub fn path_targets_archive_file(path: &Path) -> bool {
    let Some((archive_path, _inner)) = archive_boundary_candidate(path) else {
        return false;
    };
    std::fs::metadata(&archive_path).is_ok_and(|meta| meta.is_file())
}

/// Reads up to [`ARCHIVE_MAGIC_PREFIX_LEN`] bytes and checks them against
/// `format`'s magic ([`bytes_match_archive_magic`]). Best-effort read: a file
/// shorter than the prefix (a tiny zip) yields fewer bytes, and the magic check
/// tolerates that. A file that can't be opened isn't an archive.
fn file_matches_archive_magic(path: &Path, format: ArchiveFormat) -> bool {
    use std::io::Read;
    let Ok(mut file) = std::fs::File::open(path) else {
        return false;
    };
    let mut buf = [0u8; ARCHIVE_MAGIC_PREFIX_LEN];
    let mut filled = 0;
    while filled < buf.len() {
        match file.read(&mut buf[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(_) => return false,
        }
    }
    bytes_match_archive_magic(format, &buf[..filled])
}

/// Whether `bytes` (a file's first four bytes) match one of the three zip
/// start-of-file signatures: a local file header (`PK\x03\x04`), an empty
/// archive's end-of-central-directory (`PK\x05\x06`), or the spanned-archive
/// marker (`PK\x07\x08`). Fewer than four bytes isn't a zip.
///
/// The single magic-byte predicate shared by the local sniff
/// ([`file_starts_with_zip_signature`]) and the REMOTE confirm in
/// `VolumeManager::resolve` (which reads the first four bytes over the parent
/// volume's `read_range`), so local and remote agree on what "is a zip" means.
pub fn bytes_start_with_zip_signature(bytes: &[u8]) -> bool {
    matches!(
        bytes.get(..4),
        Some(b"PK\x03\x04") | Some(b"PK\x05\x06") | Some(b"PK\x07\x08")
    )
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
    fn targets_archive_file_true_for_a_real_file_named_like_an_archive() {
        let dir = tempfile::tempdir().expect("tempdir");
        // A mislabeled file (no zip magic) still counts: browsing it failed because
        // it isn't a real archive, which is the "unreadable archive" case.
        let mislabeled = dir.path().join("notreally.zip");
        std::fs::write(&mislabeled, b"plain text").expect("write file");
        assert!(path_targets_archive_file(&mislabeled));
        assert!(path_targets_archive_file(&mislabeled.join("inner")));
    }

    #[test]
    fn targets_archive_file_false_for_a_directory_or_a_non_archive_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        // A real directory named like an archive lists normally, so it's not the case.
        let fake = dir.path().join("foo.zip");
        std::fs::create_dir(&fake).expect("create dir named foo.zip");
        assert!(!path_targets_archive_file(&fake));
        // No archive-extension component at all.
        assert!(!path_targets_archive_file(&dir.path().join("plain/folder")));
        // Archive-extension name that doesn't exist on disk.
        assert!(!path_targets_archive_file(&dir.path().join("ghost.zip")));
    }

    #[test]
    fn confirm_is_none_when_the_archive_component_does_not_exist() {
        let dir = tempfile::tempdir().expect("tempdir");
        let missing = dir.path().join("ghost.zip").join("inner");
        assert!(confirm_archive_boundary(&missing).is_none());
    }

    // ---- Multi-format extension + magic ------------------------------------

    #[test]
    fn extension_check_recognizes_the_tar_family_and_7z() {
        for name in [
            "a.tar",
            "a.tar.gz",
            "a.tgz",
            "a.tar.xz",
            "a.txz",
            "a.tar.bz2",
            "a.tbz2",
            "a.tar.zst",
            "a.tzst",
            "a.7z",
            "a.TAR.GZ",
        ] {
            assert!(has_supported_archive_extension(name), "{name} should be an archive");
        }
        // A bare compressed file (not a tar) is not a browsable archive.
        assert!(!has_supported_archive_extension("photo.gz"));
        assert!(!has_supported_archive_extension("data.zst"));
    }

    #[test]
    fn confirm_matches_per_format_magic() {
        let dir = tempfile::tempdir().expect("tempdir");

        // A plain tar has no offset-0 magic; ustar sits at 257.
        let tar = dir.path().join("real.tar");
        let mut tar_bytes = vec![0u8; 512];
        tar_bytes[257..262].copy_from_slice(b"ustar");
        std::fs::write(&tar, &tar_bytes).expect("write tar");
        assert!(confirm_archive_boundary(&tar.join("inner")).is_some());

        // gzip-wrapped tar.
        let tgz = dir.path().join("real.tar.gz");
        std::fs::write(&tgz, [0x1f, 0x8b, 0x08, 0x00]).expect("write tgz");
        let (zip, inner) = confirm_archive_boundary(&tgz.join("d/f.txt")).expect("tgz boundary");
        assert_eq!(zip, tgz);
        assert_eq!(inner, PathBuf::from("d/f.txt"));

        // 7z magic.
        let sevenz = dir.path().join("real.7z");
        std::fs::write(&sevenz, [0x37, 0x7a, 0xbc, 0xaf, 0x27, 0x1c, 0x00]).expect("write 7z");
        assert!(confirm_archive_boundary(&sevenz.join("x")).is_some());
    }

    #[test]
    fn confirm_rejects_a_mislabeled_tar_gz() {
        // A `.tar.gz` name whose bytes aren't gzip must not route.
        let dir = tempfile::tempdir().expect("tempdir");
        let fake = dir.path().join("notreally.tar.gz");
        std::fs::write(&fake, b"plain text, definitely not gzip").expect("write file");
        assert!(confirm_archive_boundary(&fake.join("inner")).is_none());
    }

    #[test]
    fn candidate_splits_a_double_extension_at_the_whole_component() {
        // `.tar.gz` is one path component, so the split includes the full name —
        // never `foo.tar` with `.gz` as inner.
        let (zip, inner) = archive_boundary_candidate(Path::new("/a/foo.tar.gz/inner/b.txt")).expect("boundary");
        assert_eq!(zip, PathBuf::from("/a/foo.tar.gz"));
        assert_eq!(inner, PathBuf::from("inner/b.txt"));
    }
}
