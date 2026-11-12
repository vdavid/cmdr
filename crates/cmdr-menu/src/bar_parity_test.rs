//! Ties the bar to the frontend's registry: on each platform, every row with a command registers
//! and shows what a rebind to that command's first default key would (`rebound_row_key`, the same
//! door `update_menu_accelerator` takes), or sits on [`EXCEPTIONS`] with its reason. And no row
//! registers a key the page owns (`page_owned_keys.tsv`), past [`PAGE_OWNED_BY_DESIGN`].
//!
//! The registry side is `registry_default_keys.tsv`, which the frontend's
//! `lib/commands/default-keymap.test.ts` writes under each OS's capability fixture and checks
//! for staleness, so a key moved on either side fails one of the two tests.

use std::collections::HashMap;

use super::{RowKey, every_entry, rebound_row_key, registered_accelerators};
use crate::accelerators::{same_chord, to_menu_text};
use crate::ids::{
    EDIT_COPY_ID, EDIT_PASTE_MOVE_ID, FILE_COPY_ID, FILE_MOVE_ID, GO_BACK_ID, GO_PARENT_ID, QUICK_LOOK_ID,
    SELECT_ALL_ID, SELECT_SAME_KIND_ID, SWITCH_PANE_ID, VIEW_MODE_BRIEF_LEFT_ID, VIEW_MODE_FULL_LEFT_ID,
    VIEW_ZOOM_IN_ID, menu_id_to_command,
};
use crate::spec::{EntryKind, Platform};

const REGISTRY_DEFAULT_KEYS: &str = include_str!("registry_default_keys.tsv");

/// A row whose accelerator differs from the registry's first key on purpose.
struct Exception {
    platforms: &'static [Platform],
    menu_id: &'static str,
    reason: &'static str,
}

const MACOS: &[Platform] = &[Platform::MacOs];
const WINDOWS: &[Platform] = &[Platform::Windows];
const WINDOWS_AND_LINUX: &[Platform] = &[Platform::Windows, Platform::Linux];
const EVERY_PLATFORM: &[Platform] = &Platform::ALL;

/// The Windows bar registers the Linux bar's key where the two keymaps differ only in order, so its
/// F-key chords stay with keydown dispatch, as Linux's do.
const WINDOWS_FOLLOWS_LINUX: &str = "the first key there is an F-key chord, which the bar leaves to keydown dispatch; the Windows \
                                     bar registers Linux's first key (a default on Windows too) until its menu model is decided";

/// The registry's first key can't be registered (bare, or a key muda has no `Code` for), so the
/// bar registers the command's second default.
const SECOND_KEY: &str = "the first default can't be a bar accelerator, so the bar registers the second";

const EXCEPTIONS: &[Exception] = &[
    // `⌘+` has no muda `Code`; `⌘=` is the physical key under it.
    Exception {
        platforms: MACOS,
        menu_id: VIEW_ZOOM_IN_ID,
        reason: SECOND_KEY,
    },
    // `Backspace` is bare. macOS registers the second key, `⌘↑`; Windows and Linux show theirs,
    // `Alt+↑`, which the operation queue window (wearing this bar) uses for Move earlier.
    Exception {
        platforms: EVERY_PLATFORM,
        menu_id: GO_PARENT_ID,
        reason: SECOND_KEY,
    },
    // Windows's first view-mode keys are TC's `Ctrl+F2` / `Ctrl+F1`; Linux puts `Ctrl+1` / `Ctrl+2`
    // first, because KDE and Xfce take `Ctrl+F1` / `Ctrl+F2`.
    Exception {
        platforms: WINDOWS,
        menu_id: VIEW_MODE_FULL_LEFT_ID,
        reason: WINDOWS_FOLLOWS_LINUX,
    },
    Exception {
        platforms: WINDOWS,
        menu_id: VIEW_MODE_BRIEF_LEFT_ID,
        reason: WINDOWS_FOLLOWS_LINUX,
    },
];

