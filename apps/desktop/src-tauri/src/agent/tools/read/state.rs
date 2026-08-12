//! The `app_state` agent tool: a live snapshot of what the user is looking at —
//! both panes (folder, cursor, selection, view) plus the mounted volumes.
//!
//! Built directly from `PaneStateStore` + the shipped `snapshot_volumes` core
//! (not the private `build_state_yaml`), so the tool returns typed data, not
//! parsed YAML. `get_focused_pane` returns the pane SIDE (`"left"`/`"right"`); the
//! path comes from that side's pane state.
//!
//! The exact selection is all-or-nothing: incomplete or too big to fit one result ⇒ absent
//! with a typed [`SelectionOmitted`] reason, never a partial list the model would read as
//! the whole selection.

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Manager, Runtime};

use super::volumes::{VolumeSnapshot, to_volume_snapshots};
use crate::mcp::PaneStateStore;
use crate::mcp::pane_state::PaneState;
use crate::mcp::resources::volumes::snapshot_volumes;
use crate::mcp::{ToolError, ToolResult};

/// One pane, flattened for the model.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PaneSnapshot {
    /// The pane's current folder.
    pub path: String,
    /// The backing volume for this pane. Rename proposals use this exact id to keep
    /// image-facts reads and the staged plan on the focused volume.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volume_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volume_name: Option<String>,
    /// The item under the cursor, or `None` when the pane is empty or the cursor
    /// row isn't in the loaded window.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor_item: Option<String>,
    /// How many items are selected right now.
    pub selected_count: usize,
    /// The exact selected entries when every selected index is in the cached pane
    /// window AND listing them fits one tool result. `None` means a tool must refuse a
    /// scoped proposal instead of silently dropping rows; `selected_entries_omitted` says
    /// which case it was.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected_entries: Option<Vec<SelectedEntrySnapshot>>,
    /// Why `selected_entries` is absent. Absent itself when the entries are present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected_entries_omitted: Option<SelectionOmitted>,
    /// Total items in the folder (may exceed the loaded window on a huge dir).
    pub total_files: usize,
    pub view_mode: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub sort_field: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub sort_order: String,
    pub show_hidden: bool,
}

/// Why the exact selection couldn't be listed. Typed, so the model reads a token rather
/// than inferring from an absent field, and never reads "no names" as "nothing selected"
/// (`selected_count` is always honest).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SelectionOmitted {
    /// A selected row sits outside the cached pane window, so the list would be incomplete.
    OutsideWindow,
    /// Naming every selected file wouldn't fit one tool result. `list_pane_files` pages the
    /// same selection, so ask it instead of guessing from the count.
    TooMany,
}

/// The subset of a selected pane entry that a proposal needs. It intentionally
/// mirrors cached pane state and never probes the live filesystem.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectedEntrySnapshot {
    pub name: String,
    pub path: String,
    pub is_directory: bool,
}

/// The whole app-state snapshot.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppStateSnapshot {
    /// Which pane has focus: `"left"` or `"right"`.
    pub focused_pane: String,
    pub left: PaneSnapshot,
    pub right: PaneSnapshot,
    pub volumes: Vec<VolumeSnapshot>,
}

