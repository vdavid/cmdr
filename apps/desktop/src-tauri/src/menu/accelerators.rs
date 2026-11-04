//! Keyboard accelerators on the native bar: the frontend's shortcut strings for the platform this
//! binary runs on, and swapping the accelerator on a live menu item.
//!
//! The conversion itself is `cmdr_menu::accelerators` (pure, shared with the browser demo). The
//! tests here are the ones that need muda: they PARSE what the conversion and `MENU_BAR` emit,
//! because Tauri discards a string muda refuses and builds the item with no key, silently.

use tauri::{
    AppHandle, Runtime,
    menu::{CheckMenuItem, MenuItem},
};

use crate::ignore_poison::IgnorePoison;

use super::menu_spec::{Platform, display_accelerator_label};
use super::{MenuItemEntry, MenuState, TrackedItem};

/// Convert frontend shortcut format (⌘2) to the text a menu item shows on this platform (Cmd+2,
/// Ctrl+2 on Linux), for a POPUP. ❌ The menu bar must not use this:
/// [`frontend_shortcut_to_accelerator`] is its door. `cmdr_menu::accelerators::to_menu_text`.
pub fn frontend_shortcut_to_menu_text(shortcut: &str) -> Option<String> {
    cmdr_menu::accelerators::to_menu_text(shortcut, Platform::current())
}

/// Convert frontend shortcut format (⌘2) to a Tauri accelerator the menu BAR may register on this
/// platform, or `None` behind the modifier floor or for a key muda can't name.
/// `cmdr_menu::accelerators::to_accelerator`.
pub fn frontend_shortcut_to_accelerator(shortcut: &str) -> Option<String> {
    cmdr_menu::accelerators::to_accelerator(shortcut, Platform::current())
}

