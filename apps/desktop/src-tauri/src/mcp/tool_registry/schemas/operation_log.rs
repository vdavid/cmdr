//! Operation-log tool schemas.

use serde_json::{Value, json};

pub fn operations_list_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "since": {
                "type": "integer",
                "description": "Inclusive lower bound on the operation's start time (Unix milliseconds)"
            },
            "until": {
                "type": "integer",
                "description": "Inclusive upper bound on the operation's start time (Unix milliseconds)"
            },
            "name": {
                "type": "string",
                "description": "Match operations that touched an item with this name (folded: case- and Unicode-normalized). Exact or prefix match on the item's source name (see nameMatch), NOT a substring search."
            },
            "nameMatch": {
                "type": "string",
                "enum": ["exact", "prefix"],
                "description": "How 'name' matches: exact folded-name equality, or folded-name prefix. Default: prefix."
            },
            "kind": {
                "type": "string",
                "enum": ["copy", "move", "delete", "trash", "rename", "createFolder", "createFile", "archiveEdit"],
                "description": "Filter by operation kind"
            },
            "initiator": {
                "type": "string",
                "enum": ["user", "aiClient", "agent"],
                "description": "Filter by who initiated the operation"
            },
            "executionStatus": {
                "type": "string",
                "enum": ["queued", "running", "done", "failed", "canceled"],
                "description": "Filter by lifecycle status"
            },
            "rollbackState": {
                "type": "string",
                "enum": ["notRollbackable", "rollbackable", "rollingBack", "rolledBack", "partiallyRolledBack"],
                "description": "Filter by rollback state"
            },
            "limit": {
                "type": "integer",
                "description": "Max operations to return. Default 50, max 1000."
            },
            "offset": {
                "type": "integer",
                "description": "Number of operations to skip, for paging"
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
                "description": "The operation's id. The same id everywhere: from operations_list, a copy/move/delete response, cmdr://state operations, or the queue tool."
            },
            "limit": {
                "type": "integer",
                "description": "Max item rows to return. Default 200, max 1000."
            },
            "offset": {
                "type": "integer",
                "description": "Number of item rows to skip, for paging"
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
