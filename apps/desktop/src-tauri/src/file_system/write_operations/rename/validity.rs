//! The rename editor's checks on a name before anything is renamed: is it a
//! valid name, does a sibling hold it (by inode on the local disk, through the
//! volume elsewhere), and, where the rename copies (`Volume::rename_work`), what
//! the move it runs as would carry (`RenameByMove`). Read-only and unmanaged:
//! it runs on commit, never through the operation manager.

use std::path::{Path, PathBuf};

use super::super::ScanCostFacts;
use super::super::look_alike::{NewEntry, place_new_entry};
use crate::file_system::volume::{RenameWork, SubtreeTally};
use crate::s3_costs;

/// Result of a rename validity check.
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RenameValidityResult {
    /// Whether the new name is valid (passes filename validation).
    pub valid: bool,
    /// Validation error message, if any.
    pub error: Option<crate::file_system::validation::ValidationError>,
    /// Whether a conflict exists (a sibling with the same name).
    pub has_conflict: bool,
    /// If there's a conflict, whether it's a case-only rename of the same file (same inode).
    pub is_case_only_rename: bool,
    /// Conflicting file info, if any.
    pub conflict: Option<ConflictFileInfo>,
    /// Set when renaming this entry isn't one call on its volume
    /// (`Volume::rename_work`), so it runs as a move: what that move carries,
    /// and whether to confirm it first.
    pub by_move: Option<RenameByMove>,
}

/// The most files a rename that runs as a move carries without asking first:
/// past it, F2 opens the Move dialog instead of starting in the background.
pub(crate) const SMALL_RENAME_FILES: u64 = 100;

/// What a rename that runs as a move would carry, counted with a bounded
/// listing (`Volume::tally_subtree`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RenameByMove {
    /// Files the move copies, counted up to one past [`SMALL_RENAME_FILES`].
    pub files: u64,
    /// Their bytes.
    pub bytes: u64,
    /// `false` when the count stopped at its cap, so there are more.
    pub counted_all: bool,
    /// Big enough, uncounted, or costing money to confirm in the Move dialog
    /// first (which shows the cost); else it starts as a background move with
    /// the progress chip.
    pub confirm_first: bool,
}

impl RenameByMove {
    /// The verdict for what a bounded count found: anything past
    /// [`SMALL_RENAME_FILES`], a count that couldn't finish, or a cost that
    /// doesn't round to zero (`costs_something`) asks first.
    pub(crate) fn from_tally(tally: Option<SubtreeTally>, costs_something: bool) -> Self {
        match tally {
            Some(tally) => Self {
                files: tally.files,
                bytes: tally.bytes,
                counted_all: tally.complete,
                confirm_first: !tally.complete || tally.files > SMALL_RENAME_FILES || costs_something,
            },
            None => Self {
                files: 0,
                bytes: 0,
                counted_all: false,
                confirm_first: true,
            },
        }
    }
}

/// Metadata about a conflicting sibling file.
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ConflictFileInfo {
    pub name: String,
    /// In bytes.
    pub size: u64,
    /// Unix timestamp in seconds.
    pub modified: Option<i64>,
    pub is_directory: bool,
}

/// Validates a new filename and checks for conflicts in the same directory.
/// Uses inode comparison to detect case-only renames (valid on case-insensitive
/// APFS). When `volume_id` is not `"root"`, uses the Volume trait for conflict
/// detection (needed for MTP and other non-local volumes). `data_dir` is where
/// the S3 price table is cached, for a rename that runs as a move.
pub(crate) async fn check_rename_validity_impl(
    dir: String,
    old_name: String,
    new_name: String,
    volume_id: String,
    data_dir: Option<PathBuf>,
) -> RenameValidityResult {
    use crate::file_system::validation::{validate_filename, validate_path_length};

    let trimmed = new_name.trim();

    // Validate filename
    if let Err(error) = validate_filename(trimmed) {
        return RenameValidityResult {
            valid: false,
            error: Some(error),
            has_conflict: false,
            is_case_only_rename: false,
            conflict: None,
            by_move: None,
        };
    }

    // Validate resulting path length
    let new_path = PathBuf::from(&dir).join(trimmed);
    if let Err(error) = validate_path_length(&new_path) {
        return RenameValidityResult {
            valid: false,
            error: Some(error),
            has_conflict: false,
            is_case_only_rename: false,
            conflict: None,
            by_move: None,
        };
    }

    // Check for conflict: does a sibling with this name already exist?
    let old_path = PathBuf::from(&dir).join(&old_name);

    if volume_id != "root" {
        // Non-local volume: use Volume trait for conflict detection
        let conflict_info = check_sibling_conflict_via_volume(&volume_id, &old_path, &new_path).await;
        RenameValidityResult {
            valid: true,
            error: None,
            has_conflict: conflict_info.0,
            // MTP is case-sensitive, no case-only rename ambiguity
            is_case_only_rename: false,
            conflict: conflict_info.1,
            by_move: rename_by_move_cost(&volume_id, &old_path, data_dir.as_deref()).await,
        }
    } else {
        // Local filesystem: use symlink_metadata with inode comparison
        let conflict_info = check_sibling_conflict(&old_path, &new_path);
        RenameValidityResult {
            valid: true,
            error: None,
            has_conflict: conflict_info.0,
            is_case_only_rename: conflict_info.1,
            conflict: conflict_info.2,
            // The local disk renames everything in one call.
            by_move: None,
        }
    }
}

