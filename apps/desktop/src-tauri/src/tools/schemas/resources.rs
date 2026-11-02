//! The `read_resource` tool's schema.

use serde_json::{Value, json};

pub fn read_resource_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "uri": {
                "type": "string",
                "description": "cmdr://state, cmdr://settings, cmdr://logs, cmdr://indexing, cmdr://importance, or cmdr://dialogs/available, with its query if any"
            },
            "offset": {
                "type": "integer",
                "description": "Characters to skip: the nextOffset of the answer before."
            }
        },
        "required": ["uri"],
        "additionalProperties": false
    })
}
