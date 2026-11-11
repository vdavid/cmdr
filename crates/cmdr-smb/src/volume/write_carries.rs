//! What a streamed SMB write carries onto its file beside the bytes, and what
//! it learns closing it: the values `write_stream.rs` (the entry and its
//! compound frame) and `checkpointed_write.rs` (the streaming loop beneath it)
//! both pass around. A leaf of its own, so the edge between those two stays
//! one-way: `write_stream` hands its streaming path down to
//! `checkpointed_write`, which never names `write_stream`.

use cmdr_fs::metadata::{CarriedMetadata, ClassOutcome};

/// What a write learned closing its file, for its report.
pub(super) struct ClosedWriter {
    pub(super) xattrs_kept: ClassOutcome,
    /// The server refused the dates.
    pub(super) times_refused: bool,
    /// The bytes the server acknowledged once `finish()` drained the window.
    pub(super) confirmed: u64,
}

/// What a write puts on its file beside the bytes, asked of the source stream
/// once, before its first chunk.
pub(super) struct CarriedForWrite {
    pub(super) carried: CarriedMetadata,
    pub(super) times: Option<smb2::FileTimes>,
}

/// What [`close_writer`](super::SmbVolume::close_writer) puts on a streamed-into file, and the
/// session it does that on beside the writer.
pub(super) struct WriteCarries<'a> {
    pub(super) tree: &'a smb2::client::Tree,
    /// For the xattrs beside the writer, which owns its own connection.
    pub(super) side_conn: &'a mut smb2::client::Connection,
    pub(super) carried: &'a CarriedForWrite,
}
