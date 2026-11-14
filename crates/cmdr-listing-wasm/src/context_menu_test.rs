//! The demo's right-click menus: the rows a `show_*` command answers, and what a pick sends.

use std::collections::BTreeSet;

use serde_json::{Value, json};

use crate::context_menu::{POPUP_COMMANDS, Popup};
use crate::events_test::{ts_keys, ts_type_of_event};
use crate::menu::{Click, DemoMenu, Row, RowLabel};

fn file_args() -> Value {
    json!({
        "path": "/Users/you/Documents/report.pdf",
        "filename": "report.pdf",
        "isDirectory": false,
        "paths": [],
        "pane": { "canOpenTerminalHere": true, "canShare": true, "canTag": true, "listingId": "l1" },
        "target": { "countText": null, "sizeText": "2.1 MB" },
        "shortcuts": { "file.copy": "F5", "file.rename": "⇧F6", "selection.toggle": "Space" },
        "anchor": null,
        "sameKind": { "kind": "sameExtension", "extension": "pdf" },
    })
}

fn popup(menu: &DemoMenu, command: &str, args: Value) -> Popup {
    menu.popup(command, args, "main")
        .unwrap_or_else(|e| panic!("`{command}` refused: {e}"))
}

fn find<'a>(rows: &'a [Row], id: &str) -> Option<&'a Row> {
    rows.iter().find_map(|row| match row {
        Row::Item { id: own, .. } | Row::Check { id: own, .. } if own == id => Some(row),
        Row::Submenu { rows, .. } => find(rows, id),
        _ => None,
    })
}

fn emitted(click: Click) -> (String, Value, String) {
    match click {
        Click::Emit { event, payload, to } => (event, payload, to),
        other => panic!("expected an emit, got {other:?}"),
    }
}

/// The file menu is the crate's macOS one, with the header first, live shortcuts in a Mac's
/// glyphs, and no slot (the demo has no OS to fill one).
#[test]
fn the_file_menu_draws_the_crate_s_rows() {
    let menu = DemoMenu::new();
    let drawn = popup(&menu, "show_file_context_menu", file_args());
    assert_eq!(
        drawn.rows.first(),
        Some(&Row::Header {
            label: RowLabel::Text {
                text: "report.pdf · 2.1 MB".to_string()
            }
        })
    );
    let shortcut = |id| match find(&drawn.rows, id) {
        Some(Row::Item { shortcut, .. }) => shortcut.clone(),
        other => panic!("`{id}`: {other:?}"),
    };
    assert_eq!(shortcut("file_copy").as_deref(), Some("F5"));
    assert_eq!(shortcut("rename").as_deref(), Some("⇧F6"));
    assert_eq!(shortcut("toggle_selection").as_deref(), Some("Space"));
    // Unbound in the pushed map, so no label, whatever the bar ships with.
    assert_eq!(shortcut("file_view"), None);
    // The Mac's own rows are there; the OS-filled groups aren't.
    assert!(find(&drawn.rows, "get_info").is_some());
    assert!(find(&drawn.rows, "open_terminal_here").is_some());
    let json = serde_json::to_string(&drawn.rows).expect("serializes");
    assert!(!json.contains("services") && !json.contains("open-with"), "{json}");
    // No two separators in a row where a slot was left out.
    assert!(
        !drawn
            .rows
            .windows(2)
            .any(|pair| matches!(pair, [Row::Separator, Row::Separator])),
        "{json}"
    );
}

/// A pick sends what the native item sends: a command, to the main window.
#[test]
fn a_command_row_sends_execute_command() {
    let menu = DemoMenu::new();
    popup(&menu, "show_file_context_menu", file_args());
    let (event, payload, to) = emitted(menu.context_click("file_copy"));
    assert_eq!((event.as_str(), to.as_str()), ("execute-command", "main"));
    assert_eq!(payload, json!({ "commandId": "file.copy", "road": "relay" }));
    // A row only the context menu carries says it came from a right-click.
    let (_, payload, _) = emitted(menu.context_click("tell_me_about_context"));
    assert_eq!(
        payload,
        json!({ "commandId": "file.tellMeAbout", "road": "contextMenu" })
    );
}

/// A greyed row, or one the last popup didn't offer, does nothing.
#[test]
fn a_row_the_popup_did_not_offer_live_does_nothing() {
    let menu = DemoMenu::new();
    popup(
        &menu,
        "show_tab_context_menu",
        json!({ "isPinned": false, "canClose": false, "hasOtherUnpinnedTabs": false }),
    );
    assert_eq!(menu.context_click("tab_close"), Click::Nothing);
    assert_eq!(menu.context_click("file_copy"), Click::Nothing);
    let (event, payload, _) = emitted(menu.context_click("tab_pin"));
    assert_eq!(event, "tab-context-action");
    assert_eq!(payload, json!({ "action": "tab_pin" }));
}

