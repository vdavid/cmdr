//! The two macOS passes that reach past Tauri into AppKit, after the menu bar is built.
//!
//! `cleanup_macos_menus` strips the items AppKit injects into our Edit menu and registers the Help
//! menu; `set_macos_menu_icons` puts SF Symbols on the items worth spotting at a glance. Neither is
//! expressible through Tauri's menu API, and both have to re-run after every `app.set_menu()`.
//!
//! Both work on `NSMenu` objects, and AppKit offers no way to find one by a Tauri menu ID. So
//! everything OURS is keyed by ID and resolved to a title only at that boundary, through the live
//! Tauri menu, which keeps the match working once the labels are translated. The items AppKit itself
//! injects are the exception, and they're keyed on `NSMenuItem.identifier`. See `DETAILS.md` §
//! "Finding a menu from AppKit".

use std::panic::AssertUnwindSafe;

use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::{
    NSApplication, NSImage, NSMenu, NSMenuItem as NSMenuItemAppKit, NSUserInterfaceItemIdentification,
};
use objc2_foundation::NSString;
use tauri::{
    AppHandle, Runtime,
    menu::{Menu, MenuItemKind, Submenu},
};

use super::{
    APP_MENU_ID, CHANGELOG_ID, CHECK_FOR_UPDATES_ID, CLOSE_OTHER_TABS_ID, CLOSE_TAB_ID, COMMAND_PALETTE_ID,
    COPY_FILENAME_ID, COPY_PATH_ID, DESELECT_ALL_ID, DESELECT_FILES_ID, EDIT_COPY_ID, EDIT_CUT_ID, EDIT_ID,
    EDIT_MENU_ID, EDIT_PASTE_ID, EDIT_PASTE_MOVE_ID, ENTER_LICENSE_KEY_ID, FILE_COMPRESS_ID, FILE_COPY_ID,
    FILE_DELETE_ID, FILE_DELETE_PERMANENTLY_ID, FILE_DUPLICATE_ID, FILE_MENU_ID, FILE_MOVE_ID, FILE_NEW_FOLDER_ID,
    FILE_VIEW_ID, GET_INFO_ID, GO_BACK_ID, GO_FORWARD_ID, GO_HOME_ID, GO_LATEST_DOWNLOAD_ID, GO_MENU_ID, GO_PARENT_ID,
    GO_TO_PATH_ID, HELP_MENU_ID, HELP_SEND_ERROR_REPORT_ID, HELP_WHATS_NEW_ID, NEW_TAB_ID, NEXT_TAB_ID, OPEN_ID,
    OPEN_ONBOARDING_ID, PIN_TAB_MENU_ID, PREV_TAB_ID, QUICK_LOOK_ID, RENAME_ID, SEARCH_FILES_ID, SELECT_ALL_ID,
    SELECT_FILES_ID, SELECT_MENU_ID, SETTINGS_ID, SHOW_IN_FINDER_ID, SORT_ASCENDING_ID, SORT_BY_CREATED_ID,
    SORT_BY_EXTENSION_ID, SORT_BY_MENU_ID, SORT_BY_MODIFIED_ID, SORT_BY_NAME_ID, SORT_BY_SIZE_ID, SORT_DESCENDING_ID,
    SWAP_PANES_ID, SWITCH_PANE_ID, TAB_MENU_ID, VIEW_MENU_ID,
};

pub(crate) fn cleanup_macos_menus<R: Runtime>(app: &AppHandle<R>) {
    // This runs during Tauri's setup() which is inside tao's `did_finish_launching`
    // This is an `extern "C"` callback that aborts on panic. NSMenu operations can raise ObjC
    // exceptions (which are foreign exceptions that `catch_unwind` can't catch), so we
    // use `objc2::exception::catch` to absorb them gracefully.
    let result = objc2::exception::catch(AssertUnwindSafe(|| cleanup_macos_menus_inner(app)));
    if let Err(e) = result {
        log::warn!("Failed to clean up macOS menus: {e:?}");
    }
}

