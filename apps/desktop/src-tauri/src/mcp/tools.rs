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
}

/// Get tab tools.
fn get_tab_tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "activate_tab".to_string(),
            description: "Switch to a specific tab in a pane".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "pane": {
                        "type": "string",
                        "enum": ["left", "right"],
                        "description": "Which pane to switch tab in"
                    },
                    "tab_id": {
                        "type": "string",
                        "description": "ID of the tab to activate"
                    }
                },
                "required": ["pane", "tab_id"]
            }),
        },
        Tool {
            name: "pin_tab".to_string(),
            description: "Pin or unpin a tab. Pinned tabs stay in place when navigating to a new directory."
                .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "pane": {
                        "type": "string",
                        "enum": ["left", "right"],
                        "description": "Which pane the tab is in"
                    },
                    "tab_id": {
                        "type": "string",
                        "description": "ID of the tab to pin/unpin"
                    },
                    "pinned": {
                        "type": "boolean",
                        "description": "true to pin, false to unpin"
                    }
                },
                "required": ["pane", "tab_id", "pinned"]
            }),
        },
    ]
}

/// Get app-level command tools.
fn get_app_tools() -> Vec<Tool> {
    vec![
        Tool::no_params("quit", "Quit the application"),
        Tool::no_params("switch_pane", "Switch focus to the other pane"),
        Tool::no_params(
            "swap_panes",
            "Swap left and right pane directories, view modes, sort orders, and selections",
        ),
    ]
}

/// Get view command tools.
fn get_view_tools() -> Vec<Tool> {
    vec![
        Tool::no_params("toggle_hidden", "Toggle hidden files visibility"),
        Tool {
            name: "set_view_mode".to_string(),
            description: "Set view mode for pane".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "pane": {
                        "type": "string",
                        "enum": ["left", "right"],
                        "description": "Which pane to set view mode for"
                    },
                    "mode": {
                        "type": "string",
                        "enum": ["brief", "full"],
                        "description": "View mode to set"
                    }
                },
                "required": ["pane", "mode"]
            }),
        },
        Tool {
            name: "sort".to_string(),
            description: "Sort files in pane by field and order".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "pane": {
                        "type": "string",
                        "enum": ["left", "right"],
                        "description": "Which pane to sort"
                    },
                    "by": {
                        "type": "string",
                        "enum": ["name", "ext", "size", "modified", "created"],
                        "description": "Field to sort by"
                    },
                    "order": {
                        "type": "string",
                        "enum": ["asc", "desc"],
                        "description": "Sort order"
                    }
                },
                "required": ["pane", "by", "order"]
            }),
        },
    ]
}

/// Get file operation tools.
fn get_file_op_tools() -> Vec<Tool> {
    vec![
        Tool::no_params("copy", "Copy selected files to other pane (triggers native dialog)"),
        Tool::no_params("mkdir", "Create folder in focused pane (triggers naming dialog)"),
        Tool::no_params("refresh", "Refresh focused pane"),
    ]
}

/// Get navigation command tools.
fn get_nav_tools() -> Vec<Tool> {
    vec![
        // Volume selection
        Tool {
            name: "select_volume".to_string(),
            description: "Switch pane to specified volume by name".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "pane": {
                        "type": "string",
                        "enum": ["left", "right"],
                        "description": "Which pane to switch"
                    },
                    "name": {
                        "type": "string",
                        "description": "Volume name to select"
                    }
                },
                "required": ["pane", "name"]
            }),
        },
        // Path navigation
        Tool {
            name: "nav_to_path".to_string(),
            description: "Navigate pane to specified path".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "pane": {
                        "type": "string",
                        "enum": ["left", "right"],
                        "description": "Which pane to navigate"
                    },
                    "path": {
                        "type": "string",
                        "description": "Absolute path to navigate to"
                    }
                },
                "required": ["pane", "path"]
            }),
        },
        // Basic navigation
        Tool::no_params("nav_to_parent", "Navigate to parent folder"),
        Tool::no_params("nav_back", "Navigate back in history"),
        Tool::no_params("nav_forward", "Navigate forward in history"),
        // Scrolling for large directories
        Tool {
            name: "scroll_to".to_string(),
            description: "Load region around specified index for large directories".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "pane": {
                        "type": "string",
                        "enum": ["left", "right"],
                        "description": "Which pane to scroll"
                    },
                    "index": {
                        "type": "integer",
                        "description": "Zero-based index to scroll to"
                    }
                },
                "required": ["pane", "index"]
            }),
        },
    ]
}

/// Get cursor and selection tools.
fn get_cursor_tools() -> Vec<Tool> {
    vec![
        // Cursor movement
        Tool {
            name: "move_cursor".to_string(),
            description: "Move cursor to index or filename. Provide either index or filename".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "pane": {
                        "type": "string",
                        "enum": ["left", "right"],
                        "description": "Which pane to move cursor in"
                    },
                    "index": {
                        "type": "integer",
                        "description": "Zero-based index to move cursor to"
                    },
                    "filename": {
                        "type": "string",
                        "description": "Filename to move cursor to"
                    }
                },
                "required": ["pane"]
            }),
        },
        Tool::no_params(
            "open_under_cursor",
            "Open/enter the item (directory, file, network host, share) under the cursor",
        ),
    ]
}

