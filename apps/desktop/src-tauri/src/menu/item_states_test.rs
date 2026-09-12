//! Tests for menu-item enable-state rules (`item_states.rs`).

use super::*;
use crate::menu::{EDIT_COPY_ID, EDIT_CUT_ID, EDIT_PASTE_ID};

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
        refused,
    }
}

#[test]
fn an_item_greys_out_while_the_dialog_gate_refuses_its_command() {
    use crate::menu::{ABOUT_ID, SERVERS_CONNECT_ID};
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
    use crate::menu::{SERVERS_CONNECT_ID, SETTINGS_ID};
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
