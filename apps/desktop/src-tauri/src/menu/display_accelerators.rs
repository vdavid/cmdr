//! The macOS pass that draws a shortcut the menu bar can't register.
//!
//! Three Select-menu items run on `*`, `+`, and `-`. None of those carries ⌘ / ⌃ / ⌥, so none can
//! become a real accelerator: AppKit fires one app-wide, ahead of the webview, and the character
//! would stop reaching text fields (`accelerators.rs`'s modifier floor). The file pane's keydown
//! handler owns the keys; this file is how the menu still tells the user about them.
//!
//! AppKit draws key equivalents itself and exposes no way to fake one, so each item gets an
//! attributed title shaped `"{label}\t{glyph}"`, right tab stop and `secondaryLabelColor` on the
//! glyph run. Same column, same dimming, no key registered. Linux can't do this at all and spells
//! the shortcut into the label instead (`menu_spec::display_accelerator_label`).
//!
//! Same AppKit boundary rules as `macos_appkit.rs`, whose lookup helpers this borrows: everything
//! ours is keyed by Tauri ID and resolved to a live title only at the boundary.

use std::collections::HashMap;
use std::panic::AssertUnwindSafe;

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{AnyThread as _, MainThreadMarker};
use objc2_app_kit::{
    NSApplication, NSColor, NSEventModifierFlags, NSFont, NSFontAttributeName, NSForegroundColorAttributeName, NSMenu,
    NSMenuItem as NSMenuItemAppKit, NSMutableParagraphStyle, NSParagraphStyleAttributeName, NSStringDrawing as _,
    NSTextTab, NSTextTabType,
};
use objc2_foundation::{NSArray, NSAttributedString, NSDictionary, NSMutableAttributedString, NSString};
use tauri::{AppHandle, Runtime, menu::Submenu};

use crate::ignore_poison::IgnorePoison as _;

use super::MenuState;
use super::macos_appkit::{find_ns_item, find_ns_submenu, menu_item_text, submenu_by_id};
use super::menu_bar::MENU_BAR;
use super::menu_spec::{EntryKind, ItemSpec, Platform, SubmenuSpec};

/// The space AppKit leaves between a menu's label column and its key-equivalent column, in points.
///
/// The one number in [`tab_stop_for`] that isn't measured, because AppKit exposes no metric for it
/// and a menu's laid-out width isn't known until it opens. Everything around it IS measured, so
/// this stays right as labels are translated and shortcuts rebound.
/// (Measured on macOS 26.0 at the default text size, 2026-09-16: the Select menu's content spans
/// 190 pt for a 92.5 pt widest label and a 34 pt `⇧⌘A`.)
const KEY_EQUIVALENT_COLUMN_GAP: f64 = 63.5;

/// Draws the display-only accelerators (`menu_spec::ItemSpec::display_accelerator`) on the
/// installed menu bar.
///
/// These combos carry no ⌘ / ⌃ / ⌥, so they can't be registered without swallowing the key
/// app-wide (`accelerators.rs`'s modifier floor). AppKit draws a key equivalent itself and
/// gives us no way to fake one, so each item gets an attributed title shaped `"{label}\t{glyph}"`
/// with a right tab stop and `secondaryLabelColor` on the glyph run: same column, same dimming,
/// no key registered.
///
/// ❗ An attributed title survives nothing. `app.set_menu()` builds fresh `NSMenuItem`s, and so
/// does `update_menu_item_accelerator`'s remove/recreate, so this runs from all four places
/// `set_macos_menu_icons` does, for the same reason. Miss one and the glyph silently vanishes.
///
/// Reads `MenuState.display_accelerators` for what a rebind changed, so an app-switch can't put
/// the spec's glyph back over the user's.
pub(crate) fn set_display_accelerators<R: Runtime>(app: &AppHandle<R>, menu_state: &MenuState<R>) {
    let result = objc2::exception::catch(AssertUnwindSafe(|| set_display_accelerators_inner(app, menu_state)));
    if let Err(e) = result {
        log::warn!(target: "menu", "Failed to draw the menu bar's display-only accelerators: {e:?}");
    }
}

