//! What a click on a menu-bar row asks for, read off the row itself.
//!
//! The desktop's `menu_handlers.rs::handle_menu_event` and the browser demo's menu both route a
//! click through [`bar_click`], so the two can't disagree about which event a row sends. Only the
//! bar's rows are here: the context menus' and the viewer bar's ids have lanes of their own.

use crate::bar::spec_entry;
use crate::ids::{
    CLOSE_TAB_ID, CommandScope, EDIT_COPY_ID, EDIT_CUT_ID, EDIT_PASTE_ID, SELECT_ALL_ID, menu_id_to_command,
};
use crate::spec::{CheckRole, EntryKind, Pane, Platform, SortCheck, ViewMode};

/// The four families that skip `execute-command`, and the plain command lane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BarClick {
    /// `execute-command` with `command_id`. A `FileScoped` one runs only while the main window has
    /// focus.
    Command {
        command_id: &'static str,
        scope: CommandScope,
    },
    /// The show-hidden check item: `settings-changed` with the toggled state. A check item toggles
    /// itself, so an `execute-command` on top would toggle twice.
    ShowHiddenFiles,
    /// A per-pane Full / Brief check item: `view-mode-changed` naming the pane, which needn't be
    /// the focused one.
    ViewMode { pane: Pane, mode: ViewMode },
    /// A View > Sort by row: `menu-sort` ([`SortCheck::wire`]), with every check put back on the
    /// requested sort first. The check moves when the frontend's sort lands and re-pushes.
    Sort(SortCheck),
    /// Cut, Copy, Paste, or Select all: `execute-command` with `command_id` in the main window,
    /// where the frontend picks file or text semantics, and the native selector anywhere else.
    Edit { command_id: &'static str },
    /// Close tab: `execute-command` `tab.close` in the main window, and closing the focused window
    /// anywhere else (⌘W's standard meaning).
    CloseTab,
}

/// What a click on `menu_id`, a row of `platform`'s bar, asks for. `None` for an id the bar doesn't
/// build there, and for a row with no command behind it.
pub fn bar_click(menu_id: &str, platform: Platform) -> Option<BarClick> {
    match spec_entry(menu_id, platform)? {
        EntryKind::Check(check) => Some(match check.role {
            CheckRole::ShowHiddenFiles => BarClick::ShowHiddenFiles,
            CheckRole::ViewMode(pane, mode) => BarClick::ViewMode { pane, mode },
            CheckRole::Sort(role) => BarClick::Sort(role),
        }),
        EntryKind::Item(item) => {
            let (command_id, scope) = menu_id_to_command(item.id)?;
            Some(match item.id {
                CLOSE_TAB_ID => BarClick::CloseTab,
                EDIT_CUT_ID | EDIT_COPY_ID | EDIT_PASTE_ID | SELECT_ALL_ID => BarClick::Edit { command_id },
                _ => BarClick::Command { command_id, scope },
            })
        }
        EntryKind::Separator | EntryKind::Predefined(..) | EntryKind::Submenu(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bar::every_entry;
    use crate::ids::{
        SHOW_HIDDEN_FILES_ID, SORT_ASCENDING_ID, SORT_BY_NAME_ID, VIEW_MODE_BRIEF_RIGHT_ID, VIEW_ZOOM_IN_ID,
    };
    use cmdr_listing::{SortColumn, SortOrder};

    /// Every clickable row the bar builds answers, on both platforms: an item or check with no
    /// answer would be a dead row.
    #[test]
    fn every_item_and_check_on_the_bar_asks_for_something() {
        for platform in [Platform::MacOs, Platform::Linux] {
            for entry in every_entry(platform) {
                let id = match entry {
                    EntryKind::Item(item) => item.id,
                    EntryKind::Check(check) => check.id,
                    _ => continue,
                };
                assert!(
                    bar_click(id, platform).is_some(),
                    "`{id}` on {platform:?} asks for nothing"
                );
            }
        }
    }

    #[test]
    fn the_four_families_skip_execute_command() {
        let mac = Platform::MacOs;
        assert_eq!(bar_click(SHOW_HIDDEN_FILES_ID, mac), Some(BarClick::ShowHiddenFiles));
        assert_eq!(
            bar_click(VIEW_MODE_BRIEF_RIGHT_ID, mac),
            Some(BarClick::ViewMode {
                pane: Pane::Right,
                mode: ViewMode::Brief
            })
        );
        assert_eq!(
            bar_click(SORT_BY_NAME_ID, mac),
            Some(BarClick::Sort(SortCheck::Column(SortColumn::Name)))
        );
        assert_eq!(
            bar_click(SORT_ASCENDING_ID, mac),
            Some(BarClick::Sort(SortCheck::Order(SortOrder::Ascending)))
        );
        assert_eq!(
            bar_click(EDIT_COPY_ID, mac),
            Some(BarClick::Edit {
                command_id: "edit.copy"
            })
        );
        assert_eq!(
            bar_click(SELECT_ALL_ID, mac),
            Some(BarClick::Edit {
                command_id: "selection.selectAll"
            })
        );
        assert_eq!(bar_click(CLOSE_TAB_ID, mac), Some(BarClick::CloseTab));
        assert_eq!(
            bar_click(VIEW_ZOOM_IN_ID, mac),
            Some(BarClick::Command {
                command_id: "view.zoom.in",
                scope: CommandScope::App
            })
        );
    }

    #[test]
    fn an_id_off_the_bar_asks_for_nothing() {
        assert_eq!(bar_click("tab_pin", Platform::MacOs), None);
        assert_eq!(bar_click("nope", Platform::MacOs), None);
    }
}
