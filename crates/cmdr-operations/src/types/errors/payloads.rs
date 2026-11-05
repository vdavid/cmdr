//! The typed payloads `WriteOperationError`'s variants carry: which side
//! refused, what a refusal's errno says, why a name or a trash was refused, what
//! a mounted drive or the home folder had to do with it, and the oversized files.
//! Leaf types with no behavior of their own, re-exported from `errors.rs` with
//! `pub use payloads::*`, so every `types::` path holds and `bindings.ts` doesn't
//! move.

use serde::{Deserialize, Serialize};

/// Which half of a transfer refused the write, for [`super::WriteOperationError::ReadOnlyDevice`].
///
/// The two are different sentences, not different wordings of one: a read-only
/// DESTINATION means "put it somewhere else", a read-only SOURCE means "you can
/// copy out of here, you just can't move out of here", because a move needs a
/// delete the source will never do.
///
/// ❌ Never decide this by inspecting a path or a message. The refusing site
/// knows which half it was looking at and says so.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum ReadOnlySide {
    /// The SOURCE can't give up its files. A copy out of it works; a move
    /// doesn't, because the source half of the move could never happen.
    Source,
    /// The DESTINATION takes no writes, so nothing can land there.
    Destination,
}

/// Which half of a transfer refused on permission grounds, for
/// [`super::WriteOperationError::PermissionDenied`].
///
/// Copying OUT of a folder that refuses reads and copying INTO one that refuses
/// writes are different fixes, and without this they read identically. Filled
/// by the site that knows which volume it asked (`map_volume_error`'s
/// `PathRole`, the destination probes); `None` where nothing says, such as a
/// `rename(2)` whose errno can't tell its two parents apart.
///
/// ❌ Never decide this by inspecting a path or a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum PermissionSide {
    /// The item being read, or the original a move removes.
    Source,
    /// Where the item was being written.
    Destination,
}

/// Which half of a transfer a volume is, for the copy that names it.
///
/// ❌ Never decided by comparing paths after the fact: the engine is handed both
/// sides when the operation starts and says which one it means.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum TransferRole {
    /// The volume the files came FROM.
    Source,
    /// The volume the files were going TO.
    Destination,
}

/// The volume that left the mount table while a transfer was running.
///
/// ❗ Every field is captured when the transfer STARTS. A volume that vanishes is
/// gone from the volume list and from the mount table by the time the error is
/// worded, so a lookup then would answer nothing and the sentence would have no
/// drive to name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DisconnectedSide {
    /// Which half of the transfer this volume was.
    pub role: TransferRole,
    /// The volume id, so a surface can match it against its own records.
    pub volume_id: String,
    /// The volume's display name, for the sentence.
    pub volume_name: String,
    /// The OTHER volume's display name: where the files that made it are, or
    /// where the ones that didn't still sit.
    pub counterpart_name: String,
}

/// What a permission refusal's errno says about whether administrator rights
/// could change the answer, for [`super::WriteOperationError::PermissionDenied`].
///
/// Rust folds `EACCES` and `EPERM` into one `ErrorKind::PermissionDenied`, and the
/// two are opposite advice: one is a folder an administrator could write to, the
/// other is macOS itself refusing, where administrator rights change nothing.
/// Derived from the errno by [`super::WriteOperationError::permission_denied`], the only
/// thing that builds the variant, so the two can't disagree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum PermissionRefusal {
    /// `EACCES`: the folder's own permissions say no. An administrator could.
    FolderPermissions,
    /// `EPERM`: the OS itself says no (System Integrity Protection, an immutable
    /// flag, a privacy protection). Administrator rights change nothing.
    SystemProtected,
    /// No errno to classify: a backend that words its own refusals (MTP, SMB), or
    /// an errno outside the two above.
    Unclassified,
    /// An object store account (S3) refused: its keys may lack the permission, or
    /// the provider may have paused the account (a usage cap, a billing hold).
    /// One answer covers both, so the advice names both.
    ObjectStoreAccount,
    /// A phone (ADB, MTP) refused: Android keeps the path for the system and its
    /// apps. ❌ Never macOS or Finder advice.
    Phone,
    /// A connected server (SFTP, WebDAV, NFS, or a mounted SMB/AFP/NFS share) refused
    /// the account or identity it was connected with. ❌ Never macOS or Finder advice,
    /// even when the backend carries an errno (NFS's `NFS3ERR_ACCES` is `EACCES`).
    Server,
}

impl From<cmdr_fs::volume::RemoteRefuser> for PermissionRefusal {
    fn from(refuser: cmdr_fs::volume::RemoteRefuser) -> Self {
        match refuser {
            cmdr_fs::volume::RemoteRefuser::Phone => Self::Phone,
            cmdr_fs::volume::RemoteRefuser::Server => Self::Server,
            cmdr_fs::volume::RemoteRefuser::ObjectStore => Self::ObjectStoreAccount,
        }
    }
}

