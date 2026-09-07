//! What the macOS Services menu acts on right now.
//!
//! AppKit asks two questions on the main thread, synchronously, with no chance to
//! await IPC: "have you got files?" while it validates the menu, and "hand them
//! over" when the user picks a service. So the answer has to be readable from a
//! lock the instant it's asked, which is what this module stores.
//!
//! The frontend pushes the CHEAP shape it already holds (a listing id plus the
//! selected indices), never the resolved paths: a select-all over a 500k-row
//! folder is four numbers and an id here, against 500k strings across IPC on every
//! keystroke of a held ⇧↓. The paths are read out of the listing cache only when a
//! service actually asks, which happens once, at the click.
//!
//! Nothing here touches AppKit, so all of it is unit-testable.

use std::path::PathBuf;
use std::sync::RwLock;

use cmdr_fs::ignore_poison::RwLockIgnorePoison;

/// Where a pane's selected rows live.
///
/// Two shapes because Cmdr has two kinds of pane, and the drag-out commands
/// already split the same way (`start_selection_drag` against `start_drag_paths`
/// in `commands/file_system/drag.rs`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum SelectedRows {
    /// Frontend indices into a cached listing, resolved on demand. `has_parent`
    /// says whether index 0 is the synthetic `..` row, exactly as
    /// `get_paths_at_indices` means it.
    Listing {
        listing_id: String,
        indices: Vec<usize>,
        include_hidden: bool,
        has_parent: bool,
    },
    /// Paths by value. The search-results snapshot keeps its rows in the frontend
    /// and has no cached listing to index into.
    Paths { paths: Vec<String> },
}

/// The focused pane's answer to "what would a hand-off to macOS act on?".
///
/// `rows` is `None` when nothing is selected, which is the whole of Finder's rule:
/// with a selection, act on the selection; with none, act on the cursor row. The
/// frontend normalizes an empty selection to `None` so "selected nothing" can't be
/// spelled two ways here.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ServicesSelection {
    /// The cursor row, or empty when the pane has none a service could take: the
    /// `..` row, an empty folder, or a pane whose rows have no file behind them
    /// (a phone, an archive's insides, the host list).
    pub cursor_path: String,
    /// The selected rows, or `None` when the selection is empty.
    pub rows: Option<SelectedRows>,
}

/// The live answer, replaced wholesale on every push.
///
/// A value store in the `ignore_poison` sense (one field pair, well-formed after
/// any single write), so a panic elsewhere costs at most a stale selection rather
/// than the app.
static SELECTION: RwLock<Option<ServicesSelection>> = RwLock::new(None);

/// Replaces what the Services menu acts on. Called from the IPC push.
pub fn set(selection: ServicesSelection) {
    *SELECTION.write_ignore_poison() = Some(selection);
}

/// Whether there is anything at all to hand a service.
///
/// Deliberately optimistic: it answers from the SHAPE of the selection without
/// touching the listing cache, because it runs inside AppKit's menu validation and
/// is asked once per registered send type. A selection whose indices have all gone
/// stale still says yes here and writes nothing later, which the user sees as a
/// service that did nothing rather than a menu that flickers.
pub fn has_anything() -> bool {
    let guard = SELECTION.read_ignore_poison();
    guard
        .as_ref()
        .is_some_and(|selection| selection.rows.is_some() || !selection.cursor_path.is_empty())
}

/// The paths a service should receive, with Finder's rule applied.
pub fn resolve() -> Vec<PathBuf> {
    let selection = SELECTION.read_ignore_poison().clone();
    let Some(selection) = selection else { return Vec::new() };
    resolve_with(&selection, |rows| match rows {
        SelectedRows::Listing {
            listing_id,
            indices,
            include_hidden,
            has_parent,
        } => crate::file_system::get_paths_at_indices(listing_id, indices, *include_hidden, *has_parent)
            .unwrap_or_default(),
        SelectedRows::Paths { paths } => paths.iter().map(PathBuf::from).collect(),
    })
}

