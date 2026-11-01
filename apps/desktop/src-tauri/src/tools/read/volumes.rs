//! The `list_volumes` agent tool: the map a model plans from. Every real volume
//! with the `root` its locations start with, how live its session is, and how
//! fresh its index is, so the agent can voice "the NAS isn't connected, so this
//! answer is from its index as of 2026-10-20" honestly (spec §2.4). Favorites are
//! folders on those volumes, so they ride their own `favorites` list.
//!
//! Two sources, one per kind of fact, so a field added for this tool can't reach
//! `cmdr://state`'s YAML by accident:
//!
//! - The shipped `snapshot_volumes_from` core (the rows `cmdr://state`'s
//!   `volumes:` section and the context envelope read), so the session and
//!   freshness tokens can't drift from the rest of the app.
//! - The location resolver's facts (`crate::location`), for what only the agent
//!   reads: each row's `root` from the one root function, the offline `asOf`, and
//!   each favorite's `location`. So a row's `root` is exactly the root a
//!   `list_dir` on that volume checks against.
//!
//! The pure [`to_volume_map`] is what the app-state tool ([`super::state`]) shares
//! to embed the volume list.

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Runtime};

use crate::location::Location;
use crate::location::resolve::{LocationInput, VolumeFacts, resolve};
use crate::search::{format_size, format_timestamp};
use crate::tools::resources::favorites::FavoriteSummary;
use crate::tools::resources::volumes::{VolumeKind, VolumeSummary, current_listing, snapshot_volumes_from};
use crate::tools::{ToolError, ToolResult};

/// One volume as the agent sees it. The honesty-bearing fields are `index_status`
/// (`fresh` / `scanning` / `stale` / `off` — only `fresh` is authoritative) and
/// `connection_state` (`direct` / `os_mount` / `disconnected` / `needs_sign_in` /
/// `needs_host_key_approval` / `saved`), both straight from the shipped snapshot
/// so they match every other surface.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VolumeSnapshot {
    pub name: String,
    pub id: String,
    pub kind: String,
    /// The app-path prefix every location on this volume starts with: a mount
    /// point (`/Volumes/naspi-1`) or a scheme root (`sftp://…`, `mtp://…`). For a
    /// volume that isn't connected, the root it last had. Absent only where no
    /// location lives (the `Network` hub row).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filesystem: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_only: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ejectable: Option<bool>,
    /// Index freshness token: `fresh` / `scanning` / `stale` / `off`. `off` means
    /// the volume has no index to answer from; only `fresh` is authoritative. A
    /// volume that isn't connected but has an index on disk reads `stale`, with
    /// [`as_of`](Self::as_of) saying how old.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index_status: Option<String>,
    /// When a not-connected volume's on-disk index was last completed
    /// (`"2026-10-20"`): the date an answer from it is as of.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub as_of: Option<String>,
    /// How live the volume's session is: `direct` / `os_mount` / `disconnected` /
    /// `needs_sign_in` / `needs_host_key_approval` / `saved`. Absent for anything
    /// with no session (a local disk, the hub row). ❗ Present on every remote
    /// backend, so it is not an "is this SMB" test; `kind` is.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connection_state: Option<String>,
    /// Volume ids proven to be the same share (one server GUID, one share name):
    /// one NAS by two addresses. A read on any of them is answered by whichever
    /// member can, when it opens as the same account. Absent when there are none.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub same_share_as: Vec<String>,
    /// Whether the DEVICE behind the row can be opened right now: `ready` /
    /// `waiting_for_authorization` / `unavailable_offline` /
    /// `unavailable_no_permissions`. Absent for anything that isn't a device.
    /// ❗ A different question from `connection_state`: a phone waiting for its
    /// "Allow USB debugging?" tap is LISTED, so without this the row reads as
    /// browsable and every call against it is a refusal you can't explain.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_readiness: Option<String>,
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

/// One favorite: a folder the user bookmarked, on whichever volume holds it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FavoriteSnapshot {
    pub name: String,
    /// The path as the user saved it, always.
    pub path: String,
    /// Where it lives, when a connected volume holds it: a ready `list_dir` input.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<Location>,
    /// `false` when nothing connected holds it (an unplugged phone, a share that
    /// isn't connected). Absent when it's reachable.
    #[serde(skip_serializing_if = "is_true")]
    pub reachable: bool,
}

fn is_true(value: &bool) -> bool {
    *value
}

/// `list_volumes`'s whole answer.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct VolumeMap {
    pub volumes: Vec<VolumeSnapshot>,
    pub favorites: Vec<FavoriteSnapshot>,
}

/// A saved share's tie to the other rows of its share: the share name they
/// share, and when this one was last connected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ShareRecency {
    pub volume_id: String,
    pub share_name: String,
    /// ISO 8601, so the strings sort in time order.
    pub last_connected_at: String,
}

/// Shape the snapshot, the resolver's facts, and the favorites into the model's
/// map. Pure, so every honesty rule here is a table test.
pub(crate) fn to_volume_map(
    summaries: &[VolumeSummary],
    favorites: &[FavoriteSummary],
    facts: &VolumeFacts,
    shares: &[ShareRecency],
) -> VolumeMap {
    let volumes = summaries.iter().map(|v| to_volume_snapshot(v, facts)).collect();
    VolumeMap {
        volumes: order_same_share_rows(volumes, shares),
        favorites: favorites.iter().map(|f| to_favorite_snapshot(f, facts)).collect(),
    }
}

