//! The four primitives the sweep's rules need, pointed at this machine's
//! filesystem or at one volume's own namespace ([`Surface`]), and the answers
//! they give. Split out of `in_flight_sweep.rs`, which holds the rules.

use std::path::Path;
use std::sync::Arc;

use super::Outcome;
use crate::local_disk::rename_exclusive as rename_local_exclusive;
use cmdr_fs::log_error::LogError;
use cmdr_fs::log_private::LogPrivate;
use cmdr_fs::metadata::{RemovalRefusal, remove_file_clearing_blockers};
use cmdr_fs::volume::{Volume, VolumeError};

/// Which filesystem answers a record's paths, and the four primitives the rules
/// need from it.
pub(super) enum Surface {
    /// This machine's filesystem.
    Local,
    /// One volume's own namespace.
    Volume(Arc<dyn Volume>),
}

/// What is standing at a path.
///
/// ❗ [`Unknown`](Self::Unknown) is never read as [`Missing`](Self::Missing): a
/// restore aimed at a path nobody could look at would bury whatever is there.
pub(super) enum Standing {
    Missing,
    /// A regular file, and how big it is.
    File {
        size: u64,
    },
    /// A directory, a symlink, or anything else that isn't a plain file.
    Other,
    /// The question couldn't be answered.
    Unknown,
}

/// How a removal ended.
pub(super) enum Removal {
    Removed,
    AlreadyGone,
    /// Including a `remove_dir` on a directory that isn't empty, which is the
    /// staging-folder rule working.
    Refused,
}

impl Removal {
    pub(super) fn into_outcome(self) -> Outcome {
        match self {
            Self::Removed => Outcome::Swept,
            Self::AlreadyGone => Outcome::AlreadyGone,
            Self::Refused => Outcome::Deferred,
        }
    }
}

/// How a no-replace rename ended.
pub(super) enum Renaming {
    Renamed,
    /// The name is taken, so the caller may try another one.
    Occupied,
    Refused,
}

impl Surface {
    pub(super) async fn standing_at(&self, path: &Path) -> Standing {
        match self {
            Self::Local => match std::fs::symlink_metadata(path) {
                Ok(meta) if meta.is_file() => Standing::File { size: meta.len() },
                Ok(_) => Standing::Other,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Standing::Missing,
                Err(e) => {
                    log::debug!(target: "copy", "couldn't read what's at {}: {e}", LogPrivate::path(path));
                    Standing::Unknown
                }
            },
            Self::Volume(volume) => match volume.get_metadata(path).await {
                Ok(entry) if entry.is_directory || entry.is_symlink => Standing::Other,
                // A backend that can't say how big a file is can't prove a
                // replacement finished, so the aside is kept.
                Ok(entry) => entry.size.map_or(Standing::Other, |size| Standing::File { size }),
                Err(VolumeError::NotFound(_)) => Standing::Missing,
                Err(e) => {
                    log::debug!(target: "copy", "couldn't read what's at {}: {e}", LogPrivate::path(path), e = LogError(&e));
                    Standing::Unknown
                }
            },
        }
    }

    pub(super) async fn remove_file(&self, path: &Path) -> Removal {
        match self {
            Self::Local => match std::fs::remove_file(path) {
                Ok(()) => Removal::Removed,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Removal::AlreadyGone,
                Err(e) => {
                    log::debug!(target: "copy", "couldn't remove the leftover {}: {e}", LogPrivate::file_path(path));
                    Removal::Refused
                }
            },
            Self::Volume(volume) => match volume.delete(path).await {
                Ok(()) => Removal::Removed,
                Err(VolumeError::NotFound(_)) => Removal::AlreadyGone,
                Err(e) => {
                    log::debug!(target: "copy", "couldn't remove the leftover {}: {e}", LogPrivate::file_path(path), e = LogError(&e));
                    Removal::Refused
                }
            },
        }
    }

    /// Removes a staged partial the record has matched as OURS.
    ///
    /// A local one that refuses is cleared and retried once: an APFS clone
    /// copies the source's `uchg` and ACL onto the temp in the same call, so a
    /// crash before the landing clears them leaves a temp `unlink` refuses
    /// (`cmdr_fs::metadata::remove_file_clearing_blockers`). ❌ Only a temp: an
    /// aside is the user's own file, and its flags are theirs.
    pub(super) async fn remove_temp(&self, path: &Path) -> Removal {
        match self {
            Self::Local => match remove_file_clearing_blockers(path) {
                Ok(()) => Removal::Removed,
                Err(RemovalRefusal::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => Removal::AlreadyGone,
                Err(refusal) => {
                    log::debug!(target: "copy", "couldn't remove the leftover {}: {refusal:?}", LogPrivate::file_path(path));
                    Removal::Refused
                }
            },
            Self::Volume(_) => self.remove_file(path).await,
        }
    }

    /// Removes a directory ONLY if it is empty.
    ///
    /// ❌ Never `remove_dir_all`, on any surface. `std::fs::remove_dir` gives
    /// the empty-only promise in the kernel; a volume has no primitive that
    /// does, so it refuses rather than listing and racing.
    pub(super) async fn remove_empty_dir(&self, path: &Path) -> Removal {
        match self {
            Self::Local => match std::fs::remove_dir(path) {
                Ok(()) => Removal::Removed,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Removal::AlreadyGone,
                Err(e) => {
                    log::debug!(target: "copy", "leaving the staging folder {}: {e}", LogPrivate::dir_path(path));
                    Removal::Refused
                }
            },
            Self::Volume(_) => {
                log::warn!(
                    target: "copy",
                    "leaving the staging folder {}: only the local filesystem can promise an empty-only removal",
                    LogPrivate::dir_path(path)
                );
                Removal::Refused
            }
        }
    }

    pub(super) async fn rename_no_replace(&self, from: &Path, to: &Path) -> Renaming {
        match self {
            // The primitive `overwrite::rename_no_replace` delegates to, named at
            // its home: `overwrite` tracks through this ledger, so reaching back
            // up into it would weld the two modules into a cycle.
            Self::Local => match rename_local_exclusive(from, to) {
                Ok(()) => Renaming::Renamed,
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Renaming::Occupied,
                Err(e) => {
                    log::debug!(
                        target: "copy",
                        "couldn't put {} at {}: {e}",
                        LogPrivate::path(from),
                        LogPrivate::path(to)
                    );
                    Renaming::Refused
                }
            },
            // `force = false` is the backends' no-clobber rename, the same one
            // `displaced_destination` leans on.
            Self::Volume(volume) => match volume.rename(from, to, false).await {
                Ok(()) => Renaming::Renamed,
                Err(VolumeError::AlreadyExists(_)) => Renaming::Occupied,
                Err(e) => {
                    log::debug!(
                        target: "copy",
                        "couldn't put {} at {}: {e}",
                        LogPrivate::path(from),
                        LogPrivate::path(to), e = LogError(&e)
                    );
                    Renaming::Refused
                }
            },
        }
    }
}
