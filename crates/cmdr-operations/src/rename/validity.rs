//! The rename editor's checks on a name before anything is renamed: is it a
//! valid name, does a sibling hold it (by inode on the local disk, through the
//! volume elsewhere), and, where the rename copies (`Volume::rename_work`), what
//! the move it runs as would carry (`RenameByMove`). Read-only and unmanaged:
//! it runs on commit, never through the operation manager.
//!
//! What that move costs is the host's to say: the check hands back what it
//! counted ([`RenameValidityCheck`]), and the host prices it before the editor
//! hears the answer.

use std::path::{Path, PathBuf};

use super::super::ScanCostFacts;
use super::super::host::EngineHost;
use cmdr_fs::log_error::LogError;
use cmdr_fs::log_private::LogPrivate;
use cmdr_fs::name_fold::fold_name;

use super::super::look_alike::{ListedFolders, NewEntry, place_new_entry};
use cmdr_fs::volume::{RenameWork, SubtreeTally, Volume};

/// Result of a rename validity check.
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RenameValidityResult {
    /// Whether the new name is valid (passes filename validation).
    pub valid: bool,
    /// Validation error message, if any.
    pub error: Option<cmdr_fs::validation::ValidationError>,
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
pub struct RenameByMove {
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

/// The editor's answer before the host has priced the move a copying rename
/// runs as. Only [`priced`](Self::priced) turns it into the
/// [`RenameValidityResult`] the editor hears, so no answer skips the cost.
#[derive(Debug)]
pub struct RenameValidityCheck {
    result: RenameValidityResult,
    /// What a fully counted copying rename carries, in the shape a cost
    /// estimate prices. `None` when there's no move, or its count didn't finish
    /// (which asks first anyway).
    move_to_price: Option<ScanCostFacts>,
}

impl RenameValidityCheck {
    fn unpriced(result: RenameValidityResult) -> Self {
        Self {
            result,
            move_to_price: None,
        }
    }

    /// The editor's answer, with `costs_something` saying whether moving what
    /// the rename carries costs money that doesn't round to zero. A move that
    /// does asks first. Called only when there's a counted move to price.
    pub fn priced(self, costs_something: impl FnOnce(&ScanCostFacts) -> bool) -> RenameValidityResult {
        let Self {
            mut result,
            move_to_price,
        } = self;
        if let (Some(by_move), Some(facts)) = (result.by_move.as_mut(), move_to_price.as_ref())
            && costs_something(facts)
        {
            by_move.confirm_first = true;
        }
        result
    }
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
pub struct ConflictFileInfo {
    pub name: String,
    /// In bytes.
    pub size: u64,
    /// Unix timestamp in seconds.
    pub modified: Option<i64>,
    pub is_directory: bool,
}

impl crate::Operations {
    /// Validates a new filename and checks for conflicts in the same directory.
    /// Uses inode comparison to detect case-only renames (valid on case-insensitive
    /// APFS). When `volume_id` is not `"root"`, uses the Volume trait for conflict
    /// detection, and the folder's listing for a case-only rename on a volume that
    /// folds case ([`found_the_source_itself`]). A rename that runs as a move comes
    /// back with what it carries, for the caller to price
    /// ([`RenameValidityCheck::priced`]).
    pub async fn check_rename_validity_impl(
        &self,
        dir: String,
        old_name: String,
        new_name: String,
        volume_id: String,
    ) -> RenameValidityCheck {
        use cmdr_fs::validation::{validate_filename, validate_path_length};

        let trimmed = new_name.trim();

        // Validate filename
        if let Err(error) = validate_filename(trimmed) {
            return RenameValidityCheck::unpriced(RenameValidityResult {
                valid: false,
                error: Some(error),
                has_conflict: false,
                is_case_only_rename: false,
                conflict: None,
                by_move: None,
            });
        }

        // Validate resulting path length
        let new_path = PathBuf::from(&dir).join(trimmed);
        if let Err(error) = validate_path_length(&new_path) {
            return RenameValidityCheck::unpriced(RenameValidityResult {
                valid: false,
                error: Some(error),
                has_conflict: false,
                is_case_only_rename: false,
                conflict: None,
                by_move: None,
            });
        }

        // Check for conflict: does a sibling with this name already exist?
        let old_path = PathBuf::from(&dir).join(&old_name);

        if self.host().volumes().local_disk(&volume_id).is_none() {
            // Not the local disk: use Volume trait for conflict detection
            let conflict_info = check_sibling_conflict_via_volume(self.host(), &volume_id, &old_path, &new_path).await;
            let (by_move, move_to_price) = match rename_by_move(self.host(), &volume_id, &old_path).await {
                Some((by_move, facts)) => (Some(by_move), facts),
                None => (None, None),
            };
            RenameValidityCheck {
                result: RenameValidityResult {
                    valid: true,
                    error: None,
                    has_conflict: conflict_info.0,
                    is_case_only_rename: conflict_info.1,
                    conflict: conflict_info.2,
                    by_move,
                },
                move_to_price,
            }
        } else {
            // Local filesystem: use symlink_metadata with inode comparison
            let conflict_info = check_sibling_conflict(&old_path, &new_path);
            RenameValidityCheck::unpriced(RenameValidityResult {
                valid: true,
                error: None,
                has_conflict: conflict_info.0,
                is_case_only_rename: conflict_info.1,
                conflict: conflict_info.2,
                // The local disk renames everything in one call.
                by_move: None,
            })
        }
    }
}

/// What renaming `old_path` would carry when it runs as a move, `None` when
/// it's one call (or the volume is gone, which the rename itself reports).
/// The count is bounded ([`SMALL_RENAME_FILES`] plus one), so F2 on a huge
/// folder answers without listing all of it. A complete count also comes back
/// as the facts a cost estimate prices (no request is sent for the estimate
/// itself); until the caller prices them ([`RenameValidityCheck::priced`]), the
/// verdict asks first only for size or an unfinished count.
async fn rename_by_move(
    host: &EngineHost,
    volume_id: &str,
    old_path: &Path,
) -> Option<(RenameByMove, Option<ScanCostFacts>)> {
    // Plain `get`: the drive's own rename cost (an in-zip rename is an archive edit).
    let volume = host.volumes().get(volume_id)?;
    match volume.rename_work(old_path).await {
        Ok(RenameWork::CopyThenDelete) => {}
        Ok(RenameWork::OneCall) | Err(_) => return None,
    }
    let tally = match volume.tally_subtree(old_path, SMALL_RENAME_FILES).await {
        Ok(tally) => Some(tally),
        Err(e) => {
            log::debug!(target: "volume", "couldn't count what renaming {} carries: {e}", LogPrivate::path(old_path), e = LogError(&e));
            None
        }
    };
    // An unfinished count asks first anyway, so only a complete one is priced.
    let move_to_price = tally
        .as_ref()
        .filter(|tally| tally.complete)
        .map(|tally| ScanCostFacts {
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
        });
    Some((RenameByMove::from_tally(tally, false), move_to_price))
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
pub fn same_local_file(left: &std::fs::Metadata, right: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    left.dev() == right.dev() && left.ino() == right.ino()
}

/// Without an inode to compare, two paths never count as one file, so a case-only
/// rename is indistinguishable from a conflict here.
#[cfg(not(unix))]
pub fn same_local_file(_left: &std::fs::Metadata, _right: &std::fs::Metadata) -> bool {
    false
}

/// Checks if a file with `new_path` exists on a non-local volume using the Volume trait's
/// `get_metadata`, or under another Unicode spelling of its name (`look_alike.rs`), which
/// the rename itself would refuse too. `old_path` as that look-alike is a respell, not a clash.
///
/// Answers `(has_conflict, is_case_only_rename, conflict)`, like [`check_sibling_conflict`].
async fn check_sibling_conflict_via_volume(
    host: &EngineHost,
    volume_id: &str,
    old_path: &Path,
    new_path: &Path,
) -> (bool, bool, Option<ConflictFileInfo>) {
    // Plain `get`, not `resolve`: renaming INTO an archive is rejected upstream, so
    // the target is always a normal sibling (incl. a `.zip` file), checked on its
    // own volume — routing to the ArchiveVolume would mis-consult the zip's index.
    let volume = match host.volumes().get(volume_id) {
        Some(v) => v,
        None => return (false, false, None),
    };

    let entry = match volume.get_metadata(new_path).await {
        Ok(e) => e,
        Err(cmdr_fs::volume::VolumeError::NotFound(_)) => {
            match place_new_entry(host.listings(), volume.as_ref(), volume_id, new_path, Some(old_path)).await {
                Ok(NewEntry::Taken(look_alike)) => *look_alike,
                // Respelled onto a name the folder holds exactly, which the rename
                // itself refuses (or, confirmed, replaces).
                Ok(NewEntry::Free(spelled)) if spelled != new_path && spelled != old_path => {
                    match volume.get_metadata(&spelled).await {
                        Ok(entry) => entry,
                        Err(_) => return (false, false, None),
                    }
                }
                // No conflict: nothing holds the name in any spelling. Several
                // look-alikes leave the rename itself to refuse, by name.
                _ => return (false, false, None),
            }
        }
        Err(_) => return (false, false, None), // Couldn't tell; the rename answers for itself
    };

    let is_case_only_rename = found_the_source_itself(host, volume.as_ref(), volume_id, old_path, new_path).await;

    let conflict = ConflictFileInfo {
        name: entry.name,
        size: entry.size.unwrap_or(0),
        modified: entry.modified_at.map(|t| t as i64),
        is_directory: entry.is_directory,
    };

    (true, is_case_only_rename, Some(conflict))
}

/// Whether a lookup of `new_path` that just answered found the entry being
/// renamed, `old_path`, under its other spelling: a case-only rename on a
/// case-insensitive volume (an SMB share, an exFAT stick). No inode to compare
/// off the local disk, and the volume names the entry the way it was asked
/// (`SmbVolume::get_metadata` does), so the folder's listing decides: it holds
/// `old_path`'s name and not `new_path`'s, so the only entry the lookup could
/// have found is the source. A case-sensitive volume holding both names lists
/// both, and that's a real conflict.
///
/// Only for names that fold to one key (`fold_name`), so a plain conflict costs
/// no listing. One that can't be read answers "a conflict", which the dialog
/// shows rather than renaming over a file nobody looked at.
async fn found_the_source_itself(
    host: &EngineHost,
    volume: &dyn Volume,
    volume_id: &str,
    old_path: &Path,
    new_path: &Path,
) -> bool {
    let (Some(dir), Some(old_name), Some(new_name)) = (
        new_path.parent(),
        old_path.file_name().and_then(|n| n.to_str()),
        new_path.file_name().and_then(|n| n.to_str()),
    ) else {
        return false;
    };
    if old_path.parent() != Some(dir) || old_name == new_name || fold_name(old_name) != fold_name(new_name) {
        return false;
    }
    match ListedFolders::with_pane_listings(host.listings(), volume, volume_id)
        .entries_in(dir)
        .await
    {
        Ok(entries) => {
            entries.iter().any(|entry| entry.name == old_name) && !entries.iter().any(|entry| entry.name == new_name)
        }
        Err(e) => {
            log::debug!(target: "volume", "couldn't list {} to tell a case-only rename: {e}", LogPrivate::dir_path(dir), e = LogError(&e));
            false
        }
    }
}
