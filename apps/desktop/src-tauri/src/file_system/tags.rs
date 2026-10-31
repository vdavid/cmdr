//! macOS Finder tags: the pure half of `com.apple.metadata:_kMDItemUserTags`. The I/O,
//! routed through the volume, is `tags_io.rs`.
//!
//! The xattr is a binary plist holding an array of strings, each `"Name\nN"`
//! where `N` is the color index (`0` none, `1` grey, `2` green, `3` purple,
//! `4` blue, `5` yellow, `6` red, `7` orange). A colorless tag may omit the
//! `\nN` suffix entirely (treated as color `0`).
//!
//! The display source of truth is the per-file xattr: Finder keeps colors
//! consistent across files, so we never consult the system tag registry.

use cmdr_fs::entry::TagRef;
#[cfg(target_os = "macos")]
use std::path::Path;

/// The extended-attribute name Finder stores tags under. macOS-only: the only
/// readers/writers are the macOS tag functions.
#[cfg(target_os = "macos")]
pub const TAGS_XATTR: &str = "com.apple.metadata:_kMDItemUserTags";

/// Reads and parses a LOCAL path's Finder tags through `getxattr`, for a caller that
/// already works in OS paths (a custom folder's icon). Returns an empty vec when the
/// xattr is absent, on any read error, or off macOS. ❌ A row in a pane goes through
/// `tags_io` instead, which also works with no mount under it.
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
///
/// macOS-only: the `plist` crate is a macOS-only dependency. The non-macOS
/// `read_tags` returns empty without ever decoding a buffer, so this never runs
/// off macOS.
#[cfg(target_os = "macos")]
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

/// Why the write path couldn't learn a file's current tags. A file in either state is
/// skipped and reported, never written: treating it as untagged would replace every
/// tag it carries with the one being toggled.
#[cfg(target_os = "macos")]
#[derive(Debug)]
pub enum TagReadError {
    /// The store refused the read: no read permission, an ACL, a dead mount, or nothing
    /// here stores tags (`VolumeError::NotSupported`).
    Unreadable(cmdr_fs::volume::VolumeError),
    /// The value isn't a plist array of strings, so Cmdr can't write it back whole.
    Undecodable,
}

#[cfg(target_os = "macos")]
impl std::fmt::Display for TagReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unreadable(e) => write!(
                f,
                "couldn't read the existing tags ({:?})",
                cmdr_fs::log_error::LogError(e)
            ),
            Self::Undecodable => f.write_str("the existing tags are in a format Cmdr can't decode"),
        }
    }
}

/// `parse_tags_plist`, but every element must decode: a bad plist, a non-array root,
/// or any non-string element is `Undecodable` rather than silently skipped.
#[cfg(target_os = "macos")]
pub fn parse_tags_plist_strict(bytes: &[u8]) -> Result<Vec<TagRef>, TagReadError> {
    let value = plist::Value::from_reader(std::io::Cursor::new(bytes)).map_err(|_| TagReadError::Undecodable)?;
    let array = value.as_array().ok_or(TagReadError::Undecodable)?;
    array
        .iter()
        .map(|v| v.as_string().map(parse_tag_string).ok_or(TagReadError::Undecodable))
        .collect()
}

/// Splits one `"Name\nN"` tag string into a `TagRef`. Splits on the FINAL newline
/// so a name may itself contain newlines; if the trailing token isn't a `0..=7`
/// color, the whole string is the name with color `0` (a colorless named tag).
///
/// macOS-only: its callers are the macOS-only plist decoders.
#[cfg(target_os = "macos")]
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

// ============================================================================
// Write path
// ============================================================================

/// The seven system color names by index (1 grey … 7 orange). Index 0 is
/// colorless and has no system tag. These match the names Finder writes for its
/// built-in color tags, so a tag Cmdr adds is indistinguishable from one Finder
/// added (and shows up in Finder's tag search).
#[cfg(target_os = "macos")]
pub(crate) fn system_color_name(color: u8) -> Option<&'static str> {
    Some(match color {
        1 => "Gray",
        2 => "Green",
        3 => "Purple",
        4 => "Blue",
        5 => "Yellow",
        6 => "Red",
        7 => "Orange",
        _ => return None,
    })
}

/// For a (possibly multi-file) selection, which of the seven colors (index 1..=7)
/// EVERY file already carries — the "applied" set that drives the context menu's
/// checked circles and `toggle_color`'s all-have→remove decision. Pure, so it's
/// unit-testable without touching the filesystem. An empty selection → none applied.
///
/// macOS-only: its sole caller builds the macOS file context menu.
#[cfg(target_os = "macos")]
pub fn applied_colors(per_file_tags: &[Vec<TagRef>]) -> [bool; 8] {
    let mut applied = [false; 8];
    for color in 1u8..=7 {
        applied[color as usize] =
            !per_file_tags.is_empty() && per_file_tags.iter().all(|tags| tags.iter().any(|t| t.color == color));
    }
    applied
}

