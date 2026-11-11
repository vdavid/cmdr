//! Tier C detection: the cheap, no-NSWorkspace checks that decide whether a
//! directory deviates from the generic `dir` glyph enough to warrant a real
//! per-path icon fetch.
//!
//! Two independent signals, both designed to keep the candidate set tiny so the
//! expensive NSWorkspace fetch (FDA-gated, 8 MB thread) runs only for the rare
//! folder that truly has its own icon:
//!
//! - **Packages**: a directory whose name ends in a listed package extension
//!   (`cmdr_fs::file_extension::is_listed_package`, no I/O) routes straight to a
//!   `pkg:{dir}` key in `get_icon_id`. A dotted folder the list doesn't know is
//!   asked of macOS once visible (`file_system::packages`), and a yes patches the
//!   same key onto its entry. The key prefix is re-exported below.
//! - **Custom-icon folders** — a folder carrying the `kHasCustomIcon` flag in its
//!   `com.apple.FinderInfo` xattr. Detecting this needs a `getxattr` syscall, so
//!   it is NOT run during bulk listing (a syscall per directory entry would
//!   regress a 100k-entry listing). Instead the frontend asks about the bounded
//!   set of *visible* directory paths, and `custom_folder_icon` runs the
//!   `getxattr`s only for those. It also tells a macOS 26+ Customize Folder
//!   symbol or emoji (accent-tinted, so Cmdr's gold recolor applies) from a
//!   tag-tinted one or pasted artwork (both drawn as-is).
//!
//! Both land on the same lifecycle as the existing `path:` keys (LRU-bounded,
//! never persisted to localStorage; see `icons::mod`): genuinely per-path,
//! unbounded by nature.

use std::path::Path;

use cmdr_fs::entry::TagRef;
pub use cmdr_fs::icons::packages::PKG_KEY_PREFIX;

use super::platform::{extended_attribute, tag_tinting_disabled};

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

/// The xattr Finder's Customize Folder (macOS 26+) writes: JSON naming the symbol or emoji drawn
/// on the folder, like `{"sym":"checkmark"}`. IconServices reads this exact name, `#S` (the
/// "syncable" flag) included; the bare `com.apple.icon.folder` draws nothing (verified on macOS
/// 27.0.1, `NSWorkspace iconForFile:` on folders built with `xattr -w`, 2026-10-29).
const FOLDER_CUSTOMIZATION_XATTR: &str = "com.apple.icon.folder#S";

/// The longest emoji value Cmdr draws, in code points. A ZWJ family with skin tones is 11; anything
/// longer can't be one glyph, so the folder falls back to the recolored macOS bitmap.
const MAX_EMOJI_CHARS: usize = 16;

/// What a custom-icon folder's icon is made of, which decides whether Cmdr may recolor it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CustomFolderIcon {
    /// Finder's Customize Folder symbol or emoji on the plain folder, which macOS tints with the
    /// system accent like any plain folder. Cmdr's gold recolor applies. `emoji` names the emoji
    /// for an emoji customization (`None` for a symbol): the gold filter grays the whole bitmap,
    /// so the frontend draws a gold plain folder and this emoji on top in its own colors (#814).
    AccentTinted { emoji: Option<String> },
    /// The same customization, tinted by a colored Finder tag. That's a color the person chose
    /// (Customize Folder's color dots ARE tags), so Cmdr keeps it.
    TagTinted,
    /// A pasted image (the `Icon\r` resource file), drawn as-is.
    Artwork,
}

impl CustomFolderIcon {
    /// True when macOS draws this folder in the system accent, so Cmdr's gold recolor applies.
    pub fn follows_accent(&self) -> bool {
        matches!(self, Self::AccentTinted { .. })
    }

    /// The emoji an accent-tinted emoji customization draws, for the frontend's gold composite.
    pub fn accent_emoji(&self) -> Option<&str> {
        match self {
            Self::AccentTinted { emoji } => emoji.as_deref(),
            Self::TagTinted | Self::Artwork => None,
        }
    }
}

