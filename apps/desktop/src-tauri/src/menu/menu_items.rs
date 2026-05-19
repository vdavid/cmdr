//! Menu item builder helpers and shared submenu factories.
//!
//! These helpers are reused by `menu_structure` (top-level menu bar assembly
//! for macOS / Linux) and by the platform `macos.rs` / `linux.rs` modules.
//! Visibility is `pub(super)` so the items stay scoped to the `menu` module.

use std::collections::HashMap;

use tauri::{
    AppHandle, Runtime,
    menu::{MenuItem, PredefinedMenuItem, Submenu},
};

use super::{
    MenuItemEntry, SORT_ASCENDING_ID, SORT_BY_CREATED_ID, SORT_BY_EXTENSION_ID, SORT_BY_MODIFIED_ID, SORT_BY_NAME_ID,
    SORT_BY_SIZE_ID, SORT_DESCENDING_ID, VIEW_ZOOM_75_ID, VIEW_ZOOM_100_ID, VIEW_ZOOM_125_ID, VIEW_ZOOM_150_ID,
    VIEW_ZOOM_IN_ID, VIEW_ZOOM_OUT_ID,
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
}
