//! Which source answers a `list_dir`, across every member of the folder's share.
//!
//! A read addressed to one member of a proven same-share, same-account group
//! (`location::same_share`) is answered by the first member and source, in order:
//!
//! 1. a LIVE index that lists the folder (`source: index`),
//! 2. one live readdir of a connected member (`source: live`, `live.rs`),
//! 3. the FRESHEST index a not-connected member left on disk (`source: offlineIndex`).
//!
//! The volume asked comes first within each tier, so a group of one reads exactly
//! as a single volume does. An answer from a sibling carries `via: { volumeId }`
//! naming the volume asked, beside its own `location`: the model never picks the
//! 220-folder offline index over the live share, and never has to know there
//! were two. With nothing able to answer: `volumeNotConnected`.

use std::sync::Arc;

use cmdr_fs::volume::Volume;
use tokio_util::sync::CancellationToken;

use super::live::LiveReadConfig;
use super::{
    GatePolicy, IndexReach, ListOptions, LiveAsk, VolumeBlock, VolumeView, answer_from_index, answer_live, to_value,
};
use crate::index_host::index;
use crate::location::miss::volume_not_connected;
use crate::location::resolve::{ResolvedLocation, VolumeFacts};
use crate::location::same_share::Via;
use crate::tools::resources::volumes::space_summary;
use crate::tools::{ToolError, ToolResult};

/// What a live read needs from the app, injected so the tests can stand a temp
/// dir in for a mount.
pub(crate) struct LiveDeps<'a> {
    pub cancel: &'a CancellationToken,
    pub cfg: LiveReadConfig,
    pub policy: GatePolicy,
    /// The registered volume by id, `None` when it left since the facts were read.
    pub volume_for: &'a (dyn Fn(&str) -> Option<Arc<dyn Volume>> + Sync),
}

/// Answer a `list_dir` on `resolved` from the best source any member of its share
/// has. See the module doc for the order.
pub(crate) async fn answer_read(
    facts: &VolumeFacts,
    resolved: &ResolvedLocation,
    opts: &ListOptions,
    deps: LiveDeps<'_>,
) -> ToolResult {
    let readers = facts.readers(resolved);

    for reader in readers.iter().filter(|r| r.live) {
        let index_id = reader.index.volume_id.as_str();
        let rows = index()
            .list_children(index_id, &reader.location.path)
            .map_err(|e| ToolError::internal(e.to_string()))?;
        if let Some(rows) = rows {
            // Resolved ONCE, then asked per row: without the walked ground, a folder
            // mid-walk reads as a settled floor.
            let mut view = connected_view(reader);
            view.walked_roots = index().status(index_id).map(|s| s.walked_roots).unwrap_or_default();
            return stamped(
                answer_from_index(&reader.location, index_id, &rows, &view, opts),
                reader,
            );
        }
    }

    if let Some(reader) = readers.iter().find(|r| r.live) {
        let view = connected_view(reader);
        let answer = answer_live(LiveAsk {
            resolved: reader,
            facts,
            opts,
            view: &view,
            volume: (deps.volume_for)(&reader.location.volume_id),
            cancel: deps.cancel,
            cfg: deps.cfg,
            policy: deps.policy,
        })
        .await;
        return stamped(answer, reader);
    }

    // Freshest first; stable, so the volume asked wins a tie.
    let mut offline: Vec<&ResolvedLocation> = readers.iter().filter(|r| r.index.stored).collect();
    offline.sort_by_key(|r| std::cmp::Reverse(r.index.stored_as_of));
    for reader in offline {
        let index_id = reader.index.volume_id.as_str();
        let rows = index()
            .list_children(index_id, &reader.location.path)
            .map_err(|e| ToolError::internal(e.to_string()))?;
        if let Some(rows) = rows {
            // No walk runs on a volume with no instance, so nothing on the page moves.
            let view = VolumeView {
                enabled: false,
                freshness: None,
                block: VolumeBlock::new(
                    reader.location.volume_id.clone(),
                    space_summary(&reader.location.volume_id),
                ),
                walked_roots: Vec::new(),
                reach: IndexReach::Offline {
                    name: reader.name.clone(),
                    as_of: reader.index.stored_as_of,
                },
            };
            return stamped(
                answer_from_index(&reader.location, index_id, &rows, &view, opts),
                reader,
            );
        }
    }

    to_value(&volume_not_connected(facts, resolved))
}

/// A connected member's view: its index's own status, and its space as polled.
fn connected_view(reader: &ResolvedLocation) -> VolumeView {
    let status = index().volume_status(&reader.index.volume_id);
    VolumeView {
        enabled: status.enabled,
        freshness: status.freshness,
        block: VolumeBlock::new(
            reader.location.volume_id.clone(),
            space_summary(&reader.location.volume_id),
        ),
        walked_roots: Vec::new(),
        reach: IndexReach::Connected,
    }
}

/// Marks an answer a sibling gave with `via`, the volume asked: its misses too,
/// so a live read that stalled on the sibling doesn't read as the asked one's.
fn stamped(answer: ToolResult, reader: &ResolvedLocation) -> ToolResult {
    let mut value = answer?;
    if let (Some(asked), Some(object)) = (&reader.via, value.as_object_mut()) {
        let via = Via {
            volume_id: asked.clone(),
        };
        object.insert(
            "via".to_string(),
            serde_json::to_value(via).map_err(|e| ToolError::internal(e.to_string()))?,
        );
    }
    Ok(value)
}
