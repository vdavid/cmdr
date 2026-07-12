//! Network (SMB) and eject tool schemas.

use serde_json::{Value, json};

pub fn connect_to_server_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "address": {
                "type": "string",
                "description": "Server address: hostname, IP, IP:port, or smb:// URL"
            }
        },
        "required": ["address"]
    })
}

pub fn remove_manual_server_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "hostId": {
                "type": "string",
                "description": "Host ID to remove (for example, manual-192-168-1-100-9445)"
            }
        },
        "required": ["hostId"]
    })
}

pub fn upgrade_smb_to_direct_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "volumeId": {
                "type": "string",
                "description": "Volume ID of the SMB share (e.g. smb-192-168-1-111-445-naspi). See cmdr://state volumes."
            }
        },
        "required": ["volumeId"]
    })
}

pub fn eject_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "volumeId": {
                "type": "string",
                "description": "Volume ID to eject (for example 'smb-…' or 'mtp-…:1'). See cmdr://state volumes."
            }
        },
        "required": ["volumeId"]
    })
}
