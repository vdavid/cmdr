//! Indexing tool schema.

use serde_json::{Value, json};

pub fn indexing_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "action": {
                "type": "string",
                "enum": ["enable", "disable", "rescan", "forget"],
                "description": "enable | disable | rescan | forget"
            },
            "volumeId": {
                "type": "string",
                "description": "Volume ID to control (for example 'root', 'smb-…', 'mtp-…:1'). See cmdr://state volumes."
            }
        },
        "required": ["action", "volumeId"]
    })
}
