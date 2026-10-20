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

use cmdr_fs::entry::TagRef;
#[cfg(target_os = "macos")]
use std::path::Path;

/// The extended-attribute name Finder stores tags under. macOS-only: the only
/// readers/writers are the macOS tag functions.
#[cfg(target_os = "macos")]
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
#[derive(Debug)]
#[cfg_attr(
    not(target_os = "macos"),
    allow(dead_code, reason = "only the macOS strict reader builds these")
)]
pub enum TagReadError {
    /// `getxattr` failed: no read permission, an ACL, a dead mount.
    Unreadable(std::io::Error),
    /// The value isn't a plist array of strings, so Cmdr can't write it back whole.
    Undecodable,
}

impl std::fmt::Display for TagReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unreadable(e) => write!(f, "couldn't read the existing tags ({e})"),
            Self::Undecodable => f.write_str("the existing tags are in a format Cmdr can't decode"),
        }
    }
}

/// The write path's reader. Unlike the lenient `read_tags` (display), it tells "no
/// tags" (`Ok(None)`) apart from "couldn't read them" (`Err`), and it refuses a value
/// it can't decode completely, so a read-modify-write never drops what it didn't
/// understand.
#[cfg(target_os = "macos")]
pub fn read_tags_strict(path: &Path) -> Result<Option<Vec<TagRef>>, TagReadError> {
    match xattr::get(path, TAGS_XATTR) {
        Ok(Some(bytes)) => parse_tags_plist_strict(&bytes).map(Some),
        Ok(None) => Ok(None),
        Err(e) => Err(TagReadError::Unreadable(e)),
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
/// macOS-only: the `plist` crate is a macOS-only dependency, and the only caller
/// (`set_tags`) is itself macOS-only.
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

/// Writes the full desired tag set to a path's `_kMDItemUserTags` xattr, replacing
/// whatever was there. An empty set REMOVES the xattr (matching Finder clearing all
/// tags), so no empty-array husk lingers.
///
/// Touches ONLY `_kMDItemUserTags`, never `com.apple.FinderInfo` (D11): that 32-byte
/// blob holds `kHasCustomIcon` plus type/creator codes, and modern Finder reads tags
/// straight from `_kMDItemUserTags`, so a custom folder icon survives a tag write.
/// `setxattr` is atomic per attribute, so there's no partial-write window for a single
/// file.
#[cfg(target_os = "macos")]
pub fn set_tags(path: &Path, tags: &[TagRef]) -> std::io::Result<()> {
    if tags.is_empty() {
        // Clear: drop the xattr entirely. Skip the remove when it's already absent so
        // an untagged file doesn't surface a spurious ENOATTR.
        if xattr::get(path, TAGS_XATTR)?.is_some() {
            xattr::remove(path, TAGS_XATTR)?;
        }
        return Ok(());
    }
    let bytes = encode_tags_plist(tags)?;
    xattr::set(path, TAGS_XATTR, &bytes)
}

/// Toggles one system color tag across a (possibly multi-file) selection, preserving
/// every OTHER tag on every file, and returns the resulting per-path tag sets so the
/// caller can patch the listing cache.
///
/// "Applied" for a file = it carries any tag of `color` (a custom tag with the same
/// color counts). Finder's multi-file rule, mirrored here: if ALL paths already carry
/// the color, remove it from all; otherwise add it to all. Removing strips every tag
/// of that color; adding appends the canonical system tag only to files that lack the
/// color (no duplicate, and a same-color custom tag is left intact). A file already in
/// the target state isn't rewritten, so we don't churn its mtime.
///
/// `color` must be 1..=7 (a real color); 0 (colorless) has no system tag to toggle.
/// On a partial failure (one file's write errors mid-loop), earlier files keep their
/// new tags and the error propagates; each per-file `setxattr` is atomic, so no single
/// file is left half-written.
///
/// A file whose current tags can't be read or fully decoded (`read_tags_strict`) is
/// left untouched and listed in `skipped`, and it doesn't count toward the all-have
/// decision: writing it would replace tags we never saw.
#[cfg(target_os = "macos")]
pub fn toggle_color(paths: &[String], color: u8) -> std::io::Result<TagToggleOutcome> {
    let Some(color_name) = system_color_name(color) else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("tag color index must be 1..=7, got {color}"),
        ));
    };

    let mut current: Vec<(String, Vec<TagRef>)> = Vec::with_capacity(paths.len());
    let mut skipped = Vec::new();
    for p in paths {
        match read_tags_strict(Path::new(p)) {
            Ok(tags) => current.push((p.clone(), tags.unwrap_or_default())),
            Err(reason) => skipped.push(TagSkip {
                path: p.clone(),
                reason,
            }),
        }
    }

    // "Add" unless every path already carries the color (Finder's all-have → remove).
    let all_have = !current.is_empty() && current.iter().all(|(_, tags)| tags.iter().any(|t| t.color == color));
    let add = !all_have;

    let path_count = paths.len();
    let mut results = Vec::with_capacity(current.len());
    let mut write_error = None;
    for (path, original) in current {
        let mut tags = original.clone();
        if add {
            if !tags.iter().any(|t| t.color == color) {
                tags.push(TagRef {
                    name: color_name.to_string(),
                    color,
                });
            }
        } else {
            tags.retain(|t| t.color != color);
        }
        // Only write the files that actually change, so re-toggling a mixed selection
        // doesn't needlessly rewrite (and re-mtime) the already-correct ones.
        if tags != original
            && let Err(e) = set_tags(Path::new(&path), &tags)
        {
            write_error = Some(e);
            break;
        }
        results.push((path, tags));
    }

    emit_tag_analytics(add, color_name, path_count, write_error.is_none() && skipped.is_empty());
    match write_error {
        Some(e) => Err(e),
        None => Ok(TagToggleOutcome {
            updates: results,
            skipped,
        }),
    }
}

/// Tells the person a toggle left some files alone (`tags-skipped`, a warning toast in
/// the main window) and logs each one with its reason. The keyboard command and the
/// context-menu circles both call this; the MCP `tag` tool reports skips in its result.
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
// (the off-macOS toggle, and the MCP adapter's starting accumulator).
#[derive(Debug, Default)]
pub struct TagToggleOutcome {
    pub updates: Vec<(String, Vec<TagRef>)>,
    pub skipped: Vec<TagSkip>,
}

/// One file a tag write left alone, and why.
#[derive(Debug)]
pub struct TagSkip {
    pub path: String,
    pub reason: TagReadError,
}

/// Reports one tag toggle, PII-free. Called from `toggle_color`'s single exit, so
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
fn emit_tag_analytics(applied: bool, color: &str, item_count: usize, succeeded: bool) {
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

/// Non-macOS: no Finder tags, so nothing to toggle.
#[cfg(not(target_os = "macos"))]
pub fn toggle_color(_paths: &[String], _color: u8) -> std::io::Result<TagToggleOutcome> {
    Ok(TagToggleOutcome::default())
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
    fn set_tags_is_readable_via_finder_url_api() {
        use objc2::rc::Retained;
        use objc2::runtime::AnyObject;
        use objc2_foundation::{NSArray, NSString, NSURL};

        let dir = temp_dir("nsurl_fidelity");
        let file = dir.join("written.txt");
        std::fs::write(&file, b"x").unwrap();

        set_tags(
            &file,
            &[
                TagRef {
                    name: "Red".to_string(),
                    color: 6,
                },
                TagRef {
                    name: "Work".to_string(),
                    color: 0,
                },
            ],
        )
        .expect("set_tags");

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
