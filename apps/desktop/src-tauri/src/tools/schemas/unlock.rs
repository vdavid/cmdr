//! Unlock tool schemas.

use serde_json::{Value, json};

pub fn unlock_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "path": {
                "type": "string",
                "description": "Which locked archive or backup repo you're answering, copied verbatim from the path on the archive-password entry in cmdr://state dialogs. Required: another surface may have answered the prompt you read and a different one may be asking now."
            },
            "password": {
                "type": "string",
                "description": "The password to try. Handed to the archive or repo only, never echoed back, never rendered in cmdr://state, never logged, and never saved to the Keychain."
            }
        },
        "required": ["path", "password"]
    })
}
