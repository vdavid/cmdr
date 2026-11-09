//! Where an inverse acts, and what a volume's "not there" means for it. Pure helpers
//! the acts (`inverse.rs`, `tags.rs`), the runner's progress, and the bookkeeping all
//! share, kept in a leaf so none of them depends on another for them.

use std::path::{Path, PathBuf};

use cmdr_fs::volume::Volume;

use super::super::store::RollbackUnit;
use super::super::types::SkipReason;
use super::ItemResult;

/// The (volume, path) a removal inverse targets: the item's dest (the copy /
/// created path), falling back to source when no dest was recorded.
pub(super) fn removal_target(unit: &RollbackUnit) -> (String, PathBuf) {
    match (&unit.dest_volume_id, &unit.dest_path) {
        (Some(vol), Some(path)) => (vol.clone(), path.clone()),
        _ => (unit.source_volume_id.clone(), unit.source_path.clone()),
    }
}

/// The verdict for an item a volume answered "not there" about, given every
/// `(volume, path)` side the reversal touches.
///
/// A volume refuses a path above its own root as `NotFound`, and a remote place's
/// root can be narrowed after the operation ran (`network/DETAILS.md` § "Editing a
/// connected place"). An item there is NOT gone: it sits where the operation left
/// it, out of the volume's reach, and counting it as reversed reports an undo that
/// removed nothing. So it fails safe; only a path every side still serves can be
/// already gone. Containment is by whole path components (`Path::starts_with`), so
/// `/share/tmp2` isn't under `/share/tmp`.
pub(super) fn missing_item(sides: &[(&dyn Volume, &Path)]) -> ItemResult {
    if sides.iter().all(|(volume, path)| path.starts_with(volume.root())) {
        ItemResult::Skipped(SkipReason::AlreadyGone)
    } else {
        ItemResult::Skipped(SkipReason::Failed)
    }
}

/// The verdict for a restore whose item is gone from BOTH sides: `Vanished`, under the same
/// containment rule as [`missing_item`] (a path a narrowed root no longer reaches isn't gone).
pub(super) fn vanished_item(sides: &[(&dyn Volume, &Path)]) -> ItemResult {
    match missing_item(sides) {
        ItemResult::Skipped(SkipReason::AlreadyGone) => ItemResult::Skipped(SkipReason::Vanished),
        out_of_reach => out_of_reach,
    }
}
