//! Pins every context menu's rows (`file.rs`, `popups.rs`) and what a pick of each asks for
//! (`click.rs`), on every host.
//!
//! A real `muda::Menu` panics off the main thread, so no unit test can build a popup and read it
//! back; the rows are the menu, so a snapshot of them pins what the user sees. `render` writes one
//! line per row, so a change shows up here as a diff to review.

use super::click::{ContextClick, NetworkHostAction, ViewerEditActionKind, VolumeAction, context_click};
use super::file::{
    FILENAME_MAX_CHARS, FileMenuFacts, PaneFacts, SELECTION_ROWS, SelectionRow, TargetFacts, file_menu, header_label,
};
use super::image_index::ImageIndexMenuState;
use super::popups::{
    BreadcrumbVolume, DetachWord, NetworkHostFacts, PaneSide, QueueRowMenuAction, QueueRowMenuEntry, breadcrumb_menu,
    date_sort_menu, date_sort_pick, function_key_bar_menu, network_host_menu, parent_row_menu, queue_row_menu,
    tab_menu, viewer_text_menu,
};
use super::{ContextLabel, ContextMenuShortcuts, ContextRow, Shortcut, Slot, truncate_for_menu_label};
use crate::ids::{
    CONTEXT_MENU_TARGET_ID, FAVORITES_ADD_CONTEXT_ID, FILE_COPY_ID, FILE_VIEW_ID, SORT_BY_ADDED_ID, SORT_BY_CREATED_ID,
    SORT_BY_MODIFIED_ID, TOGGLE_SELECTION_ID, menu_id_to_command,
};
use crate::labels::SameKindTarget;
use crate::spec::Platform;
use cmdr_sort::SortColumn;

fn label(label: &ContextLabel) -> String {
    match label {
        ContextLabel::Key(text) if text.args.is_empty() => text.key.to_string(),
        ContextLabel::Key(text) => {
            let args: Vec<String> = text.args.iter().map(|(k, v)| format!("{k}={v}")).collect();
            format!("{}({})", text.key, args.join(","))
        }
        ContextLabel::Text(text) => format!("\"{text}\""),
    }
}

fn shortcut(shortcut: Shortcut, id: &str) -> String {
    match shortcut {
        Shortcut::None => String::new(),
        Shortcut::OfMenuId(own) if own == id => " [own]".to_string(),
        Shortcut::OfMenuId(other) => format!(" [of {other}]"),
        Shortcut::OfCommand(command) => format!(" [command {command}]"),
        Shortcut::Fixed(accelerator) => format!(" [{accelerator}]"),
    }
}

/// `kind id label [shortcut] state`, nested submenus indented under their row.
fn render(rows: &[ContextRow]) -> String {
    let mut out = String::new();
    render_into(rows, 0, &mut out);
    out
}

fn render_into(rows: &[ContextRow], depth: usize, out: &mut String) {
    let pad = "  ".repeat(depth);
    for row in rows {
        let line = match row {
            ContextRow::Item(item) => format!(
                "item {} {}{}{}",
                item.id,
                label(&item.label),
                shortcut(item.shortcut, &item.id),
                if item.enabled { "" } else { " disabled" }
            ),
            ContextRow::Check { item, checked } => format!(
                "check {} {}{}{}",
                item.id,
                label(&item.label),
                shortcut(item.shortcut, &item.id),
                if *checked { " checked" } else { "" }
            ),
            ContextRow::Submenu { id, label: text, .. } => {
                format!("submenu {} {}", id.unwrap_or("-"), label(text))
            }
            ContextRow::Header(text) => format!("header \"{text}\""),
            ContextRow::Separator => "separator".to_string(),
            ContextRow::Slot(slot) => format!("slot {slot:?}"),
        };
        out.push_str(&pad);
        out.push_str(&line);
        out.push('\n');
        if let ContextRow::Submenu { rows, .. } = row {
            render_into(rows, depth + 1, out);
        }
    }
}

const EVERY_PANE_FACT: PaneFacts = PaneFacts {
    restrict_destination_actions: false,
    can_show_in_folder: false,
    can_open_terminal_here: true,
    can_share: true,
    can_tag: true,
    can_favorite: true,
    can_share_link: true,
};

