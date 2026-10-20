//! What an edit asks for ([`Changeset`]) and how it can fail
//! ([`MutationError`]): the vocabulary both the planner (`plan.rs`) and the
//! writer (`mutator.rs`) speak, kept here so the plan feeds the write one way.
//! Re-exported from `mutator`, where callers name them.

use cmdr_fs::log_error::{ErrorLogText, ErrorText, write_plain};
use cmdr_fs::log_private::LogPrivate;
use std::collections::HashMap;
use std::io;
use std::path::PathBuf;

/// Where an added entry's bytes come from.
pub enum AddSource {
    /// Stream from a local file (a copy/move INTO the archive). Never whole-buffered.
    LocalPath(PathBuf),
    /// In-memory bytes: a mkfile (empty), or a small resident payload / test input.
    Bytes(Vec<u8>),
}

/// One file to add to the archive at `inner_path` (`/`-separated, archive-root-relative).
pub struct AddEntry {
    /// Where the entry lands, `/`-separated and relative to the archive root.
    pub inner_path: String,
    /// Where its bytes come from.
    pub source: AddSource,
}

/// A batch of changes applied to a zip in a single rewrite pass. Built by the
/// driver from a resolved transfer/mutation (conflicts already resolved), then
/// handed to `mutator::apply`, which is deterministic and conflict-free.
///
/// Ordering of application: retained entries (verbatim) first, then `mkdirs`,
/// then `adds`. `deletes` and `renames` reshape the retained set (they never add
/// bytes). Inner paths are trimmed of surrounding slashes internally, so a
/// caller may pass `dir` or `/dir/` interchangeably.
#[derive(Default)]
pub struct Changeset {
    /// Files to add (compressed from their source). A mkfile is an add of empty `Bytes`.
    pub adds: Vec<AddEntry>,
    /// Explicit directory entries to create (`add_directory`).
    pub mkdirs: Vec<String>,
    /// The local folder a `mkdirs` entry is made from, by its inner path, when
    /// there is one (a copy INTO the archive): its xattrs ride an AppleDouble
    /// sidecar (see `mutator::local_folder_sidecar`). A mkdir with no entry here has
    /// no source to carry.
    pub mkdir_sources: HashMap<String, PathBuf>,
    /// Inner paths to drop. A directory target drops its whole subtree.
    pub deletes: Vec<String>,
    /// `true` for a move out: a delete takes only the entries the reader's tree
    /// shows, by where it shows them, which are exactly the ones the extract
    /// wrote out. What the tree hides under a moved folder (a `..` component, a
    /// file under a file, a file at a folder's path) never landed anywhere, so
    /// it stays. `false` (an F8) takes the whole raw subtree, hidden entries
    /// included, since the user asked for the folder gone.
    pub deletes_spare_hidden: bool,
    /// `(from, to)` inner-path renames. A directory target rewrites every descendant's prefix.
    pub renames: Vec<(String, String)>,
    /// Deflate level (1..=9) for NEWLY added entries; `None` = the `zip` crate
    /// default (level 6). Retained entries are raw-copied and unaffected. Set once
    /// per edit from the user's compression-level setting; see `add_entry_options`.
    pub compression_level: Option<i64>,
}

/// Why an archive edit failed. Cancellation is a first-class, non-error outcome
/// (the temp is abandoned, the original is untouched); the rest are genuine
/// faults. Typed so the driver classifies without message matching.
#[derive(Debug)]
pub enum MutationError {
    /// The user cancelled: the temp was removed, the original is fully intact.
    Cancelled,
    /// Couldn't open the original archive for reading.
    OpenOriginal(io::Error),
    /// The `zip` crate rejected the original or a write into the temp.
    Zip(zip::result::ZipError),
    /// The archive holds an encrypted entry the edit would have to keep, in a
    /// scheme the raw copy can't carry intact: PKWARE Strong Encryption, masked
    /// headers, or a time-checked ZipCrypto entry whose DOS time the `zip` crate
    /// couldn't parse. ZipCrypto and WinZip AES entries otherwise carry fine
    /// (`plan.rs`, `carries_its_encryption`). We refuse the whole edit rather
    /// than leave an entry no password opens. Deleting the entry is allowed (it
    /// isn't kept).
    EncryptedEntryRetained {
        /// The entry's inner path, so the host can name it.
        name: String,
    },
    /// Two entries share a name. The `zip` crate folds them into one (the later
    /// one's data), so a rewrite would silently drop the earlier copy, which
    /// the reader hides too but other tools may show. Refused before any temp
    /// exists, the original untouched.
    DuplicateEntryName {
        /// The shared name, as the reader shows it; `None` when the reader
        /// couldn't parse the archive to say which.
        name: Option<String>,
    },
    /// The edit would leave an entry under a FILE entry (`notes/a.txt` beside
    /// a file `notes`), which every reader, Cmdr's and `unzip` alike, drops, so
    /// the entry would be in the zip and nowhere to be seen. Refused before any
    /// temp exists, the original untouched. Only what the edit writes or
    /// renames is checked, so a zip that already holds such an entry stays
    /// editable. Names have no trailing slash.
    EntryUnderFile {
        /// The entry that would sit under the file.
        entry: String,
        /// The file entry it would sit under.
        file: String,
    },
    /// An added entry's source bytes couldn't be read.
    ReadSource {
        /// Where the entry would have landed in the archive.
        inner_path: String,
        /// What the read failed with.
        source: io::Error,
    },
    /// Filesystem I/O on the temp (create, flush, fsync, rename).
    Io(io::Error),
}

impl std::fmt::Display for MutationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write_plain(self, f)
    }
}

/// The one text behind `Display` and `LogError`: an entry's path inside the archive is marked.
impl ErrorLogText for MutationError {
    fn write_log_text(&self, out: &mut ErrorText<'_>) -> std::fmt::Result {
        use std::fmt::Write as _;
        match self {
            MutationError::Cancelled => write!(out, "the archive edit was cancelled"),
            MutationError::OpenOriginal(e) => write!(out, "couldn't open the archive: {e}"),
            MutationError::Zip(e) => write!(out, "the archive couldn't be rewritten: {e}"),
            MutationError::EncryptedEntryRetained { name } => {
                out.write_str("the archive can't be edited because it contains an encrypted entry ('")?;
                out.value(LogPrivate::path(name))?;
                out.write_str("')")
            }
            MutationError::DuplicateEntryName { name: Some(name) } => {
                out.write_str("the archive can't be edited because two of its entries are named '")?;
                out.value(LogPrivate::path(name))?;
                out.write_str("'")
            }
            MutationError::DuplicateEntryName { name: None } => {
                write!(
                    out,
                    "the archive can't be edited because two of its entries share a name"
                )
            }
            MutationError::EntryUnderFile { entry, file } => {
                out.write_str("the archive can't be edited because '")?;
                out.value(LogPrivate::path(entry))?;
                out.write_str("' would sit under the file '")?;
                out.value(LogPrivate::path(file))?;
                out.write_str("'")
            }
            MutationError::ReadSource { inner_path, source } => {
                out.write_str("couldn't read the source for '")?;
                out.value(LogPrivate::path(inner_path))?;
                write!(out, "': {source}")
            }
            MutationError::Io(e) => write!(out, "couldn't write the new archive: {e}"),
        }
    }
}

impl std::error::Error for MutationError {}
