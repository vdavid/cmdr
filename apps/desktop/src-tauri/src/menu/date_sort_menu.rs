//! The file list header's Sort by popup's pick event. The popup itself (Date modified, Date
//! created, and Date added, the date the pane ASKED for checked) is `cmdr_menu::context::popups`'s
//! `date_sort_menu`, opened by a right-click on either view's header and by the Full date header's
//! chevron.
//!
//! A pick comes back as [`DateSortMenuPick`], carrying the PANE: a right-click on the unfocused
//! pane's header sorts that pane, which `menu-sort` (the focused pane's lane) can't say. The
//! frontend applies it (`pane/sort-operations.ts`, `pickSortColumn`), so picking the checked
//! date does nothing there.

use cmdr_listing::SortColumn;
pub use cmdr_menu::context::popups::PaneSide;
use serde::{Deserialize, Serialize};
use tauri_specta::Event;

/// `date-sort-menu-pick`: a date was picked from a file list header's Sort by popup. Emitted to
/// the main window, which sorts `pane` by `column` unless that pane already asked for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct DateSortMenuPick {
    pub pane: PaneSide,
    pub column: SortColumn,
}
