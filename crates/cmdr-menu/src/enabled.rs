//! Which menu-bar rows are enabled: one pure fold over every input at once.
//!
//! The desktop's `menu/item_states.rs::apply_menu_item_states` is its one writer: it stores each
//! input in `MenuState` and hands this fold the lot, so no writer can undo another's verdict by
//! running after it. The browser demo asks the same fold for every row it draws.
//!
//! Three kinds of row, because the desktop keeps three kinds of handle: the tracked items
//! ([`menu_item_enabled`]), the explorer items it holds in their own fields (pin tab, show hidden,
//! the view modes, [`FieldItem`]), and the two submenus it greys as a whole ([`submenu_enabled`]).
//! An untracked item is never written, so it keeps what it was built with. [`entry_enabled`] is
//! that rule over one `MENU_BAR` row.

use std::collections::HashSet;

use crate::ids::{
    CLOSE_TAB_ID, CommandScope, EDIT_PASTE_MOVE_ID, FILE_COMPRESS_ID, FILE_COPY_ID, FILE_DELETE_ID,
    FILE_DELETE_PERMANENTLY_ID, FILE_MOVE_ID, FILE_NEW_FILE_ID, FILE_NEW_FOLDER_ID, OPEN_TERMINAL_HERE_ID,
    PIN_TAB_MENU_ID, RENAME_ID, REOPEN_CLOSED_TAB_ID, UNDO_TRASH_ID, VIEW_SET_MODE_COMMAND_ID,
    VIEW_SHOW_HIDDEN_COMMAND_ID, menu_id_to_command,
};
use crate::spec::{CheckRole, EntryKind, SubmenuRole, Tracking};

/// Everything a main-menu item's enabled state is derived from.
#[derive(Debug, Clone, Copy)]
pub struct MenuItemInputs<'a> {
    /// The main file explorer owns the menu (`activate_window_menu("main")`).
    pub explorer_menu_active: bool,
    /// The main window can't start a file operation right now (`set_file_operations_blocked`).
    pub file_operations_blocked: bool,
    /// The focused pane sits somewhere a shell can `cd` into (`set_open_terminal_here_enabled`).
    pub open_terminal_here_enabled: bool,
    /// The focused pane's closed-tab stack has entries (`set_reopen_closed_tab_enabled`).
    pub reopen_closed_tab_enabled: bool,
    /// A trash this session waits to be undone (`update_undo_trash_menu`).
    pub undo_trash_available: bool,
    /// The commands the main window's dialog gate refuses right now (`set_commands_refused_over_dialog`).
    pub refused: &'a HashSet<String>,
}

/// Whether the main-menu item `id` is enabled. The ONE place that's decided, from every input at
/// once, so no writer can undo another's verdict by running later.
pub fn menu_item_enabled(id: &str, inputs: &MenuItemInputs) -> bool {
    let command = menu_id_to_command(id);
    let refused = command.is_some_and(|(command_id, _)| inputs.refused.contains(command_id));
    // Close tab doubles as "close the focused window" (`handle_menu_event`), so it only greys out
    // while the main window is the one in front and can't close a tab.
    if id == CLOSE_TAB_ID {
        return !(inputs.explorer_menu_active && refused);
    }
    let in_scope = matches!(command, Some((_, CommandScope::App))) || inputs.explorer_menu_active;
    let own_verdict = match id {
        REOPEN_CLOSED_TAB_ID => inputs.reopen_closed_tab_enabled,
        UNDO_TRASH_ID => inputs.undo_trash_available,
        OPEN_TERMINAL_HERE_ID => inputs.open_terminal_here_enabled,
        _ if OPERATION_START_ITEM_IDS.contains(&id) => !inputs.file_operations_blocked,
        _ => true,
    };
    in_scope && own_verdict && !refused
}

/// An explorer item the desktop holds in its own `MenuState` field, outside the tracked items.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldItem {
    PinTab,
    ShowHiddenFiles,
    /// Any of the four per-pane Full / Brief items, and the two pane submenus that hold them.
    ViewMode,
}

/// Whether a [`FieldItem`] is enabled: the explorer owns the menu and the dialog gate doesn't
/// refuse its command. None of them has a verdict of its own. The check items name their command
/// by const, since they emit their own events instead of `execute-command`.
pub fn field_item_enabled(item: FieldItem, inputs: &MenuItemInputs) -> bool {
    let command_id = match item {
        FieldItem::PinTab => menu_id_to_command(PIN_TAB_MENU_ID).map_or("", |(command_id, _)| command_id),
        FieldItem::ShowHiddenFiles => VIEW_SHOW_HIDDEN_COMMAND_ID,
        FieldItem::ViewMode => VIEW_SET_MODE_COMMAND_ID,
    };
    inputs.explorer_menu_active && !inputs.refused.contains(command_id)
}

