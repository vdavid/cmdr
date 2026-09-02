//! Operation-log tool schemas.

use serde_json::{Value, json};

pub fn operations_list_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "since": {
                "type": "integer",
                "description": "Start time at or after this (Unix ms)."
            },
            "until": {
                "type": "integer",
                "description": "Start time at or before this (Unix ms)."
            },
            "name": {
                "type": "string",
                "description": "Item name to match (case- and Unicode-folded), exact or prefix per nameMatch; not a substring search."
            },
            "nameMatch": {
                "type": "string",
                "enum": ["exact", "prefix"],
                "description": "Default prefix."
            },
            "kind": {
                "type": "string",
                "enum": ["copy", "move", "delete", "trash", "rename", "createFolder", "createFile", "archiveEdit"]
            },
            "initiator": {
                "type": "string",
                "enum": ["user", "aiClient", "agent"]
            },
            "executionStatus": {
                "type": "string",
                "enum": ["queued", "running", "done", "failed", "canceled"]
            },
            "rollbackState": {
                "type": "string",
                "enum": ["notRollbackable", "rollbackable", "rollingBack", "rolledBack", "partiallyRolledBack"]
            },
            "limit": {
                "type": "integer",
                "description": "Default 50, max 1000; a page may come back shorter, with returned, total, and truncated; use offset for the rest."
            },
            "offset": {
                "type": "integer",
                "description": "Operations to skip."
            }
        },
        "required": []
    })
}

pub fn operations_get_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "operationId": {
                "type": "string",
                "description": "The operation's id, as given by operations_list, a copy/move/delete response, cmdr://state operations, or the queue tool."
            },
            "limit": {
                "type": "integer",
                "description": "Default 200, max 1000; a page may come back shorter, with returned, total, and truncated; use offset for the rest."
            },
            "offset": {
                "type": "integer",
                "description": "Item rows to skip."
            }
        },
        "required": ["operationId"]
    })
}

pub fn operations_rollback_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "operationId": {
                "type": "string",
                "description": "The operation to reverse. Same id as operations_list, a copy/move/delete response, or cmdr://state operations."
            },
            "autoConfirm": {
                "type": "boolean",
                "description": "Must be true to roll back: a rollback writes to disk, so (like copy/move/delete) it requires the bearer token. Returns once the reversal is dispatched; poll operations_get until rollbackState leaves 'rollingBack'."
            }
        },
        "required": ["operationId"]
    })
}
