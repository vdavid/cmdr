//! Favorites tool schema.

use serde_json::{Value, json};

pub fn favorites_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "action": {
                "type": "string",
                "enum": ["add", "rename", "remove", "reorder"],
                "description": "add | rename | remove | reorder"
            },
            "path": {
                "type": "string",
                "description": "For add: the folder path to favorite (~ expands to home)."
            },
            "id": {
                "type": "string",
                "description": "For rename / remove: the favorite id. See cmdr://state favorites."
            },
            "name": {
                "type": "string",
                "description": "For add (optional, defaults to the path's name) / rename (required): the display label."
            },
            "orderedIds": {
                "type": "array",
                "items": { "type": "string" },
                "description": "For reorder: the complete new ordering of favorite ids."
            }
        },
        "required": ["action"]
    })
}
