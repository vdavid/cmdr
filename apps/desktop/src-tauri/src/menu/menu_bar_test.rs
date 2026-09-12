//! Pins the menu bar each platform builds (`menu_bar.rs`), on every host.
//!
//! A real `muda::Menu` can't be built in a unit test (it panics off the main thread), so these read
//! the spec `menu_bar_builder.rs` consumes. `render` writes one line per row, so a change to either
//! bar shows up here as a diff to review.

use std::collections::HashSet;

use super::MENU_BAR;
use crate::menu::ViewMode;
use crate::menu::menu_items::APP_MENU_TITLE;
use crate::menu::menu_spec::{
    CheckRole, EntryKind, Label, Pane, Platform, Predefined, SubmenuRole, SubmenuSpec, Tracking,
};

const PLATFORMS: [Platform; 2] = [Platform::MacOs, Platform::Linux];

/// `position kind id label [accelerator] state`, nested submenus indented under their row.
const MACOS_MENU_BAR: &str = "\
menu Cmdr id=menu_app
  0 item about menu.app.about untracked
  1 item acknowledgements menu.app.acknowledgements untracked
  2 item enter_license_key menu.app.licenseEnter|menu.app.licenseDetails untracked
  3 item check_for_updates menu.app.checkForUpdates tracked
  4 item changelog menu.app.changelog tracked
  5 item open_onboarding menu.app.onboarding tracked
  6 separator
  7 item settings menu.app.settings [Cmd+,] untracked
  8 separator
  9 predefined services menu.app.services
  10 separator
  11 predefined hide menu.app.hide
  12 predefined hide_others menu.app.hideOthers
  13 predefined show_all menu.app.showAll
  14 separator
  15 predefined quit menu.app.quit
menu menu.bar.file id=menu_file
  0 item open menu.file.open tracked
  1 item file_view menu.file.view [F3] tracked
  2 item edit menu.file.edit [F4] tracked
  3 separator
  4 item file_copy menu.file.copy [F5] tracked
  5 item file_move menu.file.move [F6] tracked
  6 item file_duplicate menu.file.duplicate [Cmd+D] tracked
  7 item file_compress menu.file.compress [Alt+F5] tracked
  8 item file_new_folder menu.file.newFolder [F7] tracked
  9 item file_new_file menu.file.newFile [Shift+F4] tracked
  10 item file_delete menu.file.delete [F8] tracked
  11 item file_delete_permanently menu.file.deletePermanently [Shift+F8] tracked
  12 separator
  13 item rename menu.file.rename [F2] tracked
  14 separator
  15 item show_in_finder menu.file.showInFinder [Opt+Cmd+O] tracked
  16 item open_terminal_here menu.file.openTerminalHere [Alt+Cmd+T] tracked
  17 item get_info menu.file.getInfo [Cmd+I] tracked
  18 item quick_look menu.file.quickLook [Shift+Space] tracked
menu menu.bar.edit id=menu_edit
  0 predefined undo menu.edit.undo
  1 predefined redo menu.edit.redo
  2 separator
  3 item edit_cut menu.edit.cut [Cmd+X] tracked
  4 item edit_copy menu.edit.copy [Cmd+C] tracked
  5 item edit_paste menu.edit.paste [Cmd+V] tracked
  6 item edit_paste_move menu.edit.moveHere [Alt+Cmd+V] tracked
  7 separator
  8 item copy_path menu.edit.copyPath [Cmd+Opt+C] tracked
  9 item copy_filename menu.edit.copyFilename tracked
  10 separator
  11 item search_files menu.edit.searchFiles [Cmd+F] tracked
menu menu.bar.select id=menu_select
  0 item select_all_files menu.select.all [Cmd+A] tracked
  1 item deselect_all menu.select.deselectAll [Cmd+Shift+A] tracked
  2 item invert_selection menu.select.invert tracked
  3 separator
  4 item select_files menu.select.files tracked
  5 item deselect_files menu.select.deselectFiles tracked