/// Get selection tools.
fn get_selection_tools() -> Vec<Tool> {
    vec![Tool {
        name: "select".to_string(),
        description: "Select files in pane. Use count for ranges, all for everything, count=0 to clear".to_string(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "pane": {
                    "type": "string",
                    "enum": ["left", "right"],
                    "description": "Which pane to select in"
                },
                "start": {
                    "type": "integer",
                    "description": "Zero-based start index"
                },
                "count": {
                    "type": "integer",
                    "description": "Number of items from start. 0 clears selection"
                },
                "all": {
                    "type": "boolean",
                    "description": "Select all files"
                },
                "mode": {
                    "type": "string",
                    "enum": ["replace", "add", "subtract"],
                    "description": "Selection mode (default: replace)"
                }
            },
            "required": ["pane"]
        }),
    }]
}

/// Get dialog tool.
fn get_dialog_tools() -> Vec<Tool> {
    vec![Tool {
        name: "dialog".to_string(),
        description: "Open, focus, or close dialogs".to_string(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["open", "focus", "close"],
                    "description": "Action to perform"
                },
                "type": {
                    "type": "string",
                    "enum": ["settings", "file-viewer", "about", "copy-confirmation", "mkdir-confirmation"],
                    "description": "Dialog type"
                },
                "section": {
                    "type": "string",
                    "description": "For settings: which section to open (e.g., 'shortcuts')"
                },
                "path": {
                    "type": "string",
                    "description": "For file-viewer: file path. On open without path, uses cursor file. On close without path, closes all."
                }
            },
            "required": ["action", "type"]
        }),
    }]
}

