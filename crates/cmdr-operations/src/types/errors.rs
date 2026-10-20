//! What a write operation refuses with: `WriteOperationError` and the typed
//! payloads its variants carry (`ReadOnlySide`, `TransferRole`,
//! `DisconnectedSide`, `TrashRefusalKind`, `OversizedFile`). How one reads in
//! a log line (`log_detail`) is the `errors/log_detail.rs` child's.
//!
//! One level down from `types.rs` and re-exported from it, the way `events.rs`
//! is, so every caller keeps its `types::WriteOperationError` path. Same floor
//! rule: ❌ nothing here may `use` a `cmdr-operations` sibling.

use serde::{Deserialize, Serialize};

use super::{RecoveredOriginal, StagedLeftovers};

mod log_detail;

// ============================================================================
// Error enum (following MountError pattern)
// ============================================================================

/// Which half of a transfer refused the write, for [`WriteOperationError::ReadOnlyDevice`].
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
/// [`WriteOperationError::PermissionDenied`].
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
/// could change the answer, for [`WriteOperationError::PermissionDenied`].
///
/// Rust folds `EACCES` and `EPERM` into one `ErrorKind::PermissionDenied`, and the
/// two are opposite advice: one is a folder an administrator could write to, the
/// other is macOS itself refusing, where administrator rights change nothing.
/// Derived from the errno by [`WriteOperationError::permission_denied`], the only
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
}

impl PermissionRefusal {
    /// Reads a raw errno as one of the three answers above.
    pub fn from_errno(errno: Option<i32>) -> Self {
        match errno {
            Some(libc::EACCES) => Self::FolderPermissions,
            Some(libc::EPERM) => Self::SystemProtected,
            _ => Self::Unclassified,
        }
    }
}

/// Why a name can't be a fresh-ZIP entry, for
/// [`WriteOperationError::ArchiveEntryNameRefused`]. Mirrors the archive
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

/// What a refused operation would have done to a mounted drive, for
/// [`WriteOperationError::MountedVolumeInside`]. Each is its own sentence: a
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