fn facts<'a>(filename: &'a str, is_directory: bool, pane: PaneFacts) -> FileMenuFacts<'a> {
    FileMenuFacts {
        filename,
        is_directory,
        pane,
        image_index: ImageIndexMenuState::default(),
        target: TargetFacts {
            count: 1,
            count_text: None,
            size_text: Some("2.1 MB"),
        },
        same_kind: None,
    }
}

const MAC_FILE_ROW: &str = "\
header \"photo.jpg · 2.1 MB\"
separator
item open menu.file.open
slot OpenWith
item file_view menu.file.view [own]
item edit menu.context.edit [own]
separator
submenu - menu.context.selection
  item toggle_selection menu.context.toggleSelection [own]
  item select_all_files menu.select.all [own]
  item deselect_all menu.select.deselectAll [own]
  item select_same_kind menu.select.sameExtension(extension=jpg) [own]
  item invert_selection menu.select.invert [own]
  separator
  item select_files menu.select.files [own]
  item deselect_files menu.select.deselectFiles [own]
separator
slot TagColors
item file_copy menu.file.copy [own]
item file_move menu.file.move [own]
item file_duplicate menu.file.duplicate [own]
item file_compress menu.file.compress [own]
item rename menu.file.rename [own]
separator
item file_new_folder menu.file.newFolder [own]
item file_new_file menu.file.newFile [own]
separator
item file_delete menu.file.delete [own]
separator
item show_in_finder menu.file.showInFileManager [own]
item open_terminal_here menu.file.openTerminalHere [own]
slot Share
item copy_filename menu.context.copyNamed(name=photo.jpg) [own]
item copy_path menu.edit.copyPath [own]
submenu share_link_submenu menu.context.copyShareLink
  item share_link_seven_days menu.context.shareLinkSevenDays [own]
  item share_link_one_day menu.context.shareLinkOneDay [own]
  item share_link_one_hour menu.context.shareLinkOneHour [own]
slot GoogleDrive
slot ICloudEviction
slot FileProvider
separator
item get_info menu.file.getInfo [own]
item quick_look menu.file.quickLook
slot Services
";

#[test]
fn a_mac_file_row_reads_top_to_bottom() {
    let same_kind = SameKindTarget::SameExtension {
        extension: "jpg".to_string(),
    };
    let facts = FileMenuFacts {
        same_kind: Some(&same_kind),
        ..facts("photo.jpg", false, EVERY_PANE_FACT)
    };
    assert_eq!(render(&file_menu(&facts, Platform::MacOs)), MAC_FILE_ROW);
}

const LINUX_FOLDER_ROW: &str = "\
header \"Photos\"
separator
submenu - menu.context.selection
  item toggle_selection menu.context.toggleSelection [own]
  item select_all_files menu.select.all [own]
  item deselect_all menu.select.deselectAll [own]
  item select_same_kind menu.select.sameKind [own]
  item invert_selection menu.select.invert [own]
  separator
  item select_files menu.select.files [own]
  item deselect_files menu.select.deselectFiles [own]
separator
item file_copy menu.file.copy [own]
item file_move menu.file.move [own]
item file_duplicate menu.file.duplicate [own]
item file_compress menu.file.compress [own]
item rename menu.file.rename [own]
separator
item file_new_folder menu.file.newFolder [own]
item file_new_file menu.file.newFile [own]
separator
item file_delete menu.file.delete [own]
separator
item show_in_finder menu.file.showInFileManager [own]
item copy_filename menu.context.copyNamed(name=Photos) [own]
item copy_path menu.edit.copyPath [own]
separator
item favorites_add_context menu.context.addToFavorites
separator
item media_index_add_folder menu.mediaIndex.addFolder
item media_index_exclude_folder menu.mediaIndex.excludeFolder
";

/// Linux has none of the macOS rows and no slot (no OS to fill one); a folder has no Open group
/// and no share link, and gets favorites and the image-search group.
#[test]
fn a_linux_folder_row_has_no_mac_rows() {
    let facts = FileMenuFacts {
        image_index: ImageIndexMenuState {
            enabled: true,
            ..ImageIndexMenuState::default()
        },
        target: TargetFacts::default(),
        ..facts("Photos", true, EVERY_PANE_FACT)
    };
    assert_eq!(render(&file_menu(&facts, Platform::Linux)), LINUX_FOLDER_ROW);
}

