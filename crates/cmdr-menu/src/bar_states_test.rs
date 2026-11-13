//! Pins every menu-bar row's enabled state and command, on each platform, under the input sets
//! that matter: the explorer in front, another window in front, a dialog up, and a fresh launch.
//!
//! Pinned in the desktop app before the bar and its fold moved here, and moved verbatim, so the
//! tables are the bar a user saw then: `entry_enabled` applies the same handle rule the desktop's
//! `apply_menu_item_states` does.

use std::collections::HashSet;

use super::*;
use crate::bar::every_entry;
use crate::spec::{EntryKind, Platform};

/// The four input sets, in the table's column order.
fn scenarios<'a>(dialog: &'a HashSet<String>, nothing: &'a HashSet<String>) -> [MenuItemInputs<'a>; 4] {
    let explorer = MenuItemInputs {
        explorer_menu_active: true,
        file_operations_blocked: false,
        open_terminal_here_enabled: true,
        reopen_closed_tab_enabled: true,
        undo_trash_available: true,
        refused: nothing,
    };
    [
        explorer,
        MenuItemInputs {
            explorer_menu_active: false,
            ..explorer
        },
        MenuItemInputs {
            refused: dialog,
            file_operations_blocked: true,
            ..explorer
        },
        MenuItemInputs {
            open_terminal_here_enabled: false,
            reopen_closed_tab_enabled: false,
            undo_trash_available: false,
            ..explorer
        },
    ]
}

/// A realistic dialog-gate answer: a sample of `BLOCKED_BY_DIALOGS`, with one command per kind of
/// handle (a tracked item, an app-scoped item, close tab, pin tab, both check families, a sort column,
/// and the untracked rows: the app menu's and a Zoom preset).
fn dialog_refusals() -> HashSet<String> {
    [
        "file.copy",
        "file.view",
        "app.about",
        "tab.close",
        "tab.togglePin",
        "view.showHidden",
        "view.setMode",
        "sort.byName",
        "nav.goToPath",
        // The app menu's rows and a Zoom preset, which macOS builds untracked.
        "app.settings",
        "app.licenseKey",
        "app.acknowledgements",
        "view.zoom.set100",
    ]
    .iter()
    .map(|id| (*id).to_string())
    .collect()
}

fn row_name(entry: &EntryKind) -> String {
    match entry {
        EntryKind::Item(item) => item.id.to_string(),
        EntryKind::Check(check) => check.id.to_string(),
        EntryKind::Submenu(nested) => format!("submenu:{}", nested.title.catalog_key(false).unwrap_or("?")),
        EntryKind::Separator | EntryKind::Predefined(..) => String::new(),
    }
}

fn command(entry: &EntryKind) -> String {
    let id = match entry {
        EntryKind::Item(item) => item.id,
        EntryKind::Check(check) => check.id,
        _ => return "-".to_string(),
    };
    match menu_id_to_command(id) {
        Some((command_id, CommandScope::App)) => format!("{command_id}@app"),
        Some((command_id, CommandScope::FileScoped)) => format!("{command_id}@file"),
        None => "-".to_string(),
    }
}

/// `id command explorer other dialog fresh`, one row per item, check, and submenu, 1 = enabled.
fn render(platform: Platform) -> String {
    let refusals = dialog_refusals();
    let nothing = HashSet::new();
    let inputs = scenarios(&refusals, &nothing);
    let mut out = String::new();
    for entry in every_entry(platform) {
        let verdicts: Vec<&str> = inputs
            .iter()
            .filter_map(|inputs| entry_enabled(entry, inputs))
            .map(|enabled| if enabled { "1" } else { "0" })
            .collect();
        if verdicts.is_empty() {
            continue;
        }
        out.push_str(&format!(
            "{} {} {}\n",
            row_name(entry),
            command(entry),
            verdicts.join(" ")
        ));
    }
    out
}

fn assert_pinned(platform: Platform, pinned: &str) {
    let built = render(platform);
    assert!(
        built == pinned,
        "the {platform:?} bar's states no longer match the pinned table. Whole table as built:\n{built}"
    );
}

