//! Which archive format a file name denotes.
//!
//! Format is decided by file-name SUFFIX (extension-only, no I/O), and this is
//! the single source of truth for it. `FileEntry.is_archive` uses
//! [`has_supported_archive_extension`] at listing time, where a magic-byte sniff
//! per entry would be a round-trip-per-file on a remote backend; the app's
//! boundary detector confirms with magic bytes once, at navigation time.
//!
//! Only the naming lives here. The decoders that unwrap a tar's outer
//! compression are the app's, next to the rest of the archive reading core.

use std::path::Path;

/// The archive formats Cmdr browses as folders.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveFormat {
    /// A zip archive (the first-class, read+write format).
    Zip,
    /// A tar archive, possibly wrapped in one whole-file compression stream.
    Tar(TarCodec),
    /// A 7z archive (read-only).
    SevenZ,
}

/// The outer compression wrapping a tar's byte stream. `Plain` is an
/// uncompressed `.tar` (random-access members); the rest wrap the whole tar in
/// one sequential stream (no random access — the archive reading core's
/// `index` module explains the sequential class).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TarCodec {
    /// An uncompressed `.tar`: members are randomly accessible.
    Plain,
    /// `.tar.gz` / `.tgz`.
    Gzip,
    /// `.tar.bz2` / `.tbz2` / `.tbz`.
    Bzip2,
    /// `.tar.xz` / `.txz`.
    Xz,
    /// `.tar.zst` / `.tzst`.
    Zstd,
}

impl ArchiveFormat {
    /// The format as a person (or a model) names it: `zip`, `tar`, `tar.gz`,
    /// `tar.bz2`, `tar.xz`, `tar.zst`, `7z`. The canonical suffix without its dot,
    /// so it round-trips through [`format_for_name`].
    pub fn label(self) -> &'static str {
        match self {
            ArchiveFormat::Zip => "zip",
            ArchiveFormat::Tar(TarCodec::Plain) => "tar",
            ArchiveFormat::Tar(TarCodec::Gzip) => "tar.gz",
            ArchiveFormat::Tar(TarCodec::Bzip2) => "tar.bz2",
            ArchiveFormat::Tar(TarCodec::Xz) => "tar.xz",
            ArchiveFormat::Tar(TarCodec::Zstd) => "tar.zst",
            ArchiveFormat::SevenZ => "7z",
        }
    }

    /// Whether extracting from this format is inherently SEQUENTIAL: the whole
    /// stream (or a solid block) must be decoded front-to-back, so there's no
    /// cheap random access to an arbitrary entry. A plain `.tar` and a `.zip` are
    /// random-access; a compressed tar and 7z are sequential.
    ///
    /// Drives the copy planner's one-pass strategy (the O(n²) trap: a per-entry
    /// random read of a sequential archive re-decodes the prefix every time).
    pub fn is_sequential(self) -> bool {
        match self {
            ArchiveFormat::Zip => false,
            ArchiveFormat::Tar(TarCodec::Plain) => false,
            ArchiveFormat::Tar(_) => true,
            ArchiveFormat::SevenZ => true,
        }
    }
}

/// The archive format a file NAME denotes, or `None` if it isn't a browsable
/// archive. Suffix-only, case-insensitive, no I/O.
///
/// Longest suffix wins, so `foo.tar.gz` is a gzip-compressed tar, not a plain
/// `.tar` (which its `.tar` substring would suggest) nor a bare `.gz`. A bare
/// `.gz` / `.xz` / `.bz2` / `.zst` (a single compressed file, not a tar) is
/// deliberately NOT an archive — there's nothing to browse.
pub fn format_for_name(name: &str) -> Option<ArchiveFormat> {
    let lower = name.to_ascii_lowercase();
    // Ordered longest-first so `.tar.gz` matches before `.tar`. Each arm's suffix
    // must be preceded by a real stem (a leading-dot dotfile like `.tar` is not an
    // archive), matching `Path::extension`'s "needs a stem" rule.
    const SUFFIXES: &[(&str, ArchiveFormat)] = &[
        (".tar.gz", ArchiveFormat::Tar(TarCodec::Gzip)),
        (".tar.bz2", ArchiveFormat::Tar(TarCodec::Bzip2)),
        (".tar.xz", ArchiveFormat::Tar(TarCodec::Xz)),
        (".tar.zst", ArchiveFormat::Tar(TarCodec::Zstd)),
        (".tgz", ArchiveFormat::Tar(TarCodec::Gzip)),
        (".tbz2", ArchiveFormat::Tar(TarCodec::Bzip2)),
        (".tbz", ArchiveFormat::Tar(TarCodec::Bzip2)),
        (".txz", ArchiveFormat::Tar(TarCodec::Xz)),
        (".tzst", ArchiveFormat::Tar(TarCodec::Zstd)),
        (".tar", ArchiveFormat::Tar(TarCodec::Plain)),
        (".zip", ArchiveFormat::Zip),
        (".7z", ArchiveFormat::SevenZ),
    ];
    for (suffix, format) in SUFFIXES {
        if lower.ends_with(suffix) && lower.len() > suffix.len() {
            return Some(*format);
        }
    }
    None
}

