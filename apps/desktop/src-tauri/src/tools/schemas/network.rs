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
                "description": "Volume ID to detach (for example 'vol-…', 'smb-…', 'mtp-…:1', or 'sftp-…'). See cmdr://state volumes."
            },
            "scope": {
                "type": "string",
                "enum": ["volume", "disk"],
                "description": "How much of a physical disk to take down. 'volume' (the default) takes this volume alone while its disk keeps others mounted, and the disk with the last one. 'disk' takes every volume on the disk and the disk, refusing first with outcome 'heldBeforeEject' when someone is holding one."
            }
        },
        "required": ["volumeId"]
    })
}
