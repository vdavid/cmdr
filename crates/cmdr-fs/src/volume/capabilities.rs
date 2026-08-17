//! What a volume can do, as data.
//!
//! [`VolumeCapabilities`] is the published form of the capability predicates on
//! the [`Volume`](super::Volume) trait: one struct, built by
//! [`Volume::capabilities`](super::Volume::capabilities), that travels over IPC
//! so the frontend receives capability as DATA rather than re-deriving it from a
//! volume id, a filesystem-type string, or a category.
//!
//! Only what a CONSUMER OUTSIDE the backend acts on belongs here. The predicates
//! that steer the operations engine (`max_concurrent_ops`,
//! `operations_are_local`, `listing_watch_coverage`, `space_poll_interval`,
//! `paths_are_os_visible`, …) stay on the trait and are read where they're used;
//! publishing one nobody reads is a field that drifts with nothing to notice.

use serde::{Deserialize, Serialize};

/// What a volume can do, from the frontend's point of view.
///
/// A claim about the BACKEND, not about one path or one mount: a read-only
/// mount of a writable backend still answers `is_writable: true`, and the
/// per-location `isReadOnly` flag layers on top of this.
///
/// Adding a capability means adding a predicate to
/// [`Volume`](super::Volume) and folding it in
/// [`Volume::capabilities`](super::Volume::capabilities), never computing a new
/// answer here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct VolumeCapabilities {
    /// Files and folders can be created, renamed, and deleted here.
    pub is_writable: bool,
    /// Files can be read out of here, so this volume can be the SOURCE of a copy
    /// or a move.
    pub can_export: bool,
}