/// Columns: `id command-or-- explorer other dialog fresh` (`scenarios`), 1 = enabled.
const MACOS_STATES: &str = "\
about app.about@app 1 1 0 1
acknowledgements app.acknowledgements@app 1 1 0 1
enter_license_key app.licenseKey@app 1 1 0 1
check_for_updates app.checkForUpdates@app 1 1 1 1
changelog help.whatsNew@app 1 1 1 1
open_onboarding cmdr.openOnboarding@app 1 1 1 1
cli_install app.toggleCliCommand@app 1 1 1 1
settings app.settings@app 1 1 0 1
open nav.open@file 1 0 1 1
file_view file.view@file 1 0 0 1
edit file.edit@file 1 0 1 1
file_copy file.copy@file 1 0 0 1
file_move file.move@file 1 0 0 1
file_duplicate file.duplicate@file 1 0 1 1
file_compress file.compress@file 1 0 0 1
file_new_folder file.newFolder@file 1 0 0 1
file_new_file file.newFile@file 1 0 0 1
file_delete file.delete@file 1 0 0 1
file_delete_permanently file.deletePermanently@file 1 0 0 1
rename file.rename@file 1 0 0 1
multi_rename file.multiRename@file 1 0 1 1
show_in_finder file.showInFinder@file 1 0 1 1
open_terminal_here file.openTerminalHere@file 1 0 1 0
get_info file.getInfo@file 1 0 1 1
tell_me_about file.tellMeAbout@file 1 0 1 1
quick_look file.quickLook@file 1 0 1 1
undo_trash edit.undoTrash@file 1 0 1 0
edit_cut edit.cut@app 1 1 1 1
edit_copy edit.copy@app 1 1 1 1
edit_paste edit.paste@app 1 1 1 1
edit_paste_move edit.pasteAsMove@file 1 0 0 1
copy_path file.copyPath@file 1 0 1 1
copy_filename file.copyFilename@file 1 0 1 1
search_files search.open@file 1 0 1 1
select_all_files selection.selectAll@file 1 0 1 1
deselect_all selection.deselectAll@file 1 0 1 1
select_same_kind selection.selectSameKind@file 1 0 1 1
invert_selection selection.invert@file 1 0 1 1
select_files selection.selectFiles@file 1 0 1 1
deselect_files selection.deselectFiles@file 1 0 1 1
compare_directories selection.compareDirectories@file 1 0 1 1
submenu:menu.view.leftPane - 1 0 0 1
view_mode_full_left - 1 0 0 1
view_mode_brief_left - 1 0 0 1
submenu:menu.view.rightPane - 1 0 0 1
view_mode_full_right - 1 0 0 1
view_mode_brief_right - 1 0 0 1
show_hidden_files - 1 0 0 1
submenu:menu.view.sortBy - 1 0 1 1
sort_by_name sort.byName@file 1 0 0 1
sort_by_extension sort.byExtension@file 1 0 1 1
sort_by_modified sort.byModified@file 1 0 1 1
sort_by_size sort.bySize@file 1 0 1 1
sort_by_created - 1 1 1 1
sort_by_added - 1 1 1 1
sort_ascending - 1 1 1 1
sort_descending - 1 1 1 1
submenu:menu.view.zoom - 1 1 1 1
view_zoom_75 view.zoom.set75@app 1 1 1 1
view_zoom_100 view.zoom.set100@app 1 1 0 1
view_zoom_125 view.zoom.set125@app 1 1 1 1
view_zoom_150 view.zoom.set150@app 1 1 1 1
view_zoom_in view.zoom.in@app 1 1 1 1
view_zoom_out view.zoom.out@app 1 1 1 1
switch_pane pane.switch@file 1 0 1 1
swap_panes pane.swap@file 1 0 1 1
clone_pane pane.clone@file 1 0 1 1
calculate_folder_sizes view.calculateFolderSizes@file 1 0 1 1
command_palette app.commandPalette@file 1 0 1 1
queue_show queue.show@app 1 1 1 1
operation_log log.operationLog@app 1 1 1 1
suggested_ops suggestedOps.show@app 1 1 1 1
ask_cmdr askCmdr.toggle@app 1 1 1 1
go_back nav.back@file 1 0 1 1
go_forward nav.forward@file 1 0 1 1
go_parent nav.parent@file 1 0 1 1
go_root nav.goToRoot@file 1 0 1 1
go_home nav.goHome@file 1 0 1 1
go_to_path nav.goToPath@file 1 0 0 1
go_latest_download downloads.goToLatest@file 1 0 1 1
favorites_add favorites.add@file 1 0 1 1
favorites_open favorites.open@file 1 0 1 1
servers_connect servers.connect@file 1 0 1 1
servers_show servers.show@file 1 0 1 1
new_tab tab.new@file 1 0 1 1
close_tab tab.close@file 1 1 0 1
reopen_closed_tab tab.reopen@file 1 0 1 0
next_tab tab.next@file 1 0 1 1
prev_tab tab.prev@file 1 0 1 1
pin_tab_menu tab.togglePin@file 1 0 0 1
close_other_tabs tab.closeOthers@file 1 0 1 1
help_shortcuts help.openShortcuts@app 1 1 1 1
help_accessibility help.accessibility@app 1 1 1 1
help_whats_new help.whatsNew@app 1 1 1 1
help_send_feedback feedback.send@app 1 1 1 1
help_debug_log help.viewDebugLog@app 1 1 1 1
help_send_error_report help.sendErrorReport@app 1 1 1 1
";

