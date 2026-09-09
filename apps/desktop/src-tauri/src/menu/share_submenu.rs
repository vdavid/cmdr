//! `Share` in the file context menu (macOS): one item per service macOS offers.
//!
//! Hand-built from `file_system::share`'s enumeration, which is what lets the item be
//! ABSENT when macOS offers nothing — the whole point of the submenu. The system
//! popover can't answer that question before it's on screen, so it used to come up
//! holding only `Edit Extensions…`.
//!
//! Each item's ID is `share-service:<index>` into the offer that filled the menu, and
//! `menu_handlers.rs` prefix-routes the click straight to
//! `file_system::share::perform_offered`. The index (rather than the title) is the key
//! for the house reason: a title is macOS copy in the system language, and two
//! extensions may well share one.
//!
//! The submenu closes with `Edit extensions`, which opens System Settings' Extensions
//! pane. Since the list only ever holds what macOS currently offers, that item is the
//! one route from Cmdr to the place a missing service is actually turned back on.

use tauri::image::Image;
use tauri::menu::{IconMenuItem, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Runtime};

use crate::file_system::share::ShareService;
use crate::intl::menu_t;

/// Menu item ID prefix for one offered service. Followed by its index in the offer.
pub const SHARE_SERVICE_ID_PREFIX: &str = "share-service:";

/// Menu item ID for the trailing `Edit extensions`. Deliberately outside the
/// `share-service:` family: it performs no share, and `share_service_index` must refuse
/// it (`only_this_familys_ids_resolve_to_an_index`).
pub const SHARE_EDIT_EXTENSIONS_ID: &str = "share-edit-extensions";

/// The Extensions pane of System Settings, where share extensions are turned on and off.
pub const EXTENSIONS_SETTINGS_URL: &str = "x-apple.systempreferences:com.apple.ExtensionsPreferences";

/// The submenu's own title. No trailing `…`: the house rule keeps that for a dialog
/// that changes WHAT the command acts on, and a submenu changes nothing.
fn share_label() -> String {
    menu_t("menu.context.share")
}

/// The ID for the service at `index`.
pub fn share_service_id(index: usize) -> String {
    format!("{SHARE_SERVICE_ID_PREFIX}{index}")
}

/// The offer index a clicked ID names, or `None` when the tail isn't one.
///
/// Answers rather than trusts: the ID space is shared with every other menu item, and
/// a click that resolved to a wrong index would share the wrong file.
pub fn share_service_index(id: &str) -> Option<usize> {
    id.strip_prefix(SHARE_SERVICE_ID_PREFIX)?.parse().ok()
}

/// Builds the `Share` submenu over `services`, in the order macOS gave them.
///
/// ❗ Call it only for a non-empty `services`: an empty submenu is the symptom this
/// replaced, and the caller (`menu_structure.rs`) leaves the whole item out instead.
pub fn build_share_submenu<R: Runtime>(app: &AppHandle<R>, services: &[ShareService]) -> tauri::Result<Submenu<R>> {
    let submenu = Submenu::new(app, share_label(), true)?;
    for (index, service) in services.iter().enumerate() {
        // Full-color, non-template pixels, which is the shape `IconMenuItem` renders
        // correctly (see `open_with.rs` for why SF Symbols are the ones that don't).
        // With `None` it falls back to the text-only renderer, so a failed draw costs
        // the icon and nothing else.
        let icon: Option<Image<'static>> = service
            .icon
            .as_ref()
            .map(|pixels| Image::new_owned(pixels.as_raw().clone(), pixels.width(), pixels.height()));
        let item = IconMenuItem::with_id(app, share_service_id(index), &service.title, true, icon, None::<&str>)?;
        submenu.append(&item)?;
    }
    submenu.append(&PredefinedMenuItem::separator(app)?)?;
    let edit_extensions = MenuItem::with_id(
        app,
        SHARE_EDIT_EXTENSIONS_ID,
        menu_t("menu.context.editExtensions"),
        true,
        None::<&str>,
    )?;
    submenu.append(&edit_extensions)?;
    Ok(submenu)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_id_round_trips_back_to_the_index_it_names() {
        for index in [0_usize, 1, 9, 42] {
            assert_eq!(share_service_index(&share_service_id(index)), Some(index));
        }
    }

    #[test]
    fn only_this_familys_ids_resolve_to_an_index() {
        // `handle_menu_event` walks a chain of prefix matches, so a tail that isn't an
        // index has to answer `None` rather than reach `perform_offered` with a
        // fallback: `open-with:` and `tag-color:` sit in the same ID space.
        for id in [
            "share-service:",
            "share-service:x",
            "share-service:-1",
            "share-service:1 ",
            "share-service:1.0",
            "open-with:com.apple.Preview",
            "tag-color:3",
            "share",
            SHARE_EDIT_EXTENSIONS_ID,
            "",
        ] {
            assert_eq!(share_service_index(id), None, "`{id}` must not resolve to an index");
        }
    }

    #[test]
    fn edit_extensions_sits_outside_the_share_service_id_space() {
        // The two live in one flat ID space that `handle_menu_event` walks by prefix, so
        // the trailing item must never look like a service: it performs no share, and a
        // near-miss would hand `perform_offered` an index the offer doesn't have.
        assert!(!SHARE_EDIT_EXTENSIONS_ID.starts_with(SHARE_SERVICE_ID_PREFIX));
        for index in [0_usize, 1, 42] {
            assert_ne!(share_service_id(index), SHARE_EDIT_EXTENSIONS_ID);
        }
    }

    #[test]
    fn the_extensions_deep_link_is_one_open_system_settings_url_accepts() {
        // `permissions::open_system_settings_url` refuses anything but this scheme, and
        // the refusal is a silent no-op from the menu's side: a typo here would look
        // like a dead menu item rather than an error.
        assert!(EXTENSIONS_SETTINGS_URL.starts_with("x-apple.systempreferences:"));
    }
}
