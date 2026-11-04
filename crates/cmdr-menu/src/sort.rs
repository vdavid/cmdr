//! View > Sort by's checkmarks: which column and which order the focused pane ASKED for.
//!
//! One state (the requested sort, pushed by the frontend's `update_sort_menu`), one predicate
//! ([`SortCheck::is_on`]), and the per-row answer ([`sort_check_states`]) every host writes from.
//! A date sort the location can't honor falls back to Modified in the backend, and that ❌ never
//! reaches here: the menu shows the request, like the pane's header and MCP's `sort`.

use cmdr_listing::{SortColumn, SortOrder};

use crate::bar::MENU_BAR;
use crate::spec::{CheckRole, Entry, EntryKind, SortCheck, SubmenuRole, SubmenuSpec};

/// The sort a pane asked for: what View > Sort by checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RequestedSort {
    pub column: SortColumn,
    pub order: SortOrder,
}

impl SortCheck {
    /// Whether this item is checked while the focused pane asks for `sort`. `None` is a search
    /// pane in its ranked order: no column and no order to point at, so nothing is.
    pub fn is_on(self, sort: Option<RequestedSort>) -> bool {
        match (self, sort) {
            (_, None) => false,
            (Self::Column(column), Some(sort)) => sort.column == column,
            (Self::Order(order), Some(sort)) => sort.order == order,
        }
    }

    /// What a click on this row asks the frontend for, as `menu-sort` carries it: `("sortBy", column)`
    /// or `("sortOrder", "asc" | "desc")`. `routes/(main)/menu-sort-command.ts` maps it to a `sort.*`
    /// command.
    pub const fn wire(self) -> (&'static str, &'static str) {
        match self {
            Self::Column(column) => (
                "sortBy",
                match column {
                    SortColumn::Name => "name",
                    SortColumn::Extension => "extension",
                    SortColumn::Size => "size",
                    SortColumn::Modified => "modified",
                    SortColumn::Created => "created",
                    SortColumn::Added => "added",
                },
            ),
            Self::Order(SortOrder::Ascending) => ("sortOrder", "asc"),
            Self::Order(SortOrder::Descending) => ("sortOrder", "desc"),
        }
    }
}

/// Every Sort by check item's id and whether it's checked for `sort`, read off `MENU_BAR`'s Sort
/// by rows, so a new row can't be missed here.
pub fn sort_check_states(sort: Option<RequestedSort>) -> Vec<(&'static str, bool)> {
    sort_by_spec()
        .entries
        .iter()
        .filter_map(|entry| match entry.kind {
            EntryKind::Check(check) => match check.role {
                CheckRole::Sort(role) => Some((check.id, role.is_on(sort))),
                _ => None,
            },
            _ => None,
        })
        .collect()
}

/// The Sort by submenu's spec, which `bar_test.rs` pins to exist on both platforms.
fn sort_by_spec() -> &'static SubmenuSpec {
    fn find(entries: &'static [Entry]) -> Option<&'static SubmenuSpec> {
        entries.iter().find_map(|entry| match &entry.kind {
            EntryKind::Submenu(nested) if nested.role == SubmenuRole::SortBy => Some(nested),
            EntryKind::Submenu(nested) => find(nested.entries),
            _ => None,
        })
    }
    MENU_BAR
        .iter()
        .find_map(|bar_menu| find(bar_menu.submenu.entries))
        .expect("`MENU_BAR` has a Sort by submenu")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{
        SORT_ASCENDING_ID, SORT_BY_ADDED_ID, SORT_BY_CREATED_ID, SORT_BY_EXTENSION_ID, SORT_BY_MODIFIED_ID,
        SORT_BY_NAME_ID, SORT_BY_SIZE_ID, SORT_DESCENDING_ID,
    };

    fn checked(sort: Option<RequestedSort>) -> Vec<&'static str> {
        sort_check_states(sort)
            .into_iter()
            .filter_map(|(id, on)| on.then_some(id))
            .collect()
    }

    fn sort(column: SortColumn, order: SortOrder) -> Option<RequestedSort> {
        Some(RequestedSort { column, order })
    }

    #[test]
    fn every_column_checks_its_own_item_and_the_order() {
        let cases = [
            (SortColumn::Name, SORT_BY_NAME_ID),
            (SortColumn::Extension, SORT_BY_EXTENSION_ID),
            (SortColumn::Modified, SORT_BY_MODIFIED_ID),
            (SortColumn::Size, SORT_BY_SIZE_ID),
            (SortColumn::Created, SORT_BY_CREATED_ID),
            (SortColumn::Added, SORT_BY_ADDED_ID),
        ];
        for (column, id) in cases {
            assert_eq!(
                checked(sort(column, SortOrder::Ascending)),
                [id, SORT_ASCENDING_ID],
                "{column:?}"
            );
            assert_eq!(
                checked(sort(column, SortOrder::Descending)),
                [id, SORT_DESCENDING_ID],
                "{column:?}"
            );
        }
    }

    #[test]
    fn a_ranked_search_pane_checks_nothing() {
        assert_eq!(checked(None), Vec::<&str>::new());
    }

    #[test]
    fn covers_all_eight_sort_by_rows() {
        assert_eq!(sort_check_states(None).len(), 8);
    }

    /// The words `handle_menu_event` emitted for each row before the mapping moved here.
    #[test]
    fn a_click_asks_for_the_frontends_own_words() {
        let cases = [
            (SortCheck::Column(SortColumn::Name), ("sortBy", "name")),
            (SortCheck::Column(SortColumn::Extension), ("sortBy", "extension")),
            (SortCheck::Column(SortColumn::Size), ("sortBy", "size")),
            (SortCheck::Column(SortColumn::Modified), ("sortBy", "modified")),
            (SortCheck::Column(SortColumn::Created), ("sortBy", "created")),
            (SortCheck::Column(SortColumn::Added), ("sortBy", "added")),
            (SortCheck::Order(SortOrder::Ascending), ("sortOrder", "asc")),
            (SortCheck::Order(SortOrder::Descending), ("sortOrder", "desc")),
        ];
        for (check, wire) in cases {
            assert_eq!(check.wire(), wire, "{check:?}");
        }
    }
}