/// Flatten one pane's live state. Pure, so cursor/selection reporting is testable
/// without an app handle.
pub(crate) fn pane_snapshot(state: &PaneState) -> PaneSnapshot {
    // `selected_indices` and `cursor_index` are GLOBAL listing indices while `files` holds
    // only the loaded window from `loaded_start`, so every lookup converts first (the same
    // `checked_sub` conversion `mcp::executor` uses). Reading the window with a global index
    // hands back the row sitting at that offset inside the window: silently the wrong file.
    let window_row = |index: usize| index.checked_sub(state.loaded_start).and_then(|i| state.files.get(i));
    let selected_entries = state
        .selected_indices
        .iter()
        .map(|&index| {
            window_row(index).map(|entry| SelectedEntrySnapshot {
                name: entry.name.clone(),
                path: entry.path.clone(),
                is_directory: entry.is_directory,
            })
        })
        .collect::<Option<Vec<_>>>();
    // A selection of thousands of files would serialize to more than any prompt budget, and
    // a partial list here would read as the whole selection (a scoped proposal built on it
    // would silently drop rows). So drop it entirely and say why: `list_pane_files` is the
    // tool that pages a selection honestly.
    let (selected_entries, selected_entries_omitted) = match selected_entries {
        None => (None, Some(SelectionOmitted::OutsideWindow)),
        Some(entries) => {
            let fitted = crate::mcp::fit_to_result_budget(entries);
            if fitted.truncated {
                (None, Some(SelectionOmitted::TooMany))
            } else {
                (Some(fitted.items), None)
            }
        }
    };
    PaneSnapshot {
        path: state.path.clone(),
        volume_id: state.volume_id.clone(),
        volume_name: state.volume_name.clone(),
        cursor_item: window_row(state.cursor_index).map(|f| f.name.clone()),
        selected_count: state.selected_indices.len(),
        selected_entries,
        selected_entries_omitted,
        total_files: state.total_files,
        view_mode: state.view_mode.clone(),
        sort_field: state.sort_field.clone(),
        sort_order: state.sort_order.clone(),
        show_hidden: state.show_hidden,
    }
}

/// Assemble the full snapshot from both panes, the focused side, and the volumes.
/// Pure over its inputs (the impure gather lives in the handler).
pub(crate) fn build_app_state(
    focused: String,
    left: &PaneState,
    right: &PaneState,
    volumes: Vec<VolumeSnapshot>,
) -> AppStateSnapshot {
    AppStateSnapshot {
        focused_pane: focused,
        left: pane_snapshot(left),
        right: pane_snapshot(right),
        volumes,
    }
}

/// `app_state` takes no parameters.
pub fn app_state_schema() -> Value {
    serde_json::json!({ "type": "object", "properties": {}, "additionalProperties": false })
}