fn cleanup_macos_menus_inner<R: Runtime>(app: &AppHandle<R>) {
    let mtm = MainThreadMarker::new().expect("cleanup_macos_menus_inner must be called from the main thread");
    // Whichever menu bar is installed right now: the main one, or a viewer window's. Both carry
    // the Edit and Help IDs, because AppKit does this to both.
    let Some(menu) = app.menu() else {
        return;
    };

    // macOS only puts the search field in the menu registered here. Tauri resolves the live
    // NSMenu for us, so this side needs no lookup of ours at all.
    if let Some(help_menu) = submenu_by_id(&menu, HELP_MENU_ID)
        && let Err(e) = help_menu.set_as_help_menu_for_nsapp()
    {
        log::warn!(target: "menu", "Failed to register the Help menu with AppKit: {e}");
    }

    let Some(edit_menu) = submenu_by_id(&menu, EDIT_MENU_ID) else {
        log::warn!(target: "menu", "The installed menu bar has no `{EDIT_MENU_ID}` menu, so AppKit's injected items stay");
        return;
    };
    let Ok(edit_title) = edit_menu.text() else {
        return;
    };

    let ns_app = NSApplication::sharedApplication(mtm);
    let Some(main_menu) = ns_app.mainMenu() else {
        return;
    };
    let Some(ns_edit_menu) = find_ns_submenu(&main_menu, &edit_title) else {
        log::warn!(target: "menu", "No AppKit menu titled `{edit_title}`, so its injected items stay");
        return;
    };

    strip_appkit_injected_items(&ns_edit_menu);
}

/// The `NSMenuItem.identifier` of each item AppKit injects into a menu it takes for an Edit menu:
/// Writing Tools, AutoFill, Start Dictation, and Emoji & Symbols, in that order.
///
/// The identifier, not the title. These items are injected after we build the menu, so they carry
/// none of our IDs, and their titles are AppKit's own copy, localized to the SYSTEM language: an
/// English title match finds nothing on a Swedish Mac and every one of them survives. The
/// identifier is AppKit's API identity and doesn't move. Our own items carry AppKit's default (the
/// action selector name): `fireMenuItemAction:` for muda items, `undo:` / `redo:` for the two
/// predefined ones, so none of these collide. The two underscore-prefixed names are private, which
/// is the tradeoff: still a better key than copy that changes by design.
/// (Verified on macOS 26.5.2, reading `identifier` off every Edit item at startup, 2026-08-19.)
const APPKIT_INJECTED_EDIT_ITEM_IDS: &[&str] = &[
    "__NSTextViewContextSubmenuIdentifierWritingTools",
    "_NSMenuItemAutoFillIdentifier",
    "startDictation:",
    "orderFrontCharacterPalette:",
];

/// Removes the items AppKit injects into our Edit menu, plus the separators they leave behind.
///
/// macOS injects several copies of some of them (three "Emoji & Symbols" on 26.5.2), so this
/// removes every match rather than stopping at the first.
fn strip_appkit_injected_items(menu: &NSMenu) {
    // Remove them by walking backwards. We use a manual index instead of a range because each
    // removal shifts indices; the loop must re-check against the live count after every removal.
    let mut j = menu.numberOfItems() - 1;
    while j >= 0 {
        if let Some(item) = menu.itemAtIndex(j) {
            let identifier = item.identifier().map(|id| id.to_string()).unwrap_or_default();
            if APPKIT_INJECTED_EDIT_ITEM_IDS.contains(&identifier.as_str()) {
                menu.removeItemAtIndex(j);
                // Also remove a preceding separator if present
                if j > 0
                    && let Some(prev) = menu.itemAtIndex(j - 1)
                    && prev.isSeparatorItem()
                {
                    menu.removeItemAtIndex(j - 1);
                    j -= 1; // account for the extra removal
                }
            }
        }
        j -= 1;
    }

    // Clean up any trailing separator left at the bottom
    let final_count = menu.numberOfItems();
    if final_count > 0
        && let Some(last) = menu.itemAtIndex(final_count - 1)
        && last.isSeparatorItem()
    {
        menu.removeItemAtIndex(final_count - 1);
    }
}

/// One menu's worth of SF Symbols, keyed by menu item ID.
///
/// Everything here is an ID, never a label: an item's title is user-facing text that translation
/// moves, and an icon that stops matching disappears without a sound.
struct MenuIcons {
    /// ID of the menu these icons belong to.
    menu_id: &'static str,
    /// `(menu item ID, SF Symbol name)` for the items directly inside it.
    items: &'static [(&'static str, &'static str)],
    /// Icons for menus nested one level deeper (View > Sort by).
    nested: &'static [MenuIcons],
}