fn set_display_accelerators_inner<R: Runtime>(app: &AppHandle<R>, menu_state: &MenuState<R>) {
    let mtm = MainThreadMarker::new().expect("set_display_accelerators_inner must be called from the main thread");
    let Some(menu) = app.menu() else {
        return;
    };
    let ns_app = NSApplication::sharedApplication(mtm);
    let Some(main_menu) = ns_app.mainMenu() else {
        return;
    };
    let overrides = menu_state.display_accelerators.lock_ignore_poison();
    for bar_menu in MENU_BAR.iter().filter(|bar_menu| bar_menu.is_on(Platform::MacOs)) {
        let spec = &bar_menu.submenu;
        if !wants_display_accelerators(spec, &overrides) {
            continue;
        }
        let Some(menu_id) = spec.id.on(Platform::MacOs) else {
            continue;
        };
        let Some(tauri_menu) = submenu_by_id(&menu, menu_id) else {
            log::warn!(target: "menu", "The installed menu bar has no `{menu_id}` menu, so its display accelerators are missing");
            continue;
        };
        apply_display_accelerators(&main_menu, &tauri_menu, spec, &overrides);
    }
}

/// The combo to draw beside an item: whatever the frontend last rebound it to, else the one
/// `MENU_BAR` was built with. A stored `None` means the rebind gave it a REAL accelerator, so
/// AppKit draws that and this side must draw nothing.
fn effective_display_accelerator(overrides: &HashMap<String, Option<String>>, item: &ItemSpec) -> Option<String> {
    match overrides.get(item.id) {
        Some(rebound) => rebound.clone(),
        None => item.display_accelerator.on(Platform::MacOs).map(str::to_string),
    }
}

/// Whether anything in this subtree wants a glyph right now. Checked before any AppKit lookup,
/// so an untouched menu (every one but Select, today) costs a spec walk and nothing else.
fn wants_display_accelerators(spec: &SubmenuSpec, overrides: &HashMap<String, Option<String>>) -> bool {
    spec.entries_on(Platform::MacOs).any(|entry| match entry {
        EntryKind::Item(item) => effective_display_accelerator(overrides, item).is_some(),
        EntryKind::Submenu(nested) => wants_display_accelerators(nested, overrides),
        _ => false,
    })
}

/// Puts one menu's glyphs on its `NSMenu`, then recurses into whatever nests below it.
///
/// Resolves IDs to live titles at the AppKit boundary, exactly as `apply_icon_group` does and
/// for the same reason: AppKit has never heard of a Tauri menu ID, and a title match keeps
/// working once the labels are translated.
fn apply_display_accelerators<R: Runtime>(
    ns_parent: &NSMenu,
    tauri_menu: &Submenu<R>,
    spec: &SubmenuSpec,
    overrides: &HashMap<String, Option<String>>,
) {
    let Some(ns_menu) = tauri_menu
        .text()
        .ok()
        .and_then(|title| find_ns_submenu(ns_parent, &title))
    else {
        log::warn!(target: "menu", "No AppKit menu for a bar menu that wants display accelerators, so they're missing");
        return;
    };

    let rows: Vec<(String, String)> = spec
        .entries_on(Platform::MacOs)
        .filter_map(|entry| match entry {
            EntryKind::Item(item) => {
                let shortcut = effective_display_accelerator(overrides, item)?;
                let title = tauri_menu.get(item.id).and_then(|built| menu_item_text(&built));
                match title {
                    Some(title) => Some((title, shortcut)),
                    None => {
                        log::warn!(target: "menu", "The bar holds no `{}`, so its `{shortcut}` display accelerator is missing", item.id);
                        None
                    }
                }
            }
            _ => None,
        })
        .collect();

    if !rows.is_empty() {
        let tab_location = tab_stop_for(&ns_menu, tauri_menu, &rows);
        for (title, shortcut) in &rows {
            let Some(ns_item) = find_ns_item(&ns_menu, title) else {
                log::warn!(target: "menu", "The AppKit menu holds no item titled `{title}`, so its `{shortcut}` display accelerator is missing");
                continue;
            };
            set_display_accelerator(&ns_item, title, shortcut, tab_location);
        }
    }

    for entry in spec.entries_on(Platform::MacOs) {
        let EntryKind::Submenu(nested) = entry else {
            continue;
        };
        if !wants_display_accelerators(nested, overrides) {
            continue;
        }
        let Some(nested_id) = nested.id.on(Platform::MacOs) else {
            log::warn!(target: "menu", "A nested submenu wants display accelerators but is built with no ID, so they can't be found");
            continue;
        };
        let Some(child) = tauri_menu.get(nested_id).and_then(|built| built.as_submenu().cloned()) else {
            log::warn!(target: "menu", "The bar holds no `{nested_id}` submenu, so its display accelerators are missing");
            continue;
        };
        apply_display_accelerators(&ns_menu, &child, nested, overrides);
    }
}