/// Rows whose built accelerator is a key the page owns, on purpose.
const PAGE_OWNED_BY_DESIGN: &[(&[Platform], &str, &str)] = &[
    (
        EVERY_PLATFORM,
        EDIT_COPY_ID,
        "Copy routes through `execute-command`, and the frontend copies the selected text when a field or a text \
         selection has it (`handleTextRegionShortcut`), so the key stays the field's",
    ),
    (
        WINDOWS_AND_LINUX,
        SELECT_ALL_ID,
        "the operation queue window's Select all reaches it through this row, by design \
         (`menu_handlers.rs`, `QueueSelectAll`)",
    ),
];

/// The command a row stands for, as the registry names it. The left pane's Full / Brief checks
/// carry the view-mode keys at build time (the active pane's pair does, and the build starts with
/// the left pane active); the right pane's get them only when focus moves, so they map to nothing.
fn row_command(menu_id: &str) -> Option<&'static str> {
    match menu_id {
        VIEW_MODE_FULL_LEFT_ID => Some("view.fullMode"),
        VIEW_MODE_BRIEF_LEFT_ID => Some("view.briefMode"),
        _ => menu_id_to_command(menu_id).map(|(command_id, _)| command_id),
    }
}

/// `(platform, command id)` → the first default key, empty for a command with no key.
fn registry_keys() -> HashMap<(&'static str, &'static str), &'static str> {
    REGISTRY_DEFAULT_KEYS
        .lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
        .map(|line| {
            let mut fields = line.split('\t');
            let (Some(platform), Some(command_id), Some(combo), None) =
                (fields.next(), fields.next(), fields.next(), fields.next())
            else {
                panic!("`registry_default_keys.tsv` has a malformed line: `{line}`");
            };
            ((platform, command_id), combo)
        })
        .collect()
}

/// What the row registers and shows against what the registry's first key would, as chords, or
/// `None` where they agree.
fn mismatch(built: &RowKey, expected: &RowKey, platform: Platform) -> Option<String> {
    let same = |a: &Option<String>, b: &Option<String>| match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => same_chord(a, b),
        _ => false,
    };
    // A shown key is the frontend's spelling; compare it as menu text, so `⌥⇧=` meets `Alt+Shift+=`.
    let shown = |key: &RowKey| {
        key.display
            .as_deref()
            .and_then(|shortcut| to_menu_text(shortcut, platform))
    };
    if same(&built.accelerator, &expected.accelerator) && same(&shown(built), &shown(expected)) {
        return None;
    }
    let describe = |key: &RowKey| match (&key.accelerator, &key.display) {
        (Some(accelerator), _) => format!("registers `{accelerator}`"),
        (None, Some(display)) => format!("shows `{display}`"),
        (None, None) => "has no key".to_string(),
    };
    Some(format!(
        "the bar {}, the registry's first default key {}",
        describe(built),
        describe(expected),
    ))
}

/// Every row with a command on `platform`, with what it registers and shows, and what the registry
/// implies.
fn rows(platform: Platform) -> Vec<(&'static str, RowKey, RowKey)> {
    let keys = registry_keys();
    every_entry(platform)
        .into_iter()
        .filter_map(|entry| {
            let (menu_id, accelerator, display) = match entry {
                EntryKind::Item(item) => (
                    item.id,
                    item.accelerator.on(platform),
                    item.display_accelerator.on(platform),
                ),
                EntryKind::Check(check) => (check.id, check.accelerator.on(platform), None),
                _ => return None,
            };
            let command_id = row_command(menu_id)?;
            let combo = keys.get(&(platform.wire(), command_id)).copied().unwrap_or("");
            let built = RowKey {
                accelerator: accelerator.map(str::to_string),
                display: display.map(str::to_string),
            };
            Some((menu_id, built, rebound_row_key(menu_id, combo, platform)))
        })
        .collect()
}

fn exception(platform: Platform, menu_id: &str) -> Option<&'static Exception> {
    EXCEPTIONS
        .iter()
        .find(|exception| exception.menu_id == menu_id && exception.platforms.contains(&platform))
}

