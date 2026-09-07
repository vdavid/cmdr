//! Whether two paths name the same item, the only way a volume can answer it.
//!
//! `dev+ino` doesn't exist out here, so identity is `Arc::ptr_eq` on the volume
//! plus a folded-leaf path comparison. It lives outside `conflict.rs` because
//! three callers that resolve no conflict at all ask it: `copy.rs` keeps
//! self-landing sources out of the bulk skip, `move_same.rs` drops them before
//! any engine runs, and `../../routing.rs` gives the pre-flight scan the answer
//! this engine will give.

use std::path::Path;
use std::sync::Arc;

use super::super::dest_name_index::fold;
use crate::file_system::volume::Volume;

/// Whether `source_path` and `dest_path` name the same item: the question
/// `validation::is_same_file` settles with `dev+ino` on the local-FS side, asked
/// the only way a volume can answer it.
///
/// Same volume is `Arc::ptr_eq`, which is what every path in this directory
/// already means by it (the dest-inside-source guard included): the command
/// layer hands one `Arc` for a same-volume-id transfer.
///
/// `copy.rs` asks it too, to keep the sources it covers out of the pre-known-conflict
/// bulk skip, and so does `routing::transfer_would_land_on_its_source`, which
/// gives the pre-flight conflict scan the answer this engine will give.
pub(crate) fn is_the_same_item(
    source_volume: &Arc<dyn Volume>,
    source_path: &Path,
    dest_volume: &Arc<dyn Volume>,
    dest_path: &Path,
) -> bool {
    Arc::ptr_eq(source_volume, dest_volume) && is_the_same_volume_path(source_path, dest_path)
}

/// Whether two paths on ONE volume name the same item: the SAME parent
/// directory, and a final component the destination's backend would resolve onto
/// one entry.
///
/// The leaf is compared folded (NFC + lowercase), the key `DestNameIndex` buckets
/// destination names under, so a case-differing route (SMB shares, macOS
/// volumes) or an NFC/NFD-differing one (macOS and SMB move paths between the two
/// routinely) counts. That fold answers exactly one question — "would this
/// backend treat these two NAMES as the same, in one listing" — and the leaf is
/// the only component we ever have a listing for.
///
/// ❌ The parents are NOT folded. Whether `/DCIM` and `/dcim` are one directory
/// is the backend's call, and a case-sensitive one (MTP is; an SMB share can be)
/// says no, so folding them turns a genuine cross-folder transfer into a
/// self-collision: the move writes nothing, reports `Done`, and the user is told
/// an item moved that didn't. Being wrong the other way costs the ordinary
/// conflict path on a case-insensitive backend reached by a differently-cased
/// route, which is where such a transfer landed before this rule existed.
///
/// A non-UTF-8 leaf can't be folded the way a backend would, so there only a
/// byte-exact match counts — the same stance `DestNameIndex::lookup` takes.
///
/// The same-volume move drops its self-colliding sources with this before any
/// engine runs (`move_same.rs`), which is why it isn't private to the resolver.
pub(super) fn is_the_same_volume_path(source_path: &Path, dest_path: &Path) -> bool {
    if source_path.parent() != dest_path.parent() {
        return false;
    }
    match (
        source_path.file_name().and_then(|name| name.to_str()),
        dest_path.file_name().and_then(|name| name.to_str()),
    ) {
        (Some(source), Some(dest)) => fold(source) == fold(dest),
        _ => source_path == dest_path,
    }
}

/// One volume, two paths, one item?
#[cfg(test)]
#[path = "conflict_same_item_tests.rs"]
mod same_item_tests;