/// Pure classifier behind [`custom_folder_icon`]. `None` unless `kHasCustomIcon` is set: macOS
/// draws neither artwork nor a customization without it. A customization xattr wins over an
/// `Icon\r` file when both exist (macOS draws the symbol), so its presence alone decides it.
/// `customization` is that xattr's value, if any. `tag_tinting_disabled` is a closure because it's
/// a preferences read, needed only for a customized folder with a colored tag.
pub fn classify_custom_folder_icon(
    finder_info: &[u8],
    customization: Option<&[u8]>,
    tags: &[TagRef],
    tag_tinting_disabled: impl FnOnce() -> bool,
) -> Option<CustomFolderIcon> {
    if !finder_info_has_custom_icon(finder_info) {
        return None;
    }
    let Some(customization) = customization else {
        return Some(CustomFolderIcon::Artwork);
    };
    let has_colored_tag = tags.iter().any(|tag| tag.color != 0);
    if has_colored_tag && !tag_tinting_disabled() {
        Some(CustomFolderIcon::TagTinted)
    } else {
        Some(CustomFolderIcon::AccentTinted {
            emoji: customization_emoji(customization),
        })
    }
}

/// Pure parser: the emoji a Customize Folder xattr value (`{"emoji":"🍕"}`) names. `None` for a
/// symbol (`{"sym":…}`), unreadable JSON, or text that can't be one emoji (blank, a control
/// character, past [`MAX_EMOJI_CHARS`]), since the value lands on screen as text.
pub fn customization_emoji(customization: &[u8]) -> Option<String> {
    #[derive(serde::Deserialize)]
    struct Customization {
        emoji: Option<String>,
    }
    let emoji = serde_json::from_slice::<Customization>(customization).ok()?.emoji?;
    let plausible =
        !emoji.trim().is_empty() && emoji.chars().count() <= MAX_EMOJI_CHARS && !emoji.chars().any(char::is_control);
    plausible.then_some(emoji)
}

/// Classifies a folder's custom icon from its on-disk metadata: two or three `getxattr`s and, for
/// a customized folder with a colored tag, one preferences read. `None` for a folder without one
/// (the common case) or on any read error (permission, dead mount: the folder keeps the generic
/// `dir` glyph). No NSWorkspace, no LaunchServices, so no TCC popup. Only for the visible paths
/// the frontend asks about, ❌ never during bulk listing.
pub fn custom_folder_icon(path: &Path) -> Option<CustomFolderIcon> {
    let finder_info = extended_attribute(path, "com.apple.FinderInfo")?;
    if !finder_info_has_custom_icon(&finder_info) {
        return None;
    }
    let customization = extended_attribute(path, FOLDER_CUSTOMIZATION_XATTR);
    let tags = if customization.is_some() {
        crate::file_system::tags::read_tags(path)
    } else {
        Vec::new()
    };
    classify_custom_folder_icon(&finder_info, customization.as_deref(), &tags, tag_tinting_disabled)
}

#[cfg(test)]
mod tests {
    use super::*;
    // Only the macOS-gated cases below build a real directory.
    #[cfg(target_os = "macos")]
    use crate::test_support::TestDir;
    // The package half is tested where it lives, in `cmdr_fs` (`icons::packages`,
    // `file_extension`); only the xattr half is covered here.

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

    fn tag(name: &str, color: u8) -> TagRef {
        TagRef {
            name: name.to_string(),
            color,
        }
    }

    fn tinting_on() -> bool {
        false
    }

    /// The xattr values Customize Folder writes for a symbol and for an emoji.
    const SYMBOL: &[u8] = br#"{"sym":"checkmark"}"#;
    const PIZZA: &[u8] = r#"{"emoji":"🍕"}"#.as_bytes();

    #[test]
    fn no_custom_icon_flag_means_no_custom_icon_even_with_a_customization() {
        // macOS draws neither the symbol nor artwork without `kHasCustomIcon`.
        let info = finder_info_with_flags(0);
        assert_eq!(classify_custom_folder_icon(&info, Some(SYMBOL), &[], tinting_on), None);
        assert_eq!(classify_custom_folder_icon(&[], None, &[], tinting_on), None);
    }

