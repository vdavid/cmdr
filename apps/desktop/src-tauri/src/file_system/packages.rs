//! Package detection for the folders the package list can't answer: a dotted
//! folder (`Logo.sketchbundle`) is a package when macOS says so
//! (`NSURLIsPackageKey`: a registered package type, or the `kHasBundle` bit).
//!
//! Deferred to the VISIBLE rows, the way custom folder icons and Finder tags
//! are: the frontend asks after each visible-range fetch (`enrich_packages`),
//! ❌ never the bulk listing, where an NSURL lookup per dotted folder would cost a
//! 100k-entry directory dearly. The list (`cmdr_fs::file_extension`) stays the
//! free fast path. Only a volume backed by the local filesystem asks; SMB, MTP,
//! S3, and every other backend keep the list's answer. Which OS asks what is the
//! `platform/` seam's: only macOS has packages beyond the list.

mod platform;

use std::path::Path;

use cmdr_fs::file_extension::needs_package_lookup;
use cmdr_listing::ListingId;

use crate::listing_host::listings;

/// Whether the OS presents the one folder at `path` as an item, beyond the list
/// (`NSURLIsPackageKey` on macOS, `false` elsewhere). Blocking: call it from an
/// 8 MB-stack pool thread, ❌ never the async executor or rayon. For a card asking
/// about one folder; a listing's visible range goes through [`enrich`].
pub(crate) fn is_package_to_the_os(path: &str) -> bool {
    platform::is_package_to_the_os(path)
}

/// The paths among `paths` only macOS can answer: a dotted name the package
/// list doesn't know. A row with no dot, or a listed suffix, is never asked.
pub(crate) fn lookup_candidates(paths: Vec<String>) -> Vec<String> {
    paths
        .into_iter()
        .filter(|path| {
            Path::new(path)
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(needs_package_lookup)
        })
        .collect()
}

/// Asks the OS about the visible dotted folders of `listing_id` and patches the
/// answers into the listing, which tells the pane. Returns whether the lookups
/// ran out of time (the pane then keeps the list's answer for this range).
pub(crate) async fn enrich(listing_id: ListingId, paths: Vec<String>) -> bool {
    let candidates = lookup_candidates(paths);
    if candidates.is_empty() || crate::fda_gate::is_fda_pending_runtime() || !is_on_local_disk(&listing_id) {
        return false;
    }
    match platform::look_up(candidates).await {
        Some(updates) => {
            listings().apply_row_facts(&listing_id, cmdr_listing::RowFacts::Packages(updates));
            false
        }
        None => true,
    }
}

/// Whether the listing's volume is the local filesystem (`Volume::local_path`),
/// the only place an NSURL lookup reads the folder the row names.
fn is_on_local_disk(listing_id: &ListingId) -> bool {
    let Some((volume_id, _)) = listings().location(listing_id) else {
        return false;
    };
    crate::file_system::volume::manager::get_volume_manager()
        .get(&volume_id)
        .is_some_and(|volume| volume.local_path().is_some())
}

#[cfg(test)]
#[path = "packages_test.rs"]
mod tests;