/// Where to put the right tab stop for one menu, so a drawn glyph lands in the same column as the
/// real key equivalents beside it.
///
/// AppKit lays a menu out as `widest label + gap + widest key equivalent`, right-aligning the key
/// equivalents against the content's right edge. Reproducing that means measuring all three:
/// nothing here can be a fixed column, because a translated label can be half again as long as the
/// English one and a shortcut the user rebound can be any width.
///
/// The one number that can't be measured is AppKit's gap, which isn't exposed anywhere. See
/// [`KEY_EQUIVALENT_COLUMN_GAP`].
fn tab_stop_for<R: Runtime>(ns_menu: &NSMenu, tauri_menu: &Submenu<R>, rows: &[(String, String)]) -> f64 {
    let font = NSFont::menuFontOfSize(0.0);
    // SAFETY: AppKit's own attribute-name constant, an immortal static read through the
    // bindings' declared type.
    let keys: [&NSString; 1] = unsafe { [NSFontAttributeName] };
    let values: [&AnyObject; 1] = [font.as_ref()];
    let attributes: Retained<NSDictionary<NSString, AnyObject>> = NSDictionary::from_slices(&keys, &values);
    let width = |text: &str| {
        // SAFETY: the dictionary maps one real attribute key to the class it requires (`NSFont`).
        unsafe { NSString::from_str(text).sizeWithAttributes(Some(&attributes)) }.width
    };
    // muda keeps its own copy of each item's text, so these are the plain labels even where
    // this pass has already rewritten the `NSMenuItem`'s title.
    let widest_label = tauri_menu
        .items()
        .unwrap_or_default()
        .iter()
        .filter_map(menu_item_text)
        .map(|text| width(&text))
        .fold(0.0, f64::max);
    // The right column holds both kinds at once: AppKit's own key equivalents and ours.
    let widest_key_equivalent = (0..ns_menu.numberOfItems())
        .filter_map(|index| ns_menu.itemAtIndex(index))
        .filter_map(|item| key_equivalent_display(&item))
        .chain(rows.iter().map(|(_, shortcut)| shortcut.clone()))
        .map(|text| width(&text))
        .fold(0.0, f64::max);
    widest_label + KEY_EQUIVALENT_COLUMN_GAP + widest_key_equivalent
}

/// What AppKit would DRAW for an item's own key equivalent, or `None` for an item with none.
///
/// Only its width is ever used, so the modifier order is Apple's display order and nothing else
/// about the string needs to be exact.
fn key_equivalent_display(item: &NSMenuItemAppKit) -> Option<String> {
    let key = item.keyEquivalent().to_string();
    if key.is_empty() {
        return None;
    }
    let modifiers = item.keyEquivalentModifierMask();
    let mut shown = String::new();
    for (flag, glyph) in [
        (NSEventModifierFlags::Control, '⌃'),
        (NSEventModifierFlags::Option, '⌥'),
        (NSEventModifierFlags::Shift, '⇧'),
        (NSEventModifierFlags::Command, '⌘'),
    ] {
        if modifiers.contains(flag) {
            shown.push(glyph);
        }
    }
    shown.push_str(&key.to_uppercase());
    Some(shown)
}