/// Whether a submenu is enabled. The pane submenus grey with their view-mode items, so they appear
/// greyed out instead of opening to reveal disabled items; Sort by greys as a whole while the
/// explorer doesn't own the menu, and its rows are greyed by their own commands besides.
pub fn submenu_enabled(role: SubmenuRole, inputs: &MenuItemInputs) -> bool {
    match role {
        SubmenuRole::Plain => true,
        SubmenuRole::SortBy => inputs.explorer_menu_active,
        SubmenuRole::Pane(_) => field_item_enabled(FieldItem::ViewMode, inputs),
    }
}

/// Whether `MENU_BAR` row `entry` is enabled under `inputs`, or `None` for a row with no state of
/// ours (a separator, or an OS-wired predefined item, which the OS greys).
pub fn entry_enabled(entry: &EntryKind, inputs: &MenuItemInputs) -> Option<bool> {
    match entry {
        EntryKind::Item(item) => Some(match item.tracking {
            Tracking::Tracked => menu_item_enabled(item.id, inputs),
            Tracking::Untracked => item.enabled,
            Tracking::PinTab => field_item_enabled(FieldItem::PinTab, inputs),
        }),
        EntryKind::Check(check) => Some(match check.role {
            CheckRole::ShowHiddenFiles => field_item_enabled(FieldItem::ShowHiddenFiles, inputs),
            CheckRole::ViewMode(..) => field_item_enabled(FieldItem::ViewMode, inputs),
            CheckRole::Sort(_) if check.tracked => menu_item_enabled(check.id, inputs),
            // Never written: the submenu around it greys instead.
            CheckRole::Sort(_) => true,
        }),
        EntryKind::Submenu(nested) => Some(submenu_enabled(nested.role, inputs)),
        EntryKind::Separator | EntryKind::Predefined(..) => None,
    }
}

/// Records that the search box of viewer `label` took (`focused`) or gave up keyboard focus,
/// in `holder`: the viewer whose search box has it, if any.
///
/// ❗ A `false` only counts from the viewer that last claimed focus. Every viewer pushes its own
/// answer, and clicking from viewer A to viewer B crosses two of them in flight (A's input blurs,
/// B gains window focus and re-pushes); without this, whichever arrived last would win and B could
/// end up with A's answer. Both orderings land on B here.
pub fn note_viewer_search_focus(holder: &mut Option<String>, label: &str, focused: bool) {
    if focused {
        *holder = Some(label.to_string());
    } else if holder.as_deref() == Some(label) {
        *holder = None;
    }
}

/// Whether the viewer bar's Edit > Cut and Edit > Paste are live, given which viewer's search
/// box holds focus.
///
/// The search box is the only editable field in a viewer window, so anywhere else those two would
/// act on nothing, and looking live is what read as broken.
///
/// ⚠️ Chrome, like every other verdict here: a disabled item's key equivalent either still fires
/// (and forwards `cut:` to a first responder with nothing to cut) or falls through to the webview,
/// so greying can't take ⌘X / ⌘V away from the search box either way.
pub fn viewer_text_edit_enabled(holder: Option<&str>) -> bool {
    holder.is_some()
}

/// The menu items that would START a file operation, greyed out while the main
/// window can't take one.
///
/// ❌ Not the ones that steer a RUNNING operation, and not Cut / Copy: marking a
/// clipboard selection starts nothing.
///
/// ⚠️ Every id here must be `FileScoped`. `Edit > Paste` is deliberately absent
/// even though pasting files DOES start a copy: it's `App`-scoped because in
/// Settings and the viewer it forwards the native `paste:` selector, so greying it
/// for a main-window dialog would kill ⌘V in those windows' text fields. Pasting
/// files is still refused honestly by `pane/operation-start-gate.ts`; only the
/// chrome differs. `every_gated_item_is_a_real_file_scoped_menu_item` pins this.
const OPERATION_START_ITEM_IDS: &[&str] = &[
    FILE_COPY_ID,
    FILE_MOVE_ID,
    FILE_COMPRESS_ID,
    FILE_NEW_FOLDER_ID,
    FILE_NEW_FILE_ID,
    FILE_DELETE_ID,
    FILE_DELETE_PERMANENTLY_ID,
    RENAME_ID,
    EDIT_PASTE_MOVE_ID,
];

#[cfg(test)]
#[path = "enabled_test.rs"]
mod enabled_test;

#[cfg(test)]
#[path = "bar_states_test.rs"]
mod bar_states_test;
