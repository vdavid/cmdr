//! The 7z format (read-only): parse the header metadata into the shared tree
//! without decompressing, then stream one entry at a time.
//!
//! 7z keeps a metadata header (at the END of the file, like zip's central
//! directory) listing every entry with its size, so the INDEX is built from
//! metadata alone — no decompression, one header read. Extraction is SEQUENTIAL:
//! entries live in compression blocks ("folders"), and a solid block concatenates
//! several entries into one stream, so reaching a target entry decodes the block
//! prefix in front of it. `sevenz-rust2` drives that decode; we stop at the
//! target ([`super::format::ArchiveFormat::is_sequential`] is `true` for 7z).
//!
//! Encryption: a password-protected 7z decrypts with the `aes256` feature (the
//! `AES256_SHA256` coder). `sevenz-rust2` wants the password at
//! `ArchiveReader::new` time, so a per-archive password is threaded through
//! [`parse`] AND every re-open ([`SevenZStore::open_read`] → [`stream_entry`],
//! [`stream_subtree`]). Two encryption shapes: CONTENT-encrypted (`7z -mhe=off`,
//! plaintext header) lists with no password and needs one only to extract;
//! HEADER-encrypted (`7z -mhe=on`) needs the password to even read the metadata,
//! so `parse` fails without one. [`map_sevenz_err`] maps the missing/wrong password
//! to the typed [`ArchiveError::Encrypted`] / [`ArchiveError::WrongPassword`] the
//! volume layer surfaces as a prompt, at both open and decode time.

use std::collections::HashMap;
use std::sync::Arc;

use sevenz_rust2::{ArchiveReader, Password};

use super::error::ArchiveError;
use super::extract::{SubtreeMember, SubtreeTx};
use super::index::{MAX_TREE_NODES, RawEntry};
use super::name::{SanitizedName, sanitize_entry_name};
use super::reader::{ArchiveEntryReader, pump_chunks, pump_read};
use super::source::{ArchiveByteSource, SourceReader};

/// The 7z entry store: the set of readable entry paths. A read re-opens the
/// archive over a fresh byte source and decodes to the target entry.
pub(super) struct SevenZStore {
    members: HashMap<String, ()>,
}

impl SevenZStore {
    pub(super) fn new(_meta: (), members: HashMap<String, ()>) -> Self {
        Self { members }
    }

    /// Opens a streaming reader for the file at `inner` by decoding its block up
    /// to the entry. O(block prefix) for a solid archive. `total` is the tree's
    /// uncompressed size (for progress); the reader streams exactly what 7z yields.
    /// `password` decrypts a content-encrypted 7z (ignored for a plaintext one).
    pub(super) fn open_read(
        &self,
        inner: &str,
        total: u64,
        source: Arc<dyn ArchiveByteSource>,
        password: Option<&str>,
    ) -> Result<ArchiveEntryReader, ArchiveError> {
        if !self.members.contains_key(inner) {
            return Err(ArchiveError::NotFound(inner.to_string()));
        }
        let target = inner.to_string();
        let password = password.map(str::to_owned);
        Ok(ArchiveEntryReader::spawn_with(total, move |tx| {
            stream_entry(source, &target, password.as_deref(), &tx);
        }))
    }
}

/// Turns an optional password string into the `sevenz-rust2` [`Password`]
/// (`None` ⇒ empty, which reads a plaintext archive and makes an encrypted one
/// report `PasswordRequired` — mapped to [`ArchiveError::Encrypted`]).
fn to_password(password: Option<&str>) -> Password {
    password.map(Password::new).unwrap_or_else(Password::empty)
}