impl PermissionRefusal {
    /// Reads a raw errno as one of the three answers above.
    pub fn from_errno(errno: Option<i32>) -> Self {
        match errno {
            Some(crate::local_disk::errno::EACCES) => Self::FolderPermissions,
            Some(crate::local_disk::errno::EPERM) => Self::SystemProtected,
            _ => Self::Unclassified,
        }
    }
}

/// Why a name can't be a fresh-ZIP entry, for
/// [`super::WriteOperationError::ArchiveEntryNameRefused`]. Mirrors the archive
/// reader's own quarantine reasons (`cmdr_archive::read::QuarantineReason`), which
/// decide it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ArchiveNameRefusal {
    /// A `..` component, once `\` reads as a separator.
    ParentTraversal,
    /// Deeper than the reader's component limit.
    TooDeep,
    /// Nothing left once separators and `.` components go.
    Empty,
}

/// Why a transfer that names its items one by one refused to start, for
/// [`super::WriteOperationError::NamedLeavesRefused`]. Every one is checked before the
/// operation is admitted, so nothing was written (`transfer/DETAILS.md` §
/// "Named destinations").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum NamedLeavesRefusal {
    /// A name was given for an item the transfer doesn't carry. `path` is that item.
    NotASource,
    /// Two items would land under one name, compared ignoring case and Unicode
    /// form, so on many disks one would replace the other. `path` is where they'd land.
    SameName,
    /// An item would land where another item of the same transfer is, replacing
    /// it before it's read or moved. `path` is where it would land.
    OntoASource,
    /// An item already sits in the destination folder, so naming it there is a
    /// rename, not a transfer. `path` is that item.
    AlreadyInDestination,
    /// The route can't take a name per item: an archive changeset (copy or move
    /// into a zip, move out of one). `path` is the destination.
    RouteCannotName,
}

/// What a refused operation would have done to a mounted drive, for
/// [`super::WriteOperationError::MountedVolumeInside`]. Each is its own sentence: a
/// MOVE of the folder holding the drive and an Overwrite REPLACING a folder
/// both run under a move, so the operation type can't tell them apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum MountedVolumeAction {
    /// A permanent delete would have walked into the drive and emptied it.
    Delete,
    /// An Overwrite would have set aside and removed the folder holding it.
    Replace,
    /// A move onto another volume would have copied the drive's files off it,
    /// then deleted them there.
    Move,
}

/// How a refused delete's source relates to the person's own home folder, for
/// [`super::WriteOperationError::HomeFolder`] and the delete dialog's matching
/// `ScanPreviewRefusal::HomeFolder`. Each gets its own sentence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum HomeFolderRelation {
    /// The source IS the home folder (`~`, `/Users/me/`, a path through a
    /// symlinked folder that lands there).
    IsHome,
    /// The source is a folder that holds the home folder: `/Users`, `/`.
    HoldsHome,
}

/// What a home-folder refusal stopped, for [`super::WriteOperationError::HomeFolder`].
/// Each reads differently: a delete loses the files, a move or a rename leaves
/// macOS looking for the account in the wrong place. A trash is a `Delete`, and
/// a move onto another volume (copy, then delete) is a `Move`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum HomeFolderAction {
    /// A permanent delete or a move to the Trash.
    Delete,
    /// A move, on the same disk or onto another volume.
    Move,
    /// A rename in place, F2's or a batch's.
    Rename,
}

/// Why the OS wouldn't take something to the Trash.
///
/// ❗ Classified from the `NSError` domain and code at the boundary, ❌ never from
/// its words: the wording is localized and reformats between macOS releases, and
/// `error-string-match` forbids it. `delete/trash.rs::classify_trash_refusal` is the
/// one place that decides.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum TrashRefusalKind {
    /// macOS said we may not touch the item at all (`NSFileWriteNoPermissionError`,
    /// `NSFileReadNoPermissionError`, or a POSIX `EPERM`/`EACCES`).
    NotPermitted,
    /// A permission refusal where the item itself carries a lock flag (Finder's
    /// Locked, `chflags uchg`). macOS answers it with the same 513 as any other
    /// permission refusal, so it's read from the item, at the refusal.
    Locked,
    /// A permission refusal where the item isn't locked but the folder holding it
    /// takes no writes from us (`access(W_OK)` on the parent), which a move to the
    /// Trash needs. Same 513 again; read from the folder, at the refusal.
    FolderNotWritable,
    /// macOS couldn't find a Trash for the item's volume
    /// (`NSFeatureUnsupportedError`). It reads as "this volume doesn't have one",
    /// and that is what it says for a File Provider folder the app can't reach,
    /// which is why the UI treats it as permission-adjacent rather than as a fact
    /// about the disk.
    NoTrashForVolume,
    /// Anything else, including every non-macOS refusal.
    Other,
}

/// A file that exceeds the destination filesystem's per-file size limit.
/// Carried by [`super::WriteOperationError::FilesTooLargeForFilesystem`] so the dialog
/// can list the offenders.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct OversizedFile {
    pub name: String,
    pub size: u64,
}
