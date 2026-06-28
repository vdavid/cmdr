//! macOS Finder tags: reading `com.apple.metadata:_kMDItemUserTags`.
//!
//! The xattr is a binary plist holding an array of strings, each `"Name\nN"`
//! where `N` is the color index (`0` none, `1` grey, `2` green, `3` purple,
//! `4` blue, `5` yellow, `6` red, `7` orange). A colorless tag may omit the
//! `\nN` suffix entirely (treated as color `0`).
//!
//! This is the read side. Writing lives in `set_tags` (see the write path), and
//! the display source of truth is the per-file xattr — Finder keeps colors
//! consistent across files, so we never consult the system tag registry.

use crate::file_system::listing::metadata::TagRef;
#[cfg(target_os = "macos")]
use std::path::Path;

/// The extended-attribute name Finder stores tags under.
pub const TAGS_XATTR: &str = "com.apple.metadata:_kMDItemUserTags";

/// Reads and parses a path's Finder tags. Returns an empty vec when the xattr is
/// absent (the common case), on any read error (permission, dead mount), or off
/// macOS. Never blocks beyond a single `getxattr`; callers still gate this to
/// local volumes and wrap it in a timeout (a `getxattr` on a hung mount blocks).
#[cfg(target_os = "macos")]
pub fn read_tags(path: &Path) -> Vec<TagRef> {
    match xattr::get(path, TAGS_XATTR) {
        Ok(Some(bytes)) => parse_tags_plist(&bytes),
        // No xattr, or a read error: no tags. Purely additive — a file with
        // unreadable tags simply shows none.
        _ => Vec::new(),
    }
}

/// Non-macOS: Finder tags don't exist, so always empty. Keeps `FileEntry.tags`
/// cross-platform and the call sites `#[cfg]`-free.
#[cfg(not(target_os = "macos"))]
pub fn read_tags(_path: &std::path::Path) -> Vec<TagRef> {
    Vec::new()
}

/// Decodes a raw `_kMDItemUserTags` binary-plist buffer into tags. Pure (no I/O),
/// so it's unit-testable against captured Finder fixtures. Returns empty on any
/// decode failure or a non-array root — never panics on malformed input.
pub fn parse_tags_plist(bytes: &[u8]) -> Vec<TagRef> {
    let value = match plist::Value::from_reader(std::io::Cursor::new(bytes)) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    let Some(array) = value.as_array() else {
        return Vec::new();
    };
    array
        .iter()
        .filter_map(|v| v.as_string())
        .map(parse_tag_string)
        .collect()
}