/// The search-results snapshot: no Rename, Duplicate, or create items, plus Show in folder keyed
/// like Go to parent; Share and Services stay, Open terminal here greys.
#[test]
fn a_search_result_row_drops_the_destination_actions() {
    let pane = PaneFacts {
        restrict_destination_actions: true,
        can_show_in_folder: true,
        can_open_terminal_here: false,
        can_share: true,
        ..PaneFacts::default()
    };
    let rendered = render(&file_menu(&facts("a.txt", false, pane), Platform::MacOs));
    for absent in [
        "file_duplicate",
        "rename",
        "file_new_folder",
        "file_new_file",
        "TagColors",
        "share_link",
    ] {
        assert!(!rendered.contains(absent), "{absent} in\n{rendered}");
    }
    assert!(rendered.contains("item show_search_result_in_folder menu.context.showInFolder [of go_parent]\n"));
    assert!(rendered.contains("item open_terminal_here menu.file.openTerminalHere [own] disabled\n"));
    assert!(rendered.contains("slot Share\n") && rendered.contains("slot Services\n"));
}

/// A surface that can't answer anything (`PaneFacts::default()`) offers nothing that needs a yes.
#[test]
fn the_default_pane_offers_nothing_gated() {
    let rows = file_menu(&facts("a", true, PaneFacts::default()), Platform::MacOs);
    let rendered = render(&rows);
    for absent in [
        "slot TagColors",
        "slot Share",
        "slot Services",
        "favorites_add_context",
        "show_search_result",
    ] {
        assert!(!rendered.contains(absent), "{absent} in\n{rendered}");
    }
}

/// The order David settled on. A submenu is cheap to reorder by accident and expensive to notice.
#[test]
fn the_selection_submenu_splits_act_now_from_the_dialogs() {
    assert!(matches!(SELECTION_ROWS[0], SelectionRow::Item(TOGGLE_SELECTION_ID, _)));
    assert!(matches!(SELECTION_ROWS[5], SelectionRow::Separator));
}

/// Every row the crate builds, in every menu, on both platforms.
fn every_menu() -> Vec<Vec<ContextRow>> {
    let mut menus = Vec::new();
    for platform in [Platform::MacOs, Platform::Linux] {
        let image_index = ImageIndexMenuState {
            enabled: true,
            ..ImageIndexMenuState::default()
        };
        for is_directory in [false, true] {
            for pane in [
                EVERY_PANE_FACT,
                PaneFacts {
                    restrict_destination_actions: true,
                    can_show_in_folder: true,
                    ..EVERY_PANE_FACT
                },
            ] {
                menus.push(file_menu(
                    &FileMenuFacts {
                        image_index,
                        ..facts("f", is_directory, pane)
                    },
                    platform,
                ));
            }
        }
        for chosen in [false, true] {
            menus.push(file_menu(
                &FileMenuFacts {
                    image_index: ImageIndexMenuState { chosen, ..image_index },
                    ..facts("f", true, EVERY_PANE_FACT)
                },
                platform,
            ));
            menus.push(file_menu(
                &FileMenuFacts {
                    image_index: ImageIndexMenuState {
                        excluded: true,
                        ..image_index
                    },
                    ..facts("f", true, EVERY_PANE_FACT)
                },
                platform,
            ));
        }
        menus.push(viewer_text_menu(true, platform));
    }
    menus.push(parent_row_menu());
    menus.push(function_key_bar_menu());
    menus.push(breadcrumb_menu(Some(BreadcrumbVolume {
        name: "Backup",
        busy: false,
        word: DetachWord::Eject,
        offer_eject_all: true,
    })));
    menus.push(tab_menu(false, true, true));
    menus.push(network_host_menu(NetworkHostFacts {
        is_manual: true,
        is_saved: true,
        has_credentials: true,
        has_mounts: true,
    }));
    for pane in [PaneSide::Left, PaneSide::Right] {
        menus.push(date_sort_menu(pane, None));
    }
    menus.push(queue_row_menu(
        &QueueRowMenuAction::ALL.map(|action| QueueRowMenuEntry::Item { action, enabled: true }),
    ));
    menus
}

fn item_ids(rows: &[ContextRow], out: &mut Vec<String>) {
    for row in rows {
        match row {
            ContextRow::Item(item) | ContextRow::Check { item, .. } => out.push(item.id.clone()),
            ContextRow::Submenu { rows, .. } => item_ids(rows, out),
            ContextRow::Header(_) | ContextRow::Separator | ContextRow::Slot(_) => {}
        }
    }
}