menu menu.bar.view id=menu_view
  0 submenu menu.view.leftPane pane:left
      0 check view_mode_full_left menu.view.fullView [Cmd+1] view-mode:left:full
      1 check view_mode_brief_left menu.view.briefView [Cmd+2] view-mode:left:brief
  1 submenu menu.view.rightPane pane:right
      0 check view_mode_full_right menu.view.fullView view-mode:right:full
      1 check view_mode_brief_right menu.view.briefView view-mode:right:brief
  2 separator
  3 check show_hidden_files menu.view.showHiddenFiles [Cmd+Shift+.] show-hidden-files
  4 submenu menu.view.sortBy id=menu_sort_by sort-by
      0 item sort_by_name menu.sort.name [Cmd+3] tracked
      1 item sort_by_extension menu.sort.extension [Cmd+4] tracked
      2 item sort_by_modified menu.sort.dateModified [Cmd+5] tracked
      3 item sort_by_size menu.sort.size [Cmd+6] tracked
      4 item sort_by_created menu.sort.dateCreated untracked
      5 separator
      6 item sort_ascending menu.sort.ascending untracked
      7 item sort_descending menu.sort.descending untracked
  5 submenu menu.view.zoom nested
      0 item view_zoom_75 menu.zoom.percent75 untracked
      1 item view_zoom_100 menu.zoom.percent100 [Cmd+0] untracked
      2 item view_zoom_125 menu.zoom.percent125 untracked
      3 item view_zoom_150 menu.zoom.percent150 untracked
      4 separator
      5 item view_zoom_in menu.zoom.in [Cmd+Plus] untracked
      6 item view_zoom_out menu.zoom.out [Cmd+Minus] untracked
  6 separator
  7 item switch_pane menu.view.switchPane [Tab] tracked
  8 item swap_panes menu.view.swapPanes [Cmd+U] tracked
  9 separator
  10 item command_palette menu.view.commandPalette [Cmd+Shift+P] tracked
  11 item queue_show menu.view.operationQueue [Cmd+Alt+Q] tracked
  12 item operation_log menu.view.operationLog [Cmd+Alt+L] tracked
  13 item suggested_ops menu.view.suggestedOps tracked
  14 item ask_cmdr menu.view.askCmdr [Cmd+Alt+A] tracked
menu menu.bar.go id=menu_go
  0 item go_back menu.go.back [Cmd+[] tracked
  1 item go_forward menu.go.forward [Cmd+]] tracked
  2 separator
  3 item go_parent menu.go.parentFolder [Cmd+Up] tracked
  4 item go_home menu.go.home [Shift+Cmd+H] tracked
  5 separator
  6 item go_to_path menu.go.goToPath [Cmd+G] tracked
  7 item go_latest_download menu.go.goToLatestDownload [Cmd+J] tracked
  8 separator
  9 item favorites_add menu.go.addToFavorites tracked
menu menu.bar.servers id=menu_servers
  0 item servers_connect menu.servers.connectToServer [Cmd+K] tracked
  1 item servers_show menu.servers.showServers tracked
menu menu.bar.tab id=menu_tab
  0 item new_tab menu.tab.newTab [Cmd+T] tracked
  1 item close_tab menu.tab.closeTab [Cmd+W] tracked
  2 item reopen_closed_tab menu.tab.reopenClosedTab [Cmd+Shift+T] disabled tracked
  3 separator
  4 item next_tab menu.tab.nextTab [Ctrl+Tab] tracked
  5 item prev_tab menu.tab.previousTab [Ctrl+Shift+Tab] tracked
  6 separator
  7 item pin_tab_menu menu.tab.pinTab pin-tab
  8 item close_other_tabs menu.tab.closeOtherTabs tracked
menu menu.bar.window id=menu_window
  0 predefined minimize menu.window.minimize
  1 predefined maximize menu.window.zoom
menu menu.bar.help id=menu_help
  0 item help_shortcuts menu.help.keyboardShortcuts tracked
  1 separator
  2 item help_whats_new menu.help.whatsNew tracked
  3 item help_send_feedback menu.help.sendFeedback tracked
  4 item help_send_error_report menu.help.sendErrorReport tracked
";

const LINUX_MENU_BAR: &str = "\
menu menu.bar.file
  0 item open menu.file.open tracked
  1 item file_view menu.file.view tracked
  2 item edit menu.file.edit tracked
  3 separator
  4 item file_copy menu.file.copy tracked
  5 item file_move menu.file.move tracked
  6 item file_duplicate menu.file.duplicate tracked
  7 item file_compress menu.file.compress tracked
  8 item file_new_folder menu.file.newFolder tracked
  9 item file_new_file menu.file.newFile tracked
  10 item file_delete menu.file.delete tracked
  11 item file_delete_permanently menu.file.deletePermanently tracked
  12 separator
  13 item rename menu.file.rename tracked
  14 separator
  15 item show_in_finder menu.file.showInFileManager [Alt+Ctrl+O] tracked
  16 item get_info menu.file.getInfo [Cmd+I] tracked
  17 item quick_look menu.file.quickLook tracked
