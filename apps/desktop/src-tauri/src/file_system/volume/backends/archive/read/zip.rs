//! The zip format: central-directory parse and per-entry streaming decompress,
//! driving rc-zip's sans-IO state machines over an [`ArchiveByteSource`].
//!
//! This is the zip half of the multi-format core. It produces the same
//! format-neutral [`RawEntry`] list every format feeds into the shared tree
//! builder ([`super::index`]), and opens an [`ArchiveEntryReader`] the same way.
//! Everything zip-specific (rc-zip's `ArchiveFsm` / `EntryFsm`, the encryption GP
//! flag) lives here; the index and volume layers stay format-agnostic.

use std::io::Read;
use std::sync::Arc;

use rc_zip::fsm::{ArchiveFsm, EntryFsm, FsmResult};
use rc_zip::parse::Method;
use rc_zip::{Entry, EntryKind};
use zip::ZipArchive;
use zip::result::ZipError;

use super::error::ArchiveError;
use super::index::RawEntry;
use super::reader::{ArchiveEntryReader, CHUNK_SIZE, ChunkTx};
use super::source::{ArchiveByteSource, SourceReader};

/// General-purpose bit flag 0: the entry is encrypted (traditional PKWARE
/// ZipCrypto or WinZip AES). rc-zip parses the entry but does NOT decrypt, so an
/// encrypted entry's bytes are read through the `zip` crate's `by_index_decrypt`
/// (see [`open_read`]).
const GP_FLAG_ENCRYPTED: u16 = 1 << 0;

/// A zip entry's read handle: the rc-zip [`Entry`] (drives the plaintext decode)
/// plus its zero-based position in the central directory.
///
/// The ordinal addresses the `zip` crate's `by_index_decrypt` for the decrypt
/// path. rc-zip and the `zip` crate both parse the SAME central directory in the
/// SAME physical order, so the ordinal aligns across the two crates — pinned by
/// `archive_test::zip_crate_ordinals_align_with_rc_zip`. Decrypting by ordinal
/// (not by name) also sidesteps any cross-crate filename-decoding drift.
pub(super) struct ZipHandle {
    entry: Entry,
    ordinal: usize,
}

/// Whether the entry is encrypted: general-purpose flag bit 0, or the AE-x
/// (WinZip AES) marker method.
fn is_encrypted(entry: &Entry) -> bool {
    entry.flags & GP_FLAG_ENCRYPTED != 0 || entry.method == Method::Aex
}

