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
        Tool::no_params("app.quit", "Quit the application"),
        Tool::no_params("app.hide", "Hide the application window"),
        Tool::no_params("app.about", "Show the about window"),
    ]
}

/// Get view command tools.
fn get_view_tools() -> Vec<Tool> {
    vec![
        Tool::no_params("view.showHidden", "Toggle hidden files visibility"),
        Tool::no_params("view.briefMode", "Switch to Brief view mode"),
        Tool::no_params("view.fullMode", "Switch to Full view mode"),
    ]
}

/// Get pane command tools.
fn get_pane_tools() -> Vec<Tool> {
    vec![
        Tool::no_params("pane.switch", "Switch focus to the other pane"),
        Tool::no_params("pane.leftVolumeChooser", "Open volume chooser for left pane"),
        Tool::no_params("pane.rightVolumeChooser", "Open volume chooser for right pane"),
    ]
}

/// Get navigation command tools.
fn get_nav_tools() -> Vec<Tool> {
    vec![
        // Basic navigation
        Tool::no_params(
            "nav.open",
            "Open/enter the selected item (directory, file, network host, share)",
        ),
        Tool::no_params("nav.parent", "Navigate to parent folder"),
        Tool::no_params("nav.back", "Navigate back in history"),
        Tool::no_params("nav.forward", "Navigate forward in history"),
        // Cursor movement
        Tool::no_params("nav.up", "Select previous file (move cursor up)"),
        Tool::no_params("nav.down", "Select next file (move cursor down)"),
        Tool::no_params("nav.home", "Go to first file"),
        Tool::no_params("nav.end", "Go to last file"),
        Tool::no_params("nav.pageUp", "Page up"),
        Tool::no_params("nav.pageDown", "Page down"),
        // Brief mode column navigation
        Tool::no_params("nav.left", "Move to previous column (Brief mode only)"),
        Tool::no_params("nav.right", "Move to next column (Brief mode only)"),
    ]
}

/// Get sort command tools.
fn get_sort_tools() -> Vec<Tool> {
    vec![
        // Sort by column
        Tool::no_params("sort.byName", "Sort by filename"),
        Tool::no_params("sort.byExtension", "Sort by file extension"),
        Tool::no_params("sort.bySize", "Sort by file size"),
        Tool::no_params("sort.byModified", "Sort by modification date"),
        Tool::no_params("sort.byCreated", "Sort by creation date"),
        // Sort order
        Tool::no_params("sort.ascending", "Set sort order to ascending"),
        Tool::no_params("sort.descending", "Set sort order to descending"),
        Tool::no_params("sort.toggleOrder", "Toggle between ascending and descending"),
    ]
}

/// Get file action tools.
fn get_file_tools() -> Vec<Tool> {
    vec![
        Tool::no_params("file.showInFinder", "Show selected file in Finder"),
        Tool::no_params("file.copyPath", "Copy selected file path to clipboard"),
        Tool::no_params("file.copyFilename", "Copy selected filename to clipboard"),
        Tool::no_params("file.quickLook", "Preview selected file with Quick Look"),
        Tool::no_params("file.getInfo", "Open Get Info window for selected file"),
    ]
}

/// Get volume tools.
fn get_volume_tools() -> Vec<Tool> {
    vec![
        Tool::no_params("volume.list", "List all available volumes with current selection"),
        Tool::with_index("volume.selectLeft", "Select a volume for the left pane by index"),
        Tool::with_index("volume.selectRight", "Select a volume for the right pane by index"),
    ]
}

/// Get context/state query tools.
fn get_context_tools() -> Vec<Tool> {
    vec![
        // Pane state
        Tool::no_params(
            "context.getFocusedPane",
            "Get which pane is currently focused (left or right)",
        ),
        Tool::no_params("context.getLeftPanePath", "Get current volume and path of left pane"),
        Tool::no_params("context.getRightPanePath", "Get current volume and path of right pane"),
        // File listing
        Tool::no_params(
            "context.getLeftPaneContent",
            "Get visible files in left pane (name only in Brief, full details in Full mode)",
        ),
        Tool::no_params(
            "context.getRightPaneContent",
            "Get visible files in right pane (name only in Brief, full details in Full mode)",
        ),
        // Selected file info
        Tool::no_params(
            "context.getSelectedFileInfo",
            "Get info for selected file (name, size, modified date)",
        ),
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
    tools.extend(get_context_tools());
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
        // 3 app + 3 view + 3 pane + 12 nav + 8 sort + 5 file + 3 volume + 6 context = 43
        assert_eq!(tools.len(), 43);
    }

    #[test]
    fn test_tool_with_index() {
        let tool = Tool::with_index("test", "Test tool");
        assert!(tool.input_schema["properties"]["index"].is_object());
        assert_eq!(tool.input_schema["required"][0], "index");
    }
}
