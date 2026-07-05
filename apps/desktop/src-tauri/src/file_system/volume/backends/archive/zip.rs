//! The zip format: central-directory parse and per-entry streaming decompress,
//! driving rc-zip's sans-IO state machines over an [`ArchiveByteSource`].
//!
//! This is the zip half of the multi-format core. It produces the same
//! format-neutral [`RawEntry`] list every format feeds into the shared tree
//! builder ([`super::index`]), and opens an [`ArchiveEntryReader`] the same way.
//! Everything zip-specific (rc-zip's `ArchiveFsm` / `EntryFsm`, the encryption GP
//! flag) lives here; the index and volume layers stay format-agnostic.

use std::sync::Arc;

use rc_zip::fsm::{ArchiveFsm, EntryFsm, FsmResult};
use rc_zip::parse::Method;
use rc_zip::{Entry, EntryKind};

use super::error::ArchiveError;
use super::index::RawEntry;
use super::read::{ArchiveEntryReader, CHUNK_SIZE, ChunkTx};
use super::source::ArchiveByteSource;

/// General-purpose bit flag 0: the entry is encrypted (traditional PKWARE or
/// strong encryption). We don't decrypt, so extraction of such an entry is
/// rejected.
const GP_FLAG_ENCRYPTED: u16 = 1 << 0;

/// Whether the entry is encrypted: general-purpose flag bit 0, or the AE-x
/// (WinZip AES) marker method.
fn is_encrypted(entry: &Entry) -> bool {
    entry.flags & GP_FLAG_ENCRYPTED != 0 || entry.method == Method::Aex
}

/// Parses the central directory into the format-neutral entry list the tree
/// builder consumes, each paired with its rc-zip [`Entry`] (the read handle a
/// later [`open_read`] uses). This is the only I/O in the zip parse path.
pub(super) fn parse(source: &dyn ArchiveByteSource) -> Result<Vec<(RawEntry, Entry)>, ArchiveError> {
    let entries = parse_central_directory(source)?;
    Ok(entries
        .into_iter()
        .map(|entry| {
            let encrypted = is_encrypted(&entry);
            let is_symlink = entry.kind() == EntryKind::Symlink;
            // A directory is signalled either by the mode bits or (very commonly)
            // by a trailing slash on the name. A symlink is never a dir.
            let is_dir = !is_symlink && (entry.kind() == EntryKind::Directory || entry.name.ends_with('/'));
            let raw = RawEntry {
                name: entry.name.clone(),
                is_dir,
                is_symlink,
                size: entry.uncompressed_size,
                compressed_size: entry.compressed_size,
                modified: Some(entry.modified.timestamp()),
                encrypted,
            };
            (raw, entry)
        })
        .collect())
}

/// Drives rc-zip's central-directory state machine over the byte source,
/// returning the flat entry list.
fn parse_central_directory(source: &dyn ArchiveByteSource) -> Result<Vec<Entry>, ArchiveError> {
    let size = source.size();
    if size == 0 {
        // rc-zip would report an EOCD-not-found; short-circuit for clarity.
        return Err(ArchiveError::NotAnArchive);
    }

    let mut fsm = ArchiveFsm::new(size);
    loop {
        if let Some(offset) = fsm.wants_read() {
            let space = fsm.space();
            let n = source.read_at(offset, space)?;
            if n == 0 {
                return Err(ArchiveError::Corrupt(
                    "unexpected end of file while reading the central directory".to_string(),
                ));
            }
            fsm.fill(n);
        }

        fsm = match fsm.process()? {
            FsmResult::Done(archive) => return Ok(archive.entries().cloned().collect()),
            FsmResult::Continue(next) => next,
        };
    }
}

/// Opens a streaming reader over the decompressed bytes of `entry`, pulling
/// compressed bytes from `source`. Rejects an encrypted entry up front (we don't
/// decrypt).
pub(super) fn open_read(entry: &Entry, source: Arc<dyn ArchiveByteSource>) -> Result<ArchiveEntryReader, ArchiveError> {
    if is_encrypted(entry) {
        return Err(ArchiveError::Encrypted);
    }
    let total_size = entry.uncompressed_size;
    let entry = entry.clone();
    Ok(ArchiveEntryReader::spawn_with(total_size, move |tx| {
        run_producer(source, entry, tx)
    }))
}

/// The blocking producer: drives rc-zip's entry state machine to completion,
/// sending each decompressed chunk over `tx`. Runs on a `spawn_blocking` thread.
fn run_producer(source: Arc<dyn ArchiveByteSource>, entry: Entry, tx: ChunkTx) {
    // The entry data begins at its local header; the fsm parses that header then
    // the compressed data as one forward stream.
    let mut offset = entry.header_offset;
    let mut fsm = EntryFsm::new(Some(entry), None);
    let mut out = vec![0u8; CHUNK_SIZE];
    // The fsm reads ahead (its buffer always has spare room), so it asks to read
    // past the entry's own bytes into the central directory — and reaches the
    // real file end even for a complete entry. So EOF alone isn't truncation;
    // only EOF *plus* a `process` that then makes no progress is.
    let mut at_eof = false;

    loop {
        if fsm.wants_read() && !at_eof {
            let space = fsm.space();
            let n = match source.read_at(offset, space) {
                Ok(n) => n,
                Err(err) => {
                    tx.send_err(ArchiveError::from(err));
                    return;
                }
            };
            if n == 0 {
                at_eof = true;
            } else {
                offset += n as u64;
                fsm.fill(n);
            }
        }

        match fsm.process(&mut out) {
            Ok(FsmResult::Continue((next, outcome))) => {
                fsm = next;
                if outcome.bytes_written > 0 {
                    if !tx.send(out[..outcome.bytes_written].to_vec()) {
                        return; // consumer dropped the reader: cancel
                    }
                } else if at_eof && outcome.bytes_read == 0 {
                    // At EOF with no bytes consumed or produced: the fsm still
                    // wants input it can't get — the entry is truncated. Bail with
                    // a typed error instead of spinning.
                    tx.send_err(ArchiveError::Corrupt("archive entry data is truncated".to_string()));
                    return;
                }
            }
            Ok(FsmResult::Done(_)) => return,
            Err(err) => {
                tx.send_err(ArchiveError::from(err));
                return;
            }
        }
    }
}
