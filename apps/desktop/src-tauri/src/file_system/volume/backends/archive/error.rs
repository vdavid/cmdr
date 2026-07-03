//! Typed errors for the archive reading core.
//!
//! Every failure mode the reader can hit is a distinct variant, so callers
//! classify by pattern-matching, never by inspecting the message string (the
//! project `no-string-matching` rule). The `String` payloads are for display /
//! logging only.

use rc_zip::error::{Error as RcZipError, FormatError};

/// A failure while parsing or reading a zip archive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArchiveError {
    /// The bytes are not a zip archive at all: no end-of-central-directory
    /// record was found. This is what feeding a RAR/7z/plain file to the zip
    /// reader produces (magic-byte format detection is the routing layer's job,
    /// not ours).
    NotAnArchive,

    /// The archive is a zip, but its central directory (or an entry header) is
    /// corrupt or truncated: a real EOCD was found but the structure past it
    /// didn't parse, or the file ended mid-record.
    Corrupt(String),

    /// The requested entry is encrypted. We don't support extracting encrypted
    /// entries (browsing still works — the names live in the central directory);
    /// opening one for read is rejected here.
    Encrypted,

    /// The archive is a valid zip but uses something we can't handle: a
    /// compression method this build doesn't decode, or an unsupported LZMA
    /// variant.
    Unsupported(String),

    /// No entry exists at the requested inner path.
    NotFound(String),

    /// The requested inner path resolves to a directory, not a readable file.
    IsADirectory(String),

    /// The underlying byte source failed (dead mount, read error, permission).
    Io(String),
}

impl std::fmt::Display for ArchiveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotAnArchive => f.write_str("not a zip archive"),
            Self::Corrupt(msg) => write!(f, "corrupt archive: {msg}"),
            Self::Encrypted => f.write_str("archive entry is encrypted"),
            Self::Unsupported(msg) => write!(f, "unsupported archive feature: {msg}"),
            Self::NotFound(path) => write!(f, "no such entry: {path}"),
            Self::IsADirectory(path) => write!(f, "entry is a directory: {path}"),
            Self::Io(msg) => write!(f, "archive I/O error: {msg}"),
        }
    }
}

impl std::error::Error for ArchiveError {}

impl From<std::io::Error> for ArchiveError {
    fn from(err: std::io::Error) -> Self {
        // A short read at the end of the file surfaces as UnexpectedEof; that
        // means a truncated archive, not a live I/O fault, so classify it as
        // Corrupt rather than Io.
        if err.kind() == std::io::ErrorKind::UnexpectedEof {
            Self::Corrupt(err.to_string())
        } else {
            Self::Io(err.to_string())
        }
    }
}

impl From<RcZipError> for ArchiveError {
    fn from(err: RcZipError) -> Self {
        match err {
            // The single most common "this isn't a zip" signal: the reader
            // scanned the tail and never found an EOCD signature.
            RcZipError::Format(FormatError::DirectoryEndSignatureNotFound) => Self::NotAnArchive,
            // Any other structural parse failure means a real-but-broken zip.
            RcZipError::Format(fmt) => Self::Corrupt(fmt.to_string()),
            // Unsupported / not-enabled compression method, or an LZMA variant
            // we don't decode.
            RcZipError::Unsupported(u) => Self::Unsupported(u.to_string()),
            // Bad text encoding in a name/comment: rare, treat as corrupt.
            RcZipError::Encoding(e) => Self::Corrupt(format!("{e:?}")),
            // Decompression failure (bad compressed bytes) shows up mid-read.
            RcZipError::Decompression { method, msg } => Self::Corrupt(format!("{method:?}: {msg}")),
            RcZipError::IO(io) => Self::from(io),
            RcZipError::UnknownSize => Self::Io("archive size could not be determined".to_string()),
        }
    }
}