/// Handler: read the pane store + snapshot volumes, then shape it.
pub async fn execute_app_state<R: Runtime>(app: &AppHandle<R>, _params: &Value) -> ToolResult {
    let store = app
        .try_state::<PaneStateStore>()
        .ok_or_else(|| ToolError::internal("Pane state isn't available yet"))?;
    let focused = store.get_focused_pane();
    let left = store.get_left();
    let right = store.get_right();
    let volumes = to_volume_snapshots(&snapshot_volumes().await);
    let snapshot = build_app_state(focused, &left, &right, volumes);
    serde_json::to_value(&snapshot).map_err(|e| ToolError::internal(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::pane_state::PaneFileEntry;

    fn file(name: &str) -> PaneFileEntry {
        PaneFileEntry {
            name: name.to_string(),
            path: format!("/x/{name}"),
            is_directory: true,
            ..Default::default()
        }
    }

    #[test]
    fn pane_snapshot_reports_cursor_and_selection() {
        let state = PaneState {
            path: "/Users/x/Documents".to_string(),
            volume_name: Some("Macintosh HD".to_string()),
            files: vec![file("a"), file("2024"), file("c")],
            cursor_index: 1,
            selected_indices: vec![0, 2],
            total_files: 3,
            view_mode: "brief".to_string(),
            ..Default::default()
        };
        let snap = pane_snapshot(&state);
        assert_eq!(snap.cursor_item.as_deref(), Some("2024"));
        assert_eq!(snap.selected_count, 2);
        assert_eq!(snap.path, "/Users/x/Documents");
        assert_eq!(snap.volume_name.as_deref(), Some("Macintosh HD"));
        assert!(snap.selected_entries.is_some());
    }

    #[test]
    fn selection_outside_the_cached_window_is_explicitly_unrepresentable() {
        let state = PaneState {
            path: "/big".to_string(),
            files: vec![file("only-loaded")],
            selected_indices: vec![0, 5_000],
            total_files: 1_000_000,
            ..Default::default()
        };

        let snapshot = pane_snapshot(&state);
        assert_eq!(snapshot.selected_count, 2);
        assert_eq!(snapshot.selected_entries, None);
        assert_eq!(snapshot.selected_entries_omitted, Some(SelectionOmitted::OutsideWindow));
    }

    #[test]
    fn a_selection_too_big_to_list_is_omitted_with_a_reason_not_half_listed() {
        // 20k selected files would serialize to far more than any prompt budget. Half a list
        // would read as the whole selection, so the snapshot drops it and says why; the
        // count stays honest and `list_pane_files` pages the names.
        let files: Vec<PaneFileEntry> = (0..20_000)
            .map(|i| file(&format!("some-reasonably-long-file-name-{i}.jpeg")))
            .collect();
        let state = PaneState {
            path: "/shots".to_string(),
            selected_indices: (0..20_000).collect(),
            total_files: files.len(),
            files,
            ..Default::default()
        };

        let snapshot = pane_snapshot(&state);
        assert_eq!(snapshot.selected_count, 20_000, "the count is never wrong");
        assert_eq!(snapshot.selected_entries, None, "no half list");
        assert_eq!(snapshot.selected_entries_omitted, Some(SelectionOmitted::TooMany));
    }

    #[test]
    fn a_selection_that_fits_is_listed_with_no_omission_reason() {
        let state = PaneState {
            path: "/shots".to_string(),
            files: vec![file("a"), file("b")],
            selected_indices: vec![0, 1],
            total_files: 2,
            ..Default::default()
        };
        let snapshot = pane_snapshot(&state);
        assert_eq!(snapshot.selected_entries.as_ref().map(Vec::len), Some(2));
        assert_eq!(snapshot.selected_entries_omitted, None);
    }

    /// `selected_indices` and `cursor_index` are GLOBAL listing indices, while `files` is
    /// only the loaded window starting at `loaded_start` (see `PaneState`, and the
    /// `checked_sub(loaded_start)` conversions in `mcp::executor`). A scrolled pane must
    /// therefore report the rows the user actually picked, not the ones sitting at the same
    /// offset inside the window.
    #[test]
    fn a_scrolled_pane_reports_the_rows_the_user_picked() {
        let state = PaneState {
            path: "/big".to_string(),
            // Rows 10..14 are loaded; the user selected global rows 11 and 13 and parked the
            // cursor on 12.
            files: vec![file("row-10"), file("row-11"), file("row-12"), file("row-13")],
            loaded_start: 10,
            loaded_end: 14,
            cursor_index: 12,
            selected_indices: vec![11, 13],
            total_files: 5_000,
            ..Default::default()
        };

        let snapshot = pane_snapshot(&state);
        assert_eq!(snapshot.cursor_item.as_deref(), Some("row-12"));
        let names: Vec<&str> = snapshot
            .selected_entries
            .as_deref()
            .expect("both selected rows are in the window")
            .iter()
            .map(|entry| entry.name.as_str())
            .collect();
        assert_eq!(names, ["row-11", "row-13"]);
    }

    #[test]
    fn cursor_out_of_loaded_window_is_none_not_a_wrong_name() {
        // A huge dir whose cursor row isn't in the loaded window reports no cursor
        // item rather than an arbitrary in-window name.
        let state = PaneState {
            path: "/big".to_string(),
            files: vec![file("only-loaded")],
            cursor_index: 5000,
            total_files: 1_000_000,
            ..Default::default()
        };
        assert_eq!(pane_snapshot(&state).cursor_item, None);
        assert_eq!(pane_snapshot(&state).total_files, 1_000_000);
    }

    #[test]
    fn build_app_state_carries_focus_and_both_panes() {
        let left = PaneState {
            path: "/l".to_string(),
            ..Default::default()
        };
        let right = PaneState {
            path: "/r".to_string(),
            ..Default::default()
        };
        let snap = build_app_state("right".to_string(), &left, &right, vec![]);
        assert_eq!(snap.focused_pane, "right");
        assert_eq!(snap.left.path, "/l");
        assert_eq!(snap.right.path, "/r");
    }
}
