//! Ties the bar to the frontend's registry: on each platform, every row with a command registers
//! the accelerator a rebind to that command's first default key would register
//! (`to_accelerator`, the same door `update_menu_accelerator` takes), or sits on [`EXCEPTIONS`]
//! with its reason.
//!
//! The registry side is `registry_default_keys.tsv`, which the frontend's
//! `lib/commands/default-keymap.test.ts` writes under each OS's capability fixture and checks
//! for staleness, so a key moved on either side fails one of the two tests.

use std::collections::{BTreeSet, HashMap};

use super::every_entry;
use crate::accelerators::to_accelerator;
use crate::ids::{
    COMPARE_DIRECTORIES_ID, EDIT_ID, FILE_COMPRESS_ID, FILE_COPY_ID, FILE_DELETE_ID, FILE_DELETE_PERMANENTLY_ID,
    FILE_DUPLICATE_ID, FILE_MOVE_ID, FILE_NEW_FILE_ID, FILE_NEW_FOLDER_ID, FILE_VIEW_ID, GO_PARENT_ID,
    OPEN_TERMINAL_HERE_ID, QUICK_LOOK_ID, RENAME_ID, SELECT_SAME_KIND_ID, SWITCH_PANE_ID, VIEW_MODE_BRIEF_LEFT_ID,
    VIEW_MODE_FULL_LEFT_ID, VIEW_ZOOM_IN_ID, VIEW_ZOOM_OUT_ID, menu_id_to_command,
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
const WINDOWS_AND_LINUX: &[Platform] = &[Platform::Windows, Platform::Linux];
const EVERY_PLATFORM: &[Platform] = &Platform::ALL;

/// macOS's bar registers its F-keys, Tab, and Shift+Space bare, where a rebind to a bare key only
/// shows it: `to_accelerator`'s modifier floor guards the REBIND door (`menu/DETAILS.md` §
/// "Display-only accelerators"), and the bar's own rows are reviewed in `bar_test.rs`.
const MACOS_BARE_KEY: &str =
    "macOS registers this bare key itself; the modifier floor keeps only a rebind to one display-only";

/// GTK takes the F-keys (and these two zoom chords) before the webview, so the Linux bar leaves
/// them to the frontend's keydown dispatch (`menu/DETAILS.md` § "Platform differences").
const GTK_TAKES_IT: &str = "GTK takes this key before the webview, so keydown dispatch runs it; Windows follows Linux until its menu model is decided";

/// The registry's first key can't be registered (bare, or a key muda has no `Code` for), so the
/// bar registers the command's second default.
const SECOND_KEY: &str = "the first default can't be a bar accelerator, so the bar registers the second";

const EXCEPTIONS: &[Exception] = &[
    Exception {
        platforms: MACOS,
        menu_id: FILE_VIEW_ID,
        reason: MACOS_BARE_KEY,
    },
    Exception {
        platforms: MACOS,
        menu_id: EDIT_ID,
        reason: MACOS_BARE_KEY,
    },
    Exception {
        platforms: MACOS,
        menu_id: FILE_COPY_ID,
        reason: MACOS_BARE_KEY,
    },
    Exception {
        platforms: MACOS,
        menu_id: FILE_MOVE_ID,
        reason: MACOS_BARE_KEY,
    },
    Exception {
        platforms: MACOS,
        menu_id: FILE_NEW_FOLDER_ID,
        reason: MACOS_BARE_KEY,
    },
    Exception {
        platforms: MACOS,
        menu_id: FILE_NEW_FILE_ID,
        reason: MACOS_BARE_KEY,
    },
    Exception {
        platforms: MACOS,
        menu_id: FILE_DELETE_ID,
        reason: MACOS_BARE_KEY,
    },
    Exception {
        platforms: MACOS,
        menu_id: FILE_DELETE_PERMANENTLY_ID,
        reason: MACOS_BARE_KEY,
    },
    Exception {
        platforms: MACOS,
        menu_id: RENAME_ID,
        reason: MACOS_BARE_KEY,
    },
    Exception {
        platforms: MACOS,
        menu_id: OPEN_TERMINAL_HERE_ID,
        reason: MACOS_BARE_KEY,
    },
    Exception {
        platforms: MACOS,
        menu_id: QUICK_LOOK_ID,
        reason: MACOS_BARE_KEY,
    },
    Exception {
        platforms: MACOS,
        menu_id: COMPARE_DIRECTORIES_ID,
        reason: MACOS_BARE_KEY,
    },
    Exception {
        platforms: MACOS,
        menu_id: SWITCH_PANE_ID,
        reason: MACOS_BARE_KEY,
    },
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
    Exception {
        platforms: EVERY_PLATFORM,
        menu_id: SELECT_SAME_KIND_ID,
        reason: "`⌥⇧=` clears the floor, but the default stays display-only so `⌥+` stays free for typing \
                 (`menu/DETAILS.md` § \"Display-only accelerators\")",
    },
    Exception {
        platforms: WINDOWS_AND_LINUX,
        menu_id: FILE_DUPLICATE_ID,
        reason: "`Ctrl+D` is also Show favorites' there, the registry's known clash, and one chord can't sit on \
                 two rows; it goes when the clash does",
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

/// An accelerator as the chord it binds: its modifiers as a set, plus its key. `Alt+Cmd+O` and
/// `Cmd+Alt+O` are one chord, and so are `Cmd+Equal` and `Cmd+=`.
fn chord(accelerator: &str) -> (BTreeSet<&'static str>, String) {
    let mut modifiers = BTreeSet::new();
    let mut key = String::new();
    let tokens: Vec<&str> = accelerator.split('+').collect();
    for (position, token) in tokens.iter().enumerate() {
        let modifier = match token.to_ascii_uppercase().as_str() {
            "CMD" | "COMMAND" | "SUPER" | "META" => Some("meta"),
            "CTRL" | "CONTROL" => Some("ctrl"),
            "ALT" | "OPTION" => Some("alt"),
            "SHIFT" => Some("shift"),
            _ => None,
        };
        match modifier {
            Some(modifier) if position + 1 < tokens.len() => {
                modifiers.insert(modifier);
            }
            // `Cmd++`: the split's last two tokens are empty, and the key was a `+`.
            _ if token.is_empty() => key = "+".to_string(),
            _ => {
                key = match token.to_ascii_uppercase().as_str() {
                    "EQUAL" => "=".to_string(),
                    "MINUS" => "-".to_string(),
                    other => other.to_string(),
                }
            }
        }
    }
    (modifiers, key)
}

/// What the row registers against what the registry's first key would, as chords, or `None`
/// where they agree.
fn mismatch(built: Option<&str>, expected: Option<&str>) -> Option<String> {
    if built.map(chord) == expected.map(chord) {
        return None;
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
            Some((menu_id, accelerator, to_accelerator(combo, platform)))
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

#[test]
fn a_chord_is_its_modifier_set_and_key() {
    assert_eq!(chord("Alt+Cmd+O"), chord("Cmd+Alt+O"));
    assert_eq!(chord("Cmd+Equal"), chord("Cmd+="));
    assert_eq!(chord("Cmd++"), (BTreeSet::from(["meta"]), "+".to_string()));
    assert_ne!(chord("Ctrl+D"), chord("Cmd+D"));
}
