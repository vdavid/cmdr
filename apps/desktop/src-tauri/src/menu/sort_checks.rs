//! Writes View > Sort by's checkmarks onto the native items, from `MenuState.requested_sort`.
//!
//! The state, the predicate, and which row is checked for a given sort are `cmdr_menu::sort`
//! (`RequestedSort`, `SortCheck::is_on`, `sort_check_states`), shared with the browser demo. A
//! date sort the location can't honor falls back to Modified in the backend, and that ❌ never
//! reaches here: the menu shows the request, like the pane's header and MCP's `sort`.

use cmdr_menu::sort::sort_check_states;
use tauri::Runtime;

use crate::ignore_poison::IgnorePoison as _;

use super::{MenuState, cloned_handle};

/// Sets every Sort by check from `MenuState.requested_sort`, the one writer of their checked state.
///
/// Each item is found by id in the live submenu at call time, so a rebind's replacement
/// (`update_menu_item_accelerator`) is the one reached, ❌ never a stale handle. Also what undoes
/// muda's own toggle on a click: the check moves only when the frontend's sort lands and re-pushes.
pub(crate) fn sync_sort_check_states<R: Runtime>(menu_state: &MenuState<R>) {
    let sort = *menu_state.requested_sort.lock_ignore_poison();
    // A handle, so no lock is held across `set_checked`, which waits on the main thread.
    let Some(submenu) = cloned_handle(&menu_state.sort_submenu) else {
        return;
    };
    for (id, checked) in sort_check_states(sort) {
        let Some(kind) = submenu.get(id) else {
            log::warn!(target: "menu", "View > Sort by holds no `{id}` item to check");
            continue;
        };
        match kind.as_check_menuitem() {
            Some(item) => {
                if let Err(e) = item.set_checked(checked) {
                    log::warn!(target: "menu", "Couldn't set the check on `{id}`: {e}");
                }
            }
            None => log::warn!(target: "menu", "View > Sort by's `{id}` isn't a check item"),
        }
    }
}
