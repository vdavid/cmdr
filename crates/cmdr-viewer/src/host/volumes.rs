//! Which volume serves a path, for the viewer's open.
//!
//! Not the listing engine's `ListingVolumes`: the viewer needs only the registered volume
//! and the routing check, and sharing a trait would couple two engines through methods
//! neither needs.

use std::path::Path;
use std::sync::Arc;

use cmdr_fs::volume::Volume;

/// Answers "which volume serves this?" for the viewer.
pub trait ViewerVolumes: Send + Sync {
    /// The volume registered under `volume_id`, with no path routing.
    fn get(&self, volume_id: &str) -> Option<Arc<dyn Volume>>;

    /// Whether a route serves `path` rather than its parent's volume: inside an archive,
    /// a repo's virtual `.git` trees, or a backup snapshot. A string check, no I/O. Such a
    /// path has no file of its own on any volume the viewer could read by position.
    fn routes_over_its_parent(&self, path: &Path) -> bool;
}

/// Nothing registered and nothing routed.
pub struct NoVolumes;

impl ViewerVolumes for NoVolumes {
    fn get(&self, _volume_id: &str) -> Option<Arc<dyn Volume>> {
        None
    }

    fn routes_over_its_parent(&self, _path: &Path) -> bool {
        false
    }
}
