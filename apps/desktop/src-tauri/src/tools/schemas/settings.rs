//! Settings tool schema.

use serde_json::{Value, json};

pub fn set_setting_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "id": {
                "type": "string",
                "description": "Setting ID, for example 'appearance.appColor'"
            },
            "value": {
                "description": "New value for the setting"
            }
        },
        "required": ["id", "value"]
    })
}
