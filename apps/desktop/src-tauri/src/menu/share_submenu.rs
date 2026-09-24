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
//! macOS answers off the main thread (`context_menu_facts.rs`). A menu that goes up first
//! opens the submenu with a disabled "Finding share options…" line, which
//! [`fill_share_submenu`] replaces while the menu is open.
//!
//! The submenu closes with `Edit extensions`, which opens System Settings' Extensions
//! pane. Since the list only ever holds what macOS currently offers, that item is the
//! one route from Cmdr to the place a missing service is actually turned back on.

use tauri::menu::{MenuItem, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Runtime};

use super::context_menu_live::Placeheld;
use crate::file_system::share::ShareService;
use crate::intl::menu_t;

/// Menu item ID prefix for one offered service. Followed by its index in the offer.
pub const SHARE_SERVICE_ID_PREFIX: &str = "share-service:";

/// The `Share` submenu's own ID, which `context_menu_icons.rs` finds it by to put each
/// service's icon on its item. Outside the `share-service:` family, like the one below.
pub const SHARE_SUBMENU_ID: &str = "share-submenu";

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
/// replaced, and the caller (`file_context_menu.rs`) leaves the whole item out instead.
pub fn build_share_submenu<R: Runtime>(app: &AppHandle<R>, services: &[ShareService]) -> tauri::Result<Submenu<R>> {
    let submenu = Submenu::with_id(app, SHARE_SUBMENU_ID, share_label(), true)?;
    for item in service_items(app, services)? {
        submenu.append(&item)?;
    }
    submenu.append(&PredefinedMenuItem::separator(app)?)?;
    submenu.append(&edit_extensions_item(app)?)?;
    Ok(submenu)
}

/// The `Share` submenu for a menu that went up before macOS said what it offers: a disabled
/// "Finding share options…" line where the services go, then `Edit extensions` as always.
pub fn build_pending_share_submenu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Placeheld<R>> {
    let submenu = Submenu::with_id(app, SHARE_SUBMENU_ID, share_label(), true)?;
    let placeholder = MenuItem::new(app, menu_t("menu.context.shareLoading"), false, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    submenu.append(&placeholder)?;
    submenu.append(&separator)?;
    submenu.append(&edit_extensions_item(app)?)?;
    Ok(Placeheld {
        submenu,
        placeholder,
        separator,
    })
}

/// Swaps the late offer into a pending submenu, which may be open on screen.
///
/// An empty offer can't take the item away any more, the way [`build_share_submenu`]'s
/// caller does: the item sits in the context menu itself, which muda holds borrowed while
/// it's up, and hiding it would move every row below it under the pointer. So the submenu
/// says so instead, with a disabled "No share options" line, and still offers
/// `Edit extensions`, the one place a missing service is turned back on.
pub fn fill_share_submenu<R: Runtime>(
    app: &AppHandle<R>,
    pending: &Placeheld<R>,
    services: &[ShareService],
) -> tauri::Result<()> {
    pending.submenu.remove(&pending.placeholder)?;
    if services.is_empty() {
        let none = MenuItem::new(app, menu_t("menu.context.shareNone"), false, None::<&str>)?;
        return pending.submenu.insert(&none, 0);
    }
    for (position, item) in service_items(app, services)?.iter().enumerate() {
        pending.submenu.insert(item, position)?;
    }
    Ok(())
}

/// One plain item per service. Each service's own icon lands through
/// `context_menu_icons.rs`, straight from the live offer's `NSSharingService`.
fn service_items<R: Runtime>(app: &AppHandle<R>, services: &[ShareService]) -> tauri::Result<Vec<MenuItem<R>>> {
    services
        .iter()
        .enumerate()
        .map(|(index, service)| MenuItem::with_id(app, share_service_id(index), &service.title, true, None::<&str>))
        .collect()
}

fn edit_extensions_item<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<MenuItem<R>> {
    MenuItem::with_id(
        app,
        SHARE_EDIT_EXTENSIONS_ID,
        menu_t("menu.context.editExtensions"),
        true,
        None::<&str>,
    )
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
        // near-miss would hand `perform_offered` an index the offer doesn't have. The
        // submenu's own ID is held to the same rule.
        assert!(!SHARE_EDIT_EXTENSIONS_ID.starts_with(SHARE_SERVICE_ID_PREFIX));
        assert!(!SHARE_SUBMENU_ID.starts_with(SHARE_SERVICE_ID_PREFIX));
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
