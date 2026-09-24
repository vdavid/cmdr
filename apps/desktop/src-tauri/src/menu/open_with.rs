//! "Open with" submenu builder.
//!
//! Builds a native submenu from a list of candidate apps. Each item uses a stable ID
//! of the form `open-with:<bundle-id>`; `lib.rs::on_menu_event` prefix-matches that
//! and routes the click to `file_system::open_with::open_paths_with`. App URLs are
//! cached in `MenuState.open_with_apps` keyed by bundle ID so we can resolve the
//! click target without encoding paths into menu IDs.
//!
//! Each candidate is a plain item. Its app icon (loaded inside
//! `file_system::open_with::compute_open_with_choices` as it builds the candidate list)
//! lands when the menu starts tracking, through `context_menu_icons.rs`, which finds this
//! submenu by `OPEN_WITH_SUBMENU_ID`.
//!
//! The candidates come from LaunchServices off the main thread (`context_menu_facts.rs`).
//! When they haven't answered by the time the menu goes up, the submenu opens with a
//! disabled "Finding apps…" line in their place ([`build_pending_open_with_submenu`]), and
//! [`fill_open_with_submenu`] swaps the real list in while the menu is open.
//!
//! The first candidate (the OS default for the right-clicked file) gets a plain-text
//! ` (default)` suffix. TODO: muda has `set_styled_text` now (`tauri-apps/muda#353`), so
//! the remaining gap is Tauri: once its menu wrappers expose it, pass the suffix as a
//! `TextStyle::Secondary` part so it renders in `NSColor.secondaryLabelColor`, matching
//! Finder's "Open with" submenu.

use std::collections::HashMap;
use std::path::PathBuf;

use tauri::{
    AppHandle, Runtime,
    menu::{MenuItem, PredefinedMenuItem, Submenu},
};

use super::context_menu_live::Placeheld;
use crate::file_system::open_with::AppCandidate;
use crate::intl::{menu_t, menu_t_with};

/// Menu item ID prefix for "Open with" candidate apps. Followed by the app's bundle ID.
pub const OPEN_WITH_ID_PREFIX: &str = "open-with:";

/// The "Open with" submenu's own ID, which `context_menu_icons.rs` finds it by to put the
/// app icons on its items. Outside the `open-with:` family, which `handle_menu_event`
/// prefix-routes as launch targets.
pub const OPEN_WITH_SUBMENU_ID: &str = "open-with-submenu";

/// Menu item ID for "Open with → Other…" (NSOpenPanel picker).
pub const OPEN_WITH_OTHER_ID: &str = "open-with-other";

/// Builds the "Open with" submenu and returns it alongside a `bundle_id → app_path`
/// map that the caller stores in `MenuState` so click events can resolve the launch
/// target.
pub fn build_open_with_submenu<R: Runtime>(
    app: &AppHandle<R>,
    candidates: &[AppCandidate],
) -> tauri::Result<(Submenu<R>, HashMap<String, PathBuf>)> {
    let submenu = Submenu::with_id(app, OPEN_WITH_SUBMENU_ID, menu_t("menu.context.openWith"), true)?;
    // An empty intersection (a mixed selection, or no apps registered) still gets "Other…",
    // like Finder, so the user can pick an app by hand.
    let (items, bundle_to_path) = candidate_items(app, candidates)?;
    for item in &items {
        submenu.append(item)?;
    }
    if !items.is_empty() {
        submenu.append(&PredefinedMenuItem::separator(app)?)?;
    }
    submenu.append(&other_item(app)?)?;
    Ok((submenu, bundle_to_path))
}

/// The "Open with" submenu for a menu that went up before LaunchServices answered: a
/// disabled "Finding apps…" line, a separator, and "Other…", which works without the list.
pub fn build_pending_open_with_submenu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Placeheld<R>> {
    let submenu = Submenu::with_id(app, OPEN_WITH_SUBMENU_ID, menu_t("menu.context.openWith"), true)?;
    let placeholder = MenuItem::new(app, menu_t("menu.context.openWithLoading"), false, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    submenu.append(&placeholder)?;
    submenu.append(&separator)?;
    submenu.append(&other_item(app)?)?;
    Ok(Placeheld {
        submenu,
        placeholder,
        separator,
    })
}

/// Swaps the late candidate list into a pending submenu, which may be open on screen, leaving
/// the shape [`build_open_with_submenu`] would have built. Answers the `bundle_id → app_path`
/// map for `MenuState`.
///
/// ❗ Runs while the menu tracks, so it touches only the submenu and its items, never the
/// context menu itself: muda holds that one borrowed for the whole popup.
pub fn fill_open_with_submenu<R: Runtime>(
    app: &AppHandle<R>,
    pending: &Placeheld<R>,
    candidates: &[AppCandidate],
) -> tauri::Result<HashMap<String, PathBuf>> {
    let (items, bundle_to_path) = candidate_items(app, candidates)?;
    pending.submenu.remove(&pending.placeholder)?;
    if items.is_empty() {
        pending.submenu.remove(&pending.separator)?;
    }
    for (position, item) in items.iter().enumerate() {
        pending.submenu.insert(item, position)?;
    }
    Ok(bundle_to_path)
}

/// The candidates' items, and the `bundle_id → app_path` map a click resolves its app through.
type CandidateItems<R> = (Vec<MenuItem<R>>, HashMap<String, PathBuf>);

/// One item per candidate, the OS default first and marked.
fn candidate_items<R: Runtime>(app: &AppHandle<R>, candidates: &[AppCandidate]) -> tauri::Result<CandidateItems<R>> {
    let mut items = Vec::with_capacity(candidates.len());
    let mut bundle_to_path: HashMap<String, PathBuf> = HashMap::new();
    for (idx, candidate) in candidates.iter().enumerate() {
        let label = if idx == 0 {
            menu_t_with("menu.context.openWithDefault", &[("app", &candidate.display_name)])
        } else {
            candidate.display_name.clone()
        };
        let id = format!("{OPEN_WITH_ID_PREFIX}{}", candidate.bundle_id);
        items.push(MenuItem::with_id(app, &id, &label, true, None::<&str>)?);
        bundle_to_path.insert(candidate.bundle_id.clone(), candidate.app_path.clone());
    }
    Ok((items, bundle_to_path))
}

fn other_item<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<MenuItem<R>> {
    MenuItem::with_id(
        app,
        OPEN_WITH_OTHER_ID,
        menu_t("menu.context.openWithOther"),
        true,
        None::<&str>,
    )
}
