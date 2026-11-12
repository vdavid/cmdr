//! The menu bar rows whose words differ per OS. The English table carries all three texts, so
//! every seam arm's answer is checked here whichever host runs the test.

use super::*;

/// A mounted network share's detach item says each OS's verb: Finder's Eject, File Explorer's
/// Disconnect, the Linux file manager's Unmount (#1117). A drive's item says Eject everywhere.
#[test]
fn the_share_detach_rows_read_each_os_s_verb() {
    let rows: [(&str, [&str; 3]); 2] = [
        (
            "menu.volume.detachShare",
            ["Eject ({name})", "Disconnect ({name})", "Unmount ({name})"],
        ),
        (
            "menu.volume.detachShareBusy",
            [
                "Eject ({name}) (busy)",
                "Disconnect ({name}) (busy)",
                "Unmount ({name}) (busy)",
            ],
        ),
    ];
    for (key, [macos, windows, linux]) in rows {
        let text = lookup_text(BASE_LOCALE, key).unwrap_or_else(|| panic!("{key} is in the English table"));
        assert_eq!(pick(text, NativeOs::MacOs), macos, "{key}");
        assert_eq!(pick(text, NativeOs::Windows), windows, "{key}");
        assert_eq!(pick(text, NativeOs::Linux), linux, "{key}");
    }
}

/// Windows labels the trash undo "Undo delete" (the F8 action's name there) and the settings row
/// "Settings" with no ellipsis; Linux, where both rows sit under Edit, reads the macOS words.
#[test]
fn the_trash_undo_and_settings_rows_read_each_os_s_words() {
    let rows: [(&str, [&str; 3]); 4] = [
        ("menu.app.settings", ["Settings…", "Settings", "Settings…"]),
        (
            "menu.edit.undoTrash",
            ["Undo move to trash", "Undo delete", "Undo move to trash"],
        ),
        (
            "menu.edit.undoTrashOne",
            [
                "Undo move to trash of “{name}”",
                "Undo delete of “{name}”",
                "Undo move to trash of “{name}”",
            ],
        ),
        (
            "menu.edit.undoTrashMany",
            [
                // allowed-pluralize-noun: drawn only for two or more (`cmdr-menu`'s `labels.rs` picks `undoTrashOne` for one)
                "Undo move to trash of {countText} items",
                // allowed-pluralize-noun: drawn only for two or more, as above
                "Undo delete of {countText} items",
                // allowed-pluralize-noun: drawn only for two or more, as above
                "Undo move to trash of {countText} items",
            ],
        ),
    ];
    for (key, [macos, windows, linux]) in rows {
        let text = lookup_text(BASE_LOCALE, key).unwrap_or_else(|| panic!("{key} is in the English table"));
        assert_eq!(pick(text, NativeOs::MacOs), macos, "{key}");
        assert_eq!(pick(text, NativeOs::Windows), windows, "{key}");
        assert_eq!(pick(text, NativeOs::Linux), linux, "{key}");
        // This host reads its own arm's text through the same lookup the menu uses.
        assert_eq!(
            lookup(BASE_LOCALE, key),
            Some(pick(text, super::super::platform::NATIVE_TEXT_OS)),
            "{key}"
        );
    }
}
