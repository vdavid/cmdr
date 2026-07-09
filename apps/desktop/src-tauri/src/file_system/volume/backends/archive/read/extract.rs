//! One-pass subtree extraction for SEQUENTIAL archives (compressed tar, 7z).
//!
//! A per-entry read of a sequential archive re-decodes the whole prefix in front
//! of each entry ([`super::format::ArchiveFormat::is_sequential`]), so extracting
//! a subtree file-by-file is O(n²). This module decodes the stream ONCE: a
//! `spawn_blocking` producer drives a single decoder over the archive and, for
//! each wanted FILE member (in ARCHIVE order), emits a header frame followed by
//! its data chunks through one bounded channel. The consumer
//! ([`SubtreeExtractReader`]) pulls members one at a time and streams each
//! member's bytes to the destination's safe-write path.
//!
//! **Files only.** The producer yields file (and symlink) members, never
//! directories: the directory structure comes from the parsed tree (cheap, no
//! decode), so the copy engine creates the destination folders from the index and
//! reserves this one-pass decode for the byte-carrying entries. That also sidesteps
//! synthetic directories (implied by a file prefix, with no archive entry) and
//! empty explicit directories, which the tree has but the byte stream doesn't.
//!
//! **Cancellation** is drop-based, like [`super::reader::ArchiveEntryReader`]:
//! dropping the reader drops the channel receiver, so the producer's next `send`
//! fails and it stops decoding.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::mpsc;
use tokio::task::spawn_blocking;

use super::error::ArchiveError;
use super::index::EntryStore;
use super::source::ArchiveByteSource;

/// Channel depth for the member/chunk frame stream. Peak in-flight decoded
/// memory is `CHANNEL_CAPACITY × CHUNK_SIZE`, independent of the subtree's size.
const CHANNEL_CAPACITY: usize = 4;

/// One file member the one-pass extractor yields: its sanitized inner path and
/// uncompressed size (from the parsed tree, so it matches the copy scan totals).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubtreeMember {
    /// Sanitized inner path (the index key), `/`-separated, no surrounding slashes.
    pub inner_path: String,
    /// Uncompressed size in bytes.
    pub size: u64,
}

/// The frames the producer sends over the single channel: a member header, then
/// that member's data chunks, then the next member header, and so on.
pub(super) enum SubtreeFrame {
    Member(SubtreeMember),
    Chunk(Vec<u8>),
}

/// The producer's send side. Blocking sends (used only from the `spawn_blocking`
/// producer thread); each returns `false` once the consumer has dropped the
/// reader, so the producer stops decoding (the cancel path).
pub(super) struct SubtreeTx(mpsc::Sender<Result<SubtreeFrame, ArchiveError>>);

impl SubtreeTx {
    /// Announces the next file member. Returns `false` if the consumer is gone.
    pub(super) fn send_member(&self, member: SubtreeMember) -> bool {
        self.0.blocking_send(Ok(SubtreeFrame::Member(member))).is_ok()
    }

    /// Sends one decoded data chunk of the current member. Returns `false` if the
    /// consumer is gone (matches [`pump_chunks`](super::reader::pump_chunks)'s
    /// `emit` contract).
    pub(super) fn send_chunk(&self, chunk: Vec<u8>) -> bool {
        self.0.blocking_send(Ok(SubtreeFrame::Chunk(chunk))).is_ok()
    }

    /// Reports a decode / byte-source failure to the consumer (best-effort).
    pub(super) fn send_err(&self, err: ArchiveError) {
        let _ = self.0.blocking_send(Err(err));
    }
}

/// The consumer side of the one-pass extractor: pull members with
/// [`next_member`](Self::next_member), then drain each member's bytes with
/// [`next_chunk`](Self::next_chunk) before advancing.
pub struct SubtreeExtractReader {
    rx: mpsc::Receiver<Result<SubtreeFrame, ArchiveError>>,
    /// A member header read past the current member's last chunk, buffered until
    /// the next [`next_member`](Self::next_member).
    pending: Option<SubtreeMember>,
    /// Whether the current member still has (possibly unread) chunks in flight.
    current_open: bool,
    /// Whether the producer has finished (channel closed or errored).
    done: bool,
}

