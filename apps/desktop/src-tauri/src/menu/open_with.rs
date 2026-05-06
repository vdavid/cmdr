//! "Open with" submenu builder.
//!
//! Builds a native submenu from a list of candidate apps. Each item uses a stable ID
//! of the form `open-with:<bundle-id>` — `lib.rs::on_menu_event` prefix-matches that
//! and routes the click to `file_system::open_with::open_paths_with`. App URLs are
//! cached in `MenuState.open_with_apps` keyed by bundle ID so we can resolve the
//! click target without encoding paths into menu IDs.
//!
//! Each candidate uses `IconMenuItem` with the app's main icon (loaded by
//! `file_system::open_with::load_app_icon` at candidate-list build time). App icons
//! are full-color non-template images, so they render correctly through `IconMenuItem`
//! — the existing "no icons on context menus" decision in `menu/CLAUDE.md` was
//! specific to SF Symbol *template* images, which need auto-tinting that rasterized
//! bitmaps can't reproduce.
//!
//! The first candidate (the OS default for the right-clicked file) gets a plain-text
//! ` (default)` suffix. TODO: once `tauri-apps/muda#353` lands and Tauri exposes
//! `set_text_with_secondary` on its menu wrapper, render that suffix in
//! `NSColor.secondaryLabelColor` to match Finder's "Open with" submenu styling.

use std::collections::HashMap;
use std::path::PathBuf;

use tauri::image::Image;
use tauri::{
    AppHandle, Runtime,
    menu::{IconMenuItem, MenuItem, PredefinedMenuItem, Submenu},
};

use crate::file_system::open_with::AppCandidate;

/// Menu item ID prefix for "Open with" candidate apps. Followed by the app's bundle ID.
pub const OPEN_WITH_ID_PREFIX: &str = "open-with:";

/// Menu item ID for "Open with → Other..." (NSOpenPanel picker).
pub const OPEN_WITH_OTHER_ID: &str = "open-with-other";

/// Builds the "Open with" submenu and returns it alongside a `bundle_id → app_path`
/// map that the caller stores in `MenuState` so click events can resolve the launch
/// target.
pub fn build_open_with_submenu<R: Runtime>(
    app: &AppHandle<R>,
    candidates: &[AppCandidate],
) -> tauri::Result<(Submenu<R>, HashMap<String, PathBuf>)> {
    let submenu = Submenu::new(app, "Open with", true)?;
    let mut bundle_to_path: HashMap<String, PathBuf> = HashMap::new();

    if candidates.is_empty() {
        // Empty intersection (heterogeneous selection, or no apps registered).
        // Match Finder: still expose "Other..." so the user can manually pick.
    } else {
        for (idx, candidate) in candidates.iter().enumerate() {
            let label = if idx == 0 {
                format!("{} (default)", candidate.display_name)
            } else {
                candidate.display_name.clone()
            };
            let id = format!("{OPEN_WITH_ID_PREFIX}{}", candidate.bundle_id);
            // `IconMenuItem` with `Some(image)` falls back to the text-only renderer
            // when the image build fails — the menu still works without the icon.
            let icon: Option<Image<'static>> = candidate
                .icon
                .as_ref()
                .map(|i| Image::new_owned(i.rgba.clone(), i.width, i.height));
            let item = IconMenuItem::with_id(app, &id, &label, true, icon, None::<&str>)?;
            submenu.append(&item)?;
            bundle_to_path.insert(candidate.bundle_id.clone(), candidate.app_path.clone());
        }
        submenu.append(&PredefinedMenuItem::separator(app)?)?;
    }

    let other_item = MenuItem::with_id(app, OPEN_WITH_OTHER_ID, "Other\u{2026}", true, None::<&str>)?;
    submenu.append(&other_item)?;

    Ok((submenu, bundle_to_path))
}
