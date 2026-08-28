//! Archive tool schemas.

use serde_json::{Value, json};

pub fn unlock_archive_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "archivePath": {
                "type": "string",
                "description": "Which archive you're answering, copied verbatim from the archivePath on the archive-password entry in cmdr://state dialogs. Required: another surface may have answered the prompt you read and a different archive may be asking now."
            },
            "password": {
                "type": "string",
                "description": "The password to try. Stored on the archive only, never echoed back, never rendered in cmdr://state, never logged."
            }
        },
        "required": ["archivePath", "password"]
    })
}
