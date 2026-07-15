//! Filesystem identity and the per-file size limit derived from it.
//!
//! Two deliberately separate axes:
//! - [`FilesystemKind`]: WHAT the destination filesystem is (a fact). Drives the
//!   volume-picker label and any logic that keys off the format.
//! - [`MaxFileSize`]: whether the filesystem caps per-file size, derived purely
//!   from the kind. The oversized-file write guard reads this; `Unknown` and
//!   `Unlimited` never block, only `Limited` does.
//!
//! The kind → limit map (`FilesystemKind::max_file_size`) is the single source
//! of truth: the write guard, the volume DTO, and any future caller all read it.
//!
//! The pure logic (the enums, `from_raw_type`, the map) has no platform deps and
//! is unit-tested directly. `detect_filesystem_for_path` is the thin
//! platform-specific wiring that resolves a path to its OS filesystem-type
//! string and feeds it in.

use serde::{Deserialize, Serialize};

/// FAT32's hard per-file ceiling: 4 GiB minus one byte, the largest value a
/// 32-bit byte-count field can hold. A file of exactly 4 GiB does not fit.
pub const FAT32_MAX_FILE_SIZE: u64 = u32::MAX as u64; // 4_294_967_295

/// What a destination filesystem is. A factual classification used for the
/// volume-picker label and to derive the per-file size limit. `Other` carries
/// no detail of its own; the raw OS type string travels alongside in
/// [`FilesystemInfo::raw_type`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum FilesystemKind {
    Apfs,
    HfsPlus,
    Ext4,
    Btrfs,
    Xfs,
    Zfs,
    Ntfs,
    ExFat,
    /// FAT32 (and FAT16): the only common format with a hard 4 GiB per-file cap.
    Fat32,
    /// SMB share whose backing filesystem we can't see (an OS-mounted
    /// `smbfs`/`cifs` mount). Direct smb2 sessions could later resolve the real
    /// backing format; until then this stays `Unknown` for the size guard.
    Smb,
    /// MTP device. No queryable POSIX filesystem; large files stream fine.
    Mtp,
    /// Recognized nothing. The raw type string is kept in `FilesystemInfo` for
    /// display and diagnostics.
    Other,
}

impl FilesystemKind {
    /// Classifies a raw filesystem-type string from macOS `statfs.f_fstypename`,
    /// Linux `/proc/mounts`, or an SMB server's reported `FileSystemName`.
    /// Case-insensitive. Unrecognized strings become [`FilesystemKind::Other`].
    pub fn from_raw_type(raw: &str) -> Self {
        match raw.to_ascii_lowercase().as_str() {
            "apfs" => Self::Apfs,
            "hfs" | "hfsplus" | "hfs+" => Self::HfsPlus,
            "ext4" | "ext3" | "ext2" => Self::Ext4,
            "btrfs" => Self::Btrfs,
            "xfs" => Self::Xfs,
            "zfs" => Self::Zfs,
            "ntfs" | "ntfs3" | "ufsd_ntfs" => Self::Ntfs,
            "exfat" | "fuse.exfat" => Self::ExFat,
            // macOS reports both FAT16 and FAT32 as "msdos"; Linux as "vfat".
            // Both carry the 4 GiB cap, so the safe number covers either.
            "msdos" | "vfat" | "fat" | "fat32" | "fat16" => Self::Fat32,
            "smbfs" | "cifs" => Self::Smb,
            _ => Self::Other,
        }
    }

    /// The largest single file this filesystem accepts. The single source of
    /// truth for the oversized-file write guard.
    ///
    /// Only [`FilesystemKind::Fat32`] is `Limited`. Modern formats (and MTP) are
    /// `Unlimited`; formats we can't see through (`Smb`, `Other`) are `Unknown`,
    /// which the guard treats as "don't block" so it never raises a false alarm.
    pub fn max_file_size(self) -> MaxFileSize {
        match self {
            Self::Fat32 => MaxFileSize::Limited {
                bytes: FAT32_MAX_FILE_SIZE,
            },
            Self::Apfs
            | Self::HfsPlus
            | Self::Ext4
            | Self::Btrfs
            | Self::Xfs
            | Self::Zfs
            | Self::Ntfs
            | Self::ExFat
            | Self::Mtp => MaxFileSize::Unlimited,
            Self::Smb | Self::Other => MaxFileSize::Unknown,
        }
    }

