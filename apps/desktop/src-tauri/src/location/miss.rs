//! A location miss as a tool result: an `Ok` carrying a typed token, never a
//! `ToolError`.
//!
//! `view::dispatch` flattens a `ToolError` to `{ problem }`, which a model can't
//! branch on, so every tool that can't place or read a location answers this one
//! shape (`tools/DETAILS.md` § The location contract):
//! `{ miss, location, detail, root?, nearby?, sameShareAs? }`.

use serde::{Deserialize, Serialize};

use super::resolve::{LocationInput, LocationMiss, NearbyVolume, ResolvedLocation, VolumeFacts, nearby};

/// What kind of miss this is: the token a model branches on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum MissToken {
    UnknownVolume,
    PathOutsideVolume,
    NoVolumeAtPath,
    /// The volume resolved, but it isn't connected and no source the tool has can
    /// answer for it.
    VolumeNotConnected,
    /// Reading the place would raise a macOS privacy prompt (the Full Disk Access
    /// question is still open), or macOS privacy refused the read.
    NeedsDiskAccess,
    /// The volume didn't answer in time, or an earlier read on it is still stuck.
    Unreachable,
    /// Nothing is at this path on the volume, or it leads off the volume.
    NotFound,
    /// The place is a system, build, or cache folder a default search leaves out.
    Excluded,
    /// The folder's own permissions refuse the read (not macOS privacy).
    PermissionDenied,
    /// The user stopped the turn while the place was being read.
    Cancelled,
}

impl LocationMissResult {
    /// A miss on a place that resolved: `location` names the volume.
    pub(crate) fn on(location: &super::Location, miss: MissToken, detail: String) -> Self {
        Self {
            miss,
            location: LocationInput {
                volume_id: Some(location.volume_id.clone()),
                path: location.path.clone(),
            },
            detail,
            root: None,
            nearby: None,
            same_share_as: None,
        }
    }
}

/// The wire shape of every location miss.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LocationMissResult {
    pub miss: MissToken,
    /// The place asked about, tilde-expanded (or rebased, for a resolved volume).
    pub location: LocationInput,
    /// One sentence naming the next move: a miss with none gets retried verbatim.
    pub detail: String,
    /// The named volume's root, for `pathOutsideVolume`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root: Option<String>,
    /// Where to look instead, on every not-connected miss (possibly empty).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nearby: Option<Vec<NearbyVolume>>,
    /// Volume ids proven to be the same share as the volume asked (one server GUID,
    /// one share name: `same_share.rs`), on every not-connected miss. Empty for a
    /// bare path, which names no volume. A same-account sibling that could answer
    /// already did, so one listed here is a hint: another account, or one that
    /// isn't connected either.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub same_share_as: Option<Vec<String>>,
}

impl From<LocationMiss> for LocationMissResult {
    fn from(miss: LocationMiss) -> Self {
        match miss {
            LocationMiss::UnknownVolume { requested } => Self {
                miss: MissToken::UnknownVolume,
                detail: format!(
                    "No volume has the id `{}`. Call list_volumes for the ids, or send the path alone.",
                    requested.volume_id.as_deref().unwrap_or_default()
                ),
                location: requested,
                root: None,
                nearby: None,
                same_share_as: None,
            },
            LocationMiss::PathOutsideVolume { requested, root } => Self {
                miss: MissToken::PathOutsideVolume,
                detail: format!(
                    "That path isn't on this volume: its locations start with `{root}`. Send a path under that root, or the path alone to find its volume."
                ),
                location: requested,
                root: Some(root),
                nearby: None,
                same_share_as: None,
            },
            LocationMiss::NoVolumeAtPath { requested, nearby } => Self {
                miss: MissToken::NoVolumeAtPath,
                detail: if nearby.is_empty() {
                    "Nothing is mounted at this path right now, so there's nothing to read there. Call list_volumes to see what's connected.".to_string()
                } else {
                    "Nothing is mounted at this path right now, so there's nothing to read there. Retry with a `candidate` from `nearby`, or tell the user the volume needs connecting.".to_string()
                },
                location: requested,
                root: None,
                nearby: Some(nearby),
                same_share_as: Some(Vec::new()),
            },
        }
    }
}

/// The miss a tool answers when its location resolved onto a volume that isn't
/// connected and none of the tool's sources can answer for it.
pub(crate) fn volume_not_connected(facts: &VolumeFacts, resolved: &ResolvedLocation) -> LocationMissResult {
    let nearby = nearby(facts, &resolved.location.path, Some(&resolved.location.volume_id));
    let detail = if nearby.is_empty() {
        format!(
            "{} isn't connected right now, so nothing can read it. Tell the user it needs connecting first.",
            resolved.name
        )
    } else {
        format!(
            "{} isn't connected right now, so nothing can read it. Retry with a `candidate` from `nearby`, or tell the user it needs connecting first.",
            resolved.name
        )
    };
    LocationMissResult {
        miss: MissToken::VolumeNotConnected,
        location: LocationInput {
            volume_id: Some(resolved.location.volume_id.clone()),
            path: resolved.location.path.clone(),
        },
        detail,
        root: None,
        nearby: Some(nearby),
        same_share_as: Some(facts.same_share_as(&resolved.location.volume_id)),
    }
}
