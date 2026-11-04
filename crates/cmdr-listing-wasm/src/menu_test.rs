//! The demo's menu bar: the pushes it stores, the bar it draws from them, and what a click sends.

use std::collections::BTreeSet;

use serde_json::{Value, json};

use crate::events_test::{ts_keys, ts_type_of_event};
use crate::menu::{BarMenuView, Click, DemoMenu, MENU_COMMANDS, Row, RowLabel};

fn find<'a>(rows: &'a [Row], id: &str) -> Option<&'a Row> {
    rows.iter().find_map(|row| match row {
        Row::Item { id: own, .. } | Row::Check { id: own, .. } if own == id => Some(row),
        Row::Submenu { rows, .. } => find(rows, id),
        _ => None,
    })
}

fn row(bar: &[BarMenuView], id: &str) -> Row {
    bar.iter()
        .find_map(|menu| find(&menu.rows, id))
        .unwrap_or_else(|| panic!("the bar has no `{id}`"))
        .clone()
}

fn shortcut(menu: &DemoMenu, id: &str) -> Option<String> {
    match row(&menu.bar(), id) {
        Row::Item { shortcut, .. } | Row::Check { shortcut, .. } => shortcut,
        _ => None,
    }
}

fn enabled(menu: &DemoMenu, id: &str) -> bool {
    match row(&menu.bar(), id) {
        Row::Item { enabled, .. } | Row::Check { enabled, .. } => enabled,
        _ => false,
    }
}

fn checked(menu: &DemoMenu, id: &str) -> bool {
    match row(&menu.bar(), id) {
        Row::Check { checked, .. } => checked,
        _ => panic!("`{id}` isn't a check item"),
    }
}

fn push(menu: &DemoMenu, command: &str, args: Value) {
    menu.invoke(command, args)
        .unwrap_or_else(|e| panic!("`{command}` refused: {e}"));
}

fn emitted(click: Click) -> (String, Value) {
    match click {
        Click::Emit { event, payload, .. } => (event, payload),
        other => panic!("expected an emit, got {other:?}"),
    }
}

#[test]
fn the_bar_is_macos_with_the_app_menu_first() {
    let bar = DemoMenu::new().bar();
    let ids: Vec<&str> = bar.iter().map(|menu| menu.id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "menu_app",
            "menu_file",
            "menu_edit",
            "menu_select",
            "menu_view",
            "menu_go",
            "menu_tab",
            "menu_window",
            "menu_help"
        ]
    );
    assert_eq!(
        bar[0].title,
        RowLabel::Text {
            text: "Cmdr".to_string()
        }
    );
}

#[test]
fn shortcuts_are_the_glyphs_a_mac_draws() {
    let menu = DemoMenu::new();
    assert_eq!(shortcut(&menu, "new_tab").as_deref(), Some("⌘T"));
    assert_eq!(shortcut(&menu, "command_palette").as_deref(), Some("⇧⌘P"));
    assert_eq!(shortcut(&menu, "show_in_finder").as_deref(), Some("⌥⌘O"));
    assert_eq!(shortcut(&menu, "file_copy").as_deref(), Some("F5"));
    // Display-only: the frontend's own spelling.
    assert_eq!(shortcut(&menu, "invert_selection").as_deref(), Some("*"));
    assert_eq!(shortcut(&menu, "favorites_add"), None);
}

/// The browser-safe map reaches the bar the way any rebind reaches the native one.
#[test]
fn a_rebind_shows_where_the_native_bar_would_show_it() {
    let menu = DemoMenu::new();
    push(
        &menu,
        "update_menu_accelerator",
        json!({ "commandId": "tab.new", "shortcut": "⌃T" }),
    );
    assert_eq!(shortcut(&menu, "new_tab").as_deref(), Some("⌃T"));
    push(
        &menu,
        "update_menu_accelerator",
        json!({ "commandId": "sort.byName", "shortcut": "⌃3" }),
    );
    assert_eq!(shortcut(&menu, "sort_by_name").as_deref(), Some("⌃3"));
    // No command modifier: shown, never registered.
    push(
        &menu,
        "update_menu_accelerator",
        json!({ "commandId": "tab.close", "shortcut": "⇧8" }),
    );
    assert_eq!(shortcut(&menu, "close_tab").as_deref(), Some("⇧8"));
    push(
        &menu,
        "update_menu_accelerator",
        json!({ "commandId": "tab.close", "shortcut": "" }),
    );
    assert_eq!(shortcut(&menu, "close_tab"), None);
}

#[test]
fn the_view_mode_accelerator_follows_the_active_pane() {
    let menu = DemoMenu::new();
    push(
        &menu,
        "update_menu_accelerator",
        json!({ "commandId": "view.fullMode", "shortcut": "⌃1" }),
    );
    assert_eq!(shortcut(&menu, "view_mode_full_left").as_deref(), Some("⌃1"));
    assert_eq!(shortcut(&menu, "view_mode_full_right"), None);
    push(
        &menu,
        "update_view_mode_menu",
        json!({ "activePane": "right", "leftMode": "full", "rightMode": "brief" }),
    );
    assert_eq!(shortcut(&menu, "view_mode_full_left"), None);
    assert_eq!(shortcut(&menu, "view_mode_full_right").as_deref(), Some("⌃1"));
    assert!(checked(&menu, "view_mode_full_left"));
    assert!(checked(&menu, "view_mode_brief_right"));
}

