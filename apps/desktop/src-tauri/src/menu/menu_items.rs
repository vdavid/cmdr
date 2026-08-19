//! Menu item builder helpers and shared submenu factories.
//!
//! These helpers are reused by `menu_structure` (top-level menu bar assembly
//! for macOS / Linux) and by the platform `macos.rs` / `linux.rs` modules.
//! Visibility is `pub(super)` so the items stay scoped to the `menu` module.

use std::collections::HashMap;

use tauri::{
    AppHandle, Runtime,
    menu::{CheckMenuItem, MenuItem, PredefinedMenuItem, Submenu},
};

use super::{
    MenuItemEntry, SORT_ASCENDING_ID, SORT_BY_CREATED_ID, SORT_BY_EXTENSION_ID, SORT_BY_MODIFIED_ID, SORT_BY_NAME_ID,
    SORT_BY_SIZE_ID, SORT_DESCENDING_ID, VIEW_MODE_BRIEF_LEFT_ID, VIEW_MODE_BRIEF_RIGHT_ID, VIEW_MODE_FULL_LEFT_ID,
    VIEW_MODE_FULL_RIGHT_ID, VIEW_ZOOM_75_ID, VIEW_ZOOM_100_ID, VIEW_ZOOM_125_ID, VIEW_ZOOM_150_ID, VIEW_ZOOM_IN_ID,
    VIEW_ZOOM_OUT_ID, ViewMode,
};

/// Max chars in the `Copy "<filename>"` context menu label before middle-ellipsis kicks in.
/// Picked to fit typical filenames while capping pathological 100+ char names that blow the menu
/// width.
pub(super) const COPY_FILENAME_MAX_CHARS: usize = 50;

/// Platform-aware accelerator for "Copy path to clipboard".
/// On macOS: Ctrl+Cmd+C. On Linux: Ctrl+Shift+C (Ctrl+Cmd+C becomes Ctrl+Ctrl+C which is broken).
#[cfg(target_os = "macos")]
pub(crate) fn copy_path_accelerator() -> &'static str {
    "Ctrl+Cmd+C"
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn copy_path_accelerator() -> &'static str {
    "Ctrl+Shift+C"
}

/// Platform-aware accelerator for "Show in Finder / file manager".
#[cfg(target_os = "macos")]
pub(crate) fn show_in_file_manager_accelerator() -> &'static str {
    "Opt+Cmd+O"
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn show_in_file_manager_accelerator() -> &'static str {
    "Alt+Ctrl+O"
}

/// Platform-aware label for the "Show in Finder" / "Show in file manager" action.
#[cfg(target_os = "macos")]
pub(crate) fn show_in_file_manager_label() -> &'static str {
    "Show in Finder"
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn show_in_file_manager_label() -> &'static str {
    "Show in &file manager"
}

/// Platform-aware label for the per-pane view-mode CheckMenuItems.
/// Linux uses GTK mnemonics; macOS doesn't.
#[cfg(target_os = "macos")]
pub(crate) fn full_view_label() -> &'static str {
    "Full view"
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn full_view_label() -> &'static str {
    "&Full view"
}

#[cfg(target_os = "macos")]
pub(crate) fn brief_view_label() -> &'static str {
    "Brief view"
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn brief_view_label() -> &'static str {
    "&Brief view"
}

#[cfg(target_os = "macos")]
pub(crate) fn left_pane_label() -> &'static str {
    "Left pane"
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn left_pane_label() -> &'static str {
    "&Left pane"
}

#[cfg(target_os = "macos")]
pub(crate) fn right_pane_label() -> &'static str {
    "Right pane"
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn right_pane_label() -> &'static str {
    "&Right pane"
}

/// The View menu's per-pane view-mode block: four `CheckMenuItem`s and the two
/// submenus holding them.
pub(crate) struct ViewModeItems<R: Runtime> {
    pub full_left: CheckMenuItem<R>,
    pub brief_left: CheckMenuItem<R>,
    pub full_right: CheckMenuItem<R>,
    pub brief_right: CheckMenuItem<R>,
    pub left_submenu: Submenu<R>,
    pub right_submenu: Submenu<R>,
}

