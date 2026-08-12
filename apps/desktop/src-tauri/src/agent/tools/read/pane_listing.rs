//! Compact access to the focused pane's existing backend listing cache.

use std::collections::HashSet;
use std::path::Path;

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Manager, Runtime};

use crate::file_system::listing::{FileEntry, get_cached_listing};
use crate::mcp::pane_state::PaneState;
use crate::mcp::{PaneStateStore, ToolError, ToolResult};

/// The most rows one listing returns, before the size cut in [`build_pane_listing`]. Both
/// cuts report through `total` / `returned` / `truncated`, never silently.
const MAX_PANE_FILES: usize = 200;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PaneListingSource {
    path: String,
    name: String,
    is_directory: bool,
    is_symlink: bool,
    size: Option<u64>,
    modified: Option<u64>,
}

impl From<FileEntry> for PaneListingSource {
    fn from(entry: FileEntry) -> Self {
        Self {
            path: entry.path,
            name: entry.name,
            is_directory: entry.is_directory,
            is_symlink: entry.is_symlink,
            size: entry.size,
            modified: entry.modified_at,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PaneListingEntry {
    pub name: String,
    #[serde(skip_serializing_if = "crate::agent::tools::read::is_false")]
    pub is_directory: bool,
    #[serde(skip_serializing_if = "crate::agent::tools::read::is_false")]
    pub is_symlink: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PaneListingScope {
    Selection,
    Folder,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PaneListingResult {
    pub pane: String,
    pub path: String,
    pub volume_id: String,
    pub scope: PaneListingScope,
    pub total: usize,
    pub returned: usize,
    pub truncated: bool,
    pub entries: Vec<PaneListingEntry>,
}

pub(crate) fn build_pane_listing(
    pane: String,
    state: &PaneState,
    cached_entries: Vec<PaneListingSource>,
) -> Result<PaneListingResult, ToolError> {
    let volume_id = state
        .volume_id
        .clone()
        .ok_or_else(|| ToolError::internal("The focused pane's volume isn't available yet"))?;
    let (scope, entries) = if state.selected_indices.is_empty() {
        (PaneListingScope::Folder, cached_entries)
    } else {
        // `selected_indices` are GLOBAL listing indices; `files` is the loaded window from
        // `loaded_start`. Convert before indexing (as `mcp::executor` does) or a scrolled
        // pane resolves to the wrong rows, and a rename plan gets built on them.
        let selected_paths = state
            .selected_indices
            .iter()
            .map(|&index| {
                index
                    .checked_sub(state.loaded_start)
                    .and_then(|i| state.files.get(i))
                    .map(|entry| entry.path.as_str())
            })
            .collect::<Option<HashSet<_>>>()
            .ok_or_else(|| ToolError::internal("Some selected rows aren't available in the pane cache"))?;
        let entries: Vec<_> = cached_entries
            .into_iter()
            .filter(|entry| selected_paths.contains(entry.path.as_str()))
            .collect();
        if entries.len() != selected_paths.len() {
            return Err(ToolError::internal(
                "Some selected rows no longer match the focused pane's listing",
            ));
        }
        (PaneListingScope::Selection, entries)
    };
    let total = entries.len();
    let entries: Vec<_> = entries
        .into_iter()
        .take(MAX_PANE_FILES)
        .map(|entry| PaneListingEntry {
            name: entry.name,
            is_directory: entry.is_directory,
            is_symlink: entry.is_symlink,
            size: entry.size,
            modified: entry.modified,
        })
        .collect();
    // Second cut, by SIZE: 200 rows of long names can still outgrow one tool result, and a
    // result the model can't be shown is worse than a short one.
    let entries = crate::mcp::fit_to_result_budget(entries).items;
    let returned = entries.len();
    Ok(PaneListingResult {
        pane,
        path: state.path.clone(),
        volume_id,
        scope,
        total,
        returned,
        truncated: total > returned,
        entries,
    })
}

/// `list_pane_files` always reads the currently focused pane and takes no parameters.
pub fn list_pane_files_schema() -> Value {
    serde_json::json!({ "type": "object", "properties": {}, "additionalProperties": false })
}

pub async fn execute_list_pane_files<R: Runtime>(app: &AppHandle<R>, _params: &Value) -> ToolResult {
    let store = app
        .try_state::<PaneStateStore>()
        .ok_or_else(|| ToolError::internal("Pane state isn't available yet"))?;
    let pane = store.get_focused_pane();
    let state = match pane.as_str() {
        "left" => store.get_left(),
        "right" => store.get_right(),
        _ => return Err(ToolError::internal("The focused pane isn't available yet")),
    };
    let volume_id = state
        .volume_id
        .as_deref()
        .ok_or_else(|| ToolError::internal("The focused pane's volume isn't available yet"))?;
    let entries = get_cached_listing(volume_id, Path::new(&state.path))
        .ok_or_else(|| ToolError::internal("The focused pane's listing isn't available yet"))?
        .into_iter()
        .map(PaneListingSource::from)
        .collect();
    let result = build_pane_listing(pane, &state, entries)?;
    serde_json::to_value(result).map_err(|error| ToolError::internal(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::pane_state::PaneFileEntry;

    fn source(index: usize) -> PaneListingSource {
        PaneListingSource {
            path: format!("/shots/shot-{index}.png"),
            name: format!("shot-{index}.png"),
            is_directory: false,
            is_symlink: false,
            size: Some(index as u64),
            modified: Some(1_700_000_000 + index as u64),
        }
    }

    fn pane_file(index: usize) -> PaneFileEntry {
        PaneFileEntry {
            name: format!("shot-{index}.png"),
            path: format!("/shots/shot-{index}.png"),
            size: Some(index as u64),
            ..Default::default()
        }
    }

    #[test]
    fn folder_scope_is_compact_capped_and_carries_volume_id() {
        let state = PaneState {
            path: "/shots".to_string(),
            volume_id: Some("root".to_string()),
            files: (0..205).map(pane_file).collect(),
            total_files: 205,
            ..Default::default()
        };
        let result = build_pane_listing("right".to_string(), &state, (0..205).map(source).collect()).unwrap();
        assert_eq!(result.volume_id, "root");
        assert_eq!(result.scope, PaneListingScope::Folder);
        assert_eq!(result.total, 205);
        assert_eq!(result.returned, 200);
        assert!(result.truncated);
        assert_eq!(result.entries[0].name, "shot-0.png");
    }

    #[test]
    fn a_pane_of_very_long_names_still_fits_one_tool_result() {
        use crate::agent::chat::budget::{MAX_TOOL_RESULT_TOKENS, estimate_serialized_tokens};

        // 200 rows is the row cap, not a size cap: names this long blow the tool-result
        // ceiling, so the size cut takes over and the counts stay honest.
        let long_name = |index: usize| format!("{}-{index}.png", "screenshot-of-something-quite-specific".repeat(4));
        let state = PaneState {
            path: "/shots".to_string(),
            volume_id: Some("root".to_string()),
            files: (0..300)
                .map(|i| PaneFileEntry {
                    name: long_name(i),
                    path: format!("/shots/{}", long_name(i)),
                    ..pane_file(i)
                })
                .collect(),
            total_files: 300,
            ..Default::default()
        };
        let sources: Vec<PaneListingSource> = (0..300)
            .map(|i| PaneListingSource {
                path: format!("/shots/{}", long_name(i)),
                name: long_name(i),
                ..source(i)
            })
            .collect();

        let result = build_pane_listing("left".to_string(), &state, sources).unwrap();
        assert_eq!(result.total, 300, "the honest denominator is every file in scope");
        assert_eq!(result.returned, result.entries.len());
        assert!(result.truncated);
        let spent: usize = result.entries.iter().map(estimate_serialized_tokens).sum();
        assert!(
            spent <= MAX_TOOL_RESULT_TOKENS,
            "the listing must fit the tool-result ceiling (spent {spent})"
        );
    }

    #[test]
    fn selection_scope_returns_only_selected_cached_rows() {
        let state = PaneState {
            path: "/shots".to_string(),
            volume_id: Some("root".to_string()),
            files: (0..5).map(pane_file).collect(),
            selected_indices: vec![1, 3],
            ..Default::default()
        };
        let result = build_pane_listing("left".to_string(), &state, (0..5).map(source).collect()).unwrap();
        assert_eq!(result.scope, PaneListingScope::Selection);
        assert_eq!(
            result
                .entries
                .iter()
                .map(|entry| entry.name.as_str())
                .collect::<Vec<_>>(),
            ["shot-1.png", "shot-3.png"]
        );
    }

    /// `selected_indices` are GLOBAL listing indices; `files` is only the loaded window from
    /// `loaded_start` (see `PaneState` and `mcp::executor`'s `checked_sub(loaded_start)`).
    /// Reading the window with a global index hands a rename plan the wrong files.
    #[test]
    fn a_scrolled_pane_selects_the_rows_the_user_picked() {
        let state = PaneState {
            path: "/shots".to_string(),
            volume_id: Some("root".to_string()),
            // Global rows 100..104 are loaded; the user selected 101 and 103.
            files: (100..104).map(pane_file).collect(),
            loaded_start: 100,
            loaded_end: 104,
            selected_indices: vec![101, 103],
            total_files: 5_000,
            ..Default::default()
        };
        let result = build_pane_listing("left".to_string(), &state, (100..104).map(source).collect()).unwrap();
        assert_eq!(result.scope, PaneListingScope::Selection);
        assert_eq!(
            result
                .entries
                .iter()
                .map(|entry| entry.name.as_str())
                .collect::<Vec<_>>(),
            ["shot-101.png", "shot-103.png"]
        );
    }

    #[test]
    fn selection_scope_refuses_to_silently_drop_a_stale_selected_row() {
        let state = PaneState {
            path: "/shots".to_string(),
            volume_id: Some("root".to_string()),
            files: (0..5).map(pane_file).collect(),
            selected_indices: vec![1, 3],
            ..Default::default()
        };
        assert!(build_pane_listing("left".to_string(), &state, vec![source(1)]).is_err());
    }
}