impl SubtreeExtractReader {
    /// Spawns the one-pass producer for `store`'s format over `source`, emitting
    /// every file in `wanted` (inner path → uncompressed size) in archive order.
    ///
    /// Only sequential formats have a producer; a random-access store (zip, plain
    /// tar) yields nothing — the copy planner never routes those here (it gates on
    /// [`Volume::extraction_is_sequential`](crate::file_system::volume::Volume::extraction_is_sequential)),
    /// so an empty stream is the correct defensive fallback.
    pub(super) fn spawn(
        store: &EntryStore,
        source: Arc<dyn ArchiveByteSource>,
        wanted: HashMap<String, u64>,
        password: Option<&str>,
    ) -> Self {
        let (tx, rx) = mpsc::channel(CHANNEL_CAPACITY);
        let sink = SubtreeTx(tx);
        match store {
            EntryStore::Tar(tar_store) => {
                let codec = tar_store.codec();
                spawn_blocking(move || super::tar::stream_subtree(source, codec, wanted, &sink));
            }
            EntryStore::SevenZ(_) => {
                let password = password.map(str::to_owned);
                spawn_blocking(move || super::sevenz::stream_subtree(source, wanted, password.as_deref(), &sink));
            }
            // Random-access: no one-pass producer. Dropping `sink` closes the
            // channel, so the reader yields no members.
            EntryStore::Zip(_) => drop(sink),
        }
        Self {
            rx,
            pending: None,
            current_open: false,
            done: false,
        }
    }

    /// Advances to the next file member (draining any unread chunks of the current
    /// one first), or `Ok(None)` at the end of the subtree.
    pub async fn next_member(&mut self) -> Result<Option<SubtreeMember>, ArchiveError> {
        if self.current_open {
            // Skip the rest of the current member's chunks so the stream lines up
            // on the next header (the caller chose not to read this member fully).
            loop {
                match self.recv().await? {
                    Some(SubtreeFrame::Chunk(_)) => {}
                    Some(SubtreeFrame::Member(next)) => {
                        self.pending = Some(next);
                        break;
                    }
                    None => break,
                }
            }
            self.current_open = false;
        }
        if let Some(member) = self.pending.take() {
            self.current_open = true;
            return Ok(Some(member));
        }
        if self.done {
            return Ok(None);
        }
        match self.recv().await? {
            Some(SubtreeFrame::Member(member)) => {
                self.current_open = true;
                Ok(Some(member))
            }
            // The producer always sends a member header before its chunks, so a
            // chunk with no open member is a protocol violation, not a normal end.
            Some(SubtreeFrame::Chunk(_)) => Err(ArchiveError::Corrupt(
                "subtree extract: data chunk before any member header".to_string(),
            )),
            None => Ok(None),
        }
    }

    /// The next decoded chunk of the CURRENT member, or `Ok(None)` at the member's
    /// end. Call [`next_member`](Self::next_member) to advance after `None`.
    pub async fn next_chunk(&mut self) -> Result<Option<Vec<u8>>, ArchiveError> {
        if !self.current_open {
            return Ok(None);
        }
        match self.recv().await? {
            Some(SubtreeFrame::Chunk(chunk)) => Ok(Some(chunk)),
            Some(SubtreeFrame::Member(next)) => {
                // The next header means the current member's data is exhausted.
                self.pending = Some(next);
                self.current_open = false;
                Ok(None)
            }
            None => {
                self.current_open = false;
                self.done = true;
                Ok(None)
            }
        }
    }

    /// Receives one frame, mapping a producer error to `Err` (and marking the
    /// stream done so no further frame is awaited).
    async fn recv(&mut self) -> Result<Option<SubtreeFrame>, ArchiveError> {
        match self.rx.recv().await {
            Some(Ok(frame)) => Ok(Some(frame)),
            Some(Err(err)) => {
                self.done = true;
                Err(err)
            }
            None => Ok(None),
        }
    }
}
