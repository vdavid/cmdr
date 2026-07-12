//! View and tab tool schemas.

use serde_json::{Value, json};

pub fn set_view_mode_schema() -> Value {
    json!({
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
    })
}

pub fn sort_schema() -> Value {
    json!({
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
    })
}

pub fn tab_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "action": {
                "type": "string",
                "enum": ["new", "close", "close_others", "activate", "set_pinned", "reopen"],
                "description": "Action to perform on the tab"
            },
            "pane": {
                "type": "string",
                "enum": ["left", "right"],
                "description": "Which pane to operate on"
            },
            "tabId": {
                "type": "string",
                "description": "Tab ID. Defaults to active tab for close, close_others, set_pinned. Required for activate. Not used for new or reopen."
            },
            "pinned": {
                "type": "boolean",
                "description": "Pin state (only for set_pinned action)"
            }
        },
        "required": ["action", "pane"]
    })
}
