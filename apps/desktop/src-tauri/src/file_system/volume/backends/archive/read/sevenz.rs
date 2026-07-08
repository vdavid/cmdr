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
//! Encryption is out of scope: the `aes256` feature is off (it would pull an
//! `aes` version conflicting with `smb2`'s pinned pre-release; see the archive
//! `DETAILS.md`), so an AES-encrypted 7z refuses honestly as `Unsupported` (a
//! "can't open this kind" the user sees, never a "damaged archive" or a password
//! prompt) rather than being decrypted. The classification lives in
//! [`map_sevenz_err`]; matching the deferral of WinZip AES zip.

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
    pub(super) fn open_read(
        &self,
        inner: &str,
        total: u64,
        source: Arc<dyn ArchiveByteSource>,
    ) -> Result<ArchiveEntryReader, ArchiveError> {
        if !self.members.contains_key(inner) {
            return Err(ArchiveError::NotFound(inner.to_string()));
        }
        let target = inner.to_string();
        Ok(ArchiveEntryReader::spawn_with(total, move |tx| {
            stream_entry(source, &target, &tx);
        }))
    }
}

/// Parses the 7z header into the format-neutral entry list. Metadata only — no
/// block is decompressed here.
pub(super) fn parse(source: Arc<dyn ArchiveByteSource>) -> Result<(Vec<(RawEntry, ())>, ()), ArchiveError> {
    let reader = SourceReader::new(source);
    let archive = ArchiveReader::new(reader, Password::empty()).map_err(map_sevenz_err)?;
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
/// index keyed it under) and streams its bytes.
fn stream_entry(source: Arc<dyn ArchiveByteSource>, target: &str, tx: &super::reader::ChunkTx) {
    let reader = SourceReader::new(source);
    let mut archive = match ArchiveReader::new(reader, Password::empty()) {
        Ok(a) => a,
        Err(err) => return tx.send_err(map_sevenz_err(err)),
    };

    let mut streamed = false;
    let result = archive.for_each_entries(
        &mut |entry: &sevenz_rust2::ArchiveEntry, entry_reader: &mut dyn std::io::Read| {
            if entry_matches(entry.name(), target) {
                pump_read(entry_reader, tx, None);
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
        Err(err) => tx.send_err(map_sevenz_err(err)),
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
pub(super) fn stream_subtree(source: Arc<dyn ArchiveByteSource>, mut wanted: HashMap<String, u64>, tx: &SubtreeTx) {
    if wanted.is_empty() {
        return;
    }
    let reader = SourceReader::new(source);
    let mut archive = match ArchiveReader::new(reader, Password::empty()) {
        Ok(a) => a,
        Err(err) => return tx.send_err(map_sevenz_err(err)),
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
                    if let Err(err) = pump_chunks(entry_reader, None, |chunk| tx.send_chunk(chunk)) {
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
        tx.send_err(map_sevenz_err(err));
    }
}

/// Maps a `sevenz-rust2` error to a typed [`ArchiveError`], classifying by enum
/// variant (never message text, per `no-string-matching`).
///
/// The load-bearing case is encryption. We build `sevenz-rust2` with the
/// `aes256` feature OFF (its `aes` crate conflicts with `smb2`'s pin; see the
/// archive `DETAILS.md`), so an AES-encrypted 7z has an unrecognized coder and
/// the crate returns `UnsupportedCompressionMethod("AES256_SHA256")` — for a
/// header-encrypted archive at open (`ArchiveReader::new`), for a data-encrypted
/// one at decode (`for_each_entries`). Both must land on [`ArchiveError::Unsupported`]
/// (→ `VolumeError::NotSupported`, the honest "can't open this kind"), NOT
/// [`ArchiveError::Corrupt`] (which reads to the user as a DAMAGED archive) and NOT
/// [`ArchiveError::Encrypted`] (which prompts for a password a 7z read can never
/// satisfy here). A genuinely-unknown codec produces the same variant and is
/// honestly "unsupported" too, so no AES-specific coder-id check is needed.
/// (Verified on sevenz-rust2 0.21.2, `aes256` off, against real `7z`-produced
/// `-mhe=on`/`-mhe=off` fixtures, 2026-07-08.)
///
/// The `PasswordRequired` / `MaybeBadPassword` variants only arise with `aes256`
/// ON; they're mapped to `Unsupported` too so encryption never reads as damaged
/// if the feature is ever enabled without a matching password-threading path.
fn map_sevenz_err(err: sevenz_rust2::Error) -> ArchiveError {
    use sevenz_rust2::Error as E;
    match err {
        // Encryption (our `aes256`-off build) and any unknown/unsupported coder:
        // an honest "can't serve this kind", never "damaged".
        E::UnsupportedCompressionMethod(method) => ArchiveError::Unsupported(format!("7z coder: {method}")),
        E::Unsupported(msg) => ArchiveError::Unsupported(format!("7z: {msg}")),
        E::ExternalUnsupported => ArchiveError::Unsupported("7z uses an unsupported external coder".to_string()),
        E::UnsupportedVersion { major, minor } => ArchiveError::Unsupported(format!("7z format version {major}.{minor}")),
        E::PasswordRequired | E::MaybeBadPassword(_) => ArchiveError::Unsupported("7z is encrypted".to_string()),
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
