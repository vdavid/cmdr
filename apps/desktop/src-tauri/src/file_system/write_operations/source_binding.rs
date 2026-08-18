//! What a caller was promised each top-level source is, and the pre-flight that
//! holds the filesystem to it.
//!
//! A reviewed batch — Ask Cmdr's rename plan, an approved suggestion group — is
//! decided against files as they looked at review time and executed later. Often
//! much later: an approved operation can sit queued behind a forty-minute copy on
//! the same lane, and a suggestion can wait weeks for someone to click Approve. In
//! between, the file can be edited, replaced, or swapped for a different file under
//! the same name. A fingerprint is what lets the operation notice, and dropping the
//! source that changed is the only answer that stays inside what the person
//! actually approved.
//!
//! **Nothing here asks who started the operation.** A source with no expectation
//! bound to it is not checked at all, so every user-started copy, move, delete, and
//! trash runs exactly as it always did. The binding is an input a caller may supply,
//! ❌ never a policy the engine applies to some callers and not others.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::event_sinks::OperationEventSink;
use super::types::{SourceItemOutcome, WriteCompleteEvent, WriteOperationType, WriteSourceItemDoneEvent};
use crate::file_system::volume::Volume;

/// Server-owned identity of one source, captured when a caller last looked at it.
///
/// The frontend never creates this data: it holds opaque row / op ids, and the
/// backend maps them to the fingerprint it recorded itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SourceFingerprint {
    /// A source on the local filesystem, where `(device, inode)` is a real
    /// identity the kernel maintains.
    Local {
        device: u64,
        inode: u64,
        content: LocalContent,
    },
    /// A source on a Volume backend (SMB, MTP, an archive). No inode exists, so
    /// the normalized path stands in for identity and the content carries the
    /// rest.
    Remote {
        normalized_path: String,
        content: RemoteContent,
    },
}

/// What a local source's bytes looked like.
///
/// A directory answers [`LocalContent::Directory`] and nothing more. Its own size
/// and mtime move with every child write, so they describe the folder's traffic
/// rather than the folder the person picked: holding a proposed
/// `delete ~/projects/cmdr/target/` to yesterday's directory mtime would refuse it
/// after any build. `(device, inode)` plus "still a directory" is the whole
/// identity worth binding, and a directory that became a file (or the reverse)
/// still mismatches, because the two variants differ.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LocalContent {
    File { size: u64, modified_nanos: Option<u128> },
    Directory,
}

/// What a remote source looked like. `Directory` for the same reason as
/// [`LocalContent::Directory`]; `size` / `modified` are `Option` because MTP and
/// some SMB servers report neither.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RemoteContent {
    File { size: Option<u64>, modified: Option<i64> },
    Directory,
}

/// Nanoseconds in one second — the divisor that brings a local fingerprint into
/// the journal's unit.
const NANOS_PER_SECOND: u128 = 1_000_000_000;

impl SourceFingerprint {
    /// Reads a local source's identity now. `None` when the path can't be
    /// stat'd at all, which every caller treats as a mismatch: an unreadable
    /// source is not the source anybody reviewed.
    ///
    /// Symlinks are never followed (`symlink_metadata`), matching every other
    /// walker in this module: the link IS the source item.
    pub(crate) fn capture_local(path: &Path) -> Option<Self> {
        let metadata = std::fs::symlink_metadata(path).ok()?;
        let content = if metadata.file_type().is_dir() {
            LocalContent::Directory
        } else {
            LocalContent::File {
                size: metadata.len(),
                modified_nanos: metadata
                    .modified()
                    .ok()
                    .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|time| time.as_nanos()),
            }
        };
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            Some(Self::Local {
                device: metadata.dev(),
                inode: metadata.ino(),
                content,
            })
        }
        #[cfg(not(unix))]
        {
            Some(Self::Local {
                device: 0,
                inode: 0,
                content,
            })
        }
    }

    /// Reads a remote source's identity now, through the volume that owns it.
    /// `None` when the backend can't stat it.
    pub(crate) async fn capture_remote(volume: &dyn Volume, path: &Path) -> Option<Self> {
        let metadata = volume.get_metadata(path).await.ok()?;
        let content = if metadata.is_directory {
            RemoteContent::Directory
        } else {
            RemoteContent::File {
                size: metadata.size,
                modified: metadata.modified_at.map(|value| value as i64),
            }
        };
        Some(Self::Remote {
            normalized_path: normalized_path(path),
            content,
        })
    }

    /// The `(size, mtime)` identity snapshot the journal records for one applied
    /// item, in the journal's units: bytes, and Unix **seconds** for mtime.
    ///
    /// **The unit is the whole risk here.** Undo rechecks these two numbers against
    /// the live entry (`operation_log::rollback::verify_snapshot` vs
    /// `FileEntry::modified_at`), and every backend reports `modified_at` in whole
    /// Unix seconds — the local reader via `Duration::as_secs`, SMB pinned by
    /// `smb_integration_modified_at_is_unix_seconds`. Journal nanoseconds and every
    /// undo reports drift and refuses, which silently disables undo; journal nothing
    /// and identity rests on size alone, so a same-size replacement file gets renamed
    /// back in place of the original.
    ///
    /// A rename never touches the file's mtime (POSIX `rename` changes the parent
    /// directory's, not the inode's; SMB's `FILE_RENAME_INFORMATION` likewise), so
    /// the pre-rename fingerprint stays the destination's true identity.
    ///
    /// `None` for mtime when the backend reported none (MTP, some SMB servers): the
    /// recheck then falls back to size alone rather than inventing a value that
    /// would read as a match. A directory answers `(None, None)` — it has no bytes
    /// of its own to be held to.
    pub(crate) fn journal_snapshot(&self) -> (Option<i64>, Option<i64>) {
        match self {
            Self::Local {
                content: LocalContent::File { size, modified_nanos },
                ..
            } => (
                Some(*size as i64),
                // Floor-divide, matching the truncation `Duration::as_secs` applies on
                // the read side, so both readings of one file land on the same second.
                // `try_from` rather than `as`: a saturating or wrapping cast would
                // journal a timestamp that isn't the file's, and undo would then refuse
                // forever. An unrepresentable value (past year 292,277,026,596) degrades
                // to the size-only check instead of panicking.
                modified_nanos.and_then(|nanos| i64::try_from(nanos / NANOS_PER_SECOND).ok()),
            ),
            // A remote fingerprint already holds `FileEntry::modified_at`, in seconds.
            Self::Remote {
                content: RemoteContent::File { size, modified },
                ..
            } => (size.map(|size| size as i64), *modified),
            Self::Local {
                content: LocalContent::Directory,
                ..
            }
            | Self::Remote {
                content: RemoteContent::Directory,
                ..
            } => (None, None),
        }
    }
}

