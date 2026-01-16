//! MCP resource definitions.
//!
//! Defines resources for reading pane state via the MCP protocol.
//! Resources are read-only state that agents can query.

use serde::{Deserialize, Serialize};
use serde_json::json;
use tauri::{Manager, Runtime};

use super::pane_state::PaneStateStore;
#[cfg(target_os = "macos")]
use crate::volumes;

/// A resource definition for MCP.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Resource {
    pub uri: String,
    pub name: String,
    pub description: String,
    pub mime_type: String,
}

/// Resource content returned by resources/read.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceContent {
    pub uri: String,
    pub mime_type: String,
    pub text: String,
}

/// Get all available resources.
pub fn get_all_resources() -> Vec<Resource> {
    vec![
        Resource {
            uri: "cmdr://pane/focused".to_string(),
            name: "Focused pane".to_string(),
            description: "Which pane is currently focused (left or right)".to_string(),
            mime_type: "application/json".to_string(),
        },
        Resource {
            uri: "cmdr://pane/left/path".to_string(),
            name: "Left pane path".to_string(),
            description: "Current volume and path of the left pane".to_string(),
            mime_type: "application/json".to_string(),
        },
        Resource {
            uri: "cmdr://pane/right/path".to_string(),
            name: "Right pane path".to_string(),
            description: "Current volume and path of the right pane".to_string(),
            mime_type: "application/json".to_string(),
        },
        Resource {
            uri: "cmdr://pane/left/content".to_string(),
            name: "Left pane content".to_string(),
            description: "Visible files in the left pane".to_string(),
            mime_type: "application/json".to_string(),
        },
        Resource {
            uri: "cmdr://pane/right/content".to_string(),
            name: "Right pane content".to_string(),
            description: "Visible files in the right pane".to_string(),
            mime_type: "application/json".to_string(),
        },
        Resource {
            uri: "cmdr://pane/cursor".to_string(),
            name: "File under the cursor".to_string(),
            description: "Info for the file currently under the cursor (name, size, modified date)".to_string(),
            mime_type: "application/json".to_string(),
        },
        Resource {
            uri: "cmdr://status".to_string(),
            name: "App Status".to_string(),
            description: "Current status of the Cmdr application".to_string(),
            mime_type: "application/json".to_string(),
        },
        #[cfg(target_os = "macos")]
        Resource {
            uri: "cmdr://volumes".to_string(),
            name: "Volumes".to_string(),
            description: "List of available volumes (favorites, main volume, attached volumes, cloud drives). Each volume has an index for use with volume_selectLeft/volume_selectRight tools.".to_string(),
            mime_type: "application/json".to_string(),
        },
        Resource {
            uri: "cmdr://selection".to_string(),
            name: "Selected files".to_string(),
            description: "List of selected file indices in the focused pane".to_string(),
            mime_type: "application/json".to_string(),
        },
    ]
}

/// Read a resource by URI.
pub fn read_resource<R: Runtime>(app: &tauri::AppHandle<R>, uri: &str) -> Result<ResourceContent, String> {
    let store = app.try_state::<PaneStateStore>().ok_or("Pane state not available")?;

    let (content, mime_type) = match uri {
        "cmdr://pane/focused" => {
            let focused = store.get_focused_pane();
            (json!({ "focused": focused }), "application/json")
        }
        "cmdr://pane/left/path" => {
            let state = store.get_left();
            (
                json!({
                    "path": state.path,
                    "volumeId": state.volume_id
                }),
                "application/json",
            )
        }
        "cmdr://pane/right/path" => {
            let state = store.get_right();
            (
                json!({
                    "path": state.path,
                    "volumeId": state.volume_id
                }),
                "application/json",
            )
        }
        "cmdr://pane/left/content" => {
            let state = store.get_left();
            (
                json!({ "files": state.files, "viewMode": state.view_mode }),
                "application/json",
            )
        }
        "cmdr://pane/right/content" => {
            let state = store.get_right();
            (
                json!({ "files": state.files, "viewMode": state.view_mode }),
                "application/json",
            )
        }
        "cmdr://pane/cursor" => {
            let focused = store.get_focused_pane();
            let state = if focused == "left" {
                store.get_left()
            } else {
                store.get_right()
            };

            let file_under_cursor = state.files.get(state.cursor_index).map(|f| {
                json!({
                    "name": f.name,
                    "path": f.path,
                    "isDirectory": f.is_directory,
                    "size": f.size,
                    "modified": f.modified
                })
            });

            (json!({ "cursor": file_under_cursor }), "application/json")
        }
        "cmdr://status" => (json!({ "status": "ok", "app": "cmdr" }), "application/json"),
        "cmdr://selection" => {
            let focused = store.get_focused_pane();
            let state = if focused == "left" {
                store.get_left()
            } else {
                store.get_right()
            };
            (
                json!({
                    "pane": focused,
                    "selectedIndices": state.selected_indices,
                    "count": state.selected_indices.len()
                }),
                "application/json",
            )
        }
        #[cfg(target_os = "macos")]
        "cmdr://volumes" => {
            let locations = volumes::list_locations();
            let vols: Vec<serde_json::Value> = locations
                .into_iter()
                .enumerate()
                .map(|(index, loc)| {
                    json!({
                        "index": index,
                        "id": loc.id,
                        "name": loc.name,
                        "path": loc.path,
                        "category": loc.category,
                        "isEjectable": loc.is_ejectable,
                    })
                })
                .collect();
            (json!({ "volumes": vols }), "application/json")
        }
        _ => return Err(format!("Unknown resource URI: {}", uri)),
    };

    Ok(ResourceContent {
        uri: uri.to_string(),
        mime_type: mime_type.to_string(),
        text: serde_json::to_string_pretty(&content).unwrap_or_else(|_| content.to_string()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resource_count() {
        let resources = get_all_resources();
        #[cfg(target_os = "macos")]
        assert_eq!(resources.len(), 9); // 8 base + 1 selection
        #[cfg(not(target_os = "macos"))]
        assert_eq!(resources.len(), 8); // 7 base + 1 selection
    }

    #[test]
    fn test_resource_uris_are_valid() {
        let resources = get_all_resources();
        for resource in resources {
            assert!(
                resource.uri.starts_with("cmdr://"),
                "Resource URI should start with cmdr://"
            );
            assert!(!resource.name.is_empty(), "Resource name should not be empty");
            assert!(
                !resource.description.is_empty(),
                "Resource description should not be empty"
            );
        }
    }

    #[test]
    fn test_all_resources_have_json_mime_type() {
        let resources = get_all_resources();
        for resource in resources {
            assert_eq!(resource.mime_type, "application/json");
        }
    }

    #[test]
    fn test_no_duplicate_resource_uris() {
        let resources = get_all_resources();
        let mut uris: Vec<&str> = resources.iter().map(|r| r.uri.as_str()).collect();
        uris.sort();
        let original_len = uris.len();
        uris.dedup();
        assert_eq!(uris.len(), original_len, "Duplicate resource URIs detected");
    }
}