/// Errors that can occur during write operations.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "snake_case", rename_all_fields = "camelCase")]
#[allow(
    missing_docs,
    reason = "a wire type: every `///` here reaches `bindings.ts` as JSDoc, so its fields and variants stay as they ship"
)]
pub enum WriteOperationError {
    SourceNotFound {
        path: String,
    },
    /// The destination folder isn't there (or the volume can't address it), so
    /// there was nowhere to put anything. Kept SEPARATE from `SourceNotFound`
    /// because the two send the user to opposite places: a missing source reads
    /// as "your file is gone" and starts a hunt for data loss, when the file is
    /// sitting untouched and it's the target folder that's missing. Which one a
    /// `VolumeError::NotFound` becomes is decided by the `PathRole` the caller
    /// passes to `map_volume_error`, never guessed from the path.
    DestinationNotFound {
        path: String,
    },
    /// The destination folder couldn't be created because something that isn't
    /// a folder sits where it, or one of the folders above it, has to be.
    /// Refused before anything is written, by the volume engines
    /// (`VolumeError::NotADirectory` out of `create_directory_all`) and the
    /// local one (`ensure_destination_dir`) alike.
    ///
    /// ❗ `path` is the thing IN THE WAY, which is often an ancestor of the
    /// folder the user typed. It is the whole point of the variant: as a
    /// `DestinationNotFound` or a generic `IoError` the dialog named the folder
    /// Cmdr was asked to create, or nothing, and the file to move aside was
    /// never mentioned.
    DestinationNotAFolder {
        path: String,
    },
    /// The volume holding the sources is a phone its provider lists, or a saved
    /// server, that nothing has connected yet, so no volume answers for it.
    /// Refused before anything is read. `path` is the first source as the
    /// caller sent it.
    ///
    /// ❌ Never `DeviceDisconnected`, which tells the user a session dropped
    /// mid-operation, and ❌ never a "volume not found", which reads as a place
    /// that's gone. Opening it in a pane is what connects it.
    SourceNotConnected {
        path: String,
    },
    /// The destination is a phone its provider lists, or a saved server, that
    /// nothing has connected yet. Refused before anything is written. Same
    /// wording rules as `SourceNotConnected`; the two stay separate for the same
    /// reason `SourceNotFound` and `DestinationNotFound` do.
    DestinationNotConnected {
        path: String,
    },
    /// The volume holding the sources left the registry, and nothing lists or
    /// saves it any more: a phone that was unplugged, or a server that went
    /// away, typically under a search-results pane still showing its files.
    /// Refused before anything is read. `path` is the first source as the
    /// caller sent it.
    ///
    /// ❌ Never `SourceNotConnected`: there's no row to open, so "open it from
    /// the volume switcher" would send the user looking for one. ❌ Never a bare
    /// "volume not found" either, which names an internal id.
    SourceNoLongerConnected {
        path: String,
    },
    /// Overwrite not enabled.
    DestinationExists {
        path: String,
    },
    /// A write the OS refused on permission grounds.
    ///
    /// ❗ Build it with [`WriteOperationError::permission_denied`], ❌ never as a
    /// literal. `refusal` is `errno` read as advice and the two must agree; a
    /// hand-written pair ships a sentence that contradicts the number under it,
    /// and later stops the elevated-operations engine from recognizing a refusal
    /// root could fix. All four construction sites go through the constructor.
    /// Full rationale: `crates/cmdr-operations/DETAILS.md` § "Naming the folder that
    /// refused a write".
    PermissionDenied {
        /// What was being written when the refusal came back.
        path: String,
        /// The OS's own sentence, for the technical-details block ONLY.
        message: cmdr_fs::error_detail::ErrorDetail,
        /// The OS's number for the refusal, so a bug report keeps it. `None` for a
        /// backend that raises its own refusal with no errno behind it.
        errno: Option<i32>,
        /// What that errno says about administrator rights, derived from it.
        refusal: PermissionRefusal,
        /// The folder that refuses writes, PROVED with `access(W_OK)` at the
        /// refusal (`error_classification::refusing_folder`).
        ///
        /// ❗ `None` when nothing could be proved, and the message then stays the
        /// generic one. ❌ Never fill it in from the operation's shape: a
        /// `rename(2)` needs both parents and its errno says nothing about which
        /// one refused, which is how a refused move came to tell a user to check
        /// the destination when it was the source folder that said no.
        refused_folder: Option<String>,
        /// Which half of the transfer refused, when the refusing site knows.
        side: Option<PermissionSide>,
    },
    /// The destination has no room for the transfer, MEASURED before anything was
    /// written, so both numbers are real.
    ///
    /// ❌ Never build one without measuring: a write the destination refused for
    /// lack of room is [`DestinationFull`](Self::DestinationFull), which carries no
    /// sizes, so the dialog can't say "needs 0 bytes but only has 0 bytes".
    InsufficientSpace {
        required: u64,
        available: u64,
        volume_name: Option<String>,
    },
    /// The destination ran out of room (or of quota) partway through, found out
    /// from the write it refused (`ENOSPC`, `EDQUOT`, a backend's `StorageFull`).
    /// Nothing was measured, so there are no sizes to show. `path` is the item
    /// being written when it happened, for the technical details.
    DestinationFull {
        path: String,
    },
    /// Would cause infinite recursion.
    DestinationInsideSource {
        source: String,
        destination: String,
    },
    /// Two of the selected items carry the same name, so they would land on one
    /// destination path and fight over it. Refused before anything is written:
    /// a cross-filesystem move stages both under that one name, and whichever
    /// arrives second meets the first one's files instead of an empty slot.
    /// Carries both paths, so the message can show which two clashed.
    DuplicateSourceNames {
        name: String,
        first: String,
        second: String,
    },
    /// A delete, an Overwrite, or a move onto another volume reached a folder
    /// another drive, disk image, or server is mounted on, inside the selection
    /// or as the selection itself. Refused before anything is removed or
    /// copied, as Finder does: going on would empty that volume, often a NAS
    /// other people use. `mount_point` is where it's mounted, so the message
    /// can say which one to eject; `action` is what the operation would have
    /// done to it.
    MountedVolumeInside {
        mount_point: String,
        action: MountedVolumeAction,
    },
    /// A fresh ZIP would hold an entry the archive reader quarantines: a name
    /// that climbs out of the archive (a macOS `..\notes.txt` reads as
    /// `../notes.txt`), nests too deep, or turns into nothing. Refused from the
    /// plan, before a byte is compressed. `entry` is the name as it would be
    /// written, so the message can say which file.
    ArchiveEntryNameRefused {
        entry: String,
        reason: ArchiveNameRefusal,
    },
    /// Two planned fresh-ZIP entries read back as one archive path, so one
    /// would shadow the other (`a\b.txt` next to a real `a/b.txt`: the reader
    /// treats `\` as a separator). Refused from the plan, before a byte is
    /// compressed.
    ArchiveEntryNamesCollide {
        entry: String,
        other: String,
        /// Where both would land inside the archive.
        archive_path: String,
    },
    /// The zip being edited holds two entries under one name. The rewrite
    /// copies through a library that keeps only the later one, so the earlier
    /// copy would silently go (Cmdr's reader shows only the later one too, but
    /// other tools show both). Refused before anything is written. `path` is
    /// the zip, `name` the shared entry name, `None` when the reader couldn't
    /// parse the zip to say which.
    ArchiveDuplicateEntryName {
        path: String,
        name: Option<String>,
    },
    /// The edit would have to keep a password-protected entry of the zip in a
    /// scheme the rewrite can't copy intact (ZipCrypto and WinZip AES copy fine;
    /// see `cmdr_archive`'s `MutationError::EncryptedEntryRetained`). Refused
    /// before anything is written; deleting the entry is fine. `path` is the
    /// zip, `name` the entry.
    ArchiveEncryptedEntry {
        path: String,
        name: String,
    },
    /// The edit would leave an entry of the zip under a FILE entry (`notes/a.txt`
    /// beside a file `notes`), which every reader drops, so it would sit in the
    /// zip unseen. Refused before anything is written. The copy-into planner
    /// asks about that clash first, so this is the backstop. `path` is the zip,
    /// `entry` the entry that would sit under `file`, both inner paths.
    ArchiveEntryUnderFile {
        path: String,
        entry: String,
        file: String,
    },
    SymlinkLoop {
        path: String,
    },
    Cancelled {
        message: cmdr_fs::error_detail::ErrorDetail,
    },
    /// Device was disconnected during the operation (USB, MTP, etc.).
    ///
    /// `side` says WHICH of the transfer's two volumes left, named as it was
    /// when the operation started: the frontend's volume list drops an unmounted
    /// volume, so by the time this is worded nothing can look the name up any
    /// more. `None` for a backend disconnect with no typed sides behind it (MTP,
    /// SMB), where the copy stays the volume-agnostic one.
    DeviceDisconnected {
        path: String,
        side: Option<DisconnectedSide>,
    },
    /// A move's closing flush couldn't prove the copied files were on disk, so
    /// the move kept every source where it was.
    ///
    /// `path` is what the flush was syncing and `errno` the OS's number for why
    /// it wouldn't; both come from `durability::FlushFailure`. `volume_name` is
    /// the destination volume as the transfer captured it at start.
    MoveNotConfirmed {
        path: String,
        errno: Option<i32>,
        volume_name: Option<String>,
    },
    /// A device or volume refused a write, and [`ReadOnlySide`] says WHICH half
    /// of the transfer it was.
    ///
    /// ❗ The side is load-bearing for the sentence the user reads: a move OFF a
    /// read-only source is refused because the source has no delete to pair with
    /// the copy, and telling that user to "choose a different destination" sends
    /// them to fix the half that was fine.
    ReadOnlyDevice {
        path: String,
        device_name: Option<String>,
        side: ReadOnlySide,
    },
    /// The destination folder takes no writes, found out BEFORE anything was
    /// created or measured (`Volume::write_access_at`).
    ///
    /// ❗ About the FOLDER, never the device: a phone's `/` refuses writes while
    /// its shared storage takes them, so "the phone is read-only" would be a lie.
    /// `reason` carries only what the backend can tell apart; a backend that
    /// can't tell read-only from no permission says `Unexplained`.
    DestinationNotWritable {
        path: String,
        reason: cmdr_fs::volume::UnwritableReason,
    },
    /// File is locked (macOS immutable flag, "Operation not permitted" on delete).
    FileLocked {
        path: String,
    },
    /// Volume doesn't support trash (network mounts, FAT, etc.).
    TrashNotSupported {
        path: String,
    },
    /// Network connection was interrupted or timed out.
    ConnectionInterrupted {
        path: String,
    },
    /// The connection to a SERVER (SMB, SFTP, WebDAV, S3) ended the operation:
    /// Cmdr waited for it to come back and gave up, or couldn't wait at all.
    /// The connection is the cause; `path` (the file it was on) is context only.
    ///
    /// ❗ Only the outage wait's verdict builds it (`volume_outage::lost_connection`,
    /// from typed facts: a transport fault, a session that reads not live, a
    /// backend that's a server), ❌ never a path or a message. A phone (MTP, ADB)
    /// or a local drive keeps `DeviceDisconnected`: unplugged is a device, not a
    /// server.
    ServerConnectionLost {
        path: String,
        /// The server's volume, named as it was when the operation stopped.
        volume_name: String,
        /// How long Cmdr waited for it before stopping, in whole seconds. `None`
        /// when it didn't wait this time (a sign-in only a person can give, or
        /// a file that had already ridden out its share of outages).
        gave_up_after_secs: Option<u64>,
    },
    /// A delete waited out a dropped connection, and the item it was about to
    /// delete again no longer matches what its walk listed (another kind, or
    /// another size): someone put a different file there meanwhile. The delete
    /// left it and stopped; everything it deleted before stays deleted.
    ///
    /// ❗ Only `delete/outage.rs`'s re-check builds it, from the listed kind and
    /// size against a fresh stat, ❌ never a guess: a delete has no rollback.
    ChangedWhileDisconnected {
        path: String,
        /// The volume whose connection dropped, named as it was when the
        /// delete stopped.
        volume_name: String,
    },
    /// A server's reply on a signed session carried a bad or missing signature, so Cmdr refused
    /// it and stopped (`VolumeError::SignatureInvalid`, SMB-only today). Its own variant so the
    /// stopped state can say so: it's a possible sign of tampering between the Mac and the
    /// server (or a broken network device), ❌ never a blip to retry. `message` is the backend's
    /// diagnostic, for the technical details only.
    ReplySignatureRefused {
        path: String,
        message: cmdr_fs::error_detail::ErrorDetail,
    },
    /// Couldn't read from the source.
    ReadError {
        path: String,
        message: cmdr_fs::error_detail::ErrorDetail,
    },
    /// Couldn't write to the destination.
    WriteError {
        path: String,
        message: cmdr_fs::error_detail::ErrorDetail,
    },
    /// File name exceeds the destination filesystem's length limit.
    NameTooLong {
        path: String,
    },
    /// File name contains characters not allowed at the destination.
    InvalidName {
        path: String,
        message: cmdr_fs::error_detail::ErrorDetail,
    },
    /// The file is in `STATUS_DELETE_PENDING` on the server: a delete was requested
    /// but at least one open handle is keeping it alive. Transient — clears when the
    /// last handle closes. SMB-only today.
    DeletePending {
        path: String,
    },
    /// The source file is archived in cold storage (S3 Glacier Flexible
    /// Retrieval or Deep Archive) and can't be read until someone restores it.
    /// Not transient: a retry meets the same archived object.
    SourceInColdStorage {
        path: String,
    },
    /// The source changed while the copy read it (an S3 server-side copy saw
    /// it replaced, or its stream delivered more or fewer bytes than its
    /// declared length), so the copy landed nothing that could mix two
    /// versions or miss a part. A move left the source in place. A retry
    /// copies the version that's there now.
    SourceChanged {
        path: String,
    },
    /// One or more files exceed the destination filesystem's per-file size
    /// limit (FAT32's 4 GiB cap). Detected during the pre-copy scan, before any
    /// bytes are written, so the whole operation is blocked all-or-nothing
    /// rather than failing partway through.
    FilesTooLargeForFilesystem {
        /// The destination filesystem, so the message can name it ("FAT32").
        filesystem: cmdr_fs::filesystem_kind::FilesystemKind,
        /// The per-file ceiling in bytes (FAT32: 4 GiB − 1).
        max_size: u64,
        /// Up to 10 offending files (name + size), largest first.
        files: Vec<OversizedFile>,
        /// Total number of offending files (may exceed `files.len()`).
        total_count: usize,
    },
    /// Extracting from a password-protected archive needs a password. Raised when
    /// a copy/move source is inside an encrypted archive: `wrong_attempt` is
    /// `true` when the stored password was rejected (so the FE re-prompts rather
    /// than prompting fresh). The FE sets a per-archive password via
    /// `set_archive_password` and retries the operation.
    ArchiveNeedsPassword {
        path: String,
        wrong_attempt: bool,
    },
    /// A cross-volume Overwrite wrote the new file completely, then couldn't give
    /// it the destination's name, and the destination it was replacing is
    /// already gone (the safe-replace deletes it between the last byte and the
    /// rename). The new data is intact at `kept_at`, under a ` (recovered)` name.
    ///
    /// ❗ `kept_at` is the whole point of the variant: it is the only place the
    /// user's new file exists, and a message that doesn't name it leaves them
    /// hunting. Typed so nothing has to parse it back out of prose.
    NewDataKeptAt {
        /// The name the file was meant to take.
        path: String,
        /// Where the complete new data is right now.
        kept_at: String,
        /// What the destination said when the rename was refused.
        message: cmdr_fs::error_detail::ErrorDetail,
    },
    /// A replace refused before it cleared anything: Cmdr couldn't confirm its
    /// note of the landing on disk (the in-flight ledger's writer is gone, or
    /// the log refused the line), and without it a crash after the clearing
    /// delete would leave the only copy of the new bytes on a temp nothing
    /// protects. Whatever held `path` is still there, and the temp is gone.
    ///
    /// ❗ Its own variant, ❌ never an `IoError` with prose: the dialog tells the
    /// user the file was left alone, and the backend carries no sentence.
    LandingNotRecorded {
        /// The name the new file was meant to take, still holding what was there.
        path: String,
    },
    /// The operation failed, and a folder that was replacing one of the user's
    /// files had already taken its name. Everything that landed is kept, so the
    /// folder stays; the file it displaced is beside it under a ` (recovered)`
    /// name rather than being thrown away with the aside.
    ///
    /// ❗ `recovered` is the whole point of the variant, and it is never empty:
    /// nothing else in the app tells the user their file changed names. `cause`
    /// carries what actually failed, so the dialog keeps that error's own advice
    /// (a full disk still says "free up space") instead of flattening every
    /// failure into one sentence.
    OriginalsKeptAside {
        cause: Box<WriteOperationError>,
        recovered: Vec<RecoveredOriginal>,
    },
    /// A cross-volume move whose copy landed COMPLETELY and whose original could
    /// not be removed. Nothing is lost and nothing is broken: the item now exists
    /// at both ends.
    ///
    /// ❗ Its own variant, never a flag on the refusal, because the fact the user
    /// needs is different in kind: "the move failed" makes them conclude nothing
    /// happened, and the next attempt then walks into a conflict with the copy
    /// that did land (cmdr-reports#17). The way out is theirs: remove the
    /// original, or fix what refused and remove it. `cause` is what refused the
    /// delete (a locked file, a busy one, a read-only source, an SMB
    /// `DeletePending`), so its own advice survives, the same shape as
    /// [`OriginalsKeptAside`](Self::OriginalsKeptAside).
    SourceNotRemoved {
        /// The original that stayed. For a folder, the item inside it that
        /// refused to go.
        path: String,
        /// Where the complete copy is.
        landed_at: String,
        cause: Box<WriteOperationError>,
    },
    /// The operation failed, and on its way to stopping it kept new files the
    /// person has to know about: complete files waiting for their names until
    /// the destination is back, or ones saved under a ` (recovered)` name
    /// because they couldn't take their own. Without it, someone finds their
    /// original gone and a scratch or ` (recovered)` file beside it with no word
    /// about it.
    ///
    /// ❗ `kept` is never empty and holds only the fates with new data in them
    /// (`StagedLeftoverFate::holds_new_data`). `cause` is what actually failed,
    /// so its own advice survives, the same shape as
    /// [`OriginalsKeptAside`](Self::OriginalsKeptAside).
    NewFilesKept {
        cause: Box<WriteOperationError>,
        kept: Vec<StagedLeftovers>,
    },
    /// The OS refused to move items to the Trash.
    ///
    /// Separate from [`IoError`](Self::IoError) because the REASON decides what the
    /// dialog can offer, and a reason has to survive the trip as a value. As an
    /// `IoError` it arrived as one sentence macOS had written, which left the dialog
    /// with nothing to say beyond "try again" — useless advice for a refusal that
    /// retrying cannot change.
    TrashRefused {
        /// How many top-level items it refused, so the message can be plural-correct
        /// without counting the sentences in `message`.
        item_count: usize,
        /// Why, classified at the OS boundary.
        reason: TrashRefusalKind,
        /// The OS's own words, for the technical-details disclosure ONLY.
        message: cmdr_fs::error_detail::ErrorDetail,
        /// At least one refused item is online-only (`SF_DATALESS`): its contents
        /// live on the provider's servers, which is the actual reason the trash
        /// wouldn't take it.
        ///
        /// ❗ It suppresses the "grant Full Disk Access" paragraph. An evicted
        /// file refuses as `NSError` 513 as readily as 3328 (`delete/cloud_trash.rs`),
        /// so the reason alone can't tell the two apart, and sending someone to
        /// System Settings for a permission that would change nothing is a wrong
        /// answer. Read at refusal time, when the item is still there to stat.
        #[serde(default)]
        online_only: bool,
    },
    /// Catch-all for genuinely unexpected IO errors.
    IoError {
        path: String,
        message: cmdr_fs::error_detail::ErrorDetail,
    },
}