/// Get all available tools.
pub fn get_all_tools() -> Vec<Tool> {
    let mut tools = Vec::new();
    tools.extend(get_nav_tools());
    tools.extend(get_cursor_tools());
    tools.extend(get_selection_tools());
    tools.extend(get_file_op_tools());
    tools.extend(get_view_tools());
    tools.extend(get_tab_tools());
    tools.extend(get_dialog_tools());
    tools.extend(get_app_tools());
    tools
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_tools_count() {
        let tools = get_app_tools();
        // quit, switch_pane, swap_panes
        assert_eq!(tools.len(), 3);
    }

    #[test]
    fn test_nav_tools_count() {
        let tools = get_nav_tools();
        // select_volume, nav_to_path, nav_to_parent, nav_back, nav_forward, scroll_to
        assert_eq!(tools.len(), 6);
    }

    #[test]
    fn test_cursor_tools_count() {
        let tools = get_cursor_tools();
        // move_cursor, open_under_cursor
        assert_eq!(tools.len(), 2);
    }

    #[test]
    fn test_view_tools_count() {
        let tools = get_view_tools();
        // toggle_hidden, set_view_mode, sort
        assert_eq!(tools.len(), 3);
    }

    #[test]
    fn test_file_op_tools_count() {
        let tools = get_file_op_tools();
        // copy, mkdir, refresh
        assert_eq!(tools.len(), 3);
    }

    #[test]
    fn test_tab_tools_count() {
        let tools = get_tab_tools();
        // activate_tab, pin_tab
        assert_eq!(tools.len(), 2);
    }

    #[test]
    fn test_activate_tab_tool_schema() {
        let tools = get_tab_tools();
        let tool = tools.iter().find(|t| t.name == "activate_tab").unwrap();

        let schema = &tool.input_schema;
        let props = schema.get("properties").unwrap();

        assert!(props.get("pane").is_some());
        assert!(props.get("tab_id").is_some());
        assert_eq!(props["tab_id"]["type"], "string");

        let pane_enum = props.get("pane").unwrap().get("enum").unwrap().as_array().unwrap();
        assert!(pane_enum.contains(&json!("left")));
        assert!(pane_enum.contains(&json!("right")));

        let required = schema.get("required").unwrap().as_array().unwrap();
        assert_eq!(required.len(), 2);
        assert!(required.contains(&json!("pane")));
        assert!(required.contains(&json!("tab_id")));
    }

    #[test]
    fn test_all_tools_count() {
        let tools = get_all_tools();
        // 6 nav + 2 cursor + 1 selection + 3 file_op + 3 view + 2 tab + 1 dialog + 3 app = 21
        assert_eq!(tools.len(), 21);
    }

    #[test]
    fn test_dialog_tools_count() {
        let tools = get_dialog_tools();
        assert_eq!(tools.len(), 1);
    }

    #[test]
    fn test_selection_tools_count() {
        let tools = get_selection_tools();
        assert_eq!(tools.len(), 1);
    }

    #[test]
    fn test_select_tool_schema() {
        let tools = get_selection_tools();
        let select_tool = &tools[0];
        assert_eq!(select_tool.name, "select");

        let schema = &select_tool.input_schema;
        let props = schema.get("properties").unwrap();

        // Check properties exist
        assert!(props.get("pane").is_some());
        assert!(props.get("start").is_some());
        assert!(props.get("count").is_some());
        assert!(props.get("all").is_some());
        assert!(props.get("mode").is_some());

        // count should be a plain integer, not oneOf
        assert_eq!(props["count"]["type"], "integer");

        // all should be boolean
        assert_eq!(props["all"]["type"], "boolean");

        // Only pane is required
        let required = schema.get("required").unwrap().as_array().unwrap();
        assert_eq!(required.len(), 1);
        assert!(required.contains(&json!("pane")));
    }

    #[test]
    fn test_move_cursor_tool_schema() {
        let tools = get_cursor_tools();
        let tool = tools.iter().find(|t| t.name == "move_cursor").unwrap();

        let schema = &tool.input_schema;
        let props = schema.get("properties").unwrap();

        // Check properties exist with correct types
        assert!(props.get("pane").is_some());
        assert_eq!(props["index"]["type"], "integer");
        assert_eq!(props["filename"]["type"], "string");

        // Only pane is required (index/filename validated in executor)
        let required = schema.get("required").unwrap().as_array().unwrap();
        assert_eq!(required.len(), 1);
        assert!(required.contains(&json!("pane")));

        // Should NOT have a "to" property
        assert!(props.get("to").is_none());
    }

    #[test]
    fn test_dialog_tool_schema() {
        let tools = get_dialog_tools();
        let dialog_tool = &tools[0];
        assert_eq!(dialog_tool.name, "dialog");

        let schema = &dialog_tool.input_schema;
        let props = schema.get("properties").unwrap();

        // Check required properties exist
        assert!(props.get("action").is_some());
        assert!(props.get("type").is_some());
        assert!(props.get("section").is_some());
        assert!(props.get("path").is_some());

        // Check action enum values
        let action_enum = props.get("action").unwrap().get("enum").unwrap().as_array().unwrap();
        assert!(action_enum.contains(&json!("open")));
        assert!(action_enum.contains(&json!("focus")));
        assert!(action_enum.contains(&json!("close")));

        // Check type enum values
        let type_enum = props.get("type").unwrap().get("enum").unwrap().as_array().unwrap();
        assert!(type_enum.contains(&json!("settings")));
        assert!(type_enum.contains(&json!("file-viewer")));
        assert!(type_enum.contains(&json!("about")));
        assert!(type_enum.contains(&json!("copy-confirmation")));
        assert!(type_enum.contains(&json!("mkdir-confirmation")));

        // Check required fields
        let required = schema.get("required").unwrap().as_array().unwrap();
        assert_eq!(required.len(), 2);
        assert!(required.contains(&json!("action")));
        assert!(required.contains(&json!("type")));
    }

    #[test]
    fn test_sort_tool_schema() {
        let tools = get_view_tools();
        let sort_tool = tools.iter().find(|t| t.name == "sort").unwrap();

        let schema = &sort_tool.input_schema;
        let props = schema.get("properties").unwrap();

        // Check required properties exist
        assert!(props.get("pane").is_some());
        assert!(props.get("by").is_some());
        assert!(props.get("order").is_some());

        // Check by enum values
        let by_enum = props.get("by").unwrap().get("enum").unwrap().as_array().unwrap();
        assert!(by_enum.contains(&json!("name")));
        assert!(by_enum.contains(&json!("ext")));
        assert!(by_enum.contains(&json!("size")));
        assert!(by_enum.contains(&json!("modified")));
        assert!(by_enum.contains(&json!("created")));

        // Check order enum values
        let order_enum = props.get("order").unwrap().get("enum").unwrap().as_array().unwrap();
        assert!(order_enum.contains(&json!("asc")));
        assert!(order_enum.contains(&json!("desc")));

        // Check required fields
        let required = schema.get("required").unwrap().as_array().unwrap();
        assert_eq!(required.len(), 3);
        assert!(required.contains(&json!("pane")));
        assert!(required.contains(&json!("by")));
        assert!(required.contains(&json!("order")));
    }

    #[test]
    fn test_set_view_mode_tool_schema() {
        let tools = get_view_tools();
        let tool = tools.iter().find(|t| t.name == "set_view_mode").unwrap();

        let schema = &tool.input_schema;
        let props = schema.get("properties").unwrap();

        // Check required properties exist
        assert!(props.get("pane").is_some());
        assert!(props.get("mode").is_some());

        // Check mode enum values
        let mode_enum = props.get("mode").unwrap().get("enum").unwrap().as_array().unwrap();
        assert!(mode_enum.contains(&json!("brief")));
        assert!(mode_enum.contains(&json!("full")));

        // Check required fields
        let required = schema.get("required").unwrap().as_array().unwrap();
        assert_eq!(required.len(), 2);
        assert!(required.contains(&json!("pane")));
        assert!(required.contains(&json!("mode")));
    }
}