/// What renaming `old_path` would carry when it runs as a move, `None` when
/// it's one call (or the volume is gone, which the rename itself reports).
/// The count is bounded ([`SMALL_RENAME_FILES`] plus one), so F2 on a huge
/// folder answers without listing all of it, and its files are what the cost
/// estimate prices: no request is sent for the estimate itself.
async fn rename_by_move_cost(volume_id: &str, old_path: &Path, data_dir: Option<&Path>) -> Option<RenameByMove> {
    let volume = crate::file_system::volume::manager::get_volume_manager().get(volume_id)?;
    match volume.rename_work(old_path).await {
        Ok(RenameWork::CopyThenDelete) => {}
        Ok(RenameWork::OneCall) | Err(_) => return None,
    }
    let tally = match volume.tally_subtree(old_path, SMALL_RENAME_FILES).await {
        Ok(tally) => Some(tally),
        Err(e) => {
            log::debug!(target: "volume", "couldn't count what renaming {} carries: {e}", old_path.display());
            None
        }
    };
    // An unfinished count asks first anyway, so only a complete one is priced.
    let costs_something = tally.as_ref().is_some_and(|tally| {
        tally.complete && {
            let facts = ScanCostFacts {
                files: usize::try_from(tally.files).unwrap_or(usize::MAX),
                dirs: usize::try_from(tally.folders).unwrap_or(usize::MAX),
                bytes: tally.bytes,
                per_file: Some(tally.per_file.clone()),
                // The renamed entry is the one selected item: a folder, or a
                // file past the part floor.
                selected_folders: usize::from(tally.folders > 0),
                selected_file_sizes: if tally.folders > 0 {
                    Vec::new()
                } else {
                    vec![tally.bytes]
                },
            };
            !s3_costs::rounds_to_zero(&s3_costs::estimate_rename(volume_id, &facts, data_dir))
        }
    });
    Some(RenameByMove::from_tally(tally, costs_something))
}

/// Checks if a file with `new_path` exists and whether it's the same inode as `old_path`
/// (case-only rename on case-insensitive FS).
fn check_sibling_conflict(old_path: &Path, new_path: &Path) -> (bool, bool, Option<ConflictFileInfo>) {
    let new_meta = match std::fs::symlink_metadata(new_path) {
        Ok(m) => m,
        Err(_) => return (false, false, None), // No conflict
    };

    // Check if it's the same inode (case-only rename)
    let is_same_inode = std::fs::symlink_metadata(old_path).is_ok_and(|old_meta| same_local_file(&old_meta, &new_meta));

    let modified = new_meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64);

    let conflict = ConflictFileInfo {
        name: new_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default(),
        size: new_meta.len(),
        modified,
        is_directory: new_meta.is_dir(),
    };

    (true, is_same_inode, Some(conflict))
}

/// Whether two `symlink_metadata` results name one local file (same device and
/// inode), which is how a case-only rename on a case-insensitive volume shows up.
#[cfg(unix)]
pub(crate) fn same_local_file(left: &std::fs::Metadata, right: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    left.dev() == right.dev() && left.ino() == right.ino()
}

/// Without an inode to compare, two paths never count as one file, so a case-only
/// rename is indistinguishable from a conflict here.
#[cfg(not(unix))]
pub(crate) fn same_local_file(_left: &std::fs::Metadata, _right: &std::fs::Metadata) -> bool {
    false
}

/// Checks if a file with `new_path` exists on a non-local volume using the Volume trait's
/// `get_metadata`, or under another Unicode spelling of its name (`look_alike.rs`), which
/// the rename itself would refuse too. `old_path` as that look-alike is a respell, not a clash.
async fn check_sibling_conflict_via_volume(
    volume_id: &str,
    old_path: &Path,
    new_path: &Path,
) -> (bool, Option<ConflictFileInfo>) {
    // Plain `get`, not `resolve`: renaming INTO an archive is rejected upstream, so
    // the target is always a normal sibling (incl. a `.zip` file), checked on its
    // own volume — routing to the ArchiveVolume would mis-consult the zip's index.
    let volume = match crate::file_system::volume::manager::get_volume_manager().get(volume_id) {
        Some(v) => v,
        None => return (false, None),
    };

    let entry = match volume.get_metadata(new_path).await {
        Ok(e) => e,
        Err(crate::file_system::VolumeError::NotFound(_)) => {
            match place_new_entry(volume.as_ref(), volume_id, new_path, Some(old_path)).await {
                Ok(NewEntry::Taken(look_alike)) => *look_alike,
                // Respelled onto a name the folder holds exactly, which the rename
                // itself refuses (or, confirmed, replaces).
                Ok(NewEntry::Free(spelled)) if spelled != new_path && spelled != old_path => {
                    match volume.get_metadata(&spelled).await {
                        Ok(entry) => entry,
                        Err(_) => return (false, None),
                    }
                }
                // No conflict: nothing holds the name in any spelling. Several
                // look-alikes leave the rename itself to refuse, by name.
                _ => return (false, None),
            }
        }
        Err(_) => return (false, None), // Couldn't tell; the rename answers for itself
    };

    let conflict = ConflictFileInfo {
        name: entry.name,
        size: entry.size.unwrap_or(0),
        modified: entry.modified_at.map(|t| t as i64),
        is_directory: entry.is_directory,
    };

    (true, Some(conflict))
}