/// Encodes tags into a `_kMDItemUserTags` **binary** plist: an array of `"Name\nN"`
/// strings, always carrying the `\nN` color suffix (matching what Finder writes —
/// even a colorless tag is `"Name\n0"`). Pure (no I/O), so the encode↔decode
/// round-trip is unit-testable. `plist` defaults to XML, so we MUST call
/// `to_writer_binary` here — an XML body would not be Finder-compatible.
///
/// An empty set encodes to macOS's own 42-byte empty array, which is how tags clear
/// (`tags_io::write_tags`).
///
/// macOS-only: the `plist` crate is a macOS-only dependency, and the only caller
/// (`tags_io`) is itself macOS-only.
#[cfg(target_os = "macos")]
pub fn encode_tags_plist(tags: &[TagRef]) -> std::io::Result<Vec<u8>> {
    let array: Vec<plist::Value> = tags
        .iter()
        .map(|t| plist::Value::String(format!("{}\n{}", t.name, t.color)))
        .collect();
    let mut buf = Vec::new();
    plist::Value::Array(array)
        .to_writer_binary(&mut buf)
        .map_err(std::io::Error::other)?;
    Ok(buf)
}

/// One file's tags after a toggle. Adding appends the canonical system tag only when the
/// file lacks the color (no duplicate, and a same-color custom tag is left intact);
/// removing strips every tag of that color. "Has the color" means any tag of it, a custom
/// tag included. Pure, so the rule is unit-testable. `tags_io::toggle_color` decides `add`.
#[cfg(target_os = "macos")]
pub(super) fn toggled_tags(tags: &[TagRef], color: u8, color_name: &str, add: bool) -> Vec<TagRef> {
    let mut new_tags = tags.to_vec();
    if !add {
        new_tags.retain(|tag| tag.color != color);
    } else if !new_tags.iter().any(|tag| tag.color == color) {
        new_tags.push(TagRef {
            name: color_name.to_string(),
            color,
        });
    }
    new_tags
}

/// Tells the person a toggle left some files alone (`tags-skipped`, a warning toast in
/// the main window) and logs each one with its reason. The keyboard command and the
/// context-menu circles both call this; the MCP `tag` tool reports skips in its result.
#[cfg(target_os = "macos")]
pub fn report_skips<R: tauri::Runtime>(app: &tauri::AppHandle<R>, skipped: &[TagSkip]) {
    if skipped.is_empty() {
        return;
    }
    for skip in skipped {
        log::warn!(
            target: "tags",
            "left the tags on {path:?} unchanged: {}",
            skip.reason,
            path = cmdr_fs::log_private::LogPrivate::path(&skip.path)
        );
    }
    use tauri_specta::Event as _;
    let count = u32::try_from(skipped.len()).unwrap_or(u32::MAX);
    if let Err(e) = (crate::window_events::TagsSkipped { count }).emit_to(app, "main") {
        log::warn!(target: "tags", "couldn't tell the main window about {count} skipped files: {e}");
    }
}

/// What a tag write did: the resulting tag set per written-or-unchanged path (for the
/// listing cache), and the paths left alone because their tags couldn't be read.
// DEFAULT-OK: empty lists claim nothing about the disk: no file written, none skipped
// (the MCP adapter's starting accumulator).
#[cfg(target_os = "macos")]
#[derive(Debug, Default)]
pub struct TagToggleOutcome {
    pub updates: Vec<(String, Vec<TagRef>)>,
    pub skipped: Vec<TagSkip>,
}

/// One file a tag write left alone, and why.
#[cfg(target_os = "macos")]
#[derive(Debug)]
pub struct TagSkip {
    pub path: String,
    pub reason: TagReadError,
}

/// Reports one tag toggle, PII-free. Called from `tags_io::toggle_color`'s single exit, so
/// it covers all three triggers (the keyboard commands, the context-menu circles,
/// and the MCP `tag` tool) without any of them having to remember.
///
/// `color` is the Finder palette index's canonical name, a closed set of seven —
/// never a tag's own text, which is user-authored content and stays inside the
/// process. `applied` distinguishes the two halves of a toggle; without it, an
/// undo-happy user and an enthusiastic tagger produce the same number.
///
/// The `succeeded` bool covers the partial-write case, where an earlier file kept
/// its new tags and a later one didn't, and the skipped-unreadable case: a tagging feature that keeps bouncing off
/// permissions reads as "unused" otherwise.
#[cfg(target_os = "macos")]
pub(super) fn emit_tag_analytics(applied: bool, color: &str, item_count: usize, succeeded: bool) {
    crate::analytics::events::capture(
        "tag_toggled",
        serde_json::json!({
            "action": if applied { "applied" } else { "removed" },
            "color": color.to_ascii_lowercase(),
            "item_count": crate::analytics::item_count_bucket(item_count),
            "succeeded": succeeded,
        }),
    );
}