/// Builds `View > Left pane > {Full, Brief}` and the same for the right pane
/// (shared between macOS and Linux).
///
/// Both pairs always exist; only the ACTIVE pane's pair carries the accelerator,
/// so the shortcut hint visually follows focus as the user tabs between panes.
/// This is the initial build, where left is the active pane and the right pane
/// defaults to Brief; `menu_handlers::rebuild_view_mode_items` takes over from
/// there, and it depends on Full sitting at position 0 and Brief at 1 in each
/// submenu — it removes and reinserts by index, because Tauri has no
/// `set_accelerator()`.
pub(crate) fn build_view_mode_items<R: Runtime>(
    app: &AppHandle<R>,
    view_mode: ViewMode,
) -> tauri::Result<ViewModeItems<R>> {
    let full_left = CheckMenuItem::with_id(
        app,
        VIEW_MODE_FULL_LEFT_ID,
        full_view_label(),
        true,
        view_mode == ViewMode::Full,
        Some("Cmd+1"),
    )?;
    let brief_left = CheckMenuItem::with_id(
        app,
        VIEW_MODE_BRIEF_LEFT_ID,
        brief_view_label(),
        true,
        view_mode == ViewMode::Brief,
        Some("Cmd+2"),
    )?;
    let full_right = CheckMenuItem::with_id(
        app,
        VIEW_MODE_FULL_RIGHT_ID,
        full_view_label(),
        true,
        false,
        None::<&str>,
    )?;
    let brief_right = CheckMenuItem::with_id(
        app,
        VIEW_MODE_BRIEF_RIGHT_ID,
        brief_view_label(),
        true,
        true,
        None::<&str>,
    )?;

    let left_submenu = Submenu::with_items(app, left_pane_label(), true, &[&full_left, &brief_left])?;
    let right_submenu = Submenu::with_items(app, right_pane_label(), true, &[&full_right, &brief_right])?;

    Ok(ViewModeItems {
        full_left,
        brief_left,
        full_right,
        brief_right,
        left_submenu,
        right_submenu,
    })
}

/// Items returned from `build_sort_submenu` so callers can register the sort items
/// in the items HashMap for accelerator updates.
pub(crate) struct SortSubmenuItems<R: Runtime> {
    pub submenu: Submenu<R>,
    pub by_name: MenuItem<R>,
    pub by_extension: MenuItem<R>,
    pub by_modified: MenuItem<R>,
    pub by_size: MenuItem<R>,
}

/// Builds the Sort by submenu (shared between macOS and Linux).
///
/// Accelerators for Name/Extension/Date modified/Size are caller-provided so each
/// platform can pass `None` where the toolkit can't deliver the chord.
pub(crate) fn build_sort_submenu<R: Runtime>(
    app: &AppHandle<R>,
    label: &str,
    accel_name: Option<&str>,
    accel_extension: Option<&str>,
    accel_modified: Option<&str>,
    accel_size: Option<&str>,
) -> tauri::Result<SortSubmenuItems<R>> {
    let sort_by_name = MenuItem::with_id(app, SORT_BY_NAME_ID, "Name", true, accel_name)?;
    let sort_by_ext = MenuItem::with_id(app, SORT_BY_EXTENSION_ID, "Extension", true, accel_extension)?;
    let sort_by_modified = MenuItem::with_id(app, SORT_BY_MODIFIED_ID, "Date modified", true, accel_modified)?;
    let sort_by_size = MenuItem::with_id(app, SORT_BY_SIZE_ID, "Size", true, accel_size)?;
    let sort_by_created = MenuItem::with_id(app, SORT_BY_CREATED_ID, "Date created", true, None::<&str>)?;
    let sort_asc = MenuItem::with_id(app, SORT_ASCENDING_ID, "Ascending", true, None::<&str>)?;
    let sort_desc = MenuItem::with_id(app, SORT_DESCENDING_ID, "Descending", true, None::<&str>)?;

    let submenu = Submenu::with_items(
        app,
        label,
        true,
        &[
            &sort_by_name,
            &sort_by_ext,
            &sort_by_modified,
            &sort_by_size,
            &sort_by_created,
            &PredefinedMenuItem::separator(app)?,
            &sort_asc,
            &sort_desc,
        ],
    )?;

    Ok(SortSubmenuItems {
        submenu,
        by_name: sort_by_name,
        by_extension: sort_by_ext,
        by_modified: sort_by_modified,
        by_size: sort_by_size,
    })
}

