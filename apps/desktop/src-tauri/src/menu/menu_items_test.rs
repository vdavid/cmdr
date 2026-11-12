//! Tests for the menu item builder helpers (`menu_items.rs`).
//!
//! Split out under the directory's `*_test.rs` convention.

use super::*;
use cmdr_menu::context::popups::detach_label;

#[test]
fn a_disk_row_says_eject_and_a_phone_row_says_disconnect() {
    // Pre-fix a phone's native menu read "Eject (Pixel 8)" while the inline
    // control on the same row already said Disconnect.
    assert_eq!(
        resolve(&detach_label("Backup", false, DetachWord::Eject)),
        "Eject (Backup)"
    );
    assert_eq!(
        resolve(&detach_label("Backup", true, DetachWord::Eject)),
        "Eject (Backup) (busy)"
    );
    assert_eq!(
        resolve(&detach_label("Pixel 8", false, DetachWord::Disconnect)),
        "Disconnect"
    );
    assert_eq!(
        resolve(&detach_label("Pixel 8", true, DetachWord::Disconnect)),
        "Disconnect (busy)"
    );
}

#[test]
fn the_same_kind_label_says_what_the_cursor_row_would_select() {
    assert_eq!(same_kind_menu_label(None), "Select all of the same kind");
    assert_eq!(
        same_kind_menu_label(Some(&SameKindTarget::AllFolders)),
        "Select all folders"
    );
    assert_eq!(
        same_kind_menu_label(Some(&SameKindTarget::NoExtension)),
        "Select all files with no extension"
    );
    assert_eq!(
        same_kind_menu_label(Some(&SameKindTarget::SameExtension {
            extension: "pdf".to_string()
        })),
        "Select all with extension *.pdf"
    );
}

/// The payload is the frontend's `SameKindTarget` on the wire, so a rename on either side has to
/// break something. This is the only place the two spellings meet.
#[test]
fn the_same_kind_payload_deserializes_the_frontend_target() {
    let folders: SameKindTarget = serde_json::from_str(r#"{"kind":"allFolders"}"#).expect("allFolders");
    assert_eq!(folders, SameKindTarget::AllFolders);
    let none: SameKindTarget = serde_json::from_str(r#"{"kind":"noExtension"}"#).expect("noExtension");
    assert_eq!(none, SameKindTarget::NoExtension);
    let ext: SameKindTarget =
        serde_json::from_str(r#"{"kind":"sameExtension","extension":"gz"}"#).expect("sameExtension");
    assert_eq!(
        ext,
        SameKindTarget::SameExtension {
            extension: "gz".to_string()
        }
    );
}

#[test]
fn the_undo_trash_label_names_what_it_would_put_back() {
    assert_eq!(undo_trash_menu_label(None), "Undo move to trash");
    assert_eq!(
        undo_trash_menu_label(Some(&TrashUndoTarget::One {
            name: "report.pdf".to_string()
        })),
        "Undo move to trash of “report.pdf”"
    );
    assert_eq!(
        undo_trash_menu_label(Some(&TrashUndoTarget::Many {
            count_text: "1,234".to_string()
        })),
        "Undo move to trash of 1,234 items"
    );
}

/// The payload is the frontend's `TrashUndoTarget` (`trash-undo.ts`) on the wire.
#[test]
fn the_undo_trash_payload_deserializes_the_frontend_target() {
    let one: TrashUndoTarget = serde_json::from_str(r#"{"kind":"one","name":"a.txt"}"#).expect("one");
    assert_eq!(
        one,
        TrashUndoTarget::One {
            name: "a.txt".to_string()
        }
    );
    let many: TrashUndoTarget = serde_json::from_str(r#"{"kind":"many","countText":"3"}"#).expect("many");
    assert_eq!(
        many,
        TrashUndoTarget::Many {
            count_text: "3".to_string()
        }
    );
}

#[test]
fn the_detach_word_comes_off_the_volume_id() {
    use cmdr_fs::volume::{adb_volume_id, mtp_device_id, path_volume_id};

    assert_eq!(
        detach_word_for_volume_id(&adb_volume_id("39041FDJH00A0K")),
        DetachWord::Disconnect
    );
    // MTP keeps Eject: it earns the promise by closing the device session.
    assert_eq!(
        detach_word_for_volume_id(&mtp_device_id("39041FDJH00A0K")),
        DetachWord::Eject
    );
    assert_eq!(
        detach_word_for_volume_id(&path_volume_id("/Volumes/Backup")),
        DetachWord::Eject
    );
    // A mounted SMB share, whichever backend serves it (#1117): its id is the only fact both
    // native menus carry, and the scheme says share on every OS.
    assert_eq!(
        detach_word_for_volume_id(&cmdr_fs::volume::smb_volume_id("naspi", 445, "media")),
        DetachWord::Share
    );
}

/// A share's item draws its own keys (each OS's verb: `intl/native_strings_per_os_test.rs`) (#1117).
#[test]
fn a_share_row_draws_the_share_keys() {
    assert_eq!(
        detach_label("media", false, DetachWord::Share).key,
        "menu.volume.detachShare"
    );
    assert_eq!(
        detach_label("media", true, DetachWord::Share).key,
        "menu.volume.detachShareBusy"
    );
}

/// Menu labels end with the U+2026 ellipsis character, never three periods.
/// macOS kerns `...` visibly worse next to system items, and the SF Symbol pass finds an
/// `NSMenuItem` by its title, so a stray `...` costs the item its icon.
///
/// Labels themselves live in the message catalog now, so the catalog is where
/// the real guard is (`native_strings::menu_labels_end_with_the_ellipsis_character`).
/// This one keeps watch over the literals still written in these files, so a
/// new one can't slip in with `...`.
#[test]
fn menu_labels_use_the_ellipsis_character() {
    const SOURCES: [(&str, &str); 4] = [
        (
            "cmdr-menu bar.rs",
            include_str!("../../../../../crates/cmdr-menu/src/bar.rs"),
        ),
        ("menu_structure.rs", include_str!("menu_structure.rs")),
        ("file_context_menu.rs", include_str!("file_context_menu.rs")),
        ("open_with.rs", include_str!("open_with.rs")),
    ];

    for (name, source) in SOURCES {
        for (line_number, line) in source.lines().enumerate() {
            assert!(
                !line.contains("...\""),
                "{name}:{} ends a menu label with `...`; use `\\u{{2026}}`: {}",
                line_number + 1,
                line.trim()
            );
        }
    }
}