/// Finder's rule, with the listing lookup handed in so it can be tested without a
/// listing cache: a selection wins, the cursor row is the fallback, and a selection
/// that resolves to nothing (its listing is gone, or every index is stale) falls
/// back to the cursor rather than handing a service an empty pasteboard.
fn resolve_with(selection: &ServicesSelection, resolve_rows: impl Fn(&SelectedRows) -> Vec<PathBuf>) -> Vec<PathBuf> {
    if let Some(rows) = selection.rows.as_ref() {
        let paths = resolve_rows(rows);
        if !paths.is_empty() {
            return paths;
        }
    }
    if selection.cursor_path.is_empty() {
        return Vec::new();
    }
    vec![PathBuf::from(&selection.cursor_path)]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn listing(indices: &[usize]) -> SelectedRows {
        SelectedRows::Listing {
            listing_id: "listing-1".to_string(),
            indices: indices.to_vec(),
            include_hidden: false,
            has_parent: true,
        }
    }

    /// Stands in for the listing cache: index N becomes `/dir/N.txt`.
    fn fake_listing(rows: &SelectedRows) -> Vec<PathBuf> {
        match rows {
            SelectedRows::Listing { indices, .. } => {
                indices.iter().map(|i| PathBuf::from(format!("/dir/{i}.txt"))).collect()
            }
            SelectedRows::Paths { paths } => paths.iter().map(PathBuf::from).collect(),
        }
    }

    #[test]
    fn a_selection_wins_over_the_cursor_row() {
        let selection = ServicesSelection {
            cursor_path: "/dir/cursor.txt".to_string(),
            rows: Some(listing(&[3, 7])),
        };
        assert_eq!(
            resolve_with(&selection, fake_listing),
            vec![PathBuf::from("/dir/3.txt"), PathBuf::from("/dir/7.txt")]
        );
    }

    #[test]
    fn with_nothing_selected_the_cursor_row_is_what_a_service_gets() {
        // Finder's rule, and the one Cmdr's own context menu already follows
        // (`snapshot-context-menu.ts`): no selection means "act on the row I'm on".
        let selection = ServicesSelection {
            cursor_path: "/dir/cursor.txt".to_string(),
            rows: None,
        };
        assert_eq!(
            resolve_with(&selection, fake_listing),
            vec![PathBuf::from("/dir/cursor.txt")]
        );
    }

    #[test]
    fn a_pane_with_neither_hands_over_nothing() {
        // A pane whose rows have no file behind them pushes both fields empty, and
        // the Services menu must then look exactly as it did before this feature.
        assert!(resolve_with(&ServicesSelection::default(), fake_listing).is_empty());
    }

    #[test]
    fn a_selection_whose_listing_has_gone_falls_back_to_the_cursor() {
        // The listing cache evicts, and a navigation can land between the push and
        // the click. Handing a service an empty pasteboard would look like the app
        // ignoring the click; the cursor row is the same answer the user would get
        // with nothing selected.
        let selection = ServicesSelection {
            cursor_path: "/dir/cursor.txt".to_string(),
            rows: Some(listing(&[1])),
        };
        assert_eq!(
            resolve_with(&selection, |_| Vec::new()),
            vec![PathBuf::from("/dir/cursor.txt")]
        );
    }

    #[test]
    fn a_snapshot_pane_carries_its_paths_by_value() {
        // The search-results pane has no listing id, so its rows can only travel
        // as paths. Same rule, other shape.
        let selection = ServicesSelection {
            cursor_path: "/elsewhere/cursor.txt".to_string(),
            rows: Some(SelectedRows::Paths {
                paths: vec!["/a/one.txt".to_string(), "/b/two.txt".to_string()],
            }),
        };
        assert_eq!(
            resolve_with(&selection, fake_listing),
            vec![PathBuf::from("/a/one.txt"), PathBuf::from("/b/two.txt")]
        );
    }

    #[test]
    fn the_shape_test_says_yes_for_a_cursor_alone_and_no_for_an_empty_pane() {
        // `has_anything` runs inside AppKit's menu validation and must not read the
        // listing cache, so it answers from the shape. Both halves pinned.
        set(ServicesSelection::default());
        assert!(!has_anything());
        set(ServicesSelection {
            cursor_path: "/dir/cursor.txt".to_string(),
            rows: None,
        });
        assert!(has_anything());
    }
}
