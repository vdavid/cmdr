//! SF Symbols on right-click menu items (macOS).
//!
//! The menu BAR gets its icons at build time (`macos_appkit.rs`'s `MENU_BAR_ICONS`),
//! because Tauri hands out the installed bar's `NSMenu`. A context menu's it does not
//! (muda's `ns_menu()` sits behind Tauri's sealed `ContextMenuBase`), so this reaches
//! the items through `NSMenuDidBeginTrackingNotification`, exactly as
//! `services_context.rs` does and for exactly the same reason. AppKit posts it before
//! the menu is laid out, which is why an image set here still gets its gutter.
//!
//! ## Why not `IconMenuItem`, which needs no AppKit at all
//!
//! Because it can't render a template image. muda turns the RGBA it's given into a PNG
//! and hands `NSImage` that, never calling `setTemplate:` — so the bitmap draws as
//! literal pixels. A monochrome glyph then stays whatever colour it was baked in: it
//! disappears in the mode it wasn't baked for, and it stays dark on the accent-coloured
//! fill of a highlighted row while the label beside it turns white. `NSMenuItem`'s own
//! `setImage:` with a real symbol image is the whole fix: AppKit tints it for light,
//! dark, and highlight, and it tracks the menu's font size.
//!
//! `IconMenuItem` stays right for the icons that ARE pixels — app icons in "Open with",
//! `NSSharingService` icons in `Share`, the tag colour circles — which is why those
//! don't come through here.

use std::cell::{Cell, RefCell};

use objc2::MainThreadMarker;
use objc2_app_kit::NSMenu;
use objc2_foundation::NSNotification;
use tauri::Runtime;
use tauri::menu::Menu;

use super::macos_appkit::{find_ns_item, menu_item_text, observe_menu_tracking, set_sf_symbol, tracking_menu};
use super::{DRIVE_COPY_LINK_ID, DRIVE_OPEN_ID};

/// `(menu item ID, SF Symbol name)` for the file context menu.
///
/// Everything here is an ID, never a label: a title is user-facing text that
/// translation moves, and an icon that stops matching disappears without a sound.
/// Items with no entry show no icon, which is the norm — icons mark the actions worth
/// spotting at a glance, not every line.
///
/// The two Drive items are the whole list today. `link` is the same symbol the menu
/// bar's `Copy path` carries, on purpose: the same concept gets the same glyph, which is
/// already how `Copy` shares `document.on.document` across two menus.
const FILE_CONTEXT_ICONS: &[(&str, &str)] = &[(DRIVE_OPEN_ID, "arrow.up.forward.app"), (DRIVE_COPY_LINK_ID, "link")];

/// Icons armed for the next context menu to open.
///
/// A title rather than an ID, because AppKit has never heard of a Tauri menu ID: the
/// title is read off the live Tauri item at arm time, which is the house rule for
/// crossing that boundary and what keeps the match working once labels are translated.
pub struct IconLoan(MainThreadMarker);

impl Drop for IconLoan {
    fn drop(&mut self) {
        ARMED.with(|slot| slot.borrow_mut().clear());
    }
}