    #[test]
    fn a_flagged_folder_without_a_customization_is_pasted_artwork() {
        let info = finder_info_with_flags(K_HAS_CUSTOM_ICON);
        // Tags don't tint artwork, so a colored tag changes nothing.
        let tags = [tag("Red", 6)];
        assert_eq!(
            classify_custom_folder_icon(&info, None, &tags, tinting_on),
            Some(CustomFolderIcon::Artwork)
        );
    }

    #[test]
    fn an_untagged_customization_follows_the_accent() {
        // Issue #110: the green "checkmark design" folder under a green accent.
        let info = finder_info_with_flags(K_HAS_CUSTOM_ICON);
        let icon = classify_custom_folder_icon(&info, Some(SYMBOL), &[], tinting_on);
        assert_eq!(icon, Some(CustomFolderIcon::AccentTinted { emoji: None }));
        assert!(icon.is_some_and(|icon| icon.follows_accent()));
    }

    #[test]
    fn a_colorless_tag_leaves_a_customization_on_the_accent() {
        let info = finder_info_with_flags(K_HAS_CUSTOM_ICON);
        let tags = [tag("Work", 0)];
        assert_eq!(
            classify_custom_folder_icon(&info, Some(SYMBOL), &tags, tinting_on),
            Some(CustomFolderIcon::AccentTinted { emoji: None })
        );
    }

    #[test]
    fn a_colored_tag_tints_a_customization_with_the_persons_color() {
        let info = finder_info_with_flags(K_HAS_CUSTOM_ICON);
        let tags = [tag("Work", 0), tag("Red", 6)];
        let icon = classify_custom_folder_icon(&info, Some(PIZZA), &tags, tinting_on);
        assert_eq!(icon, Some(CustomFolderIcon::TagTinted));
        assert!(!icon.is_some_and(|icon| icon.follows_accent()));
    }

    #[test]
    fn with_tag_tinting_switched_off_a_tagged_customization_follows_the_accent() {
        let info = finder_info_with_flags(K_HAS_CUSTOM_ICON);
        let tags = [tag("Red", 6)];
        assert_eq!(
            classify_custom_folder_icon(&info, Some(SYMBOL), &tags, || true),
            Some(CustomFolderIcon::AccentTinted { emoji: None })
        );
    }

    #[test]
    fn the_tinting_switch_is_read_only_for_a_customization_with_a_colored_tag() {
        let info = finder_info_with_flags(K_HAS_CUSTOM_ICON);
        let untouched = || -> bool { panic!("read the tinting switch without needing it") };
        classify_custom_folder_icon(&info, Some(SYMBOL), &[], untouched);
        classify_custom_folder_icon(&info, None, &[tag("Red", 6)], untouched);
    }

    #[test]
    fn an_emoji_customization_carries_its_emoji_for_the_gold_composite() {
        // Issue #814: the gold recolor grays the whole bitmap, so the frontend draws the emoji
        // itself over a gold folder, and needs to know which one.
        let info = finder_info_with_flags(K_HAS_CUSTOM_ICON);
        let icon = classify_custom_folder_icon(&info, Some(PIZZA), &[], tinting_on);
        assert_eq!(
            icon,
            Some(CustomFolderIcon::AccentTinted {
                emoji: Some("🍕".to_string())
            })
        );
        assert_eq!(icon.as_ref().and_then(CustomFolderIcon::accent_emoji), Some("🍕"));
    }