/// Add to favorites favorites the stashed path through the app's own `add_favorite` command.
#[test]
fn add_to_favorites_invokes_the_command_on_the_right_clicked_folder() {
    let menu = DemoMenu::new();
    popup(
        &menu,
        "show_parent_row_context_menu",
        json!({ "parentPath": "/Users/you", "anchor": { "x": 10.0, "y": 20.0 } }),
    );
    assert_eq!(
        menu.context_click("favorites_add_context"),
        Click::Invoke {
            command: "add_favorite".to_string(),
            args: json!({ "path": "/Users/you", "name": null }),
        }
    );
}

/// The viewer's pick goes back to the viewer it was popped over, a queue row's to the queue.
#[test]
fn a_pick_goes_to_the_window_its_menu_answers_to() {
    let menu = DemoMenu::new();
    menu.popup("show_viewer_context_menu", json!({ "hasSelection": true }), "viewer-3")
        .expect("opens");
    let (event, payload, to) = emitted(menu.context_click("viewer_context_copy"));
    assert_eq!(
        (event.as_str(), to.as_str()),
        ("viewer-context-menu-action", "viewer-3")
    );
    assert_eq!(payload, json!({ "action": "copy" }));

    menu.popup(
        "show_queue_row_context_menu",
        json!({ "entries": [{ "kind": "item", "action": "cancel", "enabled": true }], "shortcuts": { "queue.cancel": "C" } }),
        "queue",
    )
    .expect("opens");
    let (event, payload, to) = emitted(menu.context_click("queue_row_cancel"));
    assert_eq!((event.as_str(), to.as_str()), ("queue-row-context-action", "queue"));
    assert_eq!(payload, json!({ "action": "cancel" }));
}

/// Every popup command answers, and every event a pick sends carries exactly the keys
/// `bindings.ts` declares for it.
#[test]
fn every_popup_opens_and_every_pick_matches_the_bindings() {
    let menu = DemoMenu::new();
    let opens: [(&str, Value, &str); 9] = [
        ("show_file_context_menu", file_args(), "file_copy"),
        (
            "show_breadcrumb_context_menu",
            json!({ "shortcuts": {}, "ejectVolumeId": "v1", "ejectVolumeName": "Backup", "offerEjectAll": true }),
            "eject_all_volumes",
        ),
        (
            "show_parent_row_context_menu",
            json!({ "parentPath": "/Users", "anchor": null }),
            "favorites_add_context",
        ),
        (
            "show_tab_context_menu",
            json!({ "isPinned": true, "canClose": true, "hasOtherUnpinnedTabs": true }),
            "tab_close_others",
        ),
        (
            "show_date_sort_menu",
            json!({ "pane": "right", "requested": "modified", "anchor": null }),
            "date_sort_menu_right_added",
        ),
        ("show_function_key_bar_context_menu", json!({}), "function_key_bar_hide"),
        (
            "show_viewer_context_menu",
            json!({ "hasSelection": false }),
            "viewer_context_select_all",
        ),
        (
            "show_queue_row_context_menu",
            json!({ "entries": [{ "kind": "check", "action": "select", "enabled": true, "checked": false }] }),
            "queue_row_select",
        ),
        (
            "show_network_host_context_menu",
            json!({ "rowId": "r1", "host": { "id": "h1", "name": "nas" }, "isManual": true, "isSaved": true, "hasCredentials": true, "anchor": null }),
            "network_host_forget_secret",
        ),
    ];
    let commands: BTreeSet<&str> = opens.iter().map(|(command, ..)| *command).collect();
    assert_eq!(commands, POPUP_COMMANDS.iter().copied().collect());
    for (command, args, pick) in opens {
        assert!(DemoMenu::opens_popup(command));
        menu.popup(command, args, "main")
            .unwrap_or_else(|e| panic!("`{command}` refused: {e}"));
        let (event, payload, _) = match menu.context_click(pick) {
            // Add to favorites is a command call, pinned on its own above.
            Click::Invoke { .. } => continue,
            click => emitted(click),
        };
        let keys: BTreeSet<String> = payload
            .as_object()
            .map(|object| object.keys().cloned().collect())
            .unwrap_or_default();
        let ts_type = ts_type_of_event(&event);
        if payload.is_null() {
            continue;
        }
        assert_eq!(keys, ts_keys(ts_type), "`{event}` from `{pick}`");
    }
}

/// The breadcrumb's Eject carries the stashed volume, in the frontend's own action words.
#[test]
fn eject_names_the_stashed_volume() {
    let menu = DemoMenu::new();
    popup(
        &menu,
        "show_breadcrumb_context_menu",
        json!({ "shortcuts": {}, "ejectVolumeId": "v1", "ejectVolumeName": "Backup", "offerEjectAll": true }),
    );
    let (_, payload, _) = emitted(menu.context_click("eject_all_volumes"));
    assert_eq!(
        payload,
        json!({ "action": "eject-all", "volumeId": "v1", "volumeName": "Backup" })
    );
}

#[test]
fn an_unknown_popup_is_refused() {
    assert!(DemoMenu::new().popup("show_nothing", json!({}), "main").is_err());
}