const LINUX_STATES: &str = "\
open nav.open@file 1 0 1 1
file_view file.view@file 1 0 0 1
edit file.edit@file 1 0 1 1
file_copy file.copy@file 1 0 0 1
file_move file.move@file 1 0 0 1
file_duplicate file.duplicate@file 1 0 1 1
file_compress file.compress@file 1 0 0 1
file_new_folder file.newFolder@file 1 0 0 1
file_new_file file.newFile@file 1 0 0 1
file_delete file.delete@file 1 0 0 1
file_delete_permanently file.deletePermanently@file 1 0 0 1
rename file.rename@file 1 0 0 1
multi_rename file.multiRename@file 1 0 1 1
show_in_finder file.showInFinder@file 1 0 1 1
tell_me_about file.tellMeAbout@file 1 0 1 1
quit app.quit@app 1 1 1 1
undo_trash edit.undoTrash@file 1 0 1 0
edit_cut edit.cut@app 1 1 1 1
edit_copy edit.copy@app 1 1 1 1
edit_paste edit.paste@app 1 1 1 1
edit_paste_move edit.pasteAsMove@file 1 0 0 1
copy_path file.copyPath@file 1 0 1 1
copy_filename file.copyFilename@file 1 0 1 1
search_files search.open@file 1 0 1 1
settings app.settings@app 1 1 0 1
enter_license_key app.licenseKey@app 1 1 0 1
check_for_updates app.checkForUpdates@app 1 1 1 1
changelog help.whatsNew@app 1 1 1 1
select_all_files selection.selectAll@file 1 0 1 1
deselect_all selection.deselectAll@file 1 0 1 1
select_same_kind selection.selectSameKind@file 1 0 1 1
invert_selection selection.invert@file 1 0 1 1
select_files selection.selectFiles@file 1 0 1 1
deselect_files selection.deselectFiles@file 1 0 1 1
compare_directories selection.compareDirectories@file 1 0 1 1
submenu:menu.view.leftPane - 1 0 0 1
view_mode_full_left - 1 0 0 1
view_mode_brief_left - 1 0 0 1
submenu:menu.view.rightPane - 1 0 0 1
view_mode_full_right - 1 0 0 1
view_mode_brief_right - 1 0 0 1
show_hidden_files - 1 0 0 1
submenu:menu.view.sortBy - 1 0 1 1
sort_by_name sort.byName@file 1 0 0 1
sort_by_extension sort.byExtension@file 1 0 1 1
sort_by_modified sort.byModified@file 1 0 1 1
sort_by_size sort.bySize@file 1 0 1 1
sort_by_created - 1 1 1 1
sort_by_added - 1 1 1 1
sort_ascending - 1 1 1 1
sort_descending - 1 1 1 1
submenu:menu.view.zoom - 1 1 1 1
view_zoom_75 view.zoom.set75@app 1 1 1 1
view_zoom_100 view.zoom.set100@app 1 1 0 1
view_zoom_125 view.zoom.set125@app 1 1 1 1
view_zoom_150 view.zoom.set150@app 1 1 1 1
view_zoom_in view.zoom.in@app 1 1 1 1
view_zoom_out view.zoom.out@app 1 1 1 1
view_full_screen view.toggleFullScreen@file 1 0 1 1
switch_pane pane.switch@file 1 0 1 1
swap_panes pane.swap@file 1 0 1 1
clone_pane pane.clone@file 1 0 1 1
calculate_folder_sizes view.calculateFolderSizes@file 1 0 1 1
command_palette app.commandPalette@file 1 0 1 1
queue_show queue.show@app 1 1 1 1
operation_log log.operationLog@app 1 1 1 1
suggested_ops suggestedOps.show@app 1 1 1 1
ask_cmdr askCmdr.toggle@app 1 1 1 1
go_back nav.back@file 1 0 1 1
go_forward nav.forward@file 1 0 1 1
go_parent nav.parent@file 1 0 1 1
go_root nav.goToRoot@file 1 0 1 1
go_home nav.goHome@file 1 0 1 1
go_to_path nav.goToPath@file 1 0 0 1
go_latest_download downloads.goToLatest@file 1 0 1 1
favorites_add favorites.add@file 1 0 1 1
favorites_open favorites.open@file 1 0 1 1
servers_connect servers.connect@file 1 0 1 1
servers_show servers.show@file 1 0 1 1
new_tab tab.new@file 1 0 1 1
close_tab tab.close@file 1 1 0 1
reopen_closed_tab tab.reopen@file 1 0 1 0
next_tab tab.next@file 1 0 1 1
prev_tab tab.prev@file 1 0 1 1
pin_tab_menu tab.togglePin@file 1 0 0 1
close_other_tabs tab.closeOthers@file 1 0 1 1
about app.about@app 1 1 0 1
acknowledgements app.acknowledgements@app 1 1 0 1
help_shortcuts help.openShortcuts@app 1 1 1 1
help_accessibility help.accessibility@app 1 1 1 1
help_whats_new help.whatsNew@app 1 1 1 1
help_send_feedback feedback.send@app 1 1 1 1
help_debug_log help.viewDebugLog@app 1 1 1 1
help_send_error_report help.sendErrorReport@app 1 1 1 1
";

