//! The `app_state` agent tool: a live snapshot of what the user is looking at —
//! both panes (folder, cursor, selection, view) plus the mounted volumes.
//!
//! Built directly from `PaneStateStore` + the shipped `snapshot_volumes` core
//! (not the private `build_state_yaml`), so the tool returns typed data, not
//! parsed YAML. `get_focused_pane` returns the pane SIDE (`"left"`/`"right"`); the
//! path comes from that side's pane state.

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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volume_name: Option<String>,
    /// The item under the cursor, or `None` when the pane is empty or the cursor
    /// row isn't in the loaded window.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor_item: Option<String>,
    /// How many items are selected right now.
    pub selected_count: usize,
    /// Total items in the folder (may exceed the loaded window on a huge dir).
    pub total_files: usize,
    pub view_mode: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub sort_field: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub sort_order: String,
    pub show_hidden: bool,
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
    PaneSnapshot {
        path: state.path.clone(),
        volume_name: state.volume_name.clone(),
        cursor_item: state.files.get(state.cursor_index).map(|f| f.name.clone()),
        selected_count: state.selected_indices.len(),
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
            size: None,
            recursive_size: None,
            modified: None,
            recursive_size_pending: None,
            tags: vec![],
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
