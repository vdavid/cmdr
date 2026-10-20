//! The backend's refusal to write through a route that has no write of its own.
//!
//! A path inside a `.zip` or inside a repo's virtual `.git/<category>/` trees is
//! served by a read-only routed volume (`OperationVolumes::resolve`). The entry
//! points that act on the drive itself (bulk rename, rename-as-move, the numbered
//! create, and the `.git` half of the instant rename and create) would otherwise
//! hand such a path straight to the parent volume, which writes a real entry
//! inside the repository's `.git` folder or treats an archive path as a plain
//! folder. The frontend refuses these in a routed pane; this holds the line for
//! every other caller (Ask Cmdr, MCP).
//!
//! The app routes a category folder ITSELF too (`cmdr_git::portal_route`), so a
//! `.git/` listing's `worktrees` row, whose path is git's real folder, can't be
//! renamed out from under every linked worktree (#529). The app-side cells in
//! `operations_host/portal_rename_tests.rs` pin that; `FakeVolumes` routes only
//! strictly under its prefix.

use std::path::Path;
use std::sync::Arc;

use cmdr_fs::volume::{RoutedKind, Volume, VolumeError};

use super::host::EngineHost;
use super::mutation_error::MutationError;
use super::types::{ReadOnlySide, WriteOperationError};

/// The read-only volume a route serves `path` with on `volume_id`, and which
/// route it is, or `None` when the drive itself serves it. The `.zip` file
/// itself is a plain file on its drive and answers `None`.
pub(crate) async fn route_serving(
    host: &EngineHost,
    volume_id: &str,
    path: &Path,
) -> Option<(Arc<dyn Volume>, RoutedKind)> {
    let volumes = host.volumes();
    // Pure string gate, so an ordinary path pays nothing.
    if !volumes.routes_over_its_parent(path) {
        return None;
    }
    let resolved = volumes.resolve(volume_id, path).await;
    Some((resolved.volume?, resolved.routed?))
}

/// The first of `paths` a route serves, as [`route_serving`] answers it.
pub(crate) async fn first_routed<'a>(
    host: &EngineHost,
    volume_id: &str,
    paths: impl IntoIterator<Item = &'a Path>,
) -> Option<(Arc<dyn Volume>, RoutedKind)> {
    for path in paths {
        if let Some(found) = route_serving(host, volume_id, path).await {
            return Some(found);
        }
    }
    None
}

/// The transfer-shaped refusal: the routed volume, named for the side that
/// touched it, as the copy and move routes word a `.git` refusal.
pub(crate) fn routed_transfer_refusal(routed: &Arc<dyn Volume>, side: ReadOnlySide) -> WriteOperationError {
    WriteOperationError::ReadOnlyDevice {
        path: routed.root().display().to_string(),
        device_name: Some(routed.name().to_string()),
        side,
    }
}

/// The instant-op refusal for a write at `path` that a route serves.
pub(crate) fn routed_mutation_refusal(path: &Path) -> MutationError {
    MutationError::Volume {
        error: VolumeError::ReadOnly(path.display().to_string()),
    }
}