/// The Windows bar: the Linux one until the Windows menu model is decided (`bar_test.rs`).
const WINDOWS_STATES: &str = "\
open nav.open@file 1 0 1 1
file_view file.view@file 1 0 0 1
edit file.edit@file 1 0 1 1
file_copy file.copy@file 1 0 0 1
file_move file.move@file 1 0 0 1
file_duplicate file.duplicate@file 1 0 1 1
file_compress file.compress@file 1 0 0 1
file_new_folder file.newFolder@file 1 0 0 1
file_new_file file.newFile@file 1 0 0 1
file_delete file.delete@file 1 0 0 1
file_delete_permanently file.deletePermanently@file 1 0 0 1
rename file.rename@file 1 0 0 1
multi_rename file.multiRename@file 1 0 1 1
show_in_finder file.showInFinder@file 1 0 1 1
tell_me_about file.tellMeAbout@file 1 0 1 1
quit app.quit@app 1 1 1 1
undo_trash edit.undoTrash@file 1 0 1 0
edit_cut edit.cut@app 1 1 1 1
edit_copy edit.copy@app 1 1 1 1
edit_paste edit.paste@app 1 1 1 1
edit_paste_move edit.pasteAsMove@file 1 0 0 1
copy_path file.copyPath@file 1 0 1 1
copy_filename file.copyFilename@file 1 0 1 1
search_files search.open@file 1 0 1 1
settings app.settings@app 1 1 0 1
enter_license_key app.licenseKey@app 1 1 0 1
check_for_updates app.checkForUpdates@app 1 1 1 1
changelog help.whatsNew@app 1 1 1 1
select_all_files selection.selectAll@file 1 0 1 1
deselect_all selection.deselectAll@file 1 0 1 1
select_same_kind selection.selectSameKind@file 1 0 1 1
invert_selection selection.invert@file 1 0 1 1
select_files selection.selectFiles@file 1 0 1 1
deselect_files selection.deselectFiles@file 1 0 1 1
compare_directories selection.compareDirectories@file 1 0 1 1
submenu:menu.view.leftPane - 1 0 0 1
view_mode_full_left - 1 0 0 1
view_mode_brief_left - 1 0 0 1
submenu:menu.view.rightPane - 1 0 0 1
view_mode_full_right - 1 0 0 1
view_mode_brief_right - 1 0 0 1
show_hidden_files - 1 0 0 1
submenu:menu.view.sortBy - 1 0 1 1
sort_by_name sort.byName@file 1 0 0 1
sort_by_extension sort.byExtension@file 1 0 1 1
sort_by_modified sort.byModified@file 1 0 1 1
sort_by_size sort.bySize@file 1 0 1 1
sort_by_created - 1 1 1 1
sort_by_added - 1 1 1 1
sort_ascending - 1 1 1 1
sort_descending - 1 1 1 1
submenu:menu.view.zoom - 1 1 1 1
view_zoom_75 view.zoom.set75@app 1 1 1 1
view_zoom_100 view.zoom.set100@app 1 1 0 1
view_zoom_125 view.zoom.set125@app 1 1 1 1
view_zoom_150 view.zoom.set150@app 1 1 1 1
view_zoom_in view.zoom.in@app 1 1 1 1
view_zoom_out view.zoom.out@app 1 1 1 1
view_full_screen view.toggleFullScreen@file 1 0 1 1
switch_pane pane.switch@file 1 0 1 1
swap_panes pane.swap@file 1 0 1 1
clone_pane pane.clone@file 1 0 1 1
calculate_folder_sizes view.calculateFolderSizes@file 1 0 1 1
command_palette app.commandPalette@file 1 0 1 1
queue_show queue.show@app 1 1 1 1
operation_log log.operationLog@app 1 1 1 1
suggested_ops suggestedOps.show@app 1 1 1 1
ask_cmdr askCmdr.toggle@app 1 1 1 1
go_back nav.back@file 1 0 1 1
go_forward nav.forward@file 1 0 1 1
go_parent nav.parent@file 1 0 1 1
go_root nav.goToRoot@file 1 0 1 1
go_home nav.goHome@file 1 0 1 1
go_to_path nav.goToPath@file 1 0 0 1
go_latest_download downloads.goToLatest@file 1 0 1 1
favorites_add favorites.add@file 1 0 1 1
favorites_open favorites.open@file 1 0 1 1
servers_connect servers.connect@file 1 0 1 1
servers_show servers.show@file 1 0 1 1
new_tab tab.new@file 1 0 1 1
close_tab tab.close@file 1 1 0 1
reopen_closed_tab tab.reopen@file 1 0 1 0
next_tab tab.next@file 1 0 1 1
prev_tab tab.prev@file 1 0 1 1
pin_tab_menu tab.togglePin@file 1 0 0 1
close_other_tabs tab.closeOthers@file 1 0 1 1
about app.about@app 1 1 0 1
acknowledgements app.acknowledgements@app 1 1 0 1
help_shortcuts help.openShortcuts@app 1 1 1 1
help_accessibility help.accessibility@app 1 1 1 1
help_whats_new help.whatsNew@app 1 1 1 1
help_send_feedback feedback.send@app 1 1 1 1
help_debug_log help.viewDebugLog@app 1 1 1 1
help_send_error_report help.sendErrorReport@app 1 1 1 1
";

#[test]
fn the_macos_bar_states() {
    assert_pinned(Platform::MacOs, MACOS_STATES);
}

#[test]
fn the_linux_bar_states() {
    assert_pinned(Platform::Linux, LINUX_STATES);
}

#[test]
fn the_windows_bar_states() {
    assert_pinned(Platform::Windows, WINDOWS_STATES);
}