/// The path of the item an in-flight failure happened on, from the variants
/// that carry one: `&String` from a shared borrow, `&mut String` from a unique
/// one. One list for [`WriteOperationError::item_path`] and
/// [`WriteOperationError::on_item_instead`], so they can't disagree on which
/// variants name an item.
macro_rules! item_path_of {
    ($error:expr) => {
        match $error {
            WriteOperationError::IoError { path, .. }
            | WriteOperationError::ReadError { path, .. }
            | WriteOperationError::WriteError { path, .. }
            | WriteOperationError::SourceNotFound { path }
            | WriteOperationError::DestinationNotFound { path }
            | WriteOperationError::DeviceDisconnected { path, .. }
            | WriteOperationError::PermissionDenied { path, .. }
            | WriteOperationError::DestinationFull { path }
            | WriteOperationError::ConnectionInterrupted { path }
            | WriteOperationError::ServerConnectionLost { path, .. }
            | WriteOperationError::ChangedWhileDisconnected { path, .. }
            | WriteOperationError::ReplySignatureRefused { path, .. }
            | WriteOperationError::FileLocked { path }
            | WriteOperationError::MoveNotConfirmed { path, .. }
            | WriteOperationError::NameTooLong { path }
            | WriteOperationError::InvalidName { path, .. }
            | WriteOperationError::DeletePending { path }
            | WriteOperationError::SourceInColdStorage { path }
            | WriteOperationError::SourceChanged { path }
            | WriteOperationError::LandingNotRecorded { path }
            | WriteOperationError::SymlinkLoop { path } => Some(path),
            _ => None,
        }
    };
}