/// Parses the 7z header into the format-neutral entry list. Metadata only — no
/// block is decompressed here. `password` is needed only for a HEADER-encrypted
/// 7z (`-mhe=on`), whose metadata is itself encrypted; a content-encrypted or
/// plaintext archive parses with `None`.
pub(super) fn parse(
    source: Arc<dyn ArchiveByteSource>,
    password: Option<&str>,
) -> Result<(Vec<(RawEntry, ())>, ()), ArchiveError> {
    let reader = SourceReader::new(source);
    let archive =
        ArchiveReader::new(reader, to_password(password)).map_err(|e| map_sevenz_err_pw(e, password.is_some()))?;
    let archive = archive.archive();

    let mut out: Vec<(RawEntry, ())> = Vec::with_capacity(archive.files.len());
    for entry in &archive.files {
        let is_dir = entry.is_directory();
        out.push((
            RawEntry {
                name: entry.name().to_string(),
                is_dir,
                // 7z symlinks are rare and not distinguished by this API surface;
                // treat every non-dir as a regular file.
                is_symlink: false,
                size: if is_dir { 0 } else { entry.size() },
                compressed_size: if is_dir { 0 } else { entry.size() },
                modified: unix_seconds(entry),
                encrypted: false,
            },
            (),
        ));
        if out.len() > MAX_TREE_NODES {
            return Err(ArchiveError::TooLarge(format!(
                "7z exceeds the {MAX_TREE_NODES}-entry limit"
            )));
        }
    }

    Ok((out, ()))
}

/// Best-effort last-modified time as Unix seconds, or `None` when the entry
/// carries no timestamp (a zero/absent `NtTime` is the 1601 epoch, before Unix
/// time, so it maps to `None`).
fn unix_seconds(entry: &sevenz_rust2::ArchiveEntry) -> Option<i64> {
    let system: std::time::SystemTime = entry.last_modified_date().into();
    system
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs() as i64)
}

/// Decodes to the entry named `target` (matching the sanitized inner path the
/// index keyed it under) and streams its bytes. `password` decrypts a
/// content-encrypted archive.
fn stream_entry(source: Arc<dyn ArchiveByteSource>, target: &str, password: Option<&str>, tx: &super::reader::ChunkTx) {
    let had_password = password.is_some();
    let reader = SourceReader::new(source);
    let mut archive = match ArchiveReader::new(reader, to_password(password)) {
        Ok(a) => a,
        Err(err) => return tx.send_err(map_sevenz_err_pw(err, had_password)),
    };

    let mut streamed = false;
    let result = archive.for_each_entries(
        &mut |entry: &sevenz_rust2::ArchiveEntry, entry_reader: &mut dyn std::io::Read| {
            if entry_matches(entry.name(), target) {
                pump_read(entry_reader, tx, None, |io| map_stream_err(io, had_password));
                streamed = true;
                // Stop: the target is delivered, don't decode the rest.
                return Ok(false);
            }
            // In a SOLID block the entries share one decode stream, so an earlier
            // entry must be fully consumed to advance the stream to the target
            // (skipping without reading desyncs the decoder — a checksum failure).
            std::io::copy(entry_reader, &mut std::io::sink())?;
            Ok(true)
        },
    );

    match result {
        Ok(_) if streamed => {}
        Ok(_) => tx.send_err(ArchiveError::NotFound(target.to_string())),
        Err(err) => tx.send_err(map_sevenz_err_pw(err, had_password)),
    }
}

/// Whether `name`'s sanitized inner path equals `target`.
fn entry_matches(name: &str, target: &str) -> bool {
    matches!(sanitize_entry_name(name), SanitizedName::Accepted(p) if p == target)
}

