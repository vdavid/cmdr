//! Tests for the menu item builder helpers (`menu_items.rs`).
//!
//! Split out under the directory's `*_test.rs` convention: most of the weight here is
//! `register_item_positions_match_submenu_order`'s small Rust-source parser, which reads
//! `macos.rs` and `linux.rs` back as text rather than exercising `menu_items.rs` itself.

use super::*;

#[test]
fn a_disk_row_says_eject_and_a_phone_row_says_disconnect() {
    // Pre-fix a phone's native menu read "Eject (Pixel 8)" while the inline
    // control on the same row already said Disconnect.
    assert_eq!(detach_label("Backup", false, DetachWord::Eject), "Eject (Backup)");
    assert_eq!(detach_label("Backup", true, DetachWord::Eject), "Eject (Backup) (busy)");
    assert_eq!(detach_label("Pixel 8", false, DetachWord::Disconnect), "Disconnect");
    assert_eq!(
        detach_label("Pixel 8", true, DetachWord::Disconnect),
        "Disconnect (busy)"
    );
}

#[test]
fn the_detach_word_comes_off_the_volume_id() {
    use cmdr_fs::volume::{adb_volume_id, mtp_device_id, path_volume_id};

    assert_eq!(
        DetachWord::for_volume_id(&adb_volume_id("39041FDJH00A0K")),
        DetachWord::Disconnect
    );
    // MTP keeps Eject: it earns the promise by closing the device session.
    assert_eq!(
        DetachWord::for_volume_id(&mtp_device_id("39041FDJH00A0K")),
        DetachWord::Eject
    );
    assert_eq!(
        DetachWord::for_volume_id(&path_volume_id("/Volumes/Backup")),
        DetachWord::Eject
    );
}

#[test]
fn test_truncate_for_menu_label_short_passes_through() {
    assert_eq!(truncate_for_menu_label("hello.txt", 50), "hello.txt");
    assert_eq!(truncate_for_menu_label("", 50), "");
    // Exactly at the limit
    let exactly_50 = "a".repeat(50);
    assert_eq!(truncate_for_menu_label(&exactly_50, 50), exactly_50);
}

#[test]
fn test_truncate_for_menu_label_long_with_extension_keeps_extension() {
    let long =
        "Obviously Awesome How to Nail Product Positioning so Customers Get It, Buy It, Love It Audiobook - m4b.epub";
    let truncated = truncate_for_menu_label(long, 50);
    assert!(truncated.chars().count() <= 50);
    assert!(
        truncated.ends_with(".epub"),
        "expected extension preserved, got: {truncated}"
    );
    assert!(truncated.contains('\u{2026}'), "expected ellipsis, got: {truncated}");
    assert!(
        truncated.starts_with("Obviously"),
        "expected prefix preserved, got: {truncated}"
    );
}

#[test]
fn test_truncate_for_menu_label_long_without_extension() {
    let long = "a".repeat(100);
    let truncated = truncate_for_menu_label(&long, 50);
    assert!(truncated.chars().count() <= 50);
    assert!(truncated.contains('\u{2026}'));
    // No extension means a ~60/40 split with the ellipsis in the middle.
    let parts: Vec<&str> = truncated.split('\u{2026}').collect();
    assert_eq!(parts.len(), 2);
    assert!(!parts[0].is_empty());
    assert!(!parts[1].is_empty());
}

#[test]
fn test_truncate_for_menu_label_multibyte_utf8() {
    // Each emoji is multi-byte in UTF-8; the helper must count chars and never split mid-byte.
    let name = "🎉".repeat(40) + ".txt";
    let truncated = truncate_for_menu_label(&name, 20);
    assert!(truncated.chars().count() <= 20);
    // Round-trip through str must succeed (already guaranteed by String, but assert it's valid):
    assert!(std::str::from_utf8(truncated.as_bytes()).is_ok());
    assert!(truncated.contains('\u{2026}'));
    assert!(truncated.ends_with(".txt"));

    // Accented chars (single codepoint each) should also work cleanly.
    let accented = "ÁrvíztűrőTükörfúrógép".repeat(5);
    let truncated2 = truncate_for_menu_label(&accented, 15);
    assert!(truncated2.chars().count() <= 15);
    assert!(std::str::from_utf8(truncated2.as_bytes()).is_ok());
}