/// ❗ A row whose pick resolves to nothing draws, looks clickable, and does nothing, on both hosts.
#[test]
fn every_row_the_crate_builds_has_a_click() {
    for menu in every_menu() {
        let mut ids = Vec::new();
        item_ids(&menu, &mut ids);
        for id in ids {
            assert!(context_click(&id).is_some(), "the `{id}` row does nothing when picked");
        }
    }
}

/// Every lane that isn't a command, read off its id.
#[test]
fn the_special_lanes_read_off_their_ids() {
    assert_eq!(context_click(FAVORITES_ADD_CONTEXT_ID), Some(ContextClick::AddFavorite));
    assert_eq!(context_click("tab_pin"), Some(ContextClick::Tab("tab_pin")));
    assert_eq!(
        context_click("eject_all_volumes"),
        Some(ContextClick::Volume(VolumeAction::EjectAll))
    );
    assert_eq!(
        context_click("network_host_forget_secret"),
        Some(ContextClick::NetworkHost(NetworkHostAction::ForgetSecret))
    );
    assert_eq!(
        context_click("viewer_context_copy"),
        Some(ContextClick::ViewerText(ViewerEditActionKind::Copy))
    );
    assert_eq!(
        context_click("queue_row_pause"),
        Some(ContextClick::QueueRow(QueueRowMenuAction::Pause))
    );
    assert_eq!(
        context_click("date_sort_menu_right_added"),
        Some(ContextClick::DateSort {
            pane: PaneSide::Right,
            column: SortColumn::Added
        })
    );
    assert!(matches!(
        context_click(FILE_COPY_ID),
        Some(ContextClick::Command {
            command_id: "file.copy",
            ..
        })
    ));
    // A slot's rows are the host's: they never classify here.
    assert_eq!(context_click("tag-color:1"), None);
    assert_eq!(context_click("open-with:com.apple.Preview"), None);
}

/// A click on the header does nothing, on top of the item being disabled: a stray mapping would
/// fire a real command from a line that is only a label.
#[test]
fn the_header_maps_to_no_command() {
    assert!(context_click(CONTEXT_MENU_TARGET_ID).is_none());
}

// --- Accelerators ---

fn shortcuts(pairs: &[(&str, &str)]) -> ContextMenuShortcuts {
    ContextMenuShortcuts::from_pairs(pairs.iter().copied())
}

/// Rebind Copy in Settings and the right-click menu says the new key.
#[test]
fn a_rebound_command_shows_the_new_key() {
    let map = shortcuts(&[("file.copy", "⌘⇧K")]);
    assert_eq!(
        map.for_menu_item(FILE_COPY_ID, Platform::MacOs).as_deref(),
        Some("Cmd+Shift+K")
    );
    assert_eq!(
        map.for_menu_item(FILE_COPY_ID, Platform::Linux).as_deref(),
        Some("Ctrl+Shift+K")
    );
}

/// A bare key stays a real label here, where the menu bar would refuse one: a popup accelerator
/// is never registered, so it can't swallow the key app-wide. Space is the only place the
/// toggle-selection shortcut is discoverable.
#[test]
fn a_bare_key_still_labels_a_popup_item() {
    let map = shortcuts(&[("selection.toggle", "Space")]);
    assert_eq!(
        map.for_menu_item(TOGGLE_SELECTION_ID, Platform::MacOs),
        Some("Space".to_string())
    );
}

/// An unbound command shows no label, and an item with no command behind it can't resolve one.
#[test]
fn an_unbound_or_commandless_item_shows_nothing() {
    let map = shortcuts(&[("file.copy", "F5"), ("favorites.add", "⌘D")]);
    assert_eq!(map.for_menu_item(FILE_VIEW_ID, Platform::MacOs), None);
    assert_eq!(map.for_menu_item(FAVORITES_ADD_CONTEXT_ID, Platform::MacOs), None);
}

