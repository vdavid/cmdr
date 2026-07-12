//! The `list_volumes` agent tool: every volume with its index freshness and,
//! for SMB, its connectivity — so the agent can voice "the NAS is disconnected,
//! so this answer is from a stale index" honestly (spec §2.4).
//!
//! Reuses the shipped `snapshot_volumes` core (the same data `cmdr://state`'s
//! `volumes:` section and the context envelope read), so the tokens can't drift
//! from the rest of the app. The pure [`to_volume_snapshots`] mapper is what the
//! app-state tool ([`super::state`]) shares to embed the volume list.

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Runtime};

use crate::mcp::resources::volumes::{VolumeSummary, snapshot_volumes};
use crate::mcp::{ToolError, ToolResult};

/// One volume as the agent sees it. The honesty-bearing fields are `index_status`
/// (`fresh` / `scanning` / `stale` / `off` — only `fresh` is authoritative) and
/// `smb_connection_state` (`direct` / `os_mount` / `disconnected`), both straight
/// from the shipped snapshot so they match every other surface.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VolumeSnapshot {
    pub name: String,
    pub id: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filesystem: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_only: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ejectable: Option<bool>,
    /// Index freshness token: `fresh` / `scanning` / `stale` / `off`. `off` means
    /// the volume isn't indexed; only `fresh` is authoritative.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index_status: Option<String>,
    /// SMB connection state: `direct` / `os_mount` / `disconnected`. Absent off SMB.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub smb_connection_state: Option<String>,
}

/// Map the shipped [`VolumeSummary`] snapshot into the agent's typed result. Pure,
/// so the honesty tokens are unit-testable without a live volume set.
pub(crate) fn to_volume_snapshots(summaries: &[VolumeSummary]) -> Vec<VolumeSnapshot> {
    summaries
        .iter()
        .map(|v| VolumeSnapshot {
            name: v.name.clone(),
            id: v.id.clone(),
            kind: v.kind.token().to_string(),
            filesystem: v.filesystem.clone(),
            read_only: v.read_only,
            ejectable: v.ejectable,
            index_status: v.index_status.map(|s| s.to_string()),
            smb_connection_state: v.smb_connection_state.map(|s| s.to_string()),
        })
        .collect()
}

/// `list_volumes` takes no parameters.
pub fn list_volumes_schema() -> Value {
    serde_json::json!({ "type": "object", "properties": {}, "additionalProperties": false })
}

/// Handler: snapshot every volume and shape it for the model.
pub async fn execute_list_volumes<R: Runtime>(_app: &AppHandle<R>, _params: &Value) -> ToolResult {
    let volumes = to_volume_snapshots(&snapshot_volumes().await);
    serde_json::to_value(serde_json::json!({ "volumes": volumes })).map_err(|e| ToolError::internal(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::resources::volumes::VolumeKind;

    fn summary(
        name: &str,
        kind: VolumeKind,
        index_status: Option<&'static str>,
        smb: Option<&'static str>,
    ) -> VolumeSummary {
        VolumeSummary {
            name: name.to_string(),
            id: name.to_lowercase(),
            kind,
            filesystem: None,
            read_only: None,
            ejectable: None,
            index_status,
            smb_connection_state: smb,
        }
    }

    #[test]
    fn tokens_pass_through_verbatim_including_off_and_disconnected() {
        // The honesty tokens must survive the mapping unchanged: a stale, disconnected
        // SMB share and an unindexed (`off`) local disk both read honestly.
        let out = to_volume_snapshots(&[
            summary("NAS", VolumeKind::Smb, Some("stale"), Some("disconnected")),
            summary("Scratch", VolumeKind::Local, Some("off"), None),
        ]);
        assert_eq!(out[0].kind, "smb");
        assert_eq!(out[0].index_status.as_deref(), Some("stale"));
        assert_eq!(out[0].smb_connection_state.as_deref(), Some("disconnected"));
        assert_eq!(out[1].kind, "local");
        assert_eq!(out[1].index_status.as_deref(), Some("off"));
        assert_eq!(out[1].smb_connection_state, None);
    }

    #[test]
    fn serializes_camel_case_and_omits_absent_fields() {
        let json = serde_json::to_value(
            &to_volume_snapshots(&[summary("Macintosh HD", VolumeKind::Local, Some("fresh"), None)])[0],
        )
        .unwrap();
        assert_eq!(json["indexStatus"], "fresh");
        assert_eq!(json["kind"], "local");
        // Absent optionals don't clutter the payload the model reads.
        assert!(json.get("smbConnectionState").is_none());
        assert!(json.get("filesystem").is_none());
    }
}