#[test]
fn test_truncate_for_menu_label_max_smaller_than_extension() {
    // When the extension is longer than the suffix budget, fall back to plain middle-ellipsis.
    // ".verylongextension" is 18 chars; with max_chars=10, suffix budget is only 4.
    let name = "stem.verylongextension";
    let truncated = truncate_for_menu_label(name, 10);
    assert!(truncated.chars().count() <= 10);
    assert!(truncated.contains('\u{2026}'));
    // Should not panic; should produce valid UTF-8.
    assert!(std::str::from_utf8(truncated.as_bytes()).is_ok());

    // Edge: max_chars = 1 yields just the ellipsis.
    assert_eq!(truncate_for_menu_label("anything.txt", 1), "\u{2026}");
    // Edge: max_chars = 0 yields empty string.
    assert_eq!(truncate_for_menu_label("anything.txt", 0), "");
}

/// Menu labels end with the U+2026 ellipsis character, never three periods.
/// macOS kerns `...` visibly worse next to system items, and the SF Symbol pass finds an
/// `NSMenuItem` by its title, so a stray `...` costs the item its icon.
///
/// Labels themselves live in the message catalog now, so the catalog is where
/// the real guard is (`native_strings::menu_labels_end_with_the_ellipsis_character`).
/// This one keeps watch over the literals still written in these files, so a
/// new one can't slip in with `...`.
#[test]
fn menu_labels_use_the_ellipsis_character() {
    const SOURCES: [(&str, &str); 4] = [
        ("macos.rs", include_str!("macos.rs")),
        ("linux.rs", include_str!("linux.rs")),
        ("menu_structure.rs", include_str!("menu_structure.rs")),
        ("open_with.rs", include_str!("open_with.rs")),
    ];

    for (name, source) in SOURCES {
        for (line_number, line) in source.lines().enumerate() {
            assert!(
                !line.contains("...\""),
                "{name}:{} ends a menu label with `...`; use `\\u{{2026}}`: {}",
                line_number + 1,
                line.trim()
            );
        }
    }
}

/// Every `register_item` position matches the item's real index in its submenu.
///
/// `MenuState` remembers `(submenu, position)` so `update_menu_item_accelerator` can
/// remove-and-reinsert an item (Tauri has no `set_accelerator()`). A wrong index moves a
/// DIFFERENT item on the first rebind, and nothing notices until a user edits a shortcut.
/// Reading the source is the only guard available here: building a real menu needs AppKit
/// on the main thread.
#[test]
fn register_item_positions_match_submenu_order() {
    const SOURCES: [(&str, &str); 2] = [
        ("macos.rs", include_str!("macos.rs")),
        ("linux.rs", include_str!("linux.rs")),
    ];

    for (name, source) in SOURCES {
        let layouts = parse_submenu_layouts(source);
        let registrations = parse_register_item_calls(source);
        assert!(
            registrations.len() > 20,
            "{name}: only {} `register_item` calls parsed; the parser is broken, not the source",
            registrations.len()
        );

        let mut checked = 0;
        let mut unreadable = Vec::new();
        for (id, item, submenu, position) in &registrations {
            let Some(entries) = layouts.get(submenu.as_str()) else {
                unreadable.push(format!("`{id}` in `{submenu}`"));
                continue;
            };
            let actual = entries.get(*position).map(String::as_str);
            assert_eq!(
                actual,
                Some(item.as_str()),
                "{name}: `{id}` registers `{item}` at position {position} of `{submenu}`, \
                 which actually holds {actual:?}. Fix the index AND the position comment above it."
            );
            checked += 1;
        }
        assert!(
            unreadable.is_empty(),
            "{name}: the parser found no item array for {unreadable:?}, so those positions go unchecked. \
             Teach `parse_submenu_layouts` the layout it missed."
        );
        assert!(
            checked > 20,
            "{name}: the verifiable-registration count fell to {checked}; the parser stopped matching the source"
        );
    }
}