/// The queue's single-letter keys show beside their items, bare, the way the row's chips print them.
#[test]
fn a_bare_queue_key_labels_its_item() {
    let map: ContextMenuShortcuts =
        serde_json::from_str(r#"{"queue.cancel": "C", "queue.toggleSelected": "Space", "queue.moveEarlier": "⌥↑"}"#)
            .expect("shortcuts parse");
    let resolve = |action: QueueRowMenuAction| Shortcut::OfCommand(action.command_id()).resolve(&map, Platform::MacOs);
    assert_eq!(resolve(QueueRowMenuAction::Cancel).as_deref(), Some("C"));
    assert_eq!(resolve(QueueRowMenuAction::Select).as_deref(), Some("Space"));
    assert_eq!(resolve(QueueRowMenuAction::MoveEarlier).as_deref(), Some("Alt+Up"));
}

// --- The small menus ---

#[test]
fn the_small_menus_read_top_to_bottom() {
    assert_eq!(
        render(&breadcrumb_menu(Some(BreadcrumbVolume {
            name: "Backup",
            busy: true,
            word: DetachWord::Eject,
            offer_eject_all: true,
        }))),
        "\
item copy_current_dir_path menu.breadcrumb.copyPath [own]
item eject_volume menu.volume.ejectBusy(name=Backup) disabled
item eject_all_volumes menu.volume.ejectAllBusy disabled
"
    );
    assert_eq!(
        render(&breadcrumb_menu(Some(BreadcrumbVolume {
            name: "Pixel 8",
            busy: false,
            word: DetachWord::Disconnect,
            offer_eject_all: false,
        }))),
        "\
item copy_current_dir_path menu.breadcrumb.copyPath [own]
item eject_volume menu.network.disconnect
"
    );
    assert_eq!(
        render(&tab_menu(true, false, false)),
        "\
item tab_pin menu.tab.unpinTab
separator
item tab_close_others menu.tab.closeOtherTabs disabled
item tab_close menu.tab.closeTab disabled
"
    );
    assert_eq!(
        render(&viewer_text_menu(false, Platform::MacOs)),
        "\
item viewer_context_copy menu.edit.copy [Cmd+C] disabled
item viewer_context_select_all menu.select.all [Cmd+A]
"
    );
    assert_eq!(
        render(&viewer_text_menu(true, Platform::Linux)),
        "\
item viewer_context_copy menu.edit.copy [Ctrl+C]
item viewer_context_select_all menu.select.all [Ctrl+A]
"
    );
    assert_eq!(
        render(&parent_row_menu()),
        "item favorites_add_context menu.context.addToFavorites\n"
    );
    assert_eq!(
        render(&function_key_bar_menu()),
        "item function_key_bar_hide menu.context.hideFunctionKeyBar\n"
    );
}

/// ❗ An SMB host's menu offers only what does something, in one group: no Disconnect with nothing
/// mounted (QA round 2).
#[test]
fn a_host_menu_offers_only_what_does_something() {
    let ids = |facts| {
        let mut ids = Vec::new();
        item_ids(&network_host_menu(facts), &mut ids);
        ids
    };
    assert_eq!(
        ids(NetworkHostFacts {
            is_manual: true,
            is_saved: true,
            ..NetworkHostFacts::default()
        }),
        ["network_host_edit", "network_host_forget_server"]
    );
    assert_eq!(
        ids(NetworkHostFacts {
            is_manual: true,
            is_saved: true,
            has_credentials: true,
            has_mounts: true,
        }),
        [
            "network_host_edit",
            "network_host_disconnect",
            "network_host_forget_secret",
            "network_host_forget_server"
        ]
    );
    assert!(ids(NetworkHostFacts::default()).is_empty());
}

#[test]
fn the_date_popup_checks_the_requested_date_only() {
    assert_eq!(
        render(&date_sort_menu(PaneSide::Right, Some(SortColumn::Created))),
        "\
check date_sort_menu_right_modified menu.sort.dateModified
check date_sort_menu_right_created menu.sort.dateCreated checked
check date_sort_menu_right_added menu.sort.dateAdded
"
    );
    // A Name sort, or a search pane in its ranked order, asked for no date.
    for requested in [Some(SortColumn::Name), Some(SortColumn::Size), None] {
        assert!(!render(&date_sort_menu(PaneSide::Left, requested)).contains("checked"));
    }
}

/// ❗ A bar id here would route the pick through `menu-sort`, sorting the FOCUSED pane; the pick
/// carries the pane the menu opened over instead.
#[test]
fn every_date_id_reads_back_as_its_pane_and_never_a_bar_id() {
    for pane in [PaneSide::Left, PaneSide::Right] {
        let mut ids = Vec::new();
        item_ids(&date_sort_menu(pane, None), &mut ids);
        for id in ids {
            assert!(![SORT_BY_MODIFIED_ID, SORT_BY_CREATED_ID, SORT_BY_ADDED_ID].contains(&id.as_str()));
            assert_eq!(menu_id_to_command(&id), None, "{id}");
            assert_eq!(date_sort_pick(&id).map(|(p, _)| p), Some(pane));
        }
    }
    assert_eq!(date_sort_pick("sort_by_added"), None);
    assert_eq!(date_sort_pick("date_sort_menu_left_name"), None);
}

#[test]
fn every_queue_action_round_trips_and_owns_its_id() {
    for action in QueueRowMenuAction::ALL {
        assert_eq!(QueueRowMenuAction::from_menu_id(action.menu_id()), Some(action));
        assert_eq!(menu_id_to_command(action.menu_id()), None, "{action:?}");
    }
    assert_eq!(QueueRowMenuAction::from_menu_id("tab_close"), None);
}

#[test]
fn queue_entries_read_the_window_s_wire_shape() {
    let entries: Vec<QueueRowMenuEntry> = serde_json::from_str(
        r#"[{"kind": "item", "action": "moveEarlier", "enabled": false}, {"kind": "separator"},
            {"kind": "check", "action": "select", "enabled": true, "checked": true}]"#,
    )
    .expect("entries parse");
    assert_eq!(
        render(&queue_row_menu(&entries)),
        "\
item queue_row_move_earlier menu.queue.moveUp [command queue.moveEarlier] disabled
separator
check queue_row_select menu.queue.select [command queue.toggleSelected] checked
"
    );
}