#[test]
fn every_bar_row_registers_its_commands_first_default_key() {
    let mut drift = Vec::new();
    for platform in Platform::ALL {
        for (menu_id, built, expected) in rows(platform) {
            if exception(platform, menu_id).is_some() {
                continue;
            }
            if let Some(why) = mismatch(&built, &expected, platform) {
                drift.push(format!("`{menu_id}` on {platform:?}: {why}"));
            }
        }
    }
    assert!(
        drift.is_empty(),
        "menu bar rows that disagree with the registry (fix the row in `bar.rs`, or list it in \
         `EXCEPTIONS` with its reason):\n  {}",
        drift.join("\n  ")
    );
}

/// An exception that no longer differs, or names a row the bar doesn't build there, is a stale
/// excuse: it would hide the next real drift on that row.
#[test]
fn every_exception_still_differs() {
    for exception in EXCEPTIONS {
        assert!(!exception.reason.is_empty(), "`{}` needs a reason", exception.menu_id);
        for &platform in exception.platforms {
            let row = rows(platform)
                .into_iter()
                .find(|(menu_id, ..)| *menu_id == exception.menu_id)
                .unwrap_or_else(|| {
                    panic!(
                        "`{}` is excused on {platform:?}, where the bar has no such row with a command",
                        exception.menu_id
                    )
                });
            let (_, built, expected) = row;
            assert!(
                mismatch(&built, &expected, platform).is_some(),
                "`{}` is excused on {platform:?} but agrees with the registry there: drop it from `EXCEPTIONS`",
                exception.menu_id
            );
        }
    }
}

/// The page-owned key a built accelerator (Tauri spelling) lands on, with why the page owns it.
fn takes_page_key(accelerator: &str, platform: Platform) -> Option<(&'static str, &'static str)> {
    super::page_owned_keys(platform)
        .find(|(combo, _)| to_menu_text(combo, platform).is_some_and(|owned| same_chord(&owned, accelerator)))
}

/// A bar accelerator fires before the page, so a row registering a key the page owns takes it from
/// every focused text field, or from a window that wears the bar.
#[test]
fn no_bar_row_registers_a_key_the_page_owns() {
    let mut taken = Vec::new();
    for platform in Platform::ALL {
        for (menu_id, accelerator) in registered_accelerators(platform) {
            let by_design = PAGE_OWNED_BY_DESIGN
                .iter()
                .any(|(platforms, id, _)| *id == menu_id && platforms.contains(&platform));
            if by_design {
                continue;
            }
            if let Some((combo, why)) = takes_page_key(accelerator, platform) {
                taken.push(format!("`{menu_id}` on {platform:?} registers `{combo}` ({why})"));
            }
        }
    }
    assert!(
        taken.is_empty(),
        "bar rows that take a key from the page (show it instead, or list the row in `PAGE_OWNED_BY_DESIGN` \
         with its reason):\n  {}",
        taken.join("\n  ")
    );
}

/// A by-design entry that no longer registers a page-owned key would excuse the next one unseen.
#[test]
fn every_page_owned_by_design_row_still_takes_its_key() {
    for (platforms, menu_id, reason) in PAGE_OWNED_BY_DESIGN {
        assert!(!reason.is_empty(), "`{menu_id}` needs a reason");
        for &platform in *platforms {
            let accelerator = registered_accelerators(platform)
                .into_iter()
                .find(|(id, _)| id == menu_id)
                .map(|(_, accelerator)| accelerator);
            assert!(
                accelerator.is_some_and(|accelerator| takes_page_key(accelerator, platform).is_some()),
                "`{menu_id}` is excused on {platform:?} but registers no page-owned key there: drop it"
            );
        }
    }
}

/// The file covers every platform, so a frontend change that dropped one can't pass as "no key".
#[test]
fn the_registry_file_names_every_platform() {
    let keys = registry_keys();
    for platform in Platform::ALL {
        assert!(
            keys.keys().any(|(on, _)| *on == platform.wire()),
            "`registry_default_keys.tsv` has no `{}` line",
            platform.wire()
        );
    }
}

