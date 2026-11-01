//! CPU-diagnostics tool schema.

use serde_json::{Value, json};

pub fn cpu_diagnostics_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "action": {
                "type": "string",
                "enum": ["measure", "mark", "sinceMark"],
                "description": "measure (default): block for windowSeconds, then report. mark: store a baseline and return at once. sinceMark: report from the last mark to now, without blocking; the mark stays, so repeated reads are cumulative."
            },
            "windowSeconds": {
                "type": "integer",
                "minimum": 1,
                "maximum": 60,
                "description": "measure only: the window length (default 10). Longer windows: mark, then sinceMark."
            },
            "maxThreads": {
                "type": "integer",
                "minimum": 0,
                "maximum": 200,
                "description": "Individual threads to list, costliest first (default 20). threadsByName always lists every name."
            }
        },
        "required": [],
        "additionalProperties": false
    })
}
