//! The `list_volumes` agent tool: every volume with its index freshness and,
//! for SMB, its connectivity — so the agent can voice "the NAS is disconnected,
//! so this answer is from a stale index" honestly (spec §2.4).
//!
//! It is also where a `search` of anything but the boot volume starts: `search`
//! takes ONE volume per call, addressed by a path in `scope`, so `mount_path` is
//! the field that makes "search my NAS" expressible at all.
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
use crate::search::format_size;

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
    /// Where the volume is mounted, and the path `search`'s `scope` names to cover
    /// this drive rather than the boot one. Absent for a volume with no filesystem
    /// path (MTP storages, the `Network` root), which is also where a search can't
    /// reach, so an absent field reads as "you can't search here".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mount_path: Option<String>,
    /// The volume's capacity in bytes, as last polled. Absent when nothing is
    /// watching this volume, never guessed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_bytes: Option<u64>,
    /// `total_bytes` spelled out (`"2 TB"`). Present exactly when its byte
    /// counterpart is: the agent can't divide by 1,024, so a capacity it has to
    /// state out loud arrives already formatted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_human: Option<String>,
    /// Free bytes, as last polled. Paired with `total_bytes` — both present or
    /// both absent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub available_bytes: Option<u64>,
    /// `available_bytes` spelled out. Present exactly when its byte counterpart is.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub available_human: Option<String>,
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
            mount_path: v.mount_path.clone(),
            total_bytes: v.space.and_then(|s| s.total_bytes()),
            total_human: v.space.and_then(|s| s.total_bytes()).map(format_size),
            available_bytes: v.space.and_then(|s| s.available_bytes()),
            available_human: v.space.and_then(|s| s.available_bytes()).map(format_size),
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
    use crate::file_system::volume::SpaceInfo;
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
            mount_path: Some(format!("/Volumes/{name}")),
            space: None,
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
    fn space_carries_a_human_form_beside_the_bytes() {
        // The agent can't divide by 1,024 reliably, so "how full is it" has to arrive
        // already spelled out; the raw bytes stay for anything that needs arithmetic.
        let mut v = summary("Macintosh HD", VolumeKind::Local, Some("fresh"), None);
        v.space = Some(SpaceInfo::bounded(2_000_000_000_000, 214_300_000_000));
        let out = to_volume_snapshots(&[v]);
        assert_eq!(out[0].total_bytes, Some(2_000_000_000_000));
        assert_eq!(
            out[0].total_human.as_deref(),
            Some(format_size(2_000_000_000_000)).as_deref()
        );
        assert_eq!(
            out[0].available_human.as_deref(),
            Some(format_size(214_300_000_000)).as_deref()
        );
    }

    #[test]
    fn an_unwatched_volume_omits_the_human_form_too() {
        // Present exactly when the byte counterparts are: "0 B free" would read as a
        // full disk.
        let out = to_volume_snapshots(&[summary("Backup HD", VolumeKind::Local, Some("off"), None)]);
        assert_eq!(out[0].total_bytes, None);
        assert_eq!(out[0].total_human, None);
        assert_eq!(out[0].available_human, None);
    }

    #[test]
    fn a_drive_carries_the_path_a_search_scope_needs() {
        // Without this the model can name a drive but can't search it: `search` takes a
        // path in `scope`, and every other field here is a name, an id, or a token.
        let mut nas = summary("naspi", VolumeKind::Smb, Some("stale"), Some("direct"));
        nas.mount_path = Some("/Volumes/naspi".to_string());
        let json = serde_json::to_value(&to_volume_snapshots(&[nas])[0]).unwrap();
        assert_eq!(json["mountPath"], "/Volumes/naspi");
    }

    #[test]
    fn a_volume_with_no_filesystem_path_omits_it_rather_than_offering_an_unsearchable_one() {
        // An MTP storage and the synthetic `Network` root have no path a search can
        // walk. An absent `mountPath` says "you can't scope a search here"; a made-up
        // one would send the model off to search nothing.
        let mut phone = summary("Pixel 8", VolumeKind::Mtp, Some("off"), None);
        phone.mount_path = None;
        let json = serde_json::to_value(&to_volume_snapshots(&[phone])[0]).unwrap();
        assert!(json.get("mountPath").is_none());
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