/// Update the accelerator for any menu item tracked in the items HashMap.
/// Removes the old item, creates a new one with the same ID/label/enabled state
/// but a new accelerator, and reinserts at the same position.
///
/// `display_accelerator` is the combo to SHOW when `new_accelerator` is `None` because the
/// modifier floor refused it. It rebuilds the label from `MenuItemEntry::label` rather than
/// from the live item, so a rebind can't stack a second `(⇧8)` onto a Linux label.
///
/// ❗ On macOS the fresh `NSMenuItem` carries no attributed title, so the caller must re-run
/// `macos_appkit::set_display_accelerators` afterwards or the glyph disappears until the next
/// menu-bar swap. Same contract as the SF Symbols.
pub fn update_menu_item_accelerator<R: Runtime>(
    app: &AppHandle<R>,
    menu_state: &MenuState<R>,
    menu_item_id: &str,
    new_accelerator: Option<&str>,
    display_accelerator: Option<&str>,
) -> tauri::Result<()> {
    let mut items_guard = menu_state.items.lock_ignore_poison();
    let entry = items_guard
        .get(menu_item_id)
        .ok_or_else(|| tauri::Error::InvalidWindowHandle)?;

    let label = entry.label.clone();
    let built_label = match display_accelerator {
        Some(shortcut) => display_accelerator_label(&label, shortcut, Platform::current()),
        None => label.clone(),
    };
    let enabled = entry.item.is_enabled()?;
    let submenu = entry.submenu.clone();
    let position = entry.position;

    // Remove old item, create a replacement of the SAME kind with the new accelerator, reinsert.
    // A check item keeps its check: View > Sort by's bound columns are check items.
    let new_item = match &entry.item {
        TrackedItem::Plain(_) => TrackedItem::Plain(MenuItem::with_id(
            app,
            menu_item_id,
            &built_label,
            enabled,
            new_accelerator,
        )?),
        TrackedItem::Check(old) => TrackedItem::Check(CheckMenuItem::with_id(
            app,
            menu_item_id,
            &built_label,
            enabled,
            old.is_checked()?,
            new_accelerator,
        )?),
    };
    submenu.remove(entry.item.as_menu_item())?;
    submenu.insert(new_item.as_menu_item(), position)?;

    // Update the HashMap entry
    items_guard.insert(
        menu_item_id.to_string(),
        MenuItemEntry {
            item: new_item,
            submenu,
            position,
            label,
        },
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cmdr_menu::accelerators::to_accelerator;

    /// ❗ The test the other ones in this file couldn't be: it PARSES the output instead of
    /// comparing it to a string we made up.
    ///
    /// Everything downstream throws a parse failure away. `tauri::menu::MenuItem::new` is
    /// `accelerator.and_then(|s| s.as_ref().parse().ok())` (`tauri-2.12.1/src/menu/normal.rs:65`),
    /// so a token muda doesn't know builds an item with no key at all, with no error, no panic and
    /// no log line. `Opt` was exactly that: every ⌥ accelerator in the app — Copy path, Show in
    /// Finder, Open terminal here, and any ⌥ combo a user rebound to — silently had none, while a
    /// green `assert_eq!(…, "Cmd+Opt+C")` right here said it was fine.
    ///
    /// muda is a dev-dependency so this parses through the same code Tauri will.
    #[test]
    fn every_accelerator_this_converter_emits_is_one_muda_can_parse() {
        // Every modifier and every key shape the converter has a branch for, plus the shifted
        // characters that have no `Code` and the combos the registry actually ships.
        let combos = [
            "⌘K",
            "⌥⌘O",
            "⌃⌘C",
            "⌘⇧P",
            "⌃⌥⇧⌘A",
            "⌘↑",
            "⌘↓",
            "⌘←",
            "⌘→",
            "⌘[",
            "⌘]",
            "⌘,",
            "⌘=",
            "⌘-",
            "⌘+",
            "⌘*",
            "⌥⇧=",
            "⌥+",
            "⇧8",
            "+",
            "-",
            "*",
            "⌘Backspace",
            "⌘⌥Escape",
            "⌥Enter",
            "⌥Space",
            "⌥Tab",
            "⌥Delete",
            "⌥Insert",
            "⌥PageUp",
            "⌥PageDown",
            "⌥Home",
            "⌥End",
            "⌥F4",
            // The Linux capture form (`formatKeyCombo` off macOS).
            "Ctrl+Shift+P",
            "Ctrl+Alt+Shift+Super+A",
            "Ctrl+↑",
            "Ctrl+Backspace",
            "Alt+F5",
            "Super+E",
            "Ctrl++",
            "Ctrl+=",
            "",
        ];
        for (combo, platform) in combos
            .into_iter()
            .flat_map(|combo| [(combo, Platform::MacOs), (combo, Platform::Linux)])
        {
            // ❗ The contract, and the whole reason this test exists: whatever comes back has to
            // PARSE. `None` is a fine answer — it routes to the display path — but a `Some` that
            // muda refuses is the silent failure, because Tauri discards the parse error and
            // builds the item with no accelerator at all.
            let Some(accelerator) = to_accelerator(combo, platform) else {
                continue;
            };
            assert!(
                accelerator.parse::<muda::accelerator::Accelerator>().is_ok(),
                "`{combo}` converts to `{accelerator}` on {platform:?}, which muda refuses — so Tauri would build \
                 the item with NO accelerator and say nothing"
            );
        }
    }

    /// The other half of the same guard: the menu bar's own hardcoded accelerator strings go
    /// straight to Tauri without passing through the converter, so they need parsing too.
    #[test]
    fn every_accelerator_the_menu_bar_hardcodes_is_one_muda_can_parse() {
        use crate::menu::menu_bar::MENU_BAR;
        use crate::menu::menu_spec::{EntryKind, SubmenuSpec};

        fn check(spec: &'static SubmenuSpec, platform: Platform) {
            for entry in spec.entries_on(platform) {
                let (id, accelerator) = match entry {
                    EntryKind::Item(item) => (item.id, item.accelerator.on(platform)),
                    EntryKind::Check(item) => (item.id, item.accelerator.on(platform)),
                    EntryKind::Submenu(nested) => {
                        check(nested, platform);
                        continue;
                    }
                    _ => continue,
                };
                let Some(accelerator) = accelerator else {
                    continue;
                };
                assert!(
                    accelerator.parse::<muda::accelerator::Accelerator>().is_ok(),
                    "`{id}` on {platform:?} is built with `{accelerator}`, which muda refuses, so \
                     the item comes up with no key and nothing says a word"
                );
            }
        }

        for platform in [Platform::MacOs, Platform::Linux] {
            for bar_menu in MENU_BAR.iter().filter(|bar_menu| bar_menu.is_on(platform)) {
                check(&bar_menu.submenu, platform);
            }
        }
    }

    /// Two items on one chord: the menu fires whichever registered last, and the other one's label
    /// lies. The likeliest source is a Linux bar where a `⌘X` turned into `Ctrl+X` beside an existing
    /// `⌃X`, so this compares what muda parses rather than the strings.
    #[test]
    fn no_two_bar_items_share_an_accelerator() {
        for platform in [Platform::MacOs, Platform::Linux] {
            let mut seen: Vec<(muda::accelerator::Accelerator, &str)> = Vec::new();
            for (id, accelerator) in crate::menu::menu_bar::registered_accelerators(platform) {
                let parsed: muda::accelerator::Accelerator = accelerator
                    .parse()
                    .expect("the bar's accelerators parse (the test above pins it)");
                if let Some((_, taken_by)) = seen.iter().find(|(other, _)| *other == parsed) {
                    panic!("`{id}` and `{taken_by}` both register `{accelerator}` on {platform:?}");
                }
                seen.push((parsed, id));
            }
        }
    }
}