#[test]
fn a_dialog_greys_what_it_refuses_and_a_greyed_row_clicks_nothing() {
    let menu = DemoMenu::new();
    assert!(enabled(&menu, "file_copy"));
    push(
        &menu,
        "set_commands_refused_over_dialog",
        json!({ "commandIds": ["file.copy"] }),
    );
    assert!(!enabled(&menu, "file_copy"));
    assert_eq!(menu.click("file_copy"), Click::Nothing);
}

#[test]
fn another_window_in_front_greys_the_explorer_and_sends_edits_natively() {
    let menu = DemoMenu::new();
    push(&menu, "activate_window_menu", json!({ "kind": "other" }));
    assert!(!enabled(&menu, "file_copy"));
    assert!(enabled(&menu, "settings"));
    assert_eq!(
        menu.click("edit_copy"),
        Click::NativeEdit {
            action: "copy".to_string()
        }
    );
    assert_eq!(menu.click("close_tab"), Click::CloseFocusedWindow);
    push(&menu, "activate_window_menu", json!({ "kind": "main" }));
    assert_eq!(
        emitted(menu.click("edit_copy")),
        ("execute-command".to_string(), json!({ "commandId": "edit.copy" }))
    );
}

#[test]
fn a_command_row_sends_execute_command() {
    assert_eq!(
        emitted(DemoMenu::new().click("go_home")),
        ("execute-command".to_string(), json!({ "commandId": "nav.goHome" }))
    );
}

#[test]
fn show_hidden_toggles_itself_and_says_so() {
    let menu = DemoMenu::new();
    assert_eq!(
        emitted(menu.click("show_hidden_files")),
        ("settings-changed".to_string(), json!({ "showHiddenFiles": true }))
    );
    assert!(checked(&menu, "show_hidden_files"));
    push(&menu, "sync_menu_show_hidden", json!({ "checked": false }));
    assert!(!checked(&menu, "show_hidden_files"));
}

#[test]
fn sort_checks_follow_the_request_and_a_click_asks_for_a_sort() {
    let menu = DemoMenu::new();
    push(
        &menu,
        "update_sort_menu",
        json!({ "sort": { "column": "size", "order": "descending" } }),
    );
    assert!(checked(&menu, "sort_by_size"));
    assert!(checked(&menu, "sort_descending"));
    assert!(!checked(&menu, "sort_by_name"));
    assert_eq!(
        emitted(menu.click("sort_by_name")),
        ("menu-sort".to_string(), json!({ "action": "sortBy", "value": "name" }))
    );
    // The check stays on the request until the frontend's sort lands.
    assert!(checked(&menu, "sort_by_size"));
}

#[test]
fn the_live_labels_follow_their_pushes() {
    let menu = DemoMenu::new();
    let label = |id: &str| match row(&menu.bar(), id) {
        Row::Item { label, .. } => label,
        _ => unreachable!(),
    };
    push(&menu, "update_pin_tab_menu", json!({ "isPinned": true }));
    assert_eq!(
        serde_json::to_value(label("pin_tab_menu")).expect("serializes"),
        json!({ "key": "menu.tab.unpinTab", "args": {} })
    );
    push(
        &menu,
        "update_select_same_kind_menu",
        json!({ "target": { "kind": "sameExtension", "extension": "pdf" } }),
    );
    assert_eq!(
        serde_json::to_value(label("select_same_kind")).expect("serializes"),
        json!({ "key": "menu.select.sameExtension", "args": { "extension": "pdf" } })
    );
    assert!(!enabled(&menu, "undo_trash"));
    push(
        &menu,
        "update_undo_trash_menu",
        json!({ "target": { "kind": "one", "name": "a.txt" } }),
    );
    assert!(enabled(&menu, "undo_trash"));
}

#[test]
fn the_os_items_and_unknown_ids_click_nothing() {
    assert_eq!(DemoMenu::new().click("nope"), Click::Nothing);
}

#[test]
fn every_command_answers_and_a_stranger_is_refused() {
    for command in MENU_COMMANDS {
        assert!(DemoMenu::handles(command));
    }
    assert!(
        DemoMenu::new()
            .invoke("list_directory_start_streaming", json!({}))
            .is_err()
    );
    assert!(
        DemoMenu::new()
            .invoke("activate_window_menu", json!({ "kind": "?" }))
            .is_err()
    );
}

/// The four events a click sends carry exactly the keys `bindings.ts` declares for them.
#[test]
fn every_event_a_click_sends_matches_the_bindings() {
    let menu = DemoMenu::new();
    for id in ["go_home", "show_hidden_files", "view_mode_full_right", "sort_ascending"] {
        let (event, payload) = emitted(menu.click(id));
        let keys: BTreeSet<String> = payload.as_object().expect("an object").keys().cloned().collect();
        assert_eq!(keys, ts_keys(ts_type_of_event(&event)), "`{event}` from `{id}`");
    }
}