menu menu.bar.edit
  0 item edit_cut menu.edit.cut [Ctrl+X] tracked
  1 item edit_copy menu.edit.copy [Ctrl+C] tracked
  2 item edit_paste menu.edit.paste [Ctrl+V] tracked
  3 item edit_paste_move menu.edit.moveHere [Ctrl+Alt+V] tracked
  4 separator
  5 item copy_path menu.edit.copyPath [Ctrl+Alt+C] tracked
  6 item copy_filename menu.edit.copyFilename tracked
  7 separator
  8 item search_files menu.edit.searchFiles [Cmd+F] tracked
  9 separator
  10 item settings menu.app.settings [Cmd+,] tracked
  11 item enter_license_key menu.app.licenseEnter|menu.app.licenseDetails untracked
  12 item check_for_updates menu.app.checkForUpdates tracked
  13 item changelog menu.app.changelog tracked
menu menu.bar.select
  0 item select_all_files menu.select.all [Cmd+A] tracked
  1 item deselect_all menu.select.deselectAll [Cmd+Shift+A] tracked
  2 item invert_selection menu.select.invert tracked
  3 separator
  4 item select_files menu.select.files tracked
  5 item deselect_files menu.select.deselectFiles tracked
menu menu.bar.view
  0 submenu menu.view.leftPane pane:left
      0 check view_mode_full_left menu.view.fullView [Cmd+1] view-mode:left:full
      1 check view_mode_brief_left menu.view.briefView [Cmd+2] view-mode:left:brief
  1 submenu menu.view.rightPane pane:right
      0 check view_mode_full_right menu.view.fullView view-mode:right:full
      1 check view_mode_brief_right menu.view.briefView view-mode:right:brief
  2 separator
  3 check show_hidden_files menu.view.showHiddenFiles [Cmd+Shift+.] show-hidden-files
  4 submenu menu.view.sortBy id=menu_sort_by sort-by
      0 item sort_by_name menu.sort.name [Cmd+3] tracked
      1 item sort_by_extension menu.sort.extension [Cmd+4] tracked
      2 item sort_by_modified menu.sort.dateModified [Cmd+5] tracked
      3 item sort_by_size menu.sort.size [Cmd+6] tracked
      4 item sort_by_created menu.sort.dateCreated untracked
      5 separator
      6 item sort_ascending menu.sort.ascending untracked
      7 item sort_descending menu.sort.descending untracked
  5 submenu menu.view.zoom nested
      0 item view_zoom_75 menu.zoom.percent75 untracked
      1 item view_zoom_100 menu.zoom.percent100 [Cmd+0] untracked
      2 item view_zoom_125 menu.zoom.percent125 untracked
      3 item view_zoom_150 menu.zoom.percent150 untracked
      4 separator
      5 item view_zoom_in menu.zoom.in untracked
      6 item view_zoom_out menu.zoom.out untracked
  6 separator
  7 item switch_pane menu.view.switchPane tracked
  8 item swap_panes menu.view.swapPanes [Cmd+U] tracked
  9 separator
  10 item command_palette menu.view.commandPalette [Cmd+Shift+P] tracked
  11 item queue_show menu.view.operationQueue [Cmd+Alt+Q] tracked
  12 item operation_log menu.view.operationLog [Cmd+Alt+L] tracked
  13 item suggested_ops menu.view.suggestedOps tracked
  14 item ask_cmdr menu.view.askCmdr [Cmd+Alt+A] tracked
menu menu.bar.go
  0 item go_back menu.go.back [Cmd+[] tracked
  1 item go_forward menu.go.forward [Cmd+]] tracked
  2 separator
  3 item go_parent menu.go.parentFolder [Cmd+Up] tracked
  4 item go_home menu.go.home [Shift+Cmd+H] tracked
  5 separator
  6 item go_to_path menu.go.goToPath [Cmd+G] tracked
  7 item go_latest_download menu.go.goToLatestDownload [Cmd+J] tracked
  8 separator
  9 item favorites_add menu.go.addToFavorites tracked
menu menu.bar.servers
  0 item servers_connect menu.servers.connectToServer [Cmd+K] tracked
  1 item servers_show menu.servers.showServers tracked
menu menu.bar.tab
  0 item new_tab menu.tab.newTab [Cmd+T] tracked
  1 item close_tab menu.tab.closeTab [Cmd+W] tracked
  2 item reopen_closed_tab menu.tab.reopenClosedTab [Cmd+Shift+T] disabled tracked
  3 separator
  4 item next_tab menu.tab.nextTab [Ctrl+Tab] tracked
  5 item prev_tab menu.tab.previousTab [Ctrl+Shift+Tab] tracked
  6 separator
  7 item pin_tab_menu menu.tab.pinTab pin-tab
  8 item close_other_tabs menu.tab.closeOtherTabs tracked