/// The pane facts read the frontend's `pane` payload, extra fields and all.
#[test]
fn pane_facts_read_the_frontend_s_payload() {
    let pane: PaneFacts = serde_json::from_str(
        r#"{"restrictDestinationActions": true, "listingId": "l1", "canShare": true, "canTag": false}"#,
    )
    .expect("pane facts parse");
    assert_eq!(
        pane,
        PaneFacts {
            restrict_destination_actions: true,
            can_share: true,
            ..PaneFacts::default()
        }
    );
}

#[test]
fn slots_appear_on_macos_only() {
    let linux = file_menu(&facts("a", false, EVERY_PANE_FACT), Platform::Linux);
    assert!(!linux.iter().any(|row| matches!(row, ContextRow::Slot(_))));
    let mac = file_menu(&facts("a", false, EVERY_PANE_FACT), Platform::MacOs);
    assert!(mac.contains(&ContextRow::Slot(Slot::Services)));
}

// --- The header line ---
// No locale pinning, and none needed: the words arrive over IPC, already in the user's language.

fn target<'a>(count: usize, count_text: Option<&'a str>, size_text: Option<&'a str>) -> TargetFacts<'a> {
    TargetFacts {
        count,
        count_text,
        size_text,
    }
}

#[test]
fn one_row_reads_its_name_then_its_size() {
    assert_eq!(
        header_label("photo.jpg", target(1, None, Some("2.1 MB"))),
        "photo.jpg · 2.1 MB"
    );
    // The folder case: no size to show, so no separator dangling after the name.
    assert_eq!(header_label("photo folder", target(1, None, None)), "photo folder");
}

#[test]
fn several_rows_read_as_the_count_the_frontend_worded_then_the_total() {
    assert_eq!(
        header_label("photo.jpg", target(3, Some("3 items"), Some("3.2 MB"))),
        "3 items · 3.2 MB"
    );
    // A selection holding a folder: the frontend can't total it honestly, so it sends `None`.
    assert_eq!(header_label("photo.jpg", target(3, Some("3 items"), None)), "3 items");
    // The count's WORDING is the frontend's; the count itself, which picks the branch, is ours.
    assert_eq!(
        header_label("photo.jpg", target(12_345, Some("12,345 items"), Some("9.9 GB"))),
        "12,345 items · 9.9 GB"
    );
}

/// `count` alone decides the branch, so a frontend that over-sends can't turn a name into a tally.
#[test]
fn one_row_shows_its_name_even_when_a_count_text_arrives() {
    assert_eq!(header_label("photo.jpg", target(1, Some("1 item"), None)), "photo.jpg");
}