/// One-pass subtree extract: decode the 7z stream ONCE and stream every file in
/// `wanted` (sanitized inner path → uncompressed size) in archive order. 7z is
/// natively single-pass via `for_each_entries`; a member not in `wanted` is still
/// read to a sink so the SOLID-block decoder advances to the next member (skipping
/// without reading desyncs it). Stops once every wanted file is delivered, so a
/// subtree near the front doesn't decode trailing blocks.
pub(super) fn stream_subtree(
    source: Arc<dyn ArchiveByteSource>,
    mut wanted: HashMap<String, u64>,
    password: Option<&str>,
    tx: &SubtreeTx,
) {
    if wanted.is_empty() {
        return;
    }
    let had_password = password.is_some();
    let reader = SourceReader::new(source);
    let mut archive = match ArchiveReader::new(reader, to_password(password)) {
        Ok(a) => a,
        Err(err) => return tx.send_err(map_sevenz_err_pw(err, had_password)),
    };

    // A send-side failure (consumer dropped) or a byte-source read error caught
    // inside the closure; carried out so the outer match doesn't re-report the
    // `for_each_entries` `Ok(false)` early-stop as an error.
    let mut caught: Option<ArchiveError> = None;
    let mut aborted = false;
    let result = archive.for_each_entries(
        &mut |entry: &sevenz_rust2::ArchiveEntry, entry_reader: &mut dyn std::io::Read| {
            let sanitized = match sanitize_entry_name(entry.name()) {
                SanitizedName::Accepted(path) => path,
                // Quarantined/unnameable entry: consume so the solid decoder advances.
                _ => {
                    std::io::copy(entry_reader, &mut std::io::sink())?;
                    return Ok(true);
                }
            };
            match wanted.remove(&sanitized) {
                Some(size) => {
                    if !tx.send_member(SubtreeMember {
                        inner_path: sanitized,
                        size,
                    }) {
                        aborted = true;
                        return Ok(false); // consumer gone: stop decoding
                    }
                    // The 7z entry reader yields exactly the entry's bytes (no limit).
                    if let Err(err) = pump_chunks(
                        entry_reader,
                        None,
                        |chunk| tx.send_chunk(chunk),
                        |io| map_stream_err(io, had_password),
                    ) {
                        caught = Some(err);
                        return Ok(false);
                    }
                    // Early stop once the whole subtree is delivered.
                    Ok(!wanted.is_empty())
                }
                None => {
                    // Not wanted, but a solid block shares one stream, so it must be
                    // consumed to advance the decoder to the next member.
                    std::io::copy(entry_reader, &mut std::io::sink())?;
                    Ok(true)
                }
            }
        },
    );

    if let Some(err) = caught {
        return tx.send_err(err);
    }
    if aborted {
        return;
    }
    if let Err(err) = result {
        tx.send_err(map_sevenz_err_pw(err, had_password));
    }
}

/// Reclassifies a decode error when a password WAS supplied. 7z AES stores no
/// password-verification value (unlike zip AES), so a wrong password isn't caught
/// up front: it decrypts to garbage that first fails a downstream integrity check
/// (`ChecksumVerificationFailed`, or `MaybeBadPassword` where the crate detects
/// it). That's indistinguishable at the crypto layer from genuine corruption — but
/// when the user just supplied a password, it's overwhelmingly a wrong one, so
/// surface the typed [`ArchiveError::WrongPassword`] re-prompt rather than a
/// "damaged archive". With no password supplied, these stay corruption
/// ([`map_sevenz_err`]).
fn map_sevenz_err_pw(err: sevenz_rust2::Error, had_password: bool) -> ArchiveError {
    use sevenz_rust2::Error as E;
    if had_password && matches!(err, E::ChecksumVerificationFailed | E::MaybeBadPassword(_)) {
        return ArchiveError::WrongPassword;
    }
    map_sevenz_err(err)
}

/// The [`pump_chunks`] error classifier for the 7z stream. A decode read error
/// wraps `sevenz-rust2`'s typed error (`io::Error::other(Error::…)`); recover the
/// wrapped value and route it through [`map_sevenz_err_pw`], so a wrong-password
/// integrity failure mid-stream is typed `WrongPassword` instead of a generic
/// `Io`. Classifies by the recovered ENUM variant, never message text (within
/// `no-string-matching`); a non-sevenz io error keeps the plain io classification.
fn map_stream_err(err: std::io::Error, had_password: bool) -> ArchiveError {
    match err.downcast::<sevenz_rust2::Error>() {
        Ok(sevenz_err) => map_sevenz_err_pw(sevenz_err, had_password),
        Err(io_err) => ArchiveError::from(io_err),
    }
}