/// Parses the central directory into the format-neutral entry list the tree
/// builder consumes, each paired with its [`ZipHandle`] (the read handle a later
/// [`open_read`] uses). This is the only I/O in the zip parse path.
pub(super) fn parse(source: &dyn ArchiveByteSource) -> Result<Vec<(RawEntry, ZipHandle)>, ArchiveError> {
    let entries = parse_central_directory(source)?;
    Ok(entries
        .into_iter()
        .enumerate()
        .map(|(ordinal, entry)| {
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
            (raw, ZipHandle { entry, ordinal })
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

/// Opens a streaming reader over the decompressed bytes of `handle`'s entry,
/// pulling compressed bytes from `source`.
///
/// A plaintext entry drives rc-zip's `EntryFsm` (`password` is ignored). An
/// encrypted entry — legacy PKWARE ZipCrypto (what macOS Archive Utility / `zip -e`
/// produce) OR WinZip AES (AE-1/AE-2, `7z -mem=AES256` / recent WinZip) — needs the
/// `zip` crate's decrypt path: with no `password`, returns [`ArchiveError::Encrypted`]
/// (the "needs a password" signal); with one, decrypts by central-directory ordinal
/// (see [`run_decrypt_producer`]). Both encryption kinds share this path — the `zip`
/// crate picks the cipher from the entry's own metadata. A wrong password is caught
/// deterministically at open for AES (its 2-byte verifier) and probabilistically for
/// ZipCrypto (a 1/256 slip caught late at the end-of-stream CRC).
pub(super) fn open_read(
    handle: &ZipHandle,
    source: Arc<dyn ArchiveByteSource>,
    password: Option<&[u8]>,
) -> Result<ArchiveEntryReader, ArchiveError> {
    let total_size = handle.entry.uncompressed_size;
    if is_encrypted(&handle.entry) {
        let Some(password) = password else {
            return Err(ArchiveError::Encrypted);
        };
        let ordinal = handle.ordinal;
        let password = password.to_vec();
        return Ok(ArchiveEntryReader::spawn_with(total_size, move |tx| {
            run_decrypt_producer(source, ordinal, &password, &tx);
        }));
    }
    let entry = handle.entry.clone();
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

/// The decrypt producer for an encrypted entry (ZipCrypto or WinZip AES): opens
/// the entry at central-directory `ordinal` through the `zip` crate over a fresh
/// [`SourceReader`] and streams the decrypted, decompressed bytes. Runs on a
/// `spawn_blocking` thread.
///
/// The `zip` crate re-parses the central directory itself and picks the cipher
/// from the entry's metadata; `ordinal` is the same index rc-zip assigned
/// (identical CD order — see [`ZipHandle`]). A wrong AES password fails at
/// `by_index_decrypt` (its 2-byte verifier) and maps via [`map_zip_err`]. A wrong
/// ZipCrypto password may slip its 1-byte open check (~1/256) and surface late as
/// an end-of-stream CRC mismatch, handled in [`pump_decrypt`].
fn run_decrypt_producer(source: Arc<dyn ArchiveByteSource>, ordinal: usize, password: &[u8], tx: &ChunkTx) {
    let reader = SourceReader::new(source);
    let mut archive = match ZipArchive::new(reader) {
        Ok(archive) => archive,
        Err(err) => return tx.send_err(map_zip_err(err)),
    };
    let mut file = match archive.by_index_decrypt(ordinal, password) {
        Ok(file) => file,
        Err(err) => return tx.send_err(map_zip_err(err)),
    };
    pump_decrypt(&mut file, tx);
}

/// Pumps a decrypted entry stream into the chunk sink in bounded blocks (peak
/// memory `CHANNEL_CAPACITY × CHUNK_SIZE`, never the whole entry).
///
/// An `InvalidData` I/O error at end of stream is an integrity check failing —
/// ZipCrypto's ~1/256 wrong password that slipped its 1-byte open check, or a
/// WinZip AES HMAC mismatch — so it maps to [`ArchiveError::WrongPassword`].
/// Classifying by the io ERROR KIND (not its message) keeps this within the
/// `no-string-matching` rule.
fn pump_decrypt(mut reader: impl Read, tx: &ChunkTx) {
    let mut buf = vec![0u8; CHUNK_SIZE];
    loop {
        match reader.read(&mut buf) {
            Ok(0) => return,
            Ok(n) => {
                if !tx.send(buf[..n].to_vec()) {
                    return; // consumer dropped the reader: cancel
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::InvalidData => {
                return tx.send_err(ArchiveError::WrongPassword);
            }
            Err(err) => return tx.send_err(ArchiveError::from(err)),
        }
    }
}

/// Maps a `zip`-crate error (from the decrypt path only) to a typed
/// [`ArchiveError`]. A wrong password is the headline case; the rest are
/// structural. The "password required" sentinel can't occur here (we only call
/// `by_index_decrypt` WITH a password), so it isn't matched — avoiding a
/// message-string comparison the `no-string-matching` rule forbids.
fn map_zip_err(err: ZipError) -> ArchiveError {
    match err {
        ZipError::InvalidPassword => ArchiveError::WrongPassword,
        ZipError::UnsupportedArchive(msg) => ArchiveError::Unsupported(msg.to_string()),
        ZipError::CompressionMethodNotSupported(id) => ArchiveError::Unsupported(format!("compression method {id}")),
        ZipError::FileNotFound => ArchiveError::NotFound(String::new()),
        ZipError::InvalidArchive(msg) => ArchiveError::Corrupt(msg.to_string()),
        ZipError::Io(io) => ArchiveError::from(io),
        // `ZipError` is `#[non_exhaustive]`; any future variant is a structural
        // fault on the path being read.
        other => ArchiveError::Corrupt(other.to_string()),
    }
}
