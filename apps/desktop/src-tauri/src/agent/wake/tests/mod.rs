//! Wake-pipeline tests.

mod coalesce;

use super::*;

/// One change in `folder` at `at`.
pub(super) fn event(folder: &str, kind: ChangeKind, at: u64) -> FolderEvent {
    FolderEvent {
        folder: folder.to_string(),
        kind,
        at,
    }
}