// macOS-only: every test here exercises tag parsing/encoding/applied-colors logic
// that depends on the macOS-only `plist` crate (`parse_tags_plist`,
// `encode_tags_plist`, `parse_tag_string`). The applied-colors model is pure, but
// the tags feature is macOS-only, so all coverage runs on macOS (matching
// `write_tests` and `macos_fs_tests` below).
#[cfg(all(test, target_os = "macos"))]
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

    #[test]
    fn strict_parse_refuses_what_the_lenient_one_drops() {
        assert!(matches!(
            parse_tags_plist_strict(b"not a plist"),
            Err(TagReadError::Undecodable)
        ));
        let mut dict = Vec::new();
        plist::Value::Dictionary(plist::Dictionary::new())
            .to_writer_binary(&mut dict)
            .unwrap();
        assert!(matches!(parse_tags_plist_strict(&dict), Err(TagReadError::Undecodable)));
        let mut mixed = Vec::new();
        plist::Value::Array(vec![plist::Value::String("Red\n6".into()), plist::Value::Boolean(true)])
            .to_writer_binary(&mut mixed)
            .unwrap();
        assert!(matches!(
            parse_tags_plist_strict(&mixed),
            Err(TagReadError::Undecodable)
        ));
        let good = encode_tags_plist(&[tag("Work", 0), tag("Red", 6)]).unwrap();
        assert_eq!(
            parse_tags_plist_strict(&good).unwrap(),
            vec![tag("Work", 0), tag("Red", 6)]
        );
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

    // --- Encode → decode SEMANTIC round-trips (not byte-equality vs a Finder
    //     reference: valid bplists differ in object-table ordering/dedup). ---

    // --- Applied-colors model (drives the context-menu checked circles) ---

    #[test]
    fn applied_colors_empty_selection_has_none() {
        assert_eq!(applied_colors(&[]), [false; 8]);
    }

    #[test]
    fn applied_colors_single_file_marks_each_present_color() {
        let files = vec![vec![tag("Red", 6), tag("Blue", 4)]];
        let applied = applied_colors(&files);
        assert!(applied[6] && applied[4]);
        assert!(!applied[1] && !applied[2] && !applied[3] && !applied[5] && !applied[7]);
        assert!(!applied[0], "colorless index is never applied");
    }

    #[test]
    fn applied_colors_multi_file_requires_all_to_have_it() {
        // Both have red; only one has blue.
        let files = vec![vec![tag("Red", 6), tag("Blue", 4)], vec![tag("Red", 6)]];
        let applied = applied_colors(&files);
        assert!(applied[6], "all files have red → applied");
        assert!(!applied[4], "not all files have blue → not applied");
    }

    #[test]
    fn applied_colors_counts_custom_same_color_tag() {
        // A custom-named red tag still counts as the red color being applied.
        let files = vec![vec![tag("Important", 6)]];
        assert!(applied_colors(&files)[6]);
    }

    #[test]
    fn encode_writes_a_binary_plist_not_xml() {
        let bytes = encode_tags_plist(&[tag("Red", 6)]).expect("encode");
        assert!(bytes.starts_with(b"bplist00"), "must be a binary plist, got {bytes:?}");
    }

    #[test]
    fn encode_decode_round_trip_single_color() {
        let tags = vec![tag("Red", 6)];
        let bytes = encode_tags_plist(&tags).expect("encode");
        assert_eq!(parse_tags_plist(&bytes), tags);
    }

    #[test]
    fn encode_decode_round_trip_multiple_with_colorless() {
        let tags = vec![tag("Important", 0), tag("Red", 6), tag("Blue", 4)];
        let bytes = encode_tags_plist(&tags).expect("encode");
        assert_eq!(parse_tags_plist(&bytes), tags);
    }

    /// What macOS writes to clear a file's tags, byte for byte (`xattr -px` after
    /// `NSURLTagNamesKey` = [], macOS 27.0.1, APFS and a QNAP alike, 2026-10-31).
    #[test]
    fn clearing_encodes_to_the_empty_plist_macos_writes() {
        let macos = hex("62706c6973743030a0080000000000000101000000000000000100000000000000000000000000000009");
        assert_eq!(encode_tags_plist(&[]).expect("encode"), macos);
    }

    #[test]
    fn encode_decode_round_trip_empty_is_no_tags() {
        let bytes = encode_tags_plist(&[]).expect("encode");
        assert!(bytes.starts_with(b"bplist00"), "empty set is still a binary plist");
        assert_eq!(parse_tags_plist(&bytes), Vec::<TagRef>::new());
    }

    #[test]
    fn encode_decode_round_trip_name_with_newline() {
        // The name itself contains a newline; decode splits on the FINAL one.
        let tags = vec![tag("two\nline", 4)];
        let bytes = encode_tags_plist(&tags).expect("encode");
        assert_eq!(parse_tags_plist(&bytes), tags);
    }
}