fn registered(accelerator: &str) -> RowKey {
    RowKey {
        accelerator: Some(accelerator.to_string()),
        display: None,
    }
}

fn shown(shortcut: &str) -> RowKey {
    RowKey {
        accelerator: None,
        display: Some(shortcut.to_string()),
    }
}

/// A rebind back to the key macOS's bar registers bare (or a reset to it) registers it the way the
/// bar does, where the modifier floor used to leave it display-only.
#[test]
fn a_rebind_back_to_a_rows_own_bare_key_registers_it_as_the_bar_does() {
    let mac = Platform::MacOs;
    assert_eq!(rebound_row_key(FILE_COPY_ID, "F5", mac), registered("F5"));
    assert_eq!(rebound_row_key(SWITCH_PANE_ID, "Tab", mac), registered("Tab"));
    assert_eq!(rebound_row_key(QUICK_LOOK_ID, "⇧Space", mac), registered("Shift+Space"));
}

/// The floor still guards every other bare key: one that isn't this row's built key only shows.
#[test]
fn a_bare_key_the_row_was_not_built_with_stays_display_only() {
    let mac = Platform::MacOs;
    assert_eq!(rebound_row_key(FILE_MOVE_ID, "F5", mac), shown("F5"));
    assert_eq!(rebound_row_key(FILE_COPY_ID, "F9", mac), shown("F9"));
    // Off macOS the bar leaves F5 to keydown dispatch, so a rebind to it only shows there too.
    assert_eq!(rebound_row_key(FILE_COPY_ID, "F5", Platform::Linux), shown("F5"));
}

/// Select same kind is built display-only on purpose (`⌥+` stays free for typing), so a push of
/// that key keeps it display-only, on every platform's spelling of it.
#[test]
fn a_push_of_a_display_only_rows_own_key_stays_display_only() {
    assert_eq!(
        rebound_row_key(SELECT_SAME_KIND_ID, "⌥⇧=", Platform::MacOs),
        shown("⌥⇧=")
    );
    assert_eq!(
        rebound_row_key(SELECT_SAME_KIND_ID, "Alt+Shift+=", Platform::Linux),
        shown("Alt+Shift+=")
    );
    // A different ⌥ chord is a deliberate rebind, and gets a real accelerator.
    assert_eq!(
        rebound_row_key(SELECT_SAME_KIND_ID, "⌥⇧A", Platform::MacOs),
        registered("Alt+Shift+A")
    );
}

/// A key the page owns only shows, wherever it's pushed: a native text key would fire the row from
/// every focused field, and a bar-wearing window's key would never reach that window.
#[test]
fn a_push_of_a_key_the_page_owns_stays_display_only() {
    // macOS: `⌥←` is word-left in a field (a shadowed `⌘[` hands Go back its last key).
    assert_eq!(rebound_row_key(GO_BACK_ID, "⌥←", Platform::MacOs), shown("⌥←"));
    // Linux: paste as plain text, and the queue window's Move earlier.
    assert_eq!(
        rebound_row_key(EDIT_PASTE_MOVE_ID, "Ctrl+Shift+V", Platform::Linux),
        shown("Ctrl+Shift+V")
    );
    assert_eq!(
        rebound_row_key(GO_PARENT_ID, "Alt+↑", Platform::Windows),
        shown("Alt+↑")
    );
    // A row's own built key still registers as the bar builds it.
    assert_eq!(
        rebound_row_key(EDIT_COPY_ID, "Ctrl+C", Platform::Linux),
        registered("Ctrl+C")
    );
    assert_eq!(
        rebound_row_key(GO_PARENT_ID, "⌘↑", Platform::MacOs),
        registered("Cmd+Up")
    );
}

#[test]
fn an_empty_push_clears_the_row() {
    let cleared = RowKey {
        accelerator: None,
        display: None,
    };
    assert_eq!(rebound_row_key(FILE_COPY_ID, "", Platform::MacOs), cleared);
}