/// No wording for several rows still says the menu acts on more than the row under the pointer:
/// ❗ never the primary filename, which restates the ambiguity this line exists to remove.
#[test]
fn several_rows_with_no_wording_still_say_how_many() {
    assert_eq!(header_label("photo.jpg", target(3, None, Some("3.2 MB"))), "3 · 3.2 MB");
    assert_eq!(header_label("photo.jpg", target(3, None, None)), "3");
}

/// An empty string is the same answer as `None` on either half, so no separator hangs off the end.
#[test]
fn an_empty_string_leaves_no_dangling_separator() {
    assert_eq!(header_label("photo.jpg", target(1, None, Some(""))), "photo.jpg");
    assert_eq!(header_label("photo.jpg", target(3, Some(""), None)), "3");
}

/// A pathological filename can't blow the menu's width; it middle-ellipsizes, keeping the extension.
#[test]
fn a_long_filename_is_truncated_in_the_middle() {
    let long = format!("{}.jpg", "a".repeat(80));
    let label = header_label(&long, target(1, None, Some("2.1 MB")));
    assert!(label.starts_with("aaa"), "{label}");
    assert!(label.contains('\u{2026}'), "{label}");
    assert!(label.ends_with(".jpg · 2.1 MB"), "{label}");
    let name = label.strip_suffix(" · 2.1 MB").expect("the size half is appended last");
    assert_eq!(name.chars().count(), FILENAME_MAX_CHARS);
}

/// Menu labels end with the U+2026 ellipsis character, never three periods: macOS kerns `...`
/// worse, and the desktop's SF Symbol pass finds an item by its title. The words live in the
/// catalog (guarded there); this keeps watch over any literal written in the menus' own source.
#[test]
fn the_menus_write_no_three_period_ellipsis() {
    for (name, source) in [
        ("file.rs", include_str!("file.rs")),
        ("popups.rs", include_str!("popups.rs")),
    ] {
        for (number, line) in source.lines().enumerate() {
            assert!(!line.contains("...\""), "{name}:{} ends a label with `...`", number + 1);
        }
    }
}

// --- truncate_for_menu_label ---

#[test]
fn truncate_passes_a_short_name_through() {
    assert_eq!(truncate_for_menu_label("hello.txt", 50), "hello.txt");
    assert_eq!(truncate_for_menu_label("", 50), "");
    let exactly_50 = "a".repeat(50);
    assert_eq!(truncate_for_menu_label(&exactly_50, 50), exactly_50);
}

#[test]
fn truncate_keeps_the_extension_of_a_long_name() {
    let long =
        "Obviously Awesome How to Nail Product Positioning so Customers Get It, Buy It, Love It Audiobook - m4b.epub";
    let truncated = truncate_for_menu_label(long, 50);
    assert!(truncated.chars().count() <= 50);
    assert!(truncated.ends_with(".epub"), "{truncated}");
    assert!(truncated.contains('\u{2026}'), "{truncated}");
    assert!(truncated.starts_with("Obviously"), "{truncated}");
}

#[test]
fn truncate_splits_a_name_without_an_extension_around_the_ellipsis() {
    let truncated = truncate_for_menu_label(&"a".repeat(100), 50);
    assert!(truncated.chars().count() <= 50);
    let parts: Vec<&str> = truncated.split('\u{2026}').collect();
    assert_eq!(parts.len(), 2);
    assert!(!parts[0].is_empty() && !parts[1].is_empty());
}

/// It counts chars, never splitting mid-byte.
#[test]
fn truncate_counts_chars_not_bytes() {
    let name = "🎉".repeat(40) + ".txt";
    let truncated = truncate_for_menu_label(&name, 20);
    assert!(truncated.chars().count() <= 20);
    assert!(truncated.contains('\u{2026}'));
    assert!(truncated.ends_with(".txt"));
    let accented = "ÁrvíztűrőTükörfúrógép".repeat(5);
    assert!(truncate_for_menu_label(&accented, 15).chars().count() <= 15);
}

/// An extension longer than the tail's budget falls back to a plain middle ellipsis.
#[test]
fn truncate_handles_a_budget_smaller_than_the_extension() {
    let truncated = truncate_for_menu_label("stem.verylongextension", 10);
    assert!(truncated.chars().count() <= 10);
    assert!(truncated.contains('\u{2026}'));
    assert_eq!(truncate_for_menu_label("anything.txt", 1), "\u{2026}");
    assert_eq!(truncate_for_menu_label("anything.txt", 0), "");
}