thread_local! {
    /// `(title, symbol)` for the menu currently going up. Main-thread-only by
    /// construction: every reader runs from the menu thread.
    static ARMED: RefCell<Vec<(String, &'static str)>> = const { RefCell::new(Vec::new()) };
    /// Whether the tracking observer is registered. Registering it lazily keeps the
    /// cost with the first right-click instead of every launch.
    static OBSERVING: Cell<bool> = const { Cell::new(false) };
}

/// Arms [`FILE_CONTEXT_ICONS`] for `menu`, which must be about to `popup()`.
///
/// ❗ Hold the returned guard until `popup()` returns, the way `ServicesLoan` is held:
/// `popup()` runs AppKit's tracking loop, so the menu only starts tracking inside it.
/// Dropping the guard early would disarm before the icons ever landed.
///
/// Answers `None` when there is nothing to do — off the main thread, or the menu built
/// none of the items with icons (a pane whose rows aren't Drive items, which is most of
/// them).
pub fn lend_context_menu_icons<R: Runtime>(menu: &Menu<R>) -> Option<IconLoan> {
    let Some(mtm) = MainThreadMarker::new() else {
        log::warn!(target: "menu", "Not on the main thread; the context menu's items show no icons");
        return None;
    };
    let armed: Vec<(String, &'static str)> = FILE_CONTEXT_ICONS
        .iter()
        .filter_map(|&(id, symbol)| Some((menu_item_text(&menu.get(id)?)?, symbol)))
        .collect();
    if armed.is_empty() {
        return None;
    }
    ensure_observing(mtm);
    ARMED.with(|slot| *slot.borrow_mut() = armed);
    Some(IconLoan(mtm))
}

/// Registers the tracking observer, once per process.
fn ensure_observing(mtm: MainThreadMarker) {
    if OBSERVING.replace(true) {
        return;
    }
    observe_menu_tracking(mtm, "put SF Symbols on the right-click menu", |mtm, note| {
        on_menu_did_begin_tracking(mtm, note);
    });
}

/// Puts the armed symbols on the tracking menu's items.
///
/// Fires for every menu the app tracks (the menu bar included), so it does nothing
/// unless icons are armed and the tracking menu carries the titles they name. A title
/// that isn't there costs an icon and nothing else, which is the same bargain the menu
/// bar's pass makes.
fn on_menu_did_begin_tracking(mtm: MainThreadMarker, notification: &NSNotification) {
    let armed = ARMED.with(|slot| slot.borrow().clone());
    if armed.is_empty() {
        return;
    }
    let Some(menu) = tracking_menu(mtm, notification) else {
        return;
    };
    apply(&menu, &armed);
}

/// Sets each armed symbol on the item carrying its title, if this menu has one.
fn apply(menu: &NSMenu, armed: &[(String, &'static str)]) {
    for (title, symbol) in armed {
        let Some(item) = find_ns_item(menu, title) else {
            continue;
        };
        set_sf_symbol(&item, symbol);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// Every ID here is one the file context menu actually builds.
    ///
    /// The icons land through AppKit, which resolves a Tauri ID to the title it
    /// currently carries and matches on that — so a stale ID costs an icon with no
    /// crash and no log line anyone reads. Building a real menu needs AppKit on the main
    /// thread, so the source is what we can check here.
    #[test]
    fn every_icon_names_an_item_the_context_menu_builds() {
        let source = include_str!("menu_structure.rs");
        let ids: HashSet<&str> = FILE_CONTEXT_ICONS.iter().map(|&(id, _)| id).collect();
        for id in ids {
            // The constant's NAME, since that's what the builder call spells.
            let name = constant_named(id).expect("every context-menu icon id is a `command_map.rs` constant");
            assert!(
                source.contains(&name),
                "`menu_structure.rs` builds no item with `{name}`, so its icon never lands"
            );
        }
    }

    /// A symbol name is a string AppKit looks up at runtime, so a typo is silent. Pin
    /// the two we ship so a rename has to be deliberate.
    #[test]
    fn the_symbols_are_the_ones_we_chose() {
        assert_eq!(
            FILE_CONTEXT_ICONS,
            &[(DRIVE_OPEN_ID, "arrow.up.forward.app"), (DRIVE_COPY_LINK_ID, "link"),]
        );
    }

    /// The `command_map.rs` constant whose value is `value`.
    fn constant_named(value: &str) -> Option<String> {
        include_str!("command_map.rs").lines().find_map(|line| {
            let rest = line.trim().strip_prefix("pub const ")?;
            let (name, rest) = rest.split_once(": &str = \"")?;
            (rest.strip_suffix("\";")? == value).then(|| name.to_string())
        })
    }
}