impl WriteOperationError {
    /// The same failure, naming the new files its stop kept (`leftovers`'
    /// groups whose fate holds new data) as [`NewFilesKept`](Self::NewFilesKept);
    /// untouched when there are none.
    #[must_use]
    pub fn with_new_files_kept(self, leftovers: &[StagedLeftovers]) -> Self {
        let kept: Vec<StagedLeftovers> = leftovers
            .iter()
            .filter(|group| group.fate.holds_new_data())
            .cloned()
            .collect();
        if kept.is_empty() {
            return self;
        }
        Self::NewFilesKept {
            cause: Box::new(self),
            kept,
        }
    }

    /// The item an in-flight failure happened ON, as its variant names it: the
    /// file or folder the operation was reading or writing when it stopped.
    ///
    /// `None` for a refusal raised before any I/O (no single item, or one that
    /// isn't a file), and for the variants whose payload is the point
    /// (`NewDataKeptAt`, `OriginalsKeptAside`, `SourceNotRemoved`, `NewFilesKept`): a caller
    /// that rewrites the error by its path would drop where the user's data
    /// is. ❌ Never read an item out of a message.
    pub fn item_path(&self) -> Option<&str> {
        item_path_of!(self).map(String::as_str)
    }

    /// The same failure, named on `to` when its item is `from` (untouched
    /// otherwise). For a staged write: its temp is a name the user never saw,
    /// so a failure there is about the file the temp was becoming.
    #[must_use]
    pub fn on_item_instead(mut self, from: &std::path::Path, to: &std::path::Path) -> Self {
        if let Some(path) = item_path_of!(&mut self)
            && std::path::Path::new(path.as_str()) == from
        {
            *path = to.display().to_string();
        }
        self
    }

