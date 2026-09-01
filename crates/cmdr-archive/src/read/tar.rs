//! The tar format (plain and compressed): one sequential scan to index, then
//! streaming per-entry extract.
//!
//! Tar has NO central directory, so the index is built by ONE forward scan of
//! the (decompressed) stream, reading each 512-byte header and skipping the data
//! between them. That scan reuses the shared Zip Slip sanitizer and synthetic
//! tree ([`super::index`]) — the safety story is format-independent.
//!
//! Access class ([`super::format::ArchiveFormat::is_sequential`]):
//!
//! - A **plain `.tar`** is random-access: each member's data lives at a known
//!   byte offset, so [`open_read`](TarStore::open_read) seeks and streams the
//!   member's exact bytes. No decompression, O(1) open.
//! - A **compressed tar** (`.tar.gz`/`.xz`/`.bz2`/`.zst`) wraps the whole tar in
//!   one sequential codec stream with no random access. A single-entry extract
//!   prefix-decodes from the start to the target member (honest O(prefix) cost);
//!   a whole-subtree extract uses [`stream_subtree`]
//!   for ONE pass (the O(n²) trap — see [`super::index`]).

use std::collections::HashMap;
use std::io::Read;
use std::ops::ControlFlow;
use std::sync::Arc;

use super::error::ArchiveError;
use super::extract::{SubtreeMember, SubtreeTx};
use super::format::{TarCodec, open_tar_decoder};
use super::index::{MAX_TREE_NODES, RawEntry};
use super::name::{SanitizedName, sanitize_entry_name};
use super::reader::{ArchiveEntryReader, ChunkTx, pump_chunks, pump_read};
use super::source::{ArchiveByteSource, SourceRangeReader, SourceReader};

/// One tar member's read handle: where its data lives.
pub(super) struct TarMember {
    /// Byte offset of the member's DATA. For a plain `.tar` this is a real file
    /// offset (used for the random-access read). For a compressed tar it's the
    /// offset in the DECOMPRESSED stream — unused there, since the reader
    /// prefix-decodes by matching the entry name rather than seeking.
    data_offset: u64,
}

/// The tar entry store: the outer codec plus each file's read handle.
pub(super) struct TarStore {
    codec: TarCodec,
    members: HashMap<String, TarMember>,
}

impl TarStore {
    pub(super) fn new(codec: TarCodec, members: HashMap<String, TarMember>) -> Self {
        Self { codec, members }
    }

    /// The outer codec, so the one-pass subtree extractor can reopen a single
    /// decoder over the whole stream (see [`stream_subtree`]).
    pub(super) fn codec(&self) -> TarCodec {
        self.codec
    }

    /// Opens a streaming reader for the file at `inner`. Plain tar reads the
    /// member's byte range directly; a compressed tar prefix-decodes to it.
    /// `total` is the tree's uncompressed size (equals the member size), passed in
    /// so the read stream reports a uniform total.
    pub(super) fn open_read(
        &self,
        inner: &str,
        total: u64,
        source: Arc<dyn ArchiveByteSource>,
    ) -> Result<ArchiveEntryReader, ArchiveError> {
        let member = self
            .members
            .get(inner)
            .ok_or_else(|| ArchiveError::NotFound(inner.to_string()))?;

        match self.codec {
            TarCodec::Plain => {
                let offset = member.data_offset;
                Ok(ArchiveEntryReader::spawn_with(total, move |tx| {
                    let range = SourceRangeReader::new(source, offset, total);
                    pump_read(range, &tx, Some(total), ArchiveError::from);
                }))
            }
            codec => {
                let target = inner.to_string();
                Ok(ArchiveEntryReader::spawn_with(total, move |tx| {
                    stream_compressed_member(source, codec, &target, total, &tx);
                }))
            }
        }
    }
}

/// Parses a tar (decompressing the outer codec on the fly) into the
/// format-neutral entry list. ONE forward scan: read each header, record the
/// member, skip its data. Bounded memory — data is never buffered, only headers.
pub(super) fn parse(
    source: Arc<dyn ArchiveByteSource>,
    codec: TarCodec,
) -> Result<Vec<(RawEntry, TarMember)>, ArchiveError> {
    let decoded = open_tar_decoder(codec, Box::new(SourceReader::new(source)))?;
    let mut archive = tar::Archive::new(decoded);
    let mut out: Vec<(RawEntry, TarMember)> = Vec::new();

    for entry in archive.entries()? {
        let entry = entry?;
        let header = entry.header();
        let entry_type = header.entry_type();

        // pax/GNU extension headers are metadata the `tar` crate folds into the
        // real entry; if one leaks through as its own entry type, skip it (it has
        // no browsable content).
        if entry_type.is_pax_global_extensions()
            || entry_type.is_pax_local_extensions()
            || entry_type.is_gnu_longname()
            || entry_type.is_gnu_longlink()
        {
            continue;
        }

        let name = String::from_utf8_lossy(&entry.path_bytes()).into_owned();
        let is_symlink = entry_type.is_symlink();
        // A directory is signalled by the type flag or a trailing slash. A
        // symlink is never a dir. Everything else (regular, hardlink, device,
        // fifo) is a browsable file — special files carry no data (size 0).
        let is_dir = !is_symlink && (entry_type.is_dir() || name.ends_with('/'));
        let size = if is_dir { 0 } else { header.size()? };
        let modified = header.mtime().ok().and_then(|m| i64::try_from(m).ok());

        out.push((
            RawEntry {
                name,
                is_dir,
                is_symlink,
                size,
                // Tar stores no compressed per-entry size; report the plain size.
                compressed_size: size,
                modified,
                // Tar has no per-entry encryption.
                encrypted: false,
            },
            TarMember {
                data_offset: entry.raw_file_position(),
            },
        ));

        // Bound the index the same way the tree builder bounds nodes: a tar with
        // a hostile number of members can't balloon memory before `build_tree`
        // catches it.
        if out.len() > MAX_TREE_NODES {
            return Err(ArchiveError::TooLarge(format!(
                "tar exceeds the {MAX_TREE_NODES}-entry limit"
            )));
        }
    }

    Ok(out)
}

