//! Tests for menu-item enable-state rules (`enabled.rs`).

use super::*;
use crate::ids::{EDIT_COPY_ID, EDIT_CUT_ID, EDIT_PASTE_ID, OPEN_TERMINAL_HERE_ID};

#[test]
fn every_gated_item_is_a_real_file_scoped_menu_item() {
    // A typo, or an id that stopped being registered, would make
    // `menu_item_enabled` skip that item's verdict SILENTLY: the menu would keep
    // offering Copy while a dialog is up, and nothing would say why. Every id
    // here has to resolve to a file-scoped command, which is what `register_item`
    // registers and what `set_menu_context` manages.
    for id in OPERATION_START_ITEM_IDS {
        let mapped = menu_id_to_command(id);
        assert!(mapped.is_some(), "{id} isn't a known menu item id");
        assert!(
            matches!(mapped, Some((_, CommandScope::FileScoped))),
            "{id} must be file-scoped, else `set_menu_context` doesn't manage it either"
        );
    }
}

fn refusing(command_ids: &[&str]) -> HashSet<String> {
    command_ids.iter().map(|id| (*id).to_string()).collect()
}

/// The main window owns the menu, every item's own verdict says yes, and nothing is refused
/// unless `refused` says so.
fn inputs(refused: &HashSet<String>) -> MenuItemInputs<'_> {
    MenuItemInputs {
        explorer_menu_active: true,
        file_operations_blocked: false,
        open_terminal_here_enabled: true,
        reopen_closed_tab_enabled: true,
        undo_trash_available: true,
        refused,
    }
}

/// Like macOS's own Edit > Undo: present always, greyed out while there's nothing to undo.
#[test]
fn undo_move_to_trash_is_live_only_while_a_trash_waits_to_be_undone() {
    use crate::ids::UNDO_TRASH_ID;
    let nothing = refusing(&[]);
    assert!(menu_item_enabled(UNDO_TRASH_ID, &inputs(&nothing)));
    let nothing_to_undo = MenuItemInputs {
        undo_trash_available: false,
        ..inputs(&nothing)
    };
    assert!(!menu_item_enabled(UNDO_TRASH_ID, &nothing_to_undo));
    assert!(!menu_item_enabled(
        UNDO_TRASH_ID,
        &inputs(&refusing(&["edit.undoTrash"]))
    ));
}

#[test]
fn an_item_greys_out_while_the_dialog_gate_refuses_its_command() {
    use crate::ids::{ABOUT_ID, SERVERS_CONNECT_ID};
    let nothing = refusing(&[]);
    assert!(menu_item_enabled(SERVERS_CONNECT_ID, &inputs(&nothing)));
    assert!(!menu_item_enabled(
        SERVERS_CONNECT_ID,
        &inputs(&refusing(&["servers.connect"]))
    ));
    // An app-wide item too: About opens a dialog in the main window, which the gate refuses
    // behind another one whichever window the menu click came from.
    assert!(!menu_item_enabled(ABOUT_ID, &inputs(&refusing(&["app.about"]))));
}

#[test]
fn an_app_item_stays_enabled_outside_the_explorer_and_a_file_item_does_not() {
    use crate::ids::{SERVERS_CONNECT_ID, SETTINGS_ID};
    let nothing = refusing(&[]);
    let settings_in_front = MenuItemInputs {
        explorer_menu_active: false,
        ..inputs(&nothing)
    };
    assert!(menu_item_enabled(SETTINGS_ID, &settings_in_front));
    assert!(!menu_item_enabled(SERVERS_CONNECT_ID, &settings_in_front));
}

#[test]
fn close_tab_still_closes_another_window_while_the_main_one_has_a_dialog_up() {
    let tab_close = refusing(&["tab.close"]);
    assert!(!menu_item_enabled(CLOSE_TAB_ID, &inputs(&tab_close)));
    let settings_in_front = MenuItemInputs {
        explorer_menu_active: false,
        ..inputs(&tab_close)
    };
    assert!(menu_item_enabled(CLOSE_TAB_ID, &settings_in_front));
}

#[test]
fn an_item_with_its_own_verdict_needs_that_verdict_and_the_gate() {
    let nothing = refusing(&[]);
    let blocked = MenuItemInputs {
        file_operations_blocked: true,
        ..inputs(&nothing)
    };
    assert!(!menu_item_enabled(FILE_COPY_ID, &blocked));
    let no_shell_here = MenuItemInputs {
        open_terminal_here_enabled: false,
        ..inputs(&nothing)
    };
    assert!(!menu_item_enabled(OPEN_TERMINAL_HERE_ID, &no_shell_here));
    let no_closed_tabs = MenuItemInputs {
        reopen_closed_tab_enabled: false,
        ..inputs(&nothing)
    };
    assert!(!menu_item_enabled(REOPEN_CLOSED_TAB_ID, &no_closed_tabs));
    assert!(menu_item_enabled(REOPEN_CLOSED_TAB_ID, &inputs(&nothing)));
    assert!(!menu_item_enabled(
        REOPEN_CLOSED_TAB_ID,
        &inputs(&refusing(&["tab.reopen"]))
    ));
}

#[test]
fn the_gate_leaves_the_commands_that_steer_a_running_operation_alone() {
    // The boundary, pinned: cancel, rollback, and the queue window are what a
    // user reaches for WHILE a dialog is up. Cut and Copy stay too — marking a
    // clipboard selection starts nothing.
    for id in [EDIT_CUT_ID, EDIT_COPY_ID] {
        assert!(
            !OPERATION_START_ITEM_IDS.contains(&id),
            "{id} doesn't start an operation, so it must stay enabled"
        );
    }
    // And `Edit > Paste`, which does start one but carries the OS text-paste
    // selector in other windows. See the const's comment.
    assert!(!OPERATION_START_ITEM_IDS.contains(&EDIT_PASTE_ID));
}

#[test]
fn the_viewers_cut_and_paste_are_live_only_while_its_search_box_has_focus() {
    assert!(!viewer_text_edit_enabled(None));
    assert!(viewer_text_edit_enabled(Some("viewer-1")));
}

#[test]
fn a_viewers_search_box_taking_focus_claims_the_menu_and_giving_it_up_releases_it() {
    let mut holder = None;
    note_viewer_search_focus(&mut holder, "viewer-1", true);
    assert_eq!(holder.as_deref(), Some("viewer-1"));
    note_viewer_search_focus(&mut holder, "viewer-1", false);
    assert_eq!(holder, None);
}

/// Clicking from one viewer to another crosses two pushes in flight: the old viewer's input
/// blurs, and the new viewer re-pushes its own state on focus-gain. Whichever order they land
/// in, the answer has to be the viewer in front — otherwise a late blur from a window the user
/// already left greys out the search box they're typing in.
#[test]
fn a_blur_from_the_viewer_left_behind_cannot_take_the_menu_from_the_one_in_front() {
    let mut blur_last = Some("viewer-1".to_string());
    note_viewer_search_focus(&mut blur_last, "viewer-2", true);
    note_viewer_search_focus(&mut blur_last, "viewer-1", false);
    assert_eq!(blur_last.as_deref(), Some("viewer-2"));

    let mut blur_first = Some("viewer-1".to_string());
    note_viewer_search_focus(&mut blur_first, "viewer-1", false);
    note_viewer_search_focus(&mut blur_first, "viewer-2", true);
    assert_eq!(blur_first.as_deref(), Some("viewer-2"));
}