/// The macOS menu bar's SF Symbols. Items with no entry here show no icon, which is the norm:
/// icons mark the actions worth spotting at a glance, not every line.
const MENU_BAR_ICONS: &[MenuIcons] = &[
    MenuIcons {
        menu_id: APP_MENU_ID,
        items: &[
            (ENTER_LICENSE_KEY_ID, "key"),
            (CHECK_FOR_UPDATES_ID, "arrow.down.circle"),
            (CHANGELOG_ID, "list.bullet.rectangle"),
            (OPEN_ONBOARDING_ID, "sparkles"),
            (SETTINGS_ID, "gearshape"),
        ],
        nested: &[],
    },
    MenuIcons {
        menu_id: FILE_MENU_ID,
        items: &[
            (OPEN_ID, "arrow.up.forward"),
            (FILE_VIEW_ID, "document"),
            (EDIT_ID, "pencil"),
            (FILE_COPY_ID, "document.on.document"),
            (FILE_MOVE_ID, "folder"),
            (FILE_DUPLICATE_ID, "plus.square.on.square"),
            (FILE_COMPRESS_ID, "archivebox"),
            (FILE_NEW_FOLDER_ID, "folder.badge.plus"),
            (FILE_DELETE_ID, "trash"),
            (FILE_DELETE_PERMANENTLY_ID, "trash.slash"),
            (RENAME_ID, "character.cursor.ibeam"),
            (SHOW_IN_FINDER_ID, "arrow.forward.circle"),
            (GET_INFO_ID, "info.circle"),
            (QUICK_LOOK_ID, "eye"),
        ],
        nested: &[],
    },
    MenuIcons {
        menu_id: EDIT_MENU_ID,
        items: &[
            (EDIT_CUT_ID, "scissors"),
            (EDIT_COPY_ID, "document.on.document"),
            (EDIT_PASTE_ID, "clipboard"),
            (EDIT_PASTE_MOVE_ID, "document.on.clipboard"),
            (COPY_PATH_ID, "link"),
            (COPY_FILENAME_ID, "textformat"),
            (SEARCH_FILES_ID, "magnifyingglass"),
        ],
        nested: &[],
    },
    MenuIcons {
        menu_id: SELECT_MENU_ID,
        items: &[
            (SELECT_ALL_ID, "checkmark.circle"),
            (DESELECT_ALL_ID, "circle"),
            (SELECT_FILES_ID, "plus.circle"),
            (DESELECT_FILES_ID, "minus.circle"),
        ],
        nested: &[],
    },
    MenuIcons {
        menu_id: VIEW_MENU_ID,
        items: &[
            (SWITCH_PANE_ID, "rectangle.2.swap"),
            (SWAP_PANES_ID, "arrow.left.arrow.right"),
            (COMMAND_PALETTE_ID, "command"),
        ],
        nested: &[MenuIcons {
            menu_id: SORT_BY_MENU_ID,
            items: &[
                (SORT_BY_NAME_ID, "textformat.alt"),
                (SORT_BY_EXTENSION_ID, "character.textbox"),
                (SORT_BY_MODIFIED_ID, "clock"),
                (SORT_BY_SIZE_ID, "ruler"),
                (SORT_BY_CREATED_ID, "calendar"),
                (SORT_ASCENDING_ID, "chevron.up"),
                (SORT_DESCENDING_ID, "chevron.down"),
            ],
            nested: &[],
        }],
    },
    MenuIcons {
        menu_id: GO_MENU_ID,
        items: &[
            (GO_BACK_ID, "chevron.left"),
            (GO_FORWARD_ID, "chevron.right"),
            (GO_PARENT_ID, "arrow.up"),
            (GO_HOME_ID, "house"),
            (GO_TO_PATH_ID, "arrow.right.to.line"),
            (GO_LATEST_DOWNLOAD_ID, "arrow.down.circle"),
        ],
        nested: &[],
    },
    MenuIcons {
        menu_id: TAB_MENU_ID,
        items: &[
            (NEW_TAB_ID, "plus"),
            (CLOSE_TAB_ID, "xmark"),
            (NEXT_TAB_ID, "arrow.right"),
            (PREV_TAB_ID, "arrow.left"),
            (PIN_TAB_MENU_ID, "pin"),
            (CLOSE_OTHER_TABS_ID, "xmark.circle"),
        ],
        nested: &[],
    },
    MenuIcons {
        menu_id: HELP_MENU_ID,
        items: &[
            (HELP_WHATS_NEW_ID, "sparkles"),
            (HELP_SEND_ERROR_REPORT_ID, "exclamationmark.bubble"),
        ],
        nested: &[],
    },
];

pub(crate) fn set_macos_menu_icons<R: Runtime>(app: &AppHandle<R>) {
    let result = objc2::exception::catch(AssertUnwindSafe(|| set_macos_menu_icons_inner(app)));
    if let Err(e) = result {
        log::warn!("Failed to set macOS menu icons: {e:?}");
    }
}