/// The comparison key a remote fingerprint is stored under, so two spellings of
/// one path can't read as two different sources.
pub(crate) fn normalized_path(path: &Path) -> String {
    cmdr_index::store::normalize_for_comparison(&path.to_string_lossy())
}

/// The top-level sources one operation is allowed to touch, and what each was
/// when its caller last looked.
///
/// **Binding is all-or-nothing.** A source this doesn't name is treated as
/// unverifiable and dropped, not waved through: a caller that supplies a partial
/// map has a bug, and the failure mode of guessing "probably fine" is acting on a
/// file nobody reviewed. A caller that wants no checking at all passes no
/// `ExpectedSources` rather than an empty one.
#[derive(Debug, Clone)]
pub(crate) struct ExpectedSources {
    by_path: HashMap<PathBuf, SourceFingerprint>,
}

impl ExpectedSources {
    // The pre-flight this feeds is wired into all four starters; what production
    // still lacks is the caller that BUILDS a binding, which is the suggestion
    // approval path (plan M4). `expect` rather than `allow` so this marker has to
    // be deleted the moment that caller lands.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "built by the suggestion-approval path, which lands in M4")
    )]
    pub(crate) fn new(entries: impl IntoIterator<Item = (PathBuf, SourceFingerprint)>) -> Self {
        Self {
            by_path: entries.into_iter().collect(),
        }
    }

    /// Whether `path`'s live local identity is the one this binding holds.
    fn matches_local(&self, path: &Path) -> bool {
        self.by_path
            .get(path)
            .is_some_and(|expected| SourceFingerprint::capture_local(path).as_ref() == Some(expected))
    }

    /// Whether `path`'s live identity on `volume` is the one this binding holds.
    async fn matches_remote(&self, volume: &dyn Volume, path: &Path) -> bool {
        let Some(expected) = self.by_path.get(path) else {
            return false;
        };
        SourceFingerprint::capture_remote(volume, path).await.as_ref() == Some(expected)
    }

    /// The positions in `sources` that still hold what the caller was promised, in
    /// the caller's own order. Every drop is announced on the sink as a `Skipped`
    /// source item, so a consumer tracking per-source outcomes learns about it on
    /// the one channel every other outcome arrives on.
    ///
    /// Positions rather than paths because a caller may hold a second list indexed
    /// against the same order (trash's `item_sizes`), and re-deriving the pairing
    /// from paths is how the two drift.
    pub(crate) fn kept_positions_local(
        &self,
        events: &dyn OperationEventSink,
        operation_id: &str,
        sources: &[PathBuf],
    ) -> Vec<usize> {
        let mut kept = Vec::with_capacity(sources.len());
        for (position, source) in sources.iter().enumerate() {
            if self.matches_local(source) {
                kept.push(position);
            } else {
                announce_skip(events, operation_id, source, local_source_is_gone(source));
            }
        }
        kept
    }

    /// The remote counterpart of [`Self::kept_positions_local`].
    pub(crate) async fn kept_positions_remote(
        &self,
        volume: &dyn Volume,
        events: &dyn OperationEventSink,
        operation_id: &str,
        sources: &[PathBuf],
    ) -> Vec<usize> {
        let mut kept = Vec::with_capacity(sources.len());
        for (position, source) in sources.iter().enumerate() {
            if self.matches_remote(volume, source).await {
                kept.push(position);
            } else {
                // One extra round trip, and only for a source that already
                // mismatched: the difference between "changed" and "gone" is what
                // decides whether every stale search snapshot may drop its row.
                let gone = !volume.exists(source).await;
                announce_skip(events, operation_id, source, gone);
            }
        }
        kept
    }
}