    /// The one way to build [`PermissionDenied`](Self::PermissionDenied).
    ///
    /// It derives `refusal` from `errno`, so no caller can ship a variant whose
    /// advice contradicts the errno it was built from.
    pub fn permission_denied(
        path: String,
        message: String,
        errno: Option<i32>,
        refused_folder: Option<String>,
        side: Option<PermissionSide>,
    ) -> Self {
        Self::PermissionDenied {
            path,
            message: message.into(),
            errno,
            refusal: PermissionRefusal::from_errno(errno),
            refused_folder,
            side,
        }
    }

    /// [`PermissionDenied`](Self::PermissionDenied) from an object store
    /// account (S3), which carries no errno: the refusal names the account, so
    /// the advice covers both of its causes (`PermissionRefusal::ObjectStoreAccount`).
    pub fn object_store_refused(path: String, message: String, side: Option<PermissionSide>) -> Self {
        Self::PermissionDenied {
            path,
            message: message.into(),
            errno: None,
            refusal: PermissionRefusal::ObjectStoreAccount,
            refused_folder: None,
            side,
        }
    }
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
/// Carried by [`WriteOperationError::FilesTooLargeForFilesystem`] so the dialog
/// can list the offenders.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct OversizedFile {
    pub name: String,
    pub size: u64,
}