/// Registers the four shortcut-bound Sort by items for accelerator updates.
///
/// The positions live here, beside the `Submenu::with_items` call that sets them.
/// `register_item_positions_match_submenu_order` can only cross-check a submenu whose
/// item array a platform file spells out itself, so indices hardcoded over there against
/// this layout would go stale unnoticed the moment the order changes.
///
/// Date created and the ascending / descending items carry no accelerator and no
/// user-customizable shortcut, so nothing needs to reinsert them.
pub(crate) fn register_sort_items<R: Runtime>(
    items: &mut HashMap<String, MenuItemEntry<R>>,
    sort_items: &SortSubmenuItems<R>,
) {
    let submenu = &sort_items.submenu;
    register_item(items, SORT_BY_NAME_ID, &sort_items.by_name, submenu, 0);
    register_item(items, SORT_BY_EXTENSION_ID, &sort_items.by_extension, submenu, 1);
    register_item(items, SORT_BY_MODIFIED_ID, &sort_items.by_modified, submenu, 2);
    register_item(items, SORT_BY_SIZE_ID, &sort_items.by_size, submenu, 3);
}

/// Builds the View > Zoom submenu (shared between macOS and Linux).
///
/// Each preset item writes `appearance.textSize` directly via the unified
/// command-execute event. Zoom in/out adjust the value by 10 percentage
/// points. `accel_in` / `accel_out` are platform-specific accelerator strings
/// (macOS uses `Cmd+Plus` / `Cmd+Minus`, Linux uses `None` because GTK
/// intercepts these keys at the toolkit level).
pub(crate) fn build_zoom_submenu<R: Runtime>(
    app: &AppHandle<R>,
    accel_100: Option<&str>,
    accel_in: Option<&str>,
    accel_out: Option<&str>,
) -> tauri::Result<Submenu<R>> {
    let zoom_75 = MenuItem::with_id(app, VIEW_ZOOM_75_ID, "75%", true, None::<&str>)?;
    let zoom_100 = MenuItem::with_id(app, VIEW_ZOOM_100_ID, "100%", true, accel_100)?;
    let zoom_125 = MenuItem::with_id(app, VIEW_ZOOM_125_ID, "125%", true, None::<&str>)?;
    let zoom_150 = MenuItem::with_id(app, VIEW_ZOOM_150_ID, "150%", true, None::<&str>)?;
    let zoom_in = MenuItem::with_id(app, VIEW_ZOOM_IN_ID, "Zoom in", true, accel_in)?;
    let zoom_out = MenuItem::with_id(app, VIEW_ZOOM_OUT_ID, "Zoom out", true, accel_out)?;

    Submenu::with_items(
        app,
        "Zoom",
        true,
        &[
            &zoom_75,
            &zoom_100,
            &zoom_125,
            &zoom_150,
            &PredefinedMenuItem::separator(app)?,
            &zoom_in,
            &zoom_out,
        ],
    )
}

/// Registers a regular MenuItem in the items HashMap for accelerator updates.
pub(crate) fn register_item<R: Runtime>(
    items: &mut HashMap<String, MenuItemEntry<R>>,
    id: &str,
    item: &MenuItem<R>,
    submenu: &Submenu<R>,
    position: usize,
) {
    items.insert(
        id.to_string(),
        MenuItemEntry {
            item: item.clone(),
            submenu: submenu.clone(),
            position,
        },
    );
}