    /// Whether this filesystem's inode (`st_ino`) is a stable, trustworthy
    /// identity for a file across its lifetime.
    ///
    /// `false` for FAT32 and exFAT: neither stores an inode, so macOS/Linux
    /// DERIVE `st_ino` from the file's first data cluster. That derived value is
    /// unstable — writing content into an empty file changes it, and a
    /// delete+create ALIASES a fresh, unrelated file onto a freed cluster's
    /// inode. The live-indexing rename pre-pass (`find_entry_by_inode` →
    /// `MoveEntryV2`) keys off inode identity, so on these formats it would both
    /// miss real renames AND, worse, mistake an inode-reused delete+create for a
    /// move (re-homing the old entry's `dir_stats` onto an unrelated file). Every
    /// other format we recognize (APFS, HFS+, ext4/btrfs/XFS/ZFS, NTFS) keeps the
    /// inode stable across rename. `Smb`/`Mtp`/`Other` return `true` (their
    /// indexes don't run the local inode-keyed rename pre-pass), so only the two
    /// derived-inode formats opt out.
    pub fn has_stable_inodes(self) -> bool {
        !matches!(self, Self::Fat32 | Self::ExFat)
    }
}

/// The largest single file a filesystem accepts. Derived from [`FilesystemKind`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum MaxFileSize {
    /// A hard per-file ceiling in bytes (FAT32). A larger file is rejected.
    Limited { bytes: u64 },
    /// No practical per-file limit (APFS, ext4, NTFS, exFAT, MTP, ...).
    Unlimited,
    /// The limit couldn't be determined. The write guard treats this as
    /// "don't block".
    Unknown,
}

/// A destination filesystem's identity plus the bits the frontend needs to show
/// it. Rides on the volume DTO (volume picker) and on the oversized-file error.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct FilesystemInfo {
    pub kind: FilesystemKind,
    /// Raw type string as reported by the OS (`statfs`/mounts) or an SMB server,
    /// for display fallback on [`FilesystemKind::Other`] and for diagnostics.
    pub raw_type: Option<String>,
    /// The largest single file this filesystem accepts, in bytes. `None` means
    /// no known limit (or unknown), so the picker can show "max 4 GB per file"
    /// only when a real cap exists.
    pub max_file_size_bytes: Option<u64>,
}

impl FilesystemInfo {
    /// Builds from a known kind plus the raw type string it came from.
    pub fn new(kind: FilesystemKind, raw_type: Option<String>) -> Self {
        let max_file_size_bytes = match kind.max_file_size() {
            MaxFileSize::Limited { bytes } => Some(bytes),
            MaxFileSize::Unlimited | MaxFileSize::Unknown => None,
        };
        Self {
            kind,
            raw_type,
            max_file_size_bytes,
        }
    }

    /// Builds from a raw OS filesystem-type string (the common path). `None`
    /// classifies as [`FilesystemKind::Other`].
    pub fn from_raw_type(raw_type: Option<String>) -> Self {
        let kind = raw_type
            .as_deref()
            .map(FilesystemKind::from_raw_type)
            .unwrap_or(FilesystemKind::Other);
        Self::new(kind, raw_type)
    }
}

/// Detects the filesystem at `path` by resolving its mount and reading the OS
/// filesystem-type string, then classifying it.
///
/// Returns [`FilesystemKind::Other`] / [`MaxFileSize::Unknown`] when the type
/// can't be resolved, so the write guard never blocks on a guess.
///
/// macOS resolves via `statfs.f_fstypename`; other Unix via `/proc/mounts`.
/// The single `statfs` is fast on local mounts (the only ones that reach the
/// local-FS copy/move path); a hung network mount would already have stalled the
/// preceding free-space query on the same destination.
#[cfg(target_os = "macos")]
pub fn detect_filesystem_for_path(path: &std::path::Path) -> FilesystemInfo {
    let raw = crate::volumes::get_mount_point(&path.to_string_lossy()).map(|(_, fs_type)| fs_type);
    FilesystemInfo::from_raw_type(raw)
}