/// Maps each `let <name> = Submenu::with_items(…, &[…])` to its ordered item expressions.
/// `Submenu::with_id_and_items` counts the same: the ID goes before the label, and neither
/// shifts the item array this reads.
///
/// rustfmt keeps a long array one entry per line but collapses a short one onto a single
/// `&[&a, &b],` line, so both shapes count: a two-item menu is as easy to misnumber as a long one.
fn parse_submenu_layouts(source: &str) -> HashMap<String, Vec<String>> {
    enum ItemArray {
        OnePerLine,
        OneLine(Vec<String>),
    }

    let mut layouts = HashMap::new();
    let mut lines = source.lines();
    while let Some(line) = lines.next() {
        let Some(rest) = line.trim().strip_prefix("let ") else {
            continue;
        };
        let Some((submenu_name, _)) = rest
            .split_once(" = Submenu::with_items(")
            .or_else(|| rest.split_once(" = Submenu::with_id_and_items("))
        else {
            continue;
        };
        // Walk to the item array, giving up if this call doesn't spell one out.
        let mut array = None;
        for inner in lines.by_ref() {
            let trimmed = inner.trim();
            if trimmed == "&[" {
                array = Some(ItemArray::OnePerLine);
                break;
            }
            if let Some(items) = trimmed.strip_prefix("&[").and_then(|rest| rest.strip_suffix("],")) {
                array = Some(ItemArray::OneLine(split_top_level(items)));
                break;
            }
            if trimmed == ")?;" {
                break;
            }
        }
        let entries = match array {
            None => continue,
            Some(ItemArray::OneLine(entries)) => entries,
            Some(ItemArray::OnePerLine) => {
                let mut entries = Vec::new();
                for inner in lines.by_ref() {
                    let entry = inner.trim();
                    if entry == "]," {
                        break;
                    }
                    if entry.starts_with("//") {
                        continue;
                    }
                    entries.push(entry.trim_end_matches(',').to_string());
                }
                entries
            }
        };
        layouts.insert(submenu_name.to_string(), entries);
    }
    layouts
}

/// Pulls `(id, item expression, submenu expression, position)` out of every `register_item` call.
fn parse_register_item_calls(source: &str) -> Vec<(String, String, String, usize)> {
    const CALL: &str = "register_item(";
    let mut calls = Vec::new();
    let mut rest = source;
    while let Some(at) = rest.find(CALL) {
        let after = &rest[at + CALL.len()..];
        let Some(end) = closing_paren(after) else {
            break;
        };
        let args: Vec<String> = split_top_level(&after[..end]);
        rest = &after[end..];
        let [_items, id, item, submenu, position] = args.as_slice() else {
            continue;
        };
        let Ok(position) = position.parse::<usize>() else {
            continue;
        };
        calls.push((
            id.clone(),
            item.clone(),
            submenu.trim_start_matches('&').to_string(),
            position,
        ));
    }
    calls
}

/// Byte offset of the `)` closing the call whose arguments `text` starts.
fn closing_paren(text: &str) -> Option<usize> {
    let mut depth = 0usize;
    for (offset, character) in text.char_indices() {
        match character {
            '(' => depth += 1,
            ')' if depth == 0 => return Some(offset),
            ')' => depth -= 1,
            _ => {}
        }
    }
    None
}

/// Splits an argument list on depth-zero commas, dropping all whitespace.
fn split_top_level(args: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut current = String::new();
    for character in args.chars() {
        match character {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                parts.push(std::mem::take(&mut current));
                continue;
            }
            _ => {}
        }
        if !character.is_whitespace() {
            current.push(character);
        }
    }
    if !current.is_empty() {
        parts.push(current);
    }
    parts
}