fn set_macos_menu_icons_inner<R: Runtime>(app: &AppHandle<R>) {
    let mtm = MainThreadMarker::new().expect("set_macos_menu_icons_inner must be called from the main thread");
    let Some(menu) = app.menu() else {
        return;
    };
    let ns_app = NSApplication::sharedApplication(mtm);
    let Some(main_menu) = ns_app.mainMenu() else {
        return;
    };

    for group in MENU_BAR_ICONS {
        let Some(tauri_menu) = submenu_by_id(&menu, group.menu_id) else {
            log::warn!(target: "menu", "The installed menu bar has no `{}` menu, so its icons are missing", group.menu_id);
            continue;
        };
        apply_icon_group(&main_menu, &tauri_menu, group);
    }
}

/// Puts one menu's SF Symbols on its `NSMenu`, then recurses into whatever nests below it.
///
/// AppKit can't be asked for a Tauri ID, so this resolves each ID to the title the Tauri item
/// currently carries and matches on that. The title comes from the same object AppKit drew, which is
/// what keeps this working once the labels are translated.
fn apply_icon_group<R: Runtime>(ns_parent: &NSMenu, tauri_menu: &Submenu<R>, group: &MenuIcons) {
    let menu_id = group.menu_id;
    let Some(ns_menu) = tauri_menu
        .text()
        .ok()
        .and_then(|title| find_ns_submenu(ns_parent, &title))
    else {
        log::warn!(target: "menu", "No AppKit menu for `{menu_id}`, so its icons are missing");
        return;
    };

    for &(item_id, symbol) in group.items {
        let Some(title) = tauri_menu.get(item_id).and_then(|item| menu_item_text(&item)) else {
            log::warn!(target: "menu", "`{menu_id}` holds no `{item_id}`, so its `{symbol}` icon is missing");
            continue;
        };
        let Some(ns_item) = find_ns_item(&ns_menu, &title) else {
            log::warn!(target: "menu", "The AppKit `{menu_id}` menu holds no item titled `{title}`, so its `{symbol}` icon is missing");
            continue;
        };
        set_sf_symbol(&ns_item, symbol);
    }

    for nested in group.nested {
        let Some(child) = tauri_menu
            .get(nested.menu_id)
            .and_then(|item| item.as_submenu().cloned())
        else {
            log::warn!(target: "menu", "`{menu_id}` holds no `{}` submenu, so its icons are missing", nested.menu_id);
            continue;
        };
        apply_icon_group(&ns_menu, &child, nested);
    }
}

/// The submenu with this ID directly inside `menu`, if it has one.
fn submenu_by_id<R: Runtime>(menu: &Menu<R>, id: &str) -> Option<Submenu<R>> {
    menu.get(id)?.as_submenu().cloned()
}

/// The title a menu item currently shows, whichever kind of item it is.
fn menu_item_text<R: Runtime>(item: &MenuItemKind<R>) -> Option<String> {
    match item {
        MenuItemKind::MenuItem(item) => item.text().ok(),
        MenuItemKind::Submenu(item) => item.text().ok(),
        MenuItemKind::Predefined(item) => item.text().ok(),
        MenuItemKind::Check(item) => item.text().ok(),
        MenuItemKind::Icon(item) => item.text().ok(),
    }
}

/// The `NSMenu` hanging off the item in `parent` with this title.
fn find_ns_submenu(parent: &NSMenu, title: &str) -> Option<Retained<NSMenu>> {
    (0..parent.numberOfItems())
        .filter_map(|index| parent.itemAtIndex(index)?.submenu())
        .find(|submenu| submenu.title().to_string() == title)
}

/// The item in `menu` with this title. Separators carry an empty title, so they never match.
fn find_ns_item(menu: &NSMenu, title: &str) -> Option<Retained<NSMenuItemAppKit>> {
    (0..menu.numberOfItems())
        .filter_map(|index| menu.itemAtIndex(index))
        .find(|item| !item.isSeparatorItem() && item.title().to_string() == title)
}

