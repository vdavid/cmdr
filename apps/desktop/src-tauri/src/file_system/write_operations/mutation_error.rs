//! Why an instant mutation (rename, new folder, new file) didn't happen, as a
//! value rather than a sentence.
//!
//! ❌ **Nothing in this file is prose a user reads.** The frontend renders every
//! word from the typed variant, through the message catalog, in ten locales
//! (`src/lib/file-operations/mutation-error-messages.ts`). The `String` fields
//! are paths and technical detail for the log and the details disclosure. Same
//! split as `WriteOperationError` on the transfer path and `ListingError` on the
//! listing path; `docs/guides/error-handling.md` is the map.
//!
//! The volume's own refusals ride through [`MutationError::Volume`] carrying the
//! whole `VolumeError`, so a backend that grows a variant reaches the frontend
//! without a second vocabulary to keep in step.

use cmdr_fs::volume::VolumeError;
use serde::{Deserialize, Serialize};

/// A typed refusal from `rename_file`, `create_directory`, `create_file`, or
/// `check_rename_permission`.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum MutationError {
    /// The name was blank, or only whitespace.
    NameEmpty,
    /// The name held a `/` or a null byte, neither of which any filesystem takes.
    NameHasDisallowedCharacter,
    /// The item to rename isn't there any more.
    NotFound {
        /// The path that was asked for.
        path: String,
    },
    /// A volume's root has no parent to be renamed within.
    CantRenameVolumeRoot,
    /// The containing folder refuses writes, so the rename can't land.
    ParentNotWritable {
        /// The folder, so the message can name it.
        path: String,
    },
    /// macOS's user-immutable flag ("Locked" in Get Info) is set.
    FileLocked {
        /// The locked item.
        path: String,
    },
    /// System Integrity Protection owns this path; no permission grant unlocks it.
    SipProtected {
        /// The protected item.
        path: String,
    },
    /// The volume left the registry between the caller reading it and the write
    /// (an unmount race). ❗ Not the same as a disconnected device: there is no
    /// backend left to ask.
    VolumeGone {
        /// The id that no longer resolves.
        volume_id: String,
    },
    /// The path crosses into an archive that can't be opened for editing right now.
    ArchiveNotEditable,
    /// The archive format is browse-and-extract only (tar, 7z); only zip is writable.
    ArchiveReadOnly,
    /// Renaming can't lift an entry OUT of an archive; that's a move.
    RenameOutOfArchive,
    /// Renaming can't carry an entry from one archive into another; that's a move.
    RenameAcrossArchives,
    /// The archive-edit driver isn't wired up yet (no Tauri event sink), which
    /// only happens before the app finishes starting.
    ArchiveEditNotReady,
    /// The archive edit refused to start.
    ArchiveEditCouldntStart {
        /// The driver's own words, for the technical-details disclosure.
        detail: String,
    },
    /// Something already holds that name.
    AlreadyExists {
        /// The name the user asked for, which is what the message quotes.
        name: String,
    },
    /// This volume has no Trash to move anything into (a network mount, a FAT
    /// stick). Permanent delete is the only way through.
    TrashNotSupported,
    /// The OS refused the move to the Trash.
    TrashRefused {
        /// What `NSFileManager` (or the Linux `trash` crate) reported, for the
        /// technical-details disclosure.
        detail: String,
    },
    /// The volume refused, and said why in its own vocabulary.
    Volume {
        /// The backend's typed answer, rendered by the frontend's volume factory.
        error: VolumeError,
    },
    /// The deadline passed before the volume answered. ❗ The write was NOT
    /// cancelled: `timeout_detached` bounds the frontend's wait, not the work,
    /// so it may still land.
    TimedOut,
    /// The one honest fallback, for a failure nothing above classifies (a
    /// panicked task, a join failure). The frontend renders a single "something
    /// went wrong" message and shows `detail` only as technical detail; ❌ it
    /// never renders `detail` as the message itself.
    Unexpected {
        /// What the layer below reported, for the log and the details disclosure.
        detail: String,
    },
}

impl std::fmt::Display for MutationError {
    /// ❗ For logs and debugging only; the frontend renders every user-facing
    /// word from the typed variant.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NameEmpty => f.write_str("name is empty"),
            Self::NameHasDisallowedCharacter => f.write_str("name holds a disallowed character"),
            Self::NotFound { path } => write!(f, "not found: {path}"),
            Self::CantRenameVolumeRoot => f.write_str("a volume root can't be renamed"),
            Self::ParentNotWritable { path } => write!(f, "parent not writable: {path}"),
            Self::FileLocked { path } => write!(f, "locked (immutable): {path}"),
            Self::SipProtected { path } => write!(f, "SIP-protected: {path}"),
            Self::VolumeGone { volume_id } => write!(f, "volume gone: {volume_id}"),
            Self::ArchiveNotEditable => f.write_str("archive not editable"),
            Self::ArchiveReadOnly => f.write_str("archive is read-only"),
            Self::RenameOutOfArchive => f.write_str("rename out of an archive"),
            Self::RenameAcrossArchives => f.write_str("rename across archives"),
            Self::ArchiveEditNotReady => f.write_str("archive-edit driver not ready"),
            Self::ArchiveEditCouldntStart { detail } => write!(f, "archive edit didn't start: {detail}"),
            Self::AlreadyExists { name } => write!(f, "already exists: {name}"),
            Self::TrashNotSupported => f.write_str("this platform has no Trash"),
            Self::TrashRefused { detail } => write!(f, "the Trash refused it: {detail}"),
            Self::Volume { error } => write!(f, "volume: {error}"),
            Self::TimedOut => f.write_str("timed out"),
            Self::Unexpected { detail } => write!(f, "unexpected: {detail}"),
        }
    }
}

impl From<VolumeError> for MutationError {
    fn from(error: VolumeError) -> Self {
        Self::Volume { error }
    }
}
