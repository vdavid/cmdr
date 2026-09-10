//! Which of a phone's directories a drive-index walk descends into.
//!
//! A phone's `/` holds the storage a person keeps files in beside trees no size
//! roll-up wants: the kernel's views (`/proc`, `/sys`, `/dev`), the system image
//! and its mounts, private app data, and second paths Android mounts onto the same
//! storage (`/storage/emulated`, `/storage/self`, `/mnt/user/…`). A walk over all
//! of it would count every photo once per alias and chase `/sys` symlink loops.
//!
//! So a walk descends the shared storage, reached through the `/sdcard` link the
//! pane browses, and each SD card under `/storage`. Every other directory keeps its
//! row and is left unwalked (`IndexWalk::RowOnly`), the model the index already
//! uses for NAS system folders. `/sdcard` is walked AS a folder, so primary storage
//! lands in the index once, under the spelling the pane shows.
//!
//! ❗ Primary storage is indexed under `/sdcard/…` and only there. A pane browsing
//! `/storage/emulated/0/…` shows no folder sizes, by design: indexing both
//! spellings would count every file twice.

use std::path::Path;

use cmdr_fs::volume::IndexWalk;

use super::AdbVolume;

/// The top-level link Android points at primary shared storage, and the one
/// spelling of that storage an index walk descends.
const SHARED_STORAGE_LINK: &str = "sdcard";

/// Where Android mounts storage volumes: primary storage's own paths, and every SD
/// card.
const STORAGE_ROOT: &str = "storage";

/// The entries under `/storage` that are second paths onto primary storage rather
/// than SD cards.
const STORAGE_ALIASES: [&str; 2] = ["emulated", "self"];

impl AdbVolume {
    /// [`Volume::index_walk`](cmdr_fs::volume::Volume::index_walk) for this phone.
    /// A path this volume can't translate (another phone's) is nothing to walk.
    pub(super) fn index_walk_impl(&self, dir: &Path, is_symlink: bool) -> IndexWalk {
        match self.to_device_path(dir) {
            Ok(device) => index_walk_for_device_path(&device, is_symlink),
            Err(_) => IndexWalk::RowOnly,
        }
    }
}

/// The walk decision for one device path, as a table.
fn index_walk_for_device_path(device: &str, is_symlink: bool) -> IndexWalk {
    // Inside storage the default rule holds.
    let as_storage = IndexWalk::unless_link(is_symlink);
    let parts: Vec<&str> = device.split('/').filter(|part| !part.is_empty()).collect();
    match parts.as_slice() {
        // The root, on the way to storage.
        [] => IndexWalk::Descend,
        // The shared-storage link, walked as the folder it points at.
        [top] if *top == SHARED_STORAGE_LINK => IndexWalk::Descend,
        [top, ..] if *top == SHARED_STORAGE_LINK => as_storage,
        // `/storage` itself, on the way to SD cards.
        [top] if *top == STORAGE_ROOT => as_storage,
        // A second path onto primary storage, which `/sdcard` already covers.
        [top, alias, ..] if *top == STORAGE_ROOT && STORAGE_ALIASES.contains(alias) => IndexWalk::RowOnly,
        // An SD card, and everything on it.
        [top, ..] if *top == STORAGE_ROOT => as_storage,
        // Everything else a phone's root holds.
        _ => IndexWalk::RowOnly,
    }
}