/// Truncate a filename for use inside a menu label, preserving the extension.
///
/// If the filename fits within `max_chars` (counted in chars, not bytes), it's returned unchanged.
/// Otherwise produces `<prefix>…<suffix>` where the suffix keeps the file extension plus a few
/// preceding chars, and the prefix takes ~60% of the budget. Operates on chars so multi-byte
/// UTF-8 sequences are never split mid-codepoint.
pub(super) fn truncate_for_menu_label(filename: &str, max_chars: usize) -> String {
    let total_chars = filename.chars().count();
    if total_chars <= max_chars {
        return filename.to_string();
    }

    // Reserve one char for the ellipsis itself.
    if max_chars == 0 {
        return String::new();
    }
    if max_chars == 1 {
        return "\u{2026}".to_string();
    }
    let budget = max_chars - 1;
    let prefix_chars = budget * 6 / 10;
    let suffix_chars = budget - prefix_chars;

    // Find the extension (everything after the last '.', but only if there's a non-empty stem).
    // `Path::extension` skips leading-dot files and returns just the ext without the dot, which is
    // what we want here; we treat names like ".gitignore" as extensionless.
    let ext_with_dot = std::path::Path::new(filename)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| format!(".{e}"))
        .unwrap_or_default();
    let ext_chars = ext_with_dot.chars().count();

    // If the extension alone doesn't fit in the suffix budget, fall back to a plain ~60/40
    // middle-ellipsis split (the extension is too long to be useful here anyway).
    let suffix: String = if ext_chars > 0 && ext_chars <= suffix_chars {
        // Keep the full extension plus some chars before it (the part of the stem near the end).
        let pre_ext_chars = suffix_chars - ext_chars;
        let stem_len = total_chars - ext_chars;
        let take_from = stem_len.saturating_sub(pre_ext_chars);
        filename
            .chars()
            .skip(take_from)
            .take(pre_ext_chars + ext_chars)
            .collect()
    } else {
        filename.chars().skip(total_chars - suffix_chars).collect()
    };

    let prefix: String = filename.chars().take(prefix_chars).collect();
    format!("{prefix}\u{2026}{suffix}")
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let long = "Obviously Awesome How to Nail Product Positioning so Customers Get It, Buy It, Love It Audiobook - m4b.epub";
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
    /// Two reasons: macOS kerns `...` visibly worse next to system items, and
    /// `set_macos_menu_icons` matches SF Symbols by exact title string, so a `...`
    /// title silently loses its icon (six of them once did).
    #[test]
    fn menu_labels_use_the_ellipsis_character() {
        const SOURCES: [(&str, &str); 4] = [
            ("macos.rs", include_str!("macos.rs")),
            ("linux.rs", include_str!("linux.rs")),
            ("menu_structure.rs", include_str!("menu_structure.rs")),
            ("open_with.rs", include_str!("open_with.rs")),
        ];
        // AppKit injects these titles with literal periods; `cleanup_macos_menus`
        // matches them byte-for-byte to strip them, so they must stay as macOS ships them.
        const SYSTEM_INJECTED: [&str; 1] = ["\"Start Dictation...\""];

        for (name, source) in SOURCES {
            for (line_number, line) in source.lines().enumerate() {
                if SYSTEM_INJECTED.iter().any(|title| line.contains(title)) {
                    continue;
                }
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
            for (id, item, submenu, position) in &registrations {
                // Submenus assembled by a helper (`build_zoom_submenu`, `build_sort_submenu`) have
                // no literal item array in this file, so their order isn't checkable from here.
                let Some(entries) = layouts.get(submenu.as_str()) else {
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
                checked > 20,
                "{name}: the verifiable-registration count fell to {checked}; the parser stopped matching the source"
            );
        }
    }

    /// Maps each `let <name> = Submenu::with_items(…, &[…])` to its ordered item expressions.
    fn parse_submenu_layouts(source: &str) -> HashMap<String, Vec<String>> {
        let mut layouts = HashMap::new();
        let mut lines = source.lines();
        while let Some(line) = lines.next() {
            let Some(rest) = line.trim().strip_prefix("let ") else {
                continue;
            };
            let Some((submenu_name, _)) = rest.split_once(" = Submenu::with_items(") else {
                continue;
            };
            // Walk to the item array, giving up if this call doesn't spell one out.
            let mut found_array = false;
            for inner in lines.by_ref() {
                match inner.trim() {
                    "&[" => {
                        found_array = true;
                        break;
                    }
                    ")?;" => break,
                    _ => {}
                }
            }
            if !found_array {
                continue;
            }
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
}
