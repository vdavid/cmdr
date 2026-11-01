//! Where a `search` call covers, placed by the one location resolver
//! (`crate::location`), so a wrong or unreachable place answers the same typed
//! miss `list_dir` does. Pure over [`VolumeFacts`]; `placement_tests.rs` pins it.
//!
//! - **No `volumeId`**: the scope routes the run exactly as before. The resolver
//!   only catches a scope path under a mount area with nothing mounted there
//!   (`noVolumeAtPath`), which would otherwise route to the boot disk and walk a
//!   folder that isn't the user's drive. ❌ Not a miss for any other bare path: a
//!   phone's or a server's scheme path routes by its own spelling, connected or
//!   not.
//! - **A `volumeId`**: every scope path must resolve onto that volume (a rebased
//!   one runs at its current root), and no scope means the volume's root. A
//!   volume that isn't connected answers from its index on disk, and a
//!   not-connected one without an index is `volumeNotConnected`.

use crate::location::miss::{LocationMissResult, volume_not_connected};
use crate::location::resolve::{LocationInput, LocationMiss, VolumeFacts, resolve};

/// What the run takes from the placement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Placement {
    /// The scope's include paths as the run takes them. `None` searches the
    /// target volume whole.
    pub include_paths: Option<Vec<String>>,
    /// The index the named volume's rows live in (`root`'s for a cloud drive), so
    /// the run doesn't route the paths again. `None` routes by the scope.
    pub on_index: Option<String>,
    /// Set when the named volume isn't connected, so the answer can say it's
    /// read off the index rather than the drive.
    pub offline: Option<Offline>,
}

/// A not-connected volume the run answers for from its index on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Offline {
    pub name: String,
    /// When its index's last walk finished (unix seconds), if one ever did.
    pub as_of: Option<u64>,
}

/// Place a call's scope. `include_paths` is the scope's includes, tilde-expanded
/// (`search::parse_scope`); `None` when the scope names none. The miss is what the
/// call answers instead of a search, boxed because it's the rare, large side.
pub(crate) fn place(
    facts: &VolumeFacts,
    volume_id: Option<&str>,
    include_paths: Option<Vec<String>>,
) -> Result<Placement, Box<LocationMissResult>> {
    let Some(volume_id) = volume_id else {
        return place_bare(facts, include_paths);
    };
    let whole = include_paths.as_ref().is_none_or(Vec::is_empty);
    let paths = match include_paths.filter(|paths| !paths.is_empty()) {
        Some(paths) => paths,
        // An unknown id still resolves below, into its `unknownVolume` miss.
        None => vec![facts.get(volume_id).map_or_else(|| "/".to_string(), |v| v.root.clone())],
    };
    let mut placed = Vec::with_capacity(paths.len());
    for path in paths {
        let input = LocationInput {
            volume_id: Some(volume_id.to_string()),
            path,
        };
        placed.push(resolve(facts, &input).map_err(|miss| Box::new(miss.into()))?);
    }
    let first = &placed[0];
    if !first.live && !first.index.stored {
        return Err(Box::new(volume_not_connected(facts, first)));
    }
    Ok(Placement {
        // The boot disk whole is the unscoped search it always was.
        include_paths: (!(whole && first.root == "/"))
            .then(|| placed.iter().map(|resolved| resolved.location.path.clone()).collect()),
        on_index: Some(first.index.volume_id.clone()),
        offline: (!first.live).then(|| Offline {
            name: first.name.clone(),
            as_of: first.index.stored_as_of,
        }),
    })
}

fn place_bare(facts: &VolumeFacts, include_paths: Option<Vec<String>>) -> Result<Placement, Box<LocationMissResult>> {
    for path in include_paths.iter().flatten() {
        if !facts.under_mount_area(path) {
            continue;
        }
        let input = LocationInput {
            volume_id: None,
            path: path.clone(),
        };
        if let Err(miss @ LocationMiss::NoVolumeAtPath { .. }) = resolve(facts, &input) {
            return Err(Box::new(miss.into()));
        }
    }
    Ok(Placement {
        include_paths,
        on_index: None,
        offline: None,
    })
}

/// The note an offline answer leads with: where it came from, and how old.
pub(crate) fn offline_note(offline: &Offline) -> String {
    match offline.as_of {
        Some(at) => format!(
            "Note: {} isn't connected, so this answers from its index as of {}, and can't see changes since. Tell the user it needs connecting for a live answer.",
            offline.name,
            crate::search::format_timestamp(at)
        ),
        None => format!(
            "Note: {} isn't connected, so this answers from what its index holds, which never finished a full walk. Tell the user it needs connecting for a live answer.",
            offline.name
        ),
    }
}

#[cfg(test)]
#[path = "placement_tests.rs"]
mod tests;
