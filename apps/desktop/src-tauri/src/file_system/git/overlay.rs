//! The portal's half of the pane: the six category rows that join a repo's real
//! `.git/` listing.
//!
//! Everything BELOW `.git/` is a routed volume (`volume.rs`), which a pane, a
//! walker, and a scan all reach the same way. `.git/` itself can't be, because
//! it is a real directory whose real entries must stay editable and deletable;
//! so the six rows reach the pane as a [`ListingOverlay`] contribution instead,
//! and no walker ever sees them. `volume/DETAILS.md` § "Architecture".

use std::path::Path;
use std::sync::Arc;

use crate::file_system::listing::FileEntry;
use crate::file_system::volume::Volume;
use crate::listing_overlays::ListingOverlay;

use super::wiring;

/// Contributes the six virtual category rows to a repo's `.git/` listing.
pub struct GitPortalOverlay;

/// Wires the portal's `.git/` rows into the listing pipeline. Idempotent, so a
/// test can call it beside the app's startup call.
pub fn register() {
    crate::listing_overlays::register_listing_overlay(Arc::new(GitPortalOverlay));
}

impl ListingOverlay for GitPortalOverlay {
    fn id(&self) -> &'static str {
        "git-portal"
    }

    /// The listed directory is called `.git`, it lives on a volume whose paths
    /// `gix` can open, and the portal is switched on.
    ///
    /// ❗ Pure string and flag work, no `stat`: this runs on every listing in
    /// the app. "Is there a repository here?" is
    /// [`extra_entries`](ListingOverlay::extra_entries)'s question.
    ///
    /// A LINKED worktree's `.git` is a gitlink FILE, so listing it fails
    /// `ENOTDIR` and this never runs — the portal lives where `.git` is a
    /// directory. The categories under it still route
    /// (`<linked>/.git/branches` and deeper resolve through the gitlink), so
    /// only the `.git/` landing listing is missing there.
    fn applies_to(&self, volume: &dyn Volume, path: &Path) -> bool {
        wiring::is_virtual_portal_enabled()
            && wiring::volume_holds_real_repos(volume)
            && path.file_name().is_some_and(|name| name == ".git")
    }

    fn extra_entries(&self, _volume: &dyn Volume, path: &Path) -> Vec<FileEntry> {
        let Some(worktree_root) = path.parent() else {
            return Vec::new();
        };
        wiring::portal().category_rows(worktree_root)
    }
}