menu menu.bar.help
  0 item about menu.app.about tracked
  1 item acknowledgements menu.app.acknowledgements untracked
  2 separator
  3 item help_shortcuts menu.help.keyboardShortcuts tracked
  4 item help_whats_new menu.help.whatsNew tracked
  5 item help_send_feedback menu.help.sendFeedback tracked
  6 item help_send_error_report menu.help.sendErrorReport tracked
";

#[test]
fn the_macos_menu_bar() {
    assert_menu_bar(Platform::MacOs, MACOS_MENU_BAR);
}

#[test]
fn the_linux_menu_bar() {
    assert_menu_bar(Platform::Linux, LINUX_MENU_BAR);
}

/// The builder hands each of these to its own `MenuState` field and panics when one is missing, so
/// every platform's bar has to build each exactly once.
#[test]
fn every_handle_menu_state_keeps_is_built_exactly_once() {
    for platform in PLATFORMS {
        let mut kept: Vec<String> = every_entry(platform)
            .into_iter()
            .filter_map(|entry| match entry {
                EntryKind::Item(item) if item.tracking == Tracking::PinTab => Some("pin-tab".to_string()),
                EntryKind::Check(check) => Some(check_role(check.role)),
                EntryKind::Submenu(nested) if nested.role != SubmenuRole::Plain => Some(submenu_role(nested.role)),
                _ => None,
            })
            .collect();
        kept.sort();
        assert_eq!(
            kept,
            [
                "pane:left",
                "pane:right",
                "pin-tab",
                "show-hidden-files",
                "sort-by",
                "view-mode:left:brief",
                "view-mode:left:full",
                "view-mode:right:brief",
                "view-mode:right:full",
            ],
            "on {platform:?}"
        );
    }
}

/// `rebuild_view_mode_items` reinserts a pane's two items by index, so Full has to sit at 0 and
/// Brief at 1, with nothing else in the submenu.
#[test]
fn a_panes_full_item_sits_at_0_and_brief_at_1() {
    for platform in PLATFORMS {
        let mut panes = 0;
        for entry in every_entry(platform) {
            let EntryKind::Submenu(nested) = entry else {
                continue;
            };
            let SubmenuRole::Pane(pane) = nested.role else {
                continue;
            };
            panes += 1;
            let roles: Vec<Option<CheckRole>> = nested
                .entries_on(platform)
                .map(|entry| match entry {
                    EntryKind::Check(check) => Some(check.role),
                    _ => None,
                })
                .collect();
            assert_eq!(
                roles,
                [
                    Some(CheckRole::ViewMode(pane, ViewMode::Full)),
                    Some(CheckRole::ViewMode(pane, ViewMode::Brief)),
                ],
                "the {pane:?} pane submenu on {platform:?}"
            );
        }
        assert_eq!(panes, 2, "pane submenus on {platform:?}");
    }
}

/// `MenuState.items` is keyed by ID, so a second item with an ID already taken would silently
/// replace the first one's registration.
#[test]
fn no_two_items_share_an_id() {
    for platform in PLATFORMS {
        let ids: Vec<&str> = every_entry(platform)
            .into_iter()
            .filter_map(|entry| match entry {
                EntryKind::Item(item) => Some(item.id),
                EntryKind::Check(check) => Some(check.id),
                _ => None,
            })
            .collect();
        let unique: HashSet<&str> = ids.iter().copied().collect();
        assert_eq!(unique.len(), ids.len(), "a duplicated item ID on {platform:?}: {ids:?}");
    }
}

fn assert_menu_bar(platform: Platform, pinned: &str) {
    let built = render(platform);
    if built == pinned {
        return;
    }
    let line = built
        .lines()
        .zip(pinned.lines())
        .position(|(built, pinned)| built != pinned)
        .unwrap_or_else(|| built.lines().count().min(pinned.lines().count()));
    panic!(
        "the {platform:?} menu bar no longer matches the pinned one, from line {}:\n  pinned: {}\n  built:  {}\n\
         If the change is intended, update the pinned text. The whole bar as built:\n{built}",
        line + 1,
        pinned.lines().nth(line).unwrap_or("(nothing)"),
        built.lines().nth(line).unwrap_or("(nothing)"),
    );
}