fn set_sf_symbol(item: &NSMenuItemAppKit, symbol_name: &str) {
    let name = NSString::from_str(symbol_name);
    if let Some(image) = NSImage::imageWithSystemSymbolName_accessibilityDescription(&name, None) {
        item.setImage(Some(&image));
    } else {
        log::warn!("SF Symbol not found: {symbol_name}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet};

    /// The main menu bar, plus the shared builders it pulls items from.
    const MAIN_MENU_SOURCES: [(&str, &str); 2] = [
        ("macos.rs", include_str!("macos.rs")),
        ("menu_items.rs", include_str!("menu_items.rs")),
    ];
    /// The viewer's menu bar. It matters too: `cleanup_macos_menus` runs against whichever bar is
    /// currently installed, and macOS swaps this one in whenever a viewer window has focus.
    const VIEWER_MENU_SOURCES: [(&str, &str); 1] = [("menu_structure.rs", include_str!("menu_structure.rs"))];

    /// Every SF Symbol lands on a menu or item the menu bar actually builds.
    ///
    /// The icons are applied through AppKit, which has never heard of a Tauri menu ID: we resolve
    /// each ID to the title it currently carries and match on that. So an ID that names nothing
    /// costs an icon and nothing else, with no crash and no log line anyone reads. Building a real
    /// menu needs AppKit on the main thread, so the source is what we can check here.
    #[test]
    fn menu_icon_ids_are_built_by_the_menu_bar() {
        let built = menu_ids_built_by(&MAIN_MENU_SOURCES);

        fn check(built: &HashSet<String>, group: &MenuIcons) {
            assert!(
                built.contains(group.menu_id),
                "the macOS menu bar builds no menu with id `{}`, so its {} icons never land",
                group.menu_id,
                group.items.len()
            );
            for (item_id, symbol) in group.items {
                assert!(
                    built.contains(*item_id),
                    "the macOS menu bar builds no item with id `{item_id}`, so the `{symbol}` icon never lands"
                );
            }
            for nested in group.nested {
                check(built, nested);
            }
        }

        for group in MENU_BAR_ICONS {
            check(&built, group);
        }
    }

    /// `cleanup_macos_menus` finds the Edit and Help menus by ID, and it runs against both menu
    /// bars. A bar missing either ID silently keeps AppKit's injected Writing Tools / AutoFill /
    /// Dictation items, or loses the Help menu's search field.
    #[test]
    fn both_menu_bars_carry_the_ids_cleanup_needs() {
        for sources in [&MAIN_MENU_SOURCES[..], &VIEWER_MENU_SOURCES[..]] {
            let built = menu_ids_built_by(sources);
            for id in [EDIT_MENU_ID, HELP_MENU_ID] {
                assert!(
                    built.contains(id),
                    "{} builds no menu with id `{id}`, so `cleanup_macos_menus` can't find it",
                    sources[0].0
                );
            }
        }
    }

    /// Menu IDs the given sources construct, as the runtime strings they resolve to.
    ///
    /// Reads the ID constant out of every `with_id` / `with_id_and_items` call and looks it up in
    /// `command_map.rs`, which is where all of them live. One of the sources is this very file, so
    /// spelling a call out in a comment here would have the parser read the comment as code.
    fn menu_ids_built_by(sources: &[(&str, &str)]) -> HashSet<String> {
        let values = menu_id_constants();
        let mut built = HashSet::new();
        for (name, source) in sources {
            for constant in constructed_id_constants(source) {
                let value = values.get(constant.as_str()).unwrap_or_else(|| {
                    panic!("`{constant}`, passed to a menu builder in {name}, is no constant in `command_map.rs`")
                });
                built.insert(value.clone());
            }
        }
        assert!(
            built.len() > 10,
            "only {} menu ids parsed out of {sources:?}; the parser is broken, not the source",
            built.len()
        );
        built
    }

    /// Every `pub const SOMETHING: &str = "…";` in `command_map.rs`, as name → value.
    fn menu_id_constants() -> HashMap<String, String> {
        include_str!("command_map.rs")
            .lines()
            .filter_map(|line| {
                let rest = line.trim().strip_prefix("pub const ")?;
                let (name, rest) = rest.split_once(": &str = \"")?;
                let value = rest.strip_suffix("\";")?;
                Some((name.to_string(), value.to_string()))
            })
            .collect()
    }

    /// Names of the ID constants passed to menu-item and submenu builders in `source`.
    fn constructed_id_constants(source: &str) -> Vec<String> {
        let mut names = Vec::new();
        for (index, _) in source.match_indices("::with_id") {
            let Some(open) = source[index..].find('(') else {
                continue;
            };
            // Every one of these builders takes the app handle first, then the id. The argument
            // list often wraps across lines, so trim rather than expecting a single space.
            let Some(after_app) = source[index + open + 1..].trim_start().strip_prefix("app,") else {
                continue;
            };
            let name: String = after_app
                .trim_start()
                .chars()
                .take_while(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || *c == '_')
                .collect();
            if !name.is_empty() {
                names.push(name);
            }
        }
        names
    }
}