/// Splits one `"Name\nN"` tag string into a `TagRef`. Splits on the FINAL newline
/// so a name may itself contain newlines; if the trailing token isn't a `0..=7`
/// color, the whole string is the name with color `0` (a colorless named tag).
fn parse_tag_string(s: &str) -> TagRef {
    if let Some((name, color_str)) = s.rsplit_once('\n')
        && let Ok(color) = color_str.parse::<u8>()
        && color <= 7
    {
        return TagRef {
            name: name.to_string(),
            color,
        };
    }
    TagRef {
        name: s.to_string(),
        color: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Decodes a hex string (as captured via `xattr -px`) to bytes.
    fn hex(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("valid hex"))
            .collect()
    }

    fn tag(name: &str, color: u8) -> TagRef {
        TagRef {
            name: name.to_string(),
            color,
        }
    }

    // --- Real fixtures captured from Finder (via `URLResourceValues.tagNames`),
    //     `xattr -px com.apple.metadata:_kMDItemUserTags <file>`. ---

    #[test]
    fn single_color_tag() {
        // red.txt -> ["Red\n6"]
        let bytes =
            hex("62706C6973743030A101555265640A36080A0000000000000101000000000000000200000000000000000000000000000010");
        assert_eq!(parse_tags_plist(&bytes), vec![tag("Red", 6)]);
    }

    #[test]
    fn five_color_tags_in_order() {
        // five-tags.txt -> Red, Orange, Yellow, Green, Blue
        let bytes = hex(
            "62706C6973743030A50102030405555265640A36584F72616E67650A375859656C6C6F770A3557477265656E0A3256426C75650A34080E141D262E0000000000000101000000000000000600000000000000000000000000000035",
        );
        assert_eq!(
            parse_tags_plist(&bytes),
            vec![
                tag("Red", 6),
                tag("Orange", 7),
                tag("Yellow", 5),
                tag("Green", 2),
                tag("Blue", 4)
            ]
        );
    }

    #[test]
    fn colorless_named_tag_is_color_zero() {
        // work-colorless.txt -> ["Work\n0"]
        let bytes = hex(
            "62706C6973743030A10156576F726B0A30080A0000000000000101000000000000000200000000000000000000000000000011",
        );
        assert_eq!(parse_tags_plist(&bytes), vec![tag("Work", 0)]);
    }

    #[test]
    fn mixed_colorless_and_colored() {
        // custom-plus-color.txt -> ["Important\n0", "Red\n6"]
        let bytes = hex(
            "62706C6973743030A201025B496D706F7274616E740A30555265640A36080B17000000000000010100000000000000030000000000000000000000000000001D",
        );
        assert_eq!(parse_tags_plist(&bytes), vec![tag("Important", 0), tag("Red", 6)]);
    }

    #[test]
    fn empty_buffer_yields_no_tags() {
        assert_eq!(parse_tags_plist(&[]), Vec::<TagRef>::new());
    }

    #[test]
    fn garbage_buffer_does_not_panic() {
        assert_eq!(parse_tags_plist(&[0xDE, 0xAD, 0xBE, 0xEF]), Vec::<TagRef>::new());
        assert_eq!(parse_tags_plist(b"not a plist at all"), Vec::<TagRef>::new());
    }

    // --- Pure string-parsing edge cases ---

    #[test]
    fn tag_string_without_newline_is_colorless() {
        assert_eq!(parse_tag_string("Plain"), tag("Plain", 0));
    }

    #[test]
    fn tag_string_with_out_of_range_color_falls_back_to_name() {
        // "9" isn't a valid 0..=7 color, so the whole string is the name.
        assert_eq!(parse_tag_string("Weird\n9"), tag("Weird\n9", 0));
    }

    #[test]
    fn tag_name_may_contain_newlines() {
        // Split on the FINAL newline only.
        assert_eq!(parse_tag_string("two\nline\n4"), tag("two\nline", 4));
    }
}

// Real-filesystem tests: exercise the actual `getxattr` path and lock in that the
// bulk listing never reads tags. macOS-only — Finder tags don't exist elsewhere.
#[cfg(all(test, target_os = "macos"))]
mod macos_fs_tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("cmdr_tags_test_{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    /// The red.txt fixture bytes (a Finder-written `["Red\n6"]` binary plist).
    const RED_TAG_PLIST: &[u8] = &[
        0x62, 0x70, 0x6C, 0x69, 0x73, 0x74, 0x30, 0x30, 0xA1, 0x01, 0x55, 0x52, 0x65, 0x64, 0x0A, 0x36, 0x08, 0x0A,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10,
    ];

    #[test]
    fn read_tags_reads_a_real_xattr() {
        let dir = temp_dir("read_real");
        let file = dir.join("tagged.txt");
        std::fs::write(&file, b"x").unwrap();
        xattr::set(&file, TAGS_XATTR, RED_TAG_PLIST).expect("set tag xattr");

        assert_eq!(
            read_tags(&file),
            vec![TagRef {
                name: "Red".to_string(),
                color: 6
            }]
        );

        // An untagged sibling has no tags and doesn't error.
        let plain = dir.join("plain.txt");
        std::fs::write(&plain, b"x").unwrap();
        assert_eq!(read_tags(&plain), Vec::<TagRef>::new());

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Guard: the bulk listing path must NOT read tags (that's the deferred
    /// `enrich_tags` pass's job). If a refactor drags a `getxattr` into
    /// `list_directory_core`, this fails — keeping the 100k-dir hot path clean.
    #[test]
    fn list_directory_core_does_not_load_tags() {
        use crate::file_system::listing::list_directory_core;

        let dir = temp_dir("core_no_tags");
        let file = dir.join("tagged.txt");
        std::fs::write(&file, b"x").unwrap();
        xattr::set(&file, TAGS_XATTR, RED_TAG_PLIST).expect("set tag xattr");
        // Sanity: the tag really is on disk.
        assert_eq!(read_tags(&file).len(), 1, "fixture precondition: file is tagged");

        let entries = list_directory_core(&dir).expect("list temp dir");
        let entry = entries.iter().find(|e| e.name == "tagged.txt").expect("entry present");
        assert!(
            entry.tags.is_empty(),
            "list_directory_core must not read tags (deferred to enrich_tags)"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
