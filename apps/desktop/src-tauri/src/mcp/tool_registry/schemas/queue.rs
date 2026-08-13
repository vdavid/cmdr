//! Queue tool schema.

use serde_json::{Value, json};

pub fn queue_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "action": {
                "type": "string",
                "enum": ["pause", "resume", "cancel", "pause_all", "resume_all"],
                "description": "pause | resume | cancel | pause_all | resume_all. pause acts on a running operation; a queued one isn't touching a device yet, so pausing it is refused rather than silently ignored."
            },
            "operationId": {
                "type": "string",
                "description": "Operation to act on (required for pause / resume / cancel unless operationIds is given). See cmdr://state operations."
            },
            "operationIds": {
                "type": "array",
                "items": { "type": "string" },
                "description": "For cancel only: several operations to cancel at once (keeps already-copied files)."
            },
            "rollback": {
                "type": "boolean",
                "description": "For cancel with a single operationId: delete already-copied files instead of keeping them. Requires the bearer token."
            }
        },
        "required": ["action"]
    })
}
