//! Memory-diagnostics tool schema.

use serde_json::{Value, json};

pub fn memory_diagnostics_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "sizesPerTag": {
                "type": "integer",
                "minimum": 0,
                "maximum": 24,
                "description": "Region-size groups to report per tag (default 8, clamped to 24). 0 asks for tag totals only. A repeated exact region size fingerprints whatever asked for those bytes."
            }
        },
        "required": [],
        "additionalProperties": false
    })
}
