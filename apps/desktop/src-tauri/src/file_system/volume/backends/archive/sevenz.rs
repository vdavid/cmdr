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
//! Encryption is out of scope: the `aes256` feature is off, so an AES-encrypted
//! 7z surfaces a typed error (header-encrypted archives fail to parse; data-only
//! encryption fails at extract) rather than being decrypted.

use std::collections::HashMap;
use std::sync::Arc;

use sevenz_rust2::{ArchiveReader, Password};

use super::error::ArchiveError;
use super::index::{MAX_TREE_NODES, RawEntry};
use super::name::{SanitizedName, sanitize_entry_name};
use super::read::{ArchiveEntryReader, pump_read};
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
fn stream_entry(source: Arc<dyn ArchiveByteSource>, target: &str, tx: &super::read::ChunkTx) {
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

/// Maps a `sevenz-rust2` error to a typed [`ArchiveError`]. Encryption without
/// the `aes256` feature, and any unknown codec, surface as `Unsupported`; a
/// malformed header as `Corrupt`.
fn map_sevenz_err(err: sevenz_rust2::Error) -> ArchiveError {
    ArchiveError::Corrupt(format!("7z: {err}"))
}