/// The sources a local operation may still touch, or `None` when the binding left
/// it nothing to do — in which case the terminal event has already gone out and
/// the caller returns `Ok(())` without touching the filesystem.
///
/// An operation with no binding gets its sources back untouched, which is the
/// whole reason this can sit at the top of every verb: it is a no-op for every
/// user-started operation.
pub(crate) fn retain_bound_sources(
    events: &dyn OperationEventSink,
    operation_id: &str,
    operation_type: WriteOperationType,
    expected: Option<&ExpectedSources>,
    sources: Vec<PathBuf>,
) -> Option<Vec<PathBuf>> {
    let Some(expected) = expected else {
        return Some(sources);
    };
    let kept = expected.kept_positions_local(events, operation_id, &sources);
    finish_or_keep(events, operation_id, operation_type, sources, kept)
}

/// [`retain_bound_sources`] for a verb whose sources live on a Volume backend
/// rather than the local filesystem.
pub(crate) async fn retain_bound_sources_remote(
    volume: &dyn Volume,
    events: &dyn OperationEventSink,
    operation_id: &str,
    operation_type: WriteOperationType,
    expected: Option<&ExpectedSources>,
    sources: Vec<PathBuf>,
) -> Option<Vec<PathBuf>> {
    let Some(expected) = expected else {
        return Some(sources);
    };
    let kept = expected.kept_positions_remote(volume, events, operation_id, &sources).await;
    finish_or_keep(events, operation_id, operation_type, sources, kept)
}

/// [`retain_bound_sources`] for a verb that carries a second list indexed against
/// the same source order. Both halves are filtered together, so a dropped source
/// takes its size with it rather than shifting every later reading onto the wrong
/// item.
pub(crate) fn retain_bound_sources_with_sizes(
    events: &dyn OperationEventSink,
    operation_id: &str,
    operation_type: WriteOperationType,
    expected: Option<&ExpectedSources>,
    sources: Vec<PathBuf>,
    item_sizes: Option<Vec<u64>>,
) -> Option<(Vec<PathBuf>, Option<Vec<u64>>)> {
    let Some(expected) = expected else {
        return Some((sources, item_sizes));
    };
    let kept = expected.kept_positions_local(events, operation_id, &sources);
    let item_sizes = item_sizes.map(|sizes| kept.iter().filter_map(|position| sizes.get(*position).copied()).collect());
    let sources = finish_or_keep(events, operation_id, operation_type, sources, kept)?;
    Some((sources, item_sizes))
}

fn finish_or_keep(
    events: &dyn OperationEventSink,
    operation_id: &str,
    operation_type: WriteOperationType,
    sources: Vec<PathBuf>,
    kept: Vec<usize>,
) -> Option<Vec<PathBuf>> {
    if kept.is_empty() {
        announce_empty_batch(events, operation_id, operation_type, sources.len());
        return None;
    }
    Some(kept.into_iter().filter_map(|position| sources.get(position).cloned()).collect())
}

/// Whether a local source is positively absent, as opposed to merely unreadable.
/// A permission error leaves `source_removed: false`, because "we couldn't look"
/// is not evidence the file is gone and the flag drives snapshot purging.
fn local_source_is_gone(source: &Path) -> bool {
    matches!(
        std::fs::symlink_metadata(source),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound
    )
}

fn announce_skip(events: &dyn OperationEventSink, operation_id: &str, source: &Path, source_removed: bool) {
    log::info!(
        target: "source_binding",
        "skipping {}: it is no longer the file this operation was approved against",
        source.display()
    );
    events.emit_source_item_done(WriteSourceItemDoneEvent {
        operation_id: operation_id.to_string(),
        source_path: source.display().to_string(),
        source_removed,
        outcome: SourceItemOutcome::Skipped,
    });
}

/// The terminal event for an operation whose binding left it nothing to do.
///
/// ❌ Not a `write-error`. Every source was already announced as a `Skipped`
/// item, so the caller has its per-source answers; a failure dialog on top of them
/// would be the engine editorializing about a decision the person is entitled to
/// make differently. Nothing was written, so `bytes_processed` is zero and
/// `files_skipped` is the whole batch.
pub(crate) fn announce_empty_batch(
    events: &dyn OperationEventSink,
    operation_id: &str,
    operation_type: WriteOperationType,
    skipped: usize,
) {
    log::info!(
        target: "source_binding",
        "operation {operation_id} has nothing left to do: all {skipped} source(s) changed since they were approved"
    );
    events.emit_complete(WriteCompleteEvent {
        operation_id: operation_id.to_string(),
        operation_type,
        files_processed: 0,
        files_skipped: skipped,
        bytes_processed: 0,
    });
}

#[cfg(test)]
mod tests;