#[cfg(target_os = "linux")]
pub fn detect_filesystem_for_path(path: &std::path::Path) -> FilesystemInfo {
    let raw = crate::file_system::linux_mounts::fs_type_for_path(path);
    FilesystemInfo::from_raw_type(raw)
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn detect_filesystem_for_path(_path: &std::path::Path) -> FilesystemInfo {
    FilesystemInfo::from_raw_type(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_known_raw_types_case_insensitively() {
        assert_eq!(FilesystemKind::from_raw_type("apfs"), FilesystemKind::Apfs);
        assert_eq!(FilesystemKind::from_raw_type("APFS"), FilesystemKind::Apfs);
        assert_eq!(FilesystemKind::from_raw_type("msdos"), FilesystemKind::Fat32);
        assert_eq!(FilesystemKind::from_raw_type("vfat"), FilesystemKind::Fat32);
        assert_eq!(FilesystemKind::from_raw_type("exfat"), FilesystemKind::ExFat);
        assert_eq!(FilesystemKind::from_raw_type("ntfs"), FilesystemKind::Ntfs);
        assert_eq!(FilesystemKind::from_raw_type("smbfs"), FilesystemKind::Smb);
        assert_eq!(FilesystemKind::from_raw_type("cifs"), FilesystemKind::Smb);
    }

    #[test]
    fn unrecognized_raw_type_is_other() {
        assert_eq!(FilesystemKind::from_raw_type("zalgofs"), FilesystemKind::Other);
        assert_eq!(FilesystemKind::from_raw_type(""), FilesystemKind::Other);
    }

    #[test]
    fn only_fat_is_limited_exfat_is_not() {
        // The load-bearing distinction: exFAT (the common big-USB format) must
        // NOT be capped, only FAT32.
        assert_eq!(
            FilesystemKind::Fat32.max_file_size(),
            MaxFileSize::Limited {
                bytes: FAT32_MAX_FILE_SIZE
            }
        );
        assert_eq!(FilesystemKind::ExFat.max_file_size(), MaxFileSize::Unlimited);
        assert_eq!(FilesystemKind::Ntfs.max_file_size(), MaxFileSize::Unlimited);
        assert_eq!(FilesystemKind::Apfs.max_file_size(), MaxFileSize::Unlimited);
        assert_eq!(FilesystemKind::Mtp.max_file_size(), MaxFileSize::Unlimited);
    }

    #[test]
    fn only_derived_inode_filesystems_lack_stable_inodes() {
        // FAT32 and exFAT synthesize `st_ino` from the first data cluster, so it
        // isn't a trustworthy identity: the local rename pre-pass must not key off
        // it (delete+create inode reuse would false-match a move).
        assert!(!FilesystemKind::Fat32.has_stable_inodes());
        assert!(!FilesystemKind::ExFat.has_stable_inodes());
        // Every other recognized format keeps the inode stable across rename.
        assert!(FilesystemKind::Apfs.has_stable_inodes());
        assert!(FilesystemKind::HfsPlus.has_stable_inodes());
        assert!(FilesystemKind::Ext4.has_stable_inodes());
        assert!(FilesystemKind::Btrfs.has_stable_inodes());
        assert!(FilesystemKind::Xfs.has_stable_inodes());
        assert!(FilesystemKind::Zfs.has_stable_inodes());
        assert!(FilesystemKind::Ntfs.has_stable_inodes());
        // Trait-scanned / unknown formats don't run the local inode pre-pass, so
        // they report trustworthy (the flag only gates the local scanner path).
        assert!(FilesystemKind::Smb.has_stable_inodes());
        assert!(FilesystemKind::Mtp.has_stable_inodes());
        assert!(FilesystemKind::Other.has_stable_inodes());
    }

    #[test]
    fn unseeable_filesystems_are_unknown_not_blocked() {
        assert_eq!(FilesystemKind::Smb.max_file_size(), MaxFileSize::Unknown);
        assert_eq!(FilesystemKind::Other.max_file_size(), MaxFileSize::Unknown);
    }

    #[test]
    fn fat32_cap_is_four_gib_minus_one() {
        // A file of exactly 4 GiB must NOT fit; one byte under must.
        assert_eq!(FAT32_MAX_FILE_SIZE, 4 * 1024 * 1024 * 1024 - 1);
    }

    #[test]
    fn info_exposes_cap_only_for_limited_filesystems() {
        assert_eq!(
            FilesystemInfo::from_raw_type(Some("msdos".to_string())).max_file_size_bytes,
            Some(FAT32_MAX_FILE_SIZE)
        );
        assert_eq!(
            FilesystemInfo::from_raw_type(Some("exfat".to_string())).max_file_size_bytes,
            None
        );
        assert_eq!(FilesystemInfo::from_raw_type(None).kind, FilesystemKind::Other);
    }
}
