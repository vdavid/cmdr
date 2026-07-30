//! What a successful mutation means for the listing cache.
//!
//! `Volume::notify_mutation` is fire-and-forget: after a create, delete, rename,
//! or `write_from_stream` lands, the backend tells the cache so the pane updates
//! without waiting for a watcher event (SMB and MTP watchers are lossy under
//! load). The trait's default is a no-op — `cmdr-fs` doesn't know this cache
//! exists — so each backend supplies the meaning. This module supplies it for
//! the backends whose paths are real `std::fs` paths.

use std::path::Path;

use crate::file_system::listing::caching::{DirectoryChange, notify_directory_changed};
use crate::file_system::listing::reading::get_single_entry;
use crate::file_system::volume::MutationEvent;

/// Patches the listing cache after a mutation on a LOCAL-FS-backed volume: stats
/// the affected entry through `std::fs` and turns it into the right
/// [`DirectoryChange`].
///
/// This is what `Volume::notify_mutation` means for a backend whose paths are
/// real `std::fs` paths. It lives here rather than on the trait because
/// `cmdr-fs` knows nothing about the listing cache; a backend on a protocol that
/// can answer `get_metadata` faster than `std::fs` would (SMB, MTP) builds its
/// own entry and calls [`notify_directory_changed`] directly instead.
///
/// Fire-and-forget: a failed stat logs and drops the patch, because the mutation
/// it follows has already succeeded and a watcher event may still land.
pub fn patch_listing_after_local_mutation(volume_id: &str, parent_path: &Path, mutation: MutationEvent) {
    match mutation {
        MutationEvent::Created(ref name) | MutationEvent::Modified(ref name) => {
            let entry_path = parent_path.join(name);
            match get_single_entry(&entry_path) {
                Ok(entry) => {
                    let change = if matches!(mutation, MutationEvent::Created(_)) {
                        DirectoryChange::Added(entry)
                    } else {
                        DirectoryChange::Modified(entry)
                    };
                    notify_directory_changed(volume_id, parent_path, change);
                }
                Err(e) => {
                    log::warn!("notify_mutation: couldn't stat {}: {}", entry_path.display(), e);
                }
            }
        }
        MutationEvent::Deleted(name) => {
            notify_directory_changed(volume_id, parent_path, DirectoryChange::Removed(name));
        }
        MutationEvent::Renamed { from, to } => {
            let new_path = parent_path.join(&to);
            match get_single_entry(&new_path) {
                Ok(entry) => {
                    notify_directory_changed(
                        volume_id,
                        parent_path,
                        DirectoryChange::Renamed {
                            old_name: from,
                            new_entry: entry,
                        },
                    );
                }
                Err(e) => {
                    log::warn!(
                        "notify_mutation: couldn't stat renamed entry {}: {}",
                        new_path.display(),
                        e
                    );
                }
            }
        }
    }
}
