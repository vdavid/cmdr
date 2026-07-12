//! Navigation, cursor, and selection tool schemas (all pane-targeting).

use serde_json::{Value, json};

pub fn select_volume_schema() -> Value {
    json!({
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
    })
}

pub fn nav_to_path_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "pane": {
                "type": "string",
                "enum": ["left", "right"],
                "description": "Which pane to navigate"
            },
            "path": {
                "type": "string",
                "description": "Path to navigate to: absolute, ~-relative, or virtual (mtp://, smb://)"
            }
        },
        "required": ["pane", "path"]
    })
}

pub fn scroll_to_schema() -> Value {
    json!({
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
    })
}

pub fn move_cursor_schema() -> Value {
    json!({
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
    })
}

pub fn select_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "pane": {
                "type": "string",
                "enum": ["left", "right"],
                "description": "Which pane to select in"
            },
            "names": {
                "type": "array",
                "items": { "type": "string" },
                "description": "Filenames to select. Errors if any name isn't in the listing."
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
    })
}
