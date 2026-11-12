//! Ties the bar to the frontend's registry: on each platform, every row with a command registers
//! the accelerator a rebind to that command's first default key would register
//! (`rebound_row_key`, the same door `update_menu_accelerator` takes), or sits on [`EXCEPTIONS`]
//! with its reason.
//!
//! The registry side is `registry_default_keys.tsv`, which the frontend's
//! `lib/commands/default-keymap.test.ts` writes under each OS's capability fixture and checks
//! for staleness, so a key moved on either side fails one of the two tests.

use std::collections::HashMap;

use super::{RowKey, every_entry, rebound_row_key};
use crate::accelerators::same_chord;
use crate::ids::{
    EDIT_PASTE_MOVE_ID, FILE_COMPRESS_ID, FILE_COPY_ID, FILE_MOVE_ID, GO_PARENT_ID, QUICK_LOOK_ID, SELECT_SAME_KIND_ID,
    SWITCH_PANE_ID, VIEW_MODE_BRIEF_LEFT_ID, VIEW_MODE_FULL_LEFT_ID, VIEW_ZOOM_IN_ID, VIEW_ZOOM_OUT_ID,
    menu_id_to_command,
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

/// GTK takes the F-keys (and these two zoom chords) before the webview, so the Linux bar leaves
/// them to the frontend's keydown dispatch (`menu/DETAILS.md` § "Platform differences").
const GTK_TAKES_IT: &str = "GTK takes this key before the webview, so keydown dispatch runs it; Windows follows Linux until its menu model is decided";

/// The registry's first key can't be registered (bare, or a key muda has no `Code` for), so the
/// bar registers the command's second default.
/// The Windows bar registers the Linux bar's key where the two keymaps differ only in order, so its
/// F-key chords stay with keydown dispatch, as Linux's do.
const WINDOWS_FOLLOWS_LINUX: &str = "the first key there is an F-key chord, which the bar leaves to keydown dispatch; the Windows \
                                     bar registers Linux's first key (a default on Windows too) until its menu model is decided";

const SECOND_KEY: &str = "the first default can't be a bar accelerator, so the bar registers the second";

const EXCEPTIONS: &[Exception] = &[
    Exception {
        platforms: WINDOWS_AND_LINUX,
        menu_id: FILE_COMPRESS_ID,
        reason: GTK_TAKES_IT,
    },
    Exception {
        platforms: WINDOWS_AND_LINUX,
        menu_id: VIEW_ZOOM_OUT_ID,
        reason: GTK_TAKES_IT,
    },
    // `⌘+` has no muda `Code`; `⌘=` is the physical key under it.
    Exception {
        platforms: MACOS,
        menu_id: VIEW_ZOOM_IN_ID,
        reason: SECOND_KEY,
    },
    // `Backspace` is bare; the bar registers `⌘↑` / `Ctrl+↑`.
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
    Exception {
        platforms: WINDOWS_AND_LINUX,
        menu_id: EDIT_PASTE_MOVE_ID,
        reason: "`Ctrl+Shift+V` is a text field's paste as plain text, and a bar accelerator fires before the \
                 page, so keydown dispatch runs it, which leaves it to a focused field",
    },
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

/// What the row registers against what the registry's first key would, as chords, or `None`
/// where they agree.
fn mismatch(built: Option<&str>, expected: Option<&str>) -> Option<String> {
    match (built, expected) {
        (None, None) => return None,
        (Some(built), Some(expected)) if same_chord(built, expected) => return None,
        _ => {}
    }
    Some(format!(
        "the bar registers {}, the registry's first default key registers {}",
        built.map_or_else(|| "nothing".to_string(), |a| format!("`{a}`")),
        expected.map_or_else(|| "nothing".to_string(), |a| format!("`{a}`")),
    ))
}

/// Every row with a command on `platform`, with what it registers and what the registry implies.
fn rows(platform: Platform) -> Vec<(&'static str, Option<&'static str>, Option<String>)> {
    let keys = registry_keys();
    every_entry(platform)
        .into_iter()
        .filter_map(|entry| {
            let (menu_id, accelerator) = match entry {
                EntryKind::Item(item) => (item.id, item.accelerator.on(platform)),
                EntryKind::Check(check) => (check.id, check.accelerator.on(platform)),
                _ => return None,
            };
            let command_id = row_command(menu_id)?;
            let combo = keys.get(&(platform.wire(), command_id)).copied().unwrap_or("");
            Some((
                menu_id,
                accelerator,
                rebound_row_key(menu_id, combo, platform).accelerator,
            ))
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
            if let Some(why) = mismatch(built, expected.as_deref()) {
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
                mismatch(built, expected.as_deref()).is_some(),
                "`{}` is excused on {platform:?} but agrees with the registry there: drop it from `EXCEPTIONS`",
                exception.menu_id
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
        shown("⌥⇧=")
    );
    // A different ⌥ chord is a deliberate rebind, and gets a real accelerator.
    assert_eq!(
        rebound_row_key(SELECT_SAME_KIND_ID, "⌥⇧A", Platform::MacOs),
        registered("Alt+Shift+A")
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