    #[test]
    fn a_symbol_or_unreadable_customization_has_no_emoji_but_still_follows_the_accent() {
        let info = finder_info_with_flags(K_HAS_CUSTOM_ICON);
        let customizations: [&[u8]; 4] = [SYMBOL, b"not json", br#"{"emoji":42}"#, b""];
        for customization in customizations {
            assert_eq!(
                classify_custom_folder_icon(&info, Some(customization), &[], tinting_on),
                Some(CustomFolderIcon::AccentTinted { emoji: None }),
                "customization {:?}",
                String::from_utf8_lossy(customization)
            );
        }
    }

    #[test]
    fn customization_emoji_keeps_multi_code_point_emoji_whole() {
        // A ZWJ family with skin tones is 11 code points; it's still one emoji.
        let family = "👨🏻‍👩🏻‍👧🏻‍👦🏻";
        let json = format!(r#"{{"emoji":"{family}"}}"#);
        assert_eq!(customization_emoji(json.as_bytes()).as_deref(), Some(family));
        assert_eq!(
            customization_emoji(r#"{"emoji":"🇸🇪"}"#.as_bytes()).as_deref(),
            Some("🇸🇪")
        );
    }

    #[test]
    fn customization_emoji_refuses_text_that_cannot_be_one_emoji() {
        // The value lands on screen as text, so anything that can't be one glyph is dropped and
        // the folder falls back to the recolored macOS bitmap.
        assert_eq!(customization_emoji(br#"{"emoji":""}"#), None);
        assert_eq!(customization_emoji(br#"{"emoji":"   "}"#), None);
        assert_eq!(customization_emoji(br#"{"emoji":"a\nb"}"#), None);
        let long = "🙂".repeat(MAX_EMOJI_CHARS + 1);
        assert_eq!(customization_emoji(format!(r#"{{"emoji":"{long}"}}"#).as_bytes()), None);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn custom_folder_icon_reads_real_xattrs() {
        let dir = TestDir::new("custom_icon_test");

        // No xattr yet → not a custom-icon folder.
        assert_eq!(custom_folder_icon(&dir), None);

        // FinderInfo with kHasCustomIcon alone → pasted artwork.
        let info = finder_info_with_flags(K_HAS_CUSTOM_ICON);
        xattr::set(&dir, "com.apple.FinderInfo", &info).expect("set xattr");
        assert_eq!(custom_folder_icon(&dir), Some(CustomFolderIcon::Artwork));

        // Plus the xattr Customize Folder writes → accent-tinted.
        xattr::set(&dir, FOLDER_CUSTOMIZATION_XATTR, SYMBOL).expect("set xattr");
        assert_eq!(
            custom_folder_icon(&dir),
            Some(CustomFolderIcon::AccentTinted { emoji: None })
        );

        // Clear the flag → no longer detected.
        let cleared = finder_info_with_flags(0);
        xattr::set(&dir, "com.apple.FinderInfo", &cleared).expect("clear xattr");
        assert_eq!(custom_folder_icon(&dir), None);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn custom_folder_icon_ignores_the_unflagged_customization_name() {
        // IconServices only reads the `#S` name, so the bare one leaves the folder drawn as artwork.
        let dir = TestDir::new("custom_icon_bare_name_test");
        xattr::set(&dir, "com.apple.FinderInfo", &finder_info_with_flags(K_HAS_CUSTOM_ICON)).expect("set xattr");
        xattr::set(&dir, "com.apple.icon.folder", br#"{"sym":"checkmark"}"#).expect("set xattr");
        assert_eq!(custom_folder_icon(&dir), Some(CustomFolderIcon::Artwork));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn the_tinting_switch_reads_through_cfpreferences() {
        // Smoke test for the FFI: either answer is valid, the machine's own setting decides it.
        let _ = tag_tinting_disabled();
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn custom_folder_icon_reads_a_colorless_tag_as_accent_tinted() {
        // A colored tag would read the machine's real tinting switch, so the real-xattr test sticks
        // to a colorless one; the colored cases are covered by the pure classifier tests above.
        let dir = TestDir::new("custom_icon_tag_test");
        xattr::set(&dir, "com.apple.FinderInfo", &finder_info_with_flags(K_HAS_CUSTOM_ICON)).expect("set xattr");
        xattr::set(&dir, FOLDER_CUSTOMIZATION_XATTR, r#"{"emoji":"🙂"}"#.as_bytes()).expect("set xattr");
        let tags = crate::file_system::tags::encode_tags_plist(&[tag("Work", 0)]).expect("encode tags");
        xattr::set(&dir, crate::file_system::tags::TAGS_XATTR, &tags).expect("set xattr");
        assert_eq!(
            custom_folder_icon(&dir),
            Some(CustomFolderIcon::AccentTinted {
                emoji: Some("🙂".to_string())
            })
        );
    }
}
