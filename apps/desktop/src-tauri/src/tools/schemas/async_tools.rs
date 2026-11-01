//! Async (`await`) tool schema.

use serde_json::{Value, json};

pub fn await_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "pane": {
                "type": "string",
                "enum": ["left", "right"],
                "description": "Which pane to watch. Required for the pane conditions; ignored for index_status / operation_complete / operations_idle."
            },
            "condition": {
                "type": "string",
                "enum": ["has_item", "not_has_item", "item_count_gte", "item_count_lte", "path", "path_contains", "index_status", "operation_complete", "operations_idle"],
                "description": "Condition to wait for: has_item / not_has_item (file list contains / no longer contains item named value — use not_has_item after a delete), item_count_gte / item_count_lte (file list has >= / <= value items), path (pane path equals value), path_contains (pane path contains value), index_status (volumeId's indexing freshness equals value: fresh / scanning / stale), operation_complete (the operation whose id is value settled — completed / cancelled / failed, reported in the result), operations_idle (no operation is running or queued; takes no value)"
            },
            "volumeId": {
                "type": "string",
                "description": "For index_status: the volume whose indexing freshness to watch (for example 'root', 'smb-…', 'mtp-…:1')."
            },
            "value": {
                "type": "string",
                "description": "Value for the condition (item name, count, path, substring, an index_status status fresh / scanning / stale, or for operation_complete the operationId). Not used by operations_idle."
            },
            "timeoutSeconds": {
                "type": "integer",
                "description": "Timeout in seconds (default 15, max 60)"
            },
            "afterGeneration": {
                "type": "integer",
                "description": "Only consider state updates after this generation number. Prevents matching stale state from before an action. Get the current generation from cmdr://state or a previous await result. Pane conditions only."
            }
        },
        "required": ["condition"]
    })
}
