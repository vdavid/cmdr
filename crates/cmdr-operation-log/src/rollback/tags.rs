//! A tag write's inverse: put each file's prior Finder-tags value back (`DETAILS.md` § "A
//! tag write restores the exact prior value").

use cmdr_fs::entry::FINDER_TAGS_XATTR;
use cmdr_fs::volume::{VolumeError, XattrReadMode};

use super::super::store::RollbackUnit;
use super::super::types::SkipReason;
use super::{ActRefusal, InverseAct, ItemResult, RollbackRunner, VolumeLookup, vanished_item};

/// Write a tag write's prior value back onto the file.
///
/// Fails safe like every inverse: the file must still carry EXACTLY the value the write
/// left. Retagged since (Finder, another Mac, a later Cmdr write) ⇒ [`SkipReason::Drift`],
/// and its new tags stay. Already back to the prior value (an earlier pass, a crash
/// resume) ⇒ the end state holds, counted as reversed. Gone ⇒ [`SkipReason::Vanished`]:
/// there's nothing to put the tags back on. The value is opaque bytes here; the codec is
/// the app's.
pub(super) async fn restore_tags(
    volumes: &dyn VolumeLookup,
    runner: &dyn RollbackRunner,
    unit: &RollbackUnit,
) -> ItemResult {
    // A tag row without its values is a journal shape bug: nothing to restore to.
    let Some(tags) = &unit.tags else {
        return ItemResult::Skipped(SkipReason::Failed);
    };
    let Some(volume) = volumes.get(&unit.source_volume_id) else {
        return ItemResult::Skipped(SkipReason::Failed);
    };
    let path = unit.source_path.as_path();
    let paths = [path.to_path_buf()];
    let answer = volume
        .read_xattr_many(&paths, FINDER_TAGS_XATTR, XattrReadMode::Verified)
        .await
        .into_iter()
        .next();
    let live = match answer {
        Some(Ok(value)) => value,
        Some(Err(VolumeError::NotFound(_))) => return vanished_item(&[(&*volume, path)]),
        Some(Err(_)) | None => return ItemResult::Skipped(SkipReason::Failed),
    };
    match live.as_deref() {
        Some(value) if value == tags.after.as_slice() => {}
        Some(value) if value == tags.before.as_slice() => return ItemResult::Skipped(SkipReason::AlreadyGone),
        // Exact bytes only: an absent attribute, or the same tags encoded differently by
        // another writer, is something this write didn't leave, so it's drift.
        _ => return ItemResult::Skipped(SkipReason::Drift),
    }
    match runner
        .perform(InverseAct::WriteXattr {
            volume_id: &unit.source_volume_id,
            volume: &volume,
            path,
            name: FINDER_TAGS_XATTR,
            value: &tags.before,
        })
        .await
    {
        Ok(()) => ItemResult::Reversed,
        Err(ActRefusal::Volume(VolumeError::NotFound(_))) => vanished_item(&[(&*volume, path)]),
        Err(_) => ItemResult::Skipped(SkipReason::Failed),
    }
}