fn render(platform: Platform) -> String {
    let mut out = String::new();
    for bar_menu in MENU_BAR.iter().filter(|bar_menu| bar_menu.is_on(platform)) {
        let spec = &bar_menu.submenu;
        out.push_str(&format!("menu {}{}\n", label(spec.title, platform), id(spec, platform)));
        render_entries(spec, platform, "  ", &mut out);
    }
    out
}

fn render_entries(spec: &SubmenuSpec, platform: Platform, indent: &str, out: &mut String) {
    for (position, entry) in spec.entries_on(platform).enumerate() {
        let line = match entry {
            EntryKind::Item(item) => {
                let disabled = if item.enabled { "" } else { " disabled" };
                let tracking = match item.tracking {
                    Tracking::Tracked => "tracked",
                    Tracking::Untracked => "untracked",
                    Tracking::PinTab => "pin-tab",
                };
                format!(
                    "item {} {}{}{disabled} {tracking}",
                    item.id,
                    label(item.label, platform),
                    accelerator(item.accelerator.on(platform)),
                )
            }
            EntryKind::Check(check) => format!(
                "check {} {}{} {}",
                check.id,
                label(check.label, platform),
                accelerator(check.accelerator.on(platform)),
                check_role(check.role),
            ),
            EntryKind::Separator => "separator".to_string(),
            EntryKind::Predefined(kind, text) => {
                format!("predefined {} {}", predefined_name(*kind), label(*text, platform))
            }
            EntryKind::Submenu(nested) => format!(
                "submenu {}{} {}",
                label(nested.title, platform),
                id(nested, platform),
                submenu_role(nested.role),
            ),
        };
        out.push_str(&format!("{indent}{position} {line}\n"));
        if let EntryKind::Submenu(nested) = entry {
            render_entries(nested, platform, &format!("{indent}    "), out);
        }
    }
}

/// Every row `platform` builds, nested ones included, in build order.
fn every_entry(platform: Platform) -> Vec<&'static EntryKind> {
    fn walk(spec: &'static SubmenuSpec, platform: Platform, out: &mut Vec<&'static EntryKind>) {
        for entry in spec.entries_on(platform) {
            out.push(entry);
            if let EntryKind::Submenu(nested) = entry {
                walk(nested, platform, out);
            }
        }
    }
    let mut out = Vec::new();
    for bar_menu in MENU_BAR.iter().filter(|bar_menu| bar_menu.is_on(platform)) {
        walk(&bar_menu.submenu, platform, &mut out);
    }
    out
}

/// The catalog key, or both keys joined by `|` for a label that depends on the license.
fn label(label: Label, platform: Platform) -> String {
    let key = |has_existing_license| {
        label
            .catalog_key(platform, has_existing_license)
            .unwrap_or(APP_MENU_TITLE)
    };
    if key(false) == key(true) {
        key(false).to_string()
    } else {
        format!("{}|{}", key(false), key(true))
    }
}

fn id(spec: &SubmenuSpec, platform: Platform) -> String {
    spec.id.on(platform).map_or_else(String::new, |id| format!(" id={id}"))
}

fn accelerator(accelerator: Option<&str>) -> String {
    accelerator.map_or_else(String::new, |accelerator| format!(" [{accelerator}]"))
}

fn check_role(role: CheckRole) -> String {
    match role {
        CheckRole::ShowHiddenFiles => "show-hidden-files".to_string(),
        CheckRole::ViewMode(pane, mode) => {
            let mode = match mode {
                ViewMode::Full => "full",
                ViewMode::Brief => "brief",
            };
            format!("view-mode:{}:{mode}", pane_name(pane))
        }
    }
}

fn submenu_role(role: SubmenuRole) -> String {
    match role {
        SubmenuRole::Plain => "nested".to_string(),
        SubmenuRole::SortBy => "sort-by".to_string(),
        SubmenuRole::Pane(pane) => format!("pane:{}", pane_name(pane)),
    }
}

fn pane_name(pane: Pane) -> &'static str {
    match pane {
        Pane::Left => "left",
        Pane::Right => "right",
    }
}

fn predefined_name(kind: Predefined) -> &'static str {
    match kind {
        Predefined::Services => "services",
        Predefined::Hide => "hide",
        Predefined::HideOthers => "hide_others",
        Predefined::ShowAll => "show_all",
        Predefined::Quit => "quit",
        Predefined::Undo => "undo",
        Predefined::Redo => "redo",
        Predefined::Minimize => "minimize",
        Predefined::Maximize => "maximize",
    }
}
