//! MCP tool definitions.
//!
//! Defines all available tools with their schemas for the MCP protocol.
//! Tools are designed to match user capabilities - agents can do exactly
//! what users can do through the UI, nothing more.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// A tool definition for MCP.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tool {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

impl Tool {
    /// Create a tool with no parameters.
    fn no_params(name: &str, description: &str) -> Self {
        Self {
            name: name.to_string(),
            description: description.to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        }
    }

    /// Create a tool with an index parameter.
    fn with_index(name: &str, description: &str) -> Self {
        Self {
            name: name.to_string(),
            description: description.to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "index": {
                        "type": "integer",
                        "description": "Zero-based index"
                    }
                },
                "required": ["index"]
            }),
        }
    }
}

/// Get app-level command tools.
fn get_app_tools() -> Vec<Tool> {
    vec![
        Tool::no_params("app_quit", "Quit the application"),
        Tool::no_params("app_hide", "Hide the application window"),
        Tool::no_params("app_about", "Show the about window"),
    ]
}

/// Get view command tools.
fn get_view_tools() -> Vec<Tool> {
    vec![
        Tool::no_params("view_showHidden", "Toggle hidden files visibility"),
        Tool::no_params("view_briefMode", "Switch to Brief view mode"),
        Tool::no_params("view_fullMode", "Switch to Full view mode"),
    ]
}

/// Get pane command tools.
fn get_pane_tools() -> Vec<Tool> {
    vec![Tool::no_params("pane_switch", "Switch focus to the other pane")]
}

/// Get navigation command tools.
fn get_nav_tools() -> Vec<Tool> {
    vec![
        // Basic navigation
        Tool::no_params(
            "nav_open",
            "Open/enter the item (directory, file, network host, share) under the cursor",
        ),
        Tool::no_params("nav_parent", "Navigate to parent folder"),
        Tool::no_params("nav_back", "Navigate back in history"),
        Tool::no_params("nav_forward", "Navigate forward in history"),
        // Cursor movement
        Tool::no_params("nav_up", "Select previous file (move cursor up)"),
        Tool::no_params("nav_down", "Select next file (move cursor down)"),
        Tool::no_params("nav_home", "Go to first file"),
        Tool::no_params("nav_end", "Go to last file"),
        Tool::no_params("nav_pageUp", "Page up"),
        Tool::no_params("nav_pageDown", "Page down"),
        // Brief mode column navigation
        Tool::no_params("nav_left", "Move to previous column (Brief mode only)"),
        Tool::no_params("nav_right", "Move to next column (Brief mode only)"),
    ]
}

/// Get sort command tools.
fn get_sort_tools() -> Vec<Tool> {
    vec![
        // Sort by column
        Tool::no_params("sort_byName", "Sort by filename"),
        Tool::no_params("sort_byExtension", "Sort by file extension"),
        Tool::no_params("sort_bySize", "Sort by file size"),
        Tool::no_params("sort_byModified", "Sort by modification date"),
        Tool::no_params("sort_byCreated", "Sort by creation date"),
        // Sort order
        Tool::no_params("sort_ascending", "Set sort order to ascending"),
        Tool::no_params("sort_descending", "Set sort order to descending"),
        Tool::no_params("sort_toggleOrder", "Toggle between ascending and descending"),
    ]
}

/// Get file action tools.
fn get_file_tools() -> Vec<Tool> {
    vec![
        Tool::no_params(
            "file_openInEditor",
            "Open file under the cursor in the default text editor",
        ),
        Tool::no_params("file_showInFinder", "Show file under the cursor in Finder"),
        Tool::no_params("file_copyPath", "Copy path of the file under the cursor to clipboard"),
        Tool::no_params(
            "file_copyFilename",
            "Copy filename of the file under the cursor to clipboard",
        ),
        Tool::no_params("file_quickLook", "Preview file under the cursor with Quick Look"),
        Tool::no_params("file_getInfo", "Open Get Info window for the file under the cursor"),
    ]
}

/// Get volume tools.
/// Note: volume listing is now a resource (cmdr://volumes), not a tool.
fn get_volume_tools() -> Vec<Tool> {
    vec![
        Tool::with_index("volume_selectLeft", "Select a volume for the left pane by index"),
        Tool::with_index("volume_selectRight", "Select a volume for the right pane by index"),
    ]
}

/// Get selection tools.
fn get_selection_tools() -> Vec<Tool> {
    vec![
        Tool::no_params("selection_clear", "Clear all selected files in the focused pane"),
        Tool::no_params("selection_selectAll", "Select all files in the focused pane"),
        Tool::no_params("selection_deselectAll", "Deselect all files in the focused pane"),
        Tool::no_params(
            "selection_toggleAtCursor",
            "Toggle selection of the file under the cursor",
        ),
        Tool {
            name: "selection_selectRange".to_string(),
            description: "Select a range of files by index".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "startIndex": {
                        "type": "integer",
                        "description": "Start index (inclusive)"
                    },
                    "endIndex": {
                        "type": "integer",
                        "description": "End index (inclusive)"
                    }
                },
                "required": ["startIndex", "endIndex"]
            }),
        },
    ]
}

/// Get all available tools.
pub fn get_all_tools() -> Vec<Tool> {
    let mut tools = Vec::new();
    tools.extend(get_app_tools());
    tools.extend(get_view_tools());
    tools.extend(get_pane_tools());
    tools.extend(get_nav_tools());
    tools.extend(get_sort_tools());
    tools.extend(get_file_tools());
    tools.extend(get_volume_tools());
    tools.extend(get_selection_tools());
    tools
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_tools_count() {
        let tools = get_app_tools();
        assert_eq!(tools.len(), 3);
    }

    #[test]
    fn test_nav_tools_count() {
        let tools = get_nav_tools();
        assert_eq!(tools.len(), 12);
    }

    #[test]
    fn test_sort_tools_count() {
        let tools = get_sort_tools();
        assert_eq!(tools.len(), 8);
    }

    #[test]
    fn test_all_tools_count() {
        let tools = get_all_tools();
        // 3 app + 3 view + 1 pane + 12 nav + 8 sort + 6 file + 2 volume + 5 selection = 40
        // (context tools and volume_list moved to resources)
        assert_eq!(tools.len(), 40);
    }

    #[test]
    fn test_selection_tools_count() {
        let tools = get_selection_tools();
        assert_eq!(tools.len(), 5);
    }

    #[test]
    fn test_tool_with_index() {
        let tool = Tool::with_index("test", "Test tool");
        assert!(tool.input_schema["properties"]["index"].is_object());
        assert_eq!(tool.input_schema["required"][0], "index");
    }
}