// Write-path tests against real temp files. macOS-only — Finder tags (and the xattr
// write) don't exist elsewhere. These are the data-safety core (principle 4).
#[cfg(all(test, target_os = "macos"))]
#[path = "tags_write_test.rs"]
mod write_tests;

// Real-filesystem tests: exercise the actual `getxattr` path and lock in that the
// bulk listing never reads tags. macOS-only — Finder tags don't exist elsewhere.
#[cfg(all(test, target_os = "macos"))]
mod macos_fs_tests {
    use super::*;
    use crate::test_support::TestDir;

    fn temp_dir(name: &str) -> TestDir {
        TestDir::new(&format!("tags_test_{name}"))
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

    /// Finder-fidelity: the tags Cmdr writes must be readable by macOS's OWN
    /// resource API (`NSURLTagNamesKey`), the same path Finder and Spotlight use.
    /// This is the proof that a Cmdr-written tag is indistinguishable from a
    /// Finder-written one — stronger than our own `read_tags` round-trip, which
    /// only proves Cmdr reads Cmdr. Colors carry implicitly via the `"Name\nN"`
    /// payload; this asserts the names surface through the system API.
    #[test]
    fn encoded_tags_are_readable_via_finder_url_api() {
        use objc2::rc::Retained;
        use objc2::runtime::AnyObject;
        use objc2_foundation::{NSArray, NSString, NSURL};

        let dir = temp_dir("nsurl_fidelity");
        let file = dir.join("written.txt");
        std::fs::write(&file, b"x").unwrap();

        let bytes = encode_tags_plist(&[
            TagRef {
                name: "Red".to_string(),
                color: 6,
            },
            TagRef {
                name: "Work".to_string(),
                color: 0,
            },
        ])
        .expect("encode");
        xattr::set(&file, TAGS_XATTR, &bytes).expect("set tag xattr");

        let ns_path = NSString::from_str(file.to_str().unwrap());
        let url = NSURL::fileURLWithPath(&ns_path);
        let key = NSString::from_str("NSURLTagNamesKey");
        let mut value: Option<Retained<AnyObject>> = None;
        // SAFETY: `url` is a valid NSURL, `key` a valid NSString, and `&mut value` a
        // valid `&mut Option<Retained<_>>` out-param; on success objc2 stores an
        // already-retained object there per its out-param convention.
        let ok = unsafe { url.getResourceValue_forKey_error(&mut value, &key) };
        assert!(ok.is_ok(), "NSURL resource read should succeed");

        // Downcast to the type-erased `NSArray<AnyObject>` (objc2 can't runtime-check
        // the element type), then downcast each element to NSString.
        let array = value
            .expect("tag names present")
            .downcast::<NSArray<AnyObject>>()
            .expect("NSURLTagNamesKey returns an NSArray");
        let names: Vec<String> = (0..array.count())
            .map(|i| {
                array
                    .objectAtIndex(i)
                    .downcast::<NSString>()
                    .expect("tag name is an NSString")
                    .to_string()
            })
            .collect();

        assert!(
            names.contains(&"Red".to_string()),
            "macOS reads Cmdr's Red tag: {names:?}"
        );
        assert!(
            names.contains(&"Work".to_string()),
            "macOS reads Cmdr's colorless Work tag: {names:?}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Guard: the bulk listing path must NOT read tags (that's the deferred
    /// `enrich_tags` pass's job). If a refactor drags a `getxattr` into
    /// `list_directory_core`, this fails — keeping the 100k-dir hot path clean.
    #[test]
    fn list_directory_core_does_not_load_tags() {
        use cmdr_local::reading::list_directory_core;

        let dir = temp_dir("core_no_tags");
        let file = dir.join("tagged.txt");
        std::fs::write(&file, b"x").unwrap();
        xattr::set(&file, TAGS_XATTR, RED_TAG_PLIST).expect("set tag xattr");
        // Sanity: the tag really is on disk.
        assert_eq!(read_tags(&file).len(), 1, "fixture precondition: file is tagged");

        let entries =
            list_directory_core(&dir, cmdr_fs::volume::host::VolumeHost::detached().listings()).expect("list temp dir");
        let entry = entries.iter().find(|e| e.name == "tagged.txt").expect("entry present");
        assert!(
            entry.tags.is_empty(),
            "list_directory_core must not read tags (deferred to enrich_tags)"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
