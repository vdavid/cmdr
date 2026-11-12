//! Where an app path sits in a volume's importance store, and back.
//!
//! An importance store keeps its volume's index paths: volume-relative
//! (`/documents` for `/Volumes/testshare-1/documents`), absolute only on the boot disk,
//! whose root is `/`. A cloud drive's store is relative to the DRIVE's root,
//! though `root`'s index walks it, so every caller passes the importance
//! volume's own root (from [`super::resolve::volume_root`]), ❌ never the
//! index's routing. Every path that crosses into or out of a store goes
//! through here: the agent's importance tools and the visit recorder.

use cmdr_fs::volume::app_paths::rebase;

/// `app_path` in the store's path space, or `None` when it isn't on the volume
/// rooted at `root`.
pub(crate) fn to_store_path(app_path: &str, root: &str) -> Option<String> {
    rebase(app_path, root, "/")
}

/// A store row's app path: `store_path` joined onto the volume's `root`.
pub(crate) fn from_store_path(store_path: &str, root: &str) -> Option<String> {
    rebase(store_path, "/", root)
}

#[cfg(test)]
#[path = "store_path_tests.rs"]
mod tests;