fn to_volume_snapshot(v: &VolumeSummary, facts: &VolumeFacts) -> VolumeSnapshot {
    let fact = facts.get(&v.id);
    // The one offline vocabulary, shared with `list_dir`'s coverage: a volume
    // that isn't connected but whose index is on disk reads `stale` plus the date
    // it's as of. A live volume's status is its index's own, untouched.
    let offline_as_of = fact
        .filter(|fact| !fact.live && v.index_status == Some("off"))
        .and_then(|fact| fact.index.stored_as_of);
    VolumeSnapshot {
        name: v.name.clone(),
        id: v.id.clone(),
        kind: v.kind.token().to_string(),
        // The resolver's root, so this and a `list_dir` on the volume can't
        // disagree. A row the resolver doesn't know (the hub) holds no location.
        root: fact.map(|fact| fact.root.clone()),
        filesystem: v.filesystem.clone(),
        read_only: v.read_only,
        ejectable: v.ejectable,
        index_status: match offline_as_of {
            Some(_) => Some("stale".to_string()),
            None => v.index_status.map(str::to_string),
        },
        as_of: offline_as_of.map(format_timestamp),
        connection_state: v.connection_state.map(str::to_string),
        same_share_as: facts.same_share_as(&v.id),
        device_readiness: v.device_readiness.map(str::to_string),
        total_bytes: v.space.and_then(|s| s.total_bytes()),
        total_human: v.space.and_then(|s| s.total_bytes()).map(format_size),
        available_bytes: v.space.and_then(|s| s.available_bytes()),
        available_human: v.space.and_then(|s| s.available_bytes()).map(format_size),
    }
}

/// A favorite's `location` is wherever its saved path resolves as a bare path: a
/// connected volume, or nothing.
fn to_favorite_snapshot(f: &FavoriteSummary, facts: &VolumeFacts) -> FavoriteSnapshot {
    let location = resolve(
        facts,
        &LocationInput {
            volume_id: None,
            path: f.path.clone(),
        },
    )
    .ok()
    .map(|resolved| resolved.location);
    FavoriteSnapshot {
        name: f.name.clone(),
        path: f.path.clone(),
        reachable: location.is_some(),
        location,
    }
}

/// Rows of one saved share (the same share name, ignoring case) sit together, at
/// the first one's place, the most recently connected first: the address that
/// worked last is the one a model should try first.
fn order_same_share_rows(rows: Vec<VolumeSnapshot>, shares: &[ShareRecency]) -> Vec<VolumeSnapshot> {
    let share_of = |row: &VolumeSnapshot| shares.iter().find(|share| share.volume_id == row.id);
    let key = |row: &VolumeSnapshot| {
        (row.kind == VolumeKind::Smb.token())
            .then(|| share_of(row))
            .flatten()
            .map(|share| share.share_name.to_lowercase())
    };
    let recency = |row: &VolumeSnapshot| share_of(row).map(|share| share.last_connected_at.clone());

    let mut placed = vec![false; rows.len()];
    let mut out = Vec::with_capacity(rows.len());
    for i in 0..rows.len() {
        if placed[i] {
            continue;
        }
        let Some(group) = key(&rows[i]) else {
            placed[i] = true;
            out.push(rows[i].clone());
            continue;
        };
        let mut members: Vec<usize> = (i..rows.len())
            .filter(|&j| !placed[j] && key(&rows[j]).as_deref() == Some(group.as_str()))
            .collect();
        // Stable, so members connected at the same instant keep the listing's order.
        members.sort_by_key(|&j| std::cmp::Reverse(recency(&rows[j])));
        for j in members {
            placed[j] = true;
            out.push(rows[j].clone());
        }
    }
    out
}

/// Every saved share's tie to its share, from the saved-share store.
fn share_recency() -> Vec<ShareRecency> {
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        crate::network::known_shares::saved_shares()
            .into_iter()
            .map(|row| ShareRecency {
                volume_id: crate::network::known_shares::place_id(&row),
                share_name: row.share_name,
                last_connected_at: row.last_connected_at,
            })
            .collect()
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    Vec::new()
}

/// The live map: one completed listing for the volumes and the favorites, plus
/// the resolver's facts.
pub(crate) async fn snapshot_volume_map() -> VolumeMap {
    let (rows, timed_out) = current_listing().await;
    let summaries = snapshot_volumes_from(&rows).await;
    let favorites = crate::tools::resources::favorites::of_listing(&rows, timed_out);
    let facts = VolumeFacts::now().await;
    to_volume_map(&summaries, &favorites, &facts, &share_recency())
}

/// `list_volumes` takes no parameters.
pub fn list_volumes_schema() -> Value {
    serde_json::json!({ "type": "object", "properties": {}, "additionalProperties": false })
}

/// Handler: snapshot every volume and favorite, and shape them for the model.
pub async fn execute_list_volumes<R: Runtime>(_app: &AppHandle<R>, _params: &Value) -> ToolResult {
    serde_json::to_value(snapshot_volume_map().await).map_err(|e| ToolError::internal(e.to_string()))
}

#[cfg(test)]
#[path = "volumes_tests.rs"]
mod tests;