/// Draws `shortcut` right-aligned and dimmed at the end of the item's title.
///
/// The font attribute is load-bearing, not decoration: an attributed title with no
/// `NSFontAttributeName` falls back to the system default face rather than the menu font, so the
/// row would render in a different typeface from every row around it.
fn set_display_accelerator(item: &NSMenuItemAppKit, label: &str, shortcut: &str, tab_location: f64) {
    let font = NSFont::menuFontOfSize(0.0);
    let paragraph = NSMutableParagraphStyle::new();
    let tab = NSTextTab::initWithType_location(NSTextTab::alloc(), NSTextTabType::RightTabStopType, tab_location);
    paragraph.setTabStops(Some(&NSArray::from_slice(&[tab.as_ref()])));

    // SAFETY: all three are AppKit's own attribute-name constants, immortal statics read
    // through the bindings' declared type.
    let (font_key, color_key, paragraph_key) = unsafe {
        (
            NSFontAttributeName,
            NSForegroundColorAttributeName,
            NSParagraphStyleAttributeName,
        )
    };
    let label_keys: [&NSString; 2] = [font_key, paragraph_key];
    let label_values: [&AnyObject; 2] = [font.as_ref(), paragraph.as_ref()];
    let label_attributes = NSDictionary::from_slices(&label_keys, &label_values);

    let dim = NSColor::secondaryLabelColor();
    let shortcut_keys: [&NSString; 3] = [font_key, paragraph_key, color_key];
    let shortcut_values: [&AnyObject; 3] = [font.as_ref(), paragraph.as_ref(), dim.as_ref()];
    let shortcut_attributes = NSDictionary::from_slices(&shortcut_keys, &shortcut_values);

    // Two runs rather than one string plus a range: the dimming applies to exactly the glyph,
    // and no UTF-16 offset arithmetic can drift.
    // SAFETY: `initWithString_attributes:` is unsafe only because it takes an uninitialized
    // allocation and an untyped attribute dictionary. The allocation is this call's own, and
    // `label_attributes` maps two real `NSAttributedStringKey`s to objects of the classes those
    // keys require (`NSFont`, `NSParagraphStyle`).
    let title = unsafe {
        NSMutableAttributedString::initWithString_attributes(
            NSMutableAttributedString::alloc(),
            &NSString::from_str(&format!("{label}\t")),
            Some(&label_attributes),
        )
    };
    // SAFETY: as above, with this call's own allocation and `shortcut_attributes`, which maps
    // three real `NSAttributedStringKey`s to an `NSFont`, an `NSParagraphStyle`, and an `NSColor`.
    let glyph = unsafe {
        NSAttributedString::initWithString_attributes(
            NSAttributedString::alloc(),
            &NSString::from_str(shortcut),
            Some(&shortcut_attributes),
        )
    };
    title.appendAttributedString(&glyph);
    item.setAttributedTitle(Some(&title));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// This pass walks `MENU_BAR` down through Tauri submenu IDs, so a menu holding a
    /// display-only accelerator has to HAVE one on macOS. A menu built without one can't be
    /// found, and its glyphs would go missing with nothing but a log line to say so.
    #[test]
    fn every_display_accelerator_sits_in_a_menu_the_pass_can_find() {
        fn check(spec: &'static SubmenuSpec, path: &str) {
            let shortcuts: Vec<&str> = spec
                .entries_on(Platform::MacOs)
                .filter_map(|entry| match entry {
                    EntryKind::Item(item) => item.display_accelerator.on(Platform::MacOs),
                    _ => None,
                })
                .collect();
            assert!(
                shortcuts.is_empty() || spec.id.on(Platform::MacOs).is_some(),
                "the `{path}` menu is built with no macOS ID, so `set_display_accelerators` can't \
                 find it and its {shortcuts:?} would never be drawn"
            );
            for entry in spec.entries_on(Platform::MacOs) {
                if let EntryKind::Submenu(nested) = entry {
                    check(nested, path);
                }
            }
        }

        for bar_menu in MENU_BAR.iter().filter(|bar_menu| bar_menu.is_on(Platform::MacOs)) {
            let path = bar_menu.submenu.id.on(Platform::MacOs).unwrap_or("(unnamed)");
            check(&bar_menu.submenu, path);
        }
    }
}
