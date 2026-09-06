//! Tier C detection: the cheap, no-NSWorkspace checks that decide whether a
//! directory deviates from the generic `dir` glyph enough to warrant a real
//! per-path icon fetch.
//!
//! Two independent signals, both designed to keep the candidate set tiny so the
//! expensive NSWorkspace fetch (FDA-gated, 8 MB thread) runs only for the rare
//! folder that truly has its own icon:
//!
//! - **Packages** — a directory whose name ends in a known package extension
//!   (`.app`, `.bundle`, …). A pure suffix check with no I/O, cheap enough to run
//!   for every entry during listing, so it routes straight to a `pkg:{dir}` key
//!   in `get_icon_id`. It lives in `cmdr_fs::icons::packages` (where `FileEntry`
//!   can reach it) and is re-exported below.
//! - **Custom-icon folders** — a folder carrying the `kHasCustomIcon` flag in its
//!   `com.apple.FinderInfo` xattr. Detecting this needs a `getxattr` syscall, so
//!   it is NOT run during bulk listing (a syscall per directory entry would
//!   regress a 100k-entry listing). Instead the frontend asks about the bounded
//!   set of *visible* directory paths, and `has_custom_folder_icon` runs the
//!   `getxattr` only for those.
//!
//! Both land on the same lifecycle as the existing `path:` keys (LRU-bounded,
//! never persisted to localStorage; see `icons::mod`): genuinely per-path,
//! unbounded by nature.

use std::path::Path;

pub use cmdr_fs::icons::packages::{PKG_KEY_PREFIX, is_package_dir, package_icon_id};

/// Byte offset of the Finder flags within a `com.apple.FinderInfo` buffer.
///
/// `com.apple.FinderInfo` is a 32-byte blob. Its first 16 bytes are the
/// `FileInfo`/`FolderInfo` struct; the Finder flags are a big-endian `u16` at
/// offset 8 (`finderFlags` in `<CarbonCore/Finder.h>`). `kHasCustomIcon` is bit
/// `0x0400` of that field.
const FINDER_FLAGS_OFFSET: usize = 8;

/// `kHasCustomIcon` bit in the Finder flags (`<CarbonCore/Finder.h>`). Set when
/// the item carries a custom icon resource (a folder the user pasted an icon onto
/// in Finder's Get Info).
const K_HAS_CUSTOM_ICON: u16 = 0x0400;

/// Pure parser: does a `com.apple.FinderInfo` byte buffer have `kHasCustomIcon`
/// set? Returns false for a short/empty buffer (the flag can't be present), never
/// panics. Split out from the syscall so it's unit-testable against a synthetic
/// buffer without touching the filesystem.
pub fn finder_info_has_custom_icon(finder_info: &[u8]) -> bool {
    // Need both bytes of the big-endian u16 at FINDER_FLAGS_OFFSET.
    if finder_info.len() < FINDER_FLAGS_OFFSET + 2 {
        return false;
    }
    let flags = u16::from_be_bytes([finder_info[FINDER_FLAGS_OFFSET], finder_info[FINDER_FLAGS_OFFSET + 1]]);
    flags & K_HAS_CUSTOM_ICON != 0
}

/// Reads the `com.apple.FinderInfo` xattr of `path` and reports whether the
/// folder carries the `kHasCustomIcon` flag. A single `getxattr` — no NSWorkspace,
/// no LaunchServices, so no TCC popup. Returns false when the xattr is absent
/// (the common case: almost no folder has a custom icon) or on any read error.
///
/// This is the gate that keeps the per-path fetch rare: the FE asks about visible
/// directory paths, this filters down to the few that truly deviate, and only
/// those reach the expensive NSWorkspace fetch.
pub fn has_custom_folder_icon(path: &Path) -> bool {
    match xattr::get(path, "com.apple.FinderInfo") {
        Ok(Some(buf)) => finder_info_has_custom_icon(&buf),
        // No xattr, or a read error (permission, dead mount): treat as no custom
        // icon. The folder degrades to the generic `dir` glyph — purely additive.
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // Only the macOS-gated cases below build a real directory.
    #[cfg(target_os = "macos")]
    use crate::test_support::TestDir;
    // The package half (`is_package_dir`, `package_icon_id`) is tested where it
    // lives, in `cmdr_fs::icons::packages`; only the xattr half is covered here.

    /// Builds a minimal `com.apple.FinderInfo` buffer with the given Finder flags.
    fn finder_info_with_flags(flags: u16) -> Vec<u8> {
        let mut buf = vec![0u8; 32];
        let bytes = flags.to_be_bytes();
        buf[FINDER_FLAGS_OFFSET] = bytes[0];
        buf[FINDER_FLAGS_OFFSET + 1] = bytes[1];
        buf
    }

    #[test]
    fn custom_icon_flag_is_read_at_the_right_offset() {
        // Only kHasCustomIcon set → detected.
        assert!(finder_info_has_custom_icon(&finder_info_with_flags(K_HAS_CUSTOM_ICON)));
        // kHasCustomIcon among other flags → still detected.
        assert!(finder_info_has_custom_icon(&finder_info_with_flags(
            K_HAS_CUSTOM_ICON | 0x0001 | 0x2000
        )));
    }

    #[test]
    fn absent_custom_icon_flag_reads_false() {
        // No flags at all.
        assert!(!finder_info_has_custom_icon(&finder_info_with_flags(0)));
        // Other flags set but NOT kHasCustomIcon (e.g. kIsInvisible 0x4000).
        assert!(!finder_info_has_custom_icon(&finder_info_with_flags(0x4000 | 0x0001)));
    }

    #[test]
    fn short_or_empty_buffer_never_panics_and_reads_false() {
        assert!(!finder_info_has_custom_icon(&[]));
        assert!(!finder_info_has_custom_icon(&[0u8; 4]));
        // Exactly one byte short of the flags field.
        assert!(!finder_info_has_custom_icon(&[0u8; FINDER_FLAGS_OFFSET + 1]));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn has_custom_folder_icon_reads_a_real_xattr() {
        let dir = TestDir::new("custom_icon_test");

        // No xattr yet → not a custom-icon folder.
        assert!(!has_custom_folder_icon(&dir));

        // Set FinderInfo with kHasCustomIcon → detected.
        let info = finder_info_with_flags(K_HAS_CUSTOM_ICON);
        xattr::set(&dir, "com.apple.FinderInfo", &info).expect("set xattr");
        assert!(has_custom_folder_icon(&dir));

        // Clear the flag → no longer detected.
        let cleared = finder_info_with_flags(0);
        xattr::set(&dir, "com.apple.FinderInfo", &cleared).expect("clear xattr");
        assert!(!has_custom_folder_icon(&dir));
    }
}