/// Maps a `sevenz-rust2` error to a typed [`ArchiveError`], classifying by enum
/// variant (never message text, per `no-string-matching`).
///
/// The load-bearing case is encryption (`aes256` ON). A password-protected 7z
/// surfaces two typed password signals:
///
/// - `PasswordRequired` ⇒ [`ArchiveError::Encrypted`] — no password supplied for
///   an encrypted archive (header-encrypted at `ArchiveReader::new`, content-
///   encrypted at decode). Maps to `VolumeError::NeedsPassword`, the prompt.
/// - `MaybeBadPassword` ⇒ [`ArchiveError::WrongPassword`] — a supplied password
///   decrypted to bytes that failed their integrity check. Maps to the "that
///   password didn't work" re-prompt.
///
/// A still-unsupported coder (`aes256` genuinely absent, or an unknown codec)
/// stays [`ArchiveError::Unsupported`] (→ `VolumeError::NotSupported`), never
/// `Corrupt` (which reads as DAMAGED). A wrong password on a HEADER-encrypted
/// archive can instead corrupt the decrypted header bytes and surface as a
/// checksum/CRC error; those stay `Corrupt` below — the frontend still lets the
/// user retry from the parent, just without the tailored "wrong password" copy.
/// (Verified on sevenz-rust2 0.21.2, `aes256` on, against real `7z`-produced
/// `-mhe=on`/`-mhe=off` fixtures — see the sevenz integration tests.)
fn map_sevenz_err(err: sevenz_rust2::Error) -> ArchiveError {
    use sevenz_rust2::Error as E;
    match err {
        // No password for an encrypted archive: the typed "needs a password" prompt.
        E::PasswordRequired => ArchiveError::Encrypted,
        // A supplied password that decrypted to bytes failing their integrity check.
        E::MaybeBadPassword(_) => ArchiveError::WrongPassword,
        // An unknown/unsupported coder: an honest "can't serve this kind", never "damaged".
        E::UnsupportedCompressionMethod(method) => ArchiveError::Unsupported(format!("7z coder: {method}")),
        E::Unsupported(msg) => ArchiveError::Unsupported(format!("7z: {msg}")),
        E::ExternalUnsupported => ArchiveError::Unsupported("7z uses an unsupported external coder".to_string()),
        E::UnsupportedVersion { major, minor } => {
            ArchiveError::Unsupported(format!("7z format version {major}.{minor}"))
        }
        // A memory-limit refusal is a resource cap, not damage: reuse the tree-size
        // rejection so it never reads as "damaged".
        E::MaxMemLimited { max_kb, actaul_kb } => {
            ArchiveError::TooLarge(format!("7z needs {actaul_kb} KB to decode, over the {max_kb} KB limit"))
        }
        // Underlying byte-source I/O: reuse the io-kind classifier (UnexpectedEof ⇒ Corrupt).
        E::Io(io, _) | E::FileOpen(io, _) => ArchiveError::from(io),
        // Everything else is a structurally broken or truncated archive. `err` is
        // still owned here (every sub-pattern binds nothing), so format it whole.
        E::BadSignature(_)
        | E::ChecksumVerificationFailed
        | E::NextHeaderCrcMismatch
        | E::BadTerminatedStreamsInfo(_)
        | E::BadTerminatedUnpackInfo
        | E::BadTerminatedPackInfo(_)
        | E::BadTerminatedSubStreamsInfo
        | E::BadTerminatedHeader(_)
        | E::Other(_)
        | E::FileNotFound => ArchiveError::Corrupt(format!("7z: {err}")),
    }
}

#[cfg(test)]
#[path = "sevenz_test.rs"]
mod sevenz_test;