/// Prefix-decodes a compressed tar to the member named `target` and streams its
/// bytes. O(prefix): everything before the member is decompressed and discarded.
fn stream_compressed_member(
    source: Arc<dyn ArchiveByteSource>,
    codec: TarCodec,
    target: &str,
    total: u64,
    tx: &ChunkTx,
) {
    let walked = for_each_member(source, codec, |entry| {
        if !entry_matches(entry, target) {
            return ControlFlow::Continue(());
        }
        pump_read(entry, tx, Some(total), ArchiveError::from);
        ControlFlow::Break(())
    });
    match walked {
        Ok(ControlFlow::Break(())) => {}
        Ok(ControlFlow::Continue(())) => tx.send_err(ArchiveError::NotFound(target.to_string())),
        Err(err) => tx.send_err(err),
    }
}

/// Opens the codec over `source` and hands every tar member to `visit`, in
/// stream order, until it breaks. `Ok(Break)` means `visit` stopped the walk,
/// `Ok(Continue)` that the stream ran out first; a decoder or header failure is
/// the `Err`, so both streaming readers report it the same way.
fn for_each_member(
    source: Arc<dyn ArchiveByteSource>,
    codec: TarCodec,
    mut visit: impl FnMut(&mut tar::Entry<'_, Box<dyn Read + Send + '_>>) -> ControlFlow<()>,
) -> Result<ControlFlow<()>, ArchiveError> {
    let decoded = open_tar_decoder(codec, Box::new(SourceReader::new(source)))?;
    let mut archive = tar::Archive::new(decoded);
    for entry in archive.entries()? {
        if visit(&mut entry?).is_break() {
            return Ok(ControlFlow::Break(()));
        }
    }
    Ok(ControlFlow::Continue(()))
}

/// Whether `entry`'s sanitized inner path equals `target` (the same sanitization
/// the index keyed the member under, so a compressed re-scan matches the stored
/// key exactly).
fn entry_matches(entry: &tar::Entry<'_, impl Read>, target: &str) -> bool {
    let name = String::from_utf8_lossy(&entry.path_bytes()).into_owned();
    matches!(sanitize_entry_name(&name), SanitizedName::Accepted(p) if p == target)
}

/// One-pass subtree extract: decode the tar stream ONCE and stream every file in
/// `wanted` (sanitized inner path → uncompressed size) in archive order. Members
/// not in `wanted` are skipped (the `tar` iterator advances past their data,
/// which for a compressed tar still flows through the one decoder — the honest
/// single-pass cost). Stops as soon as every wanted file has been delivered, so a
/// subtree near the front doesn't decode the whole tail.
pub(super) fn stream_subtree(
    source: Arc<dyn ArchiveByteSource>,
    codec: TarCodec,
    mut wanted: HashMap<String, u64>,
    tx: &SubtreeTx,
) {
    if wanted.is_empty() {
        return;
    }
    let walked = for_each_member(source, codec, |entry| {
        let sanitized = match sanitize_entry_name(&String::from_utf8_lossy(&entry.path_bytes())) {
            SanitizedName::Accepted(path) => path,
            // Quarantined/unnameable entries never reach the tree, so they're
            // never wanted; skip (the iterator advances past their data).
            _ => return ControlFlow::Continue(()),
        };
        // `remove` (not `get`) so a duplicate archive entry with the same name
        // isn't delivered twice; the tree kept the last, but the first-in-stream
        // occurrence is what the per-entry reader also matches.
        let Some(size) = wanted.remove(&sanitized) else {
            return ControlFlow::Continue(()); // not in the subtree: skip, still one forward pass
        };
        if !tx.send_member(SubtreeMember {
            inner_path: sanitized,
            size,
        }) {
            return ControlFlow::Break(()); // consumer gone: stop decoding
        }
        if let Err(err) = pump_chunks(entry, Some(size), |chunk| tx.send_chunk(chunk), ArchiveError::from) {
            tx.send_err(err);
            return ControlFlow::Break(());
        }
        if wanted.is_empty() {
            return ControlFlow::Break(()); // whole subtree delivered: don't decode the tail
        }
        ControlFlow::Continue(())
    });
    if let Err(err) = walked {
        tx.send_err(err);
    }
}