/// The archive format the last component of `path` denotes (see
/// [`format_for_name`]).
pub fn format_for_path(path: &Path) -> Option<ArchiveFormat> {
    path.file_name().and_then(|n| n.to_str()).and_then(format_for_name)
}

/// Whether `name` ends in a browsable-archive extension. A pure string check, no
/// I/O: it's what `FileEntry::new` sets `is_archive` from, for every entry of
/// every listing.
pub fn has_supported_archive_extension(name: &str) -> bool {
    format_for_name(name).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_label_round_trips_through_format_for_name() {
        for format in [
            ArchiveFormat::Zip,
            ArchiveFormat::Tar(TarCodec::Plain),
            ArchiveFormat::Tar(TarCodec::Gzip),
            ArchiveFormat::Tar(TarCodec::Bzip2),
            ArchiveFormat::Tar(TarCodec::Xz),
            ArchiveFormat::Tar(TarCodec::Zstd),
            ArchiveFormat::SevenZ,
        ] {
            assert_eq!(format_for_name(&format!("a.{}", format.label())), Some(format));
        }
    }

    #[test]
    fn detects_tar_codecs_by_suffix() {
        assert_eq!(format_for_name("a.tar"), Some(ArchiveFormat::Tar(TarCodec::Plain)));
        assert_eq!(format_for_name("a.tar.gz"), Some(ArchiveFormat::Tar(TarCodec::Gzip)));
        assert_eq!(format_for_name("a.TGZ"), Some(ArchiveFormat::Tar(TarCodec::Gzip)));
        assert_eq!(format_for_name("a.tar.xz"), Some(ArchiveFormat::Tar(TarCodec::Xz)));
        assert_eq!(format_for_name("a.txz"), Some(ArchiveFormat::Tar(TarCodec::Xz)));
        assert_eq!(format_for_name("a.tar.bz2"), Some(ArchiveFormat::Tar(TarCodec::Bzip2)));
        assert_eq!(format_for_name("a.tbz2"), Some(ArchiveFormat::Tar(TarCodec::Bzip2)));
        assert_eq!(format_for_name("a.tar.zst"), Some(ArchiveFormat::Tar(TarCodec::Zstd)));
        assert_eq!(format_for_name("a.tzst"), Some(ArchiveFormat::Tar(TarCodec::Zstd)));
        assert_eq!(format_for_name("a.zip"), Some(ArchiveFormat::Zip));
        assert_eq!(format_for_name("a.7z"), Some(ArchiveFormat::SevenZ));
    }

    #[test]
    fn tar_gz_wins_over_bare_tar_or_gz() {
        // The `.tar.gz` suffix is gzip-compressed, not the plain `.tar` its
        // substring suggests, and a bare `.gz` (not a tar) is not an archive.
        assert_eq!(
            format_for_name("backup.tar.gz"),
            Some(ArchiveFormat::Tar(TarCodec::Gzip))
        );
        assert_eq!(format_for_name("photo.gz"), None);
        assert_eq!(format_for_name("data.xz"), None);
    }

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
    fn non_archives_and_dotfiles_are_none() {
        assert_eq!(format_for_name("notes.txt"), None);
        assert_eq!(format_for_name("tar"), None);
        // A dotfile with no stem is not an archive.
        assert_eq!(format_for_name(".tar"), None);
        assert_eq!(format_for_name(".zip"), None);
        assert_eq!(format_for_name(".7z"), None);
    }

    #[test]
    fn sequential_class_is_correct_per_format() {
        assert!(!ArchiveFormat::Zip.is_sequential());
        assert!(!ArchiveFormat::Tar(TarCodec::Plain).is_sequential());
        assert!(ArchiveFormat::Tar(TarCodec::Gzip).is_sequential());
        assert!(ArchiveFormat::SevenZ.is_sequential());
    }
}
