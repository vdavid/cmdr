//! How [`InMemoryVolume`] lets a copy carry a file on after an outage: the
//! version its read streams name (`Volume::resume_read_stream`), and the write
//! that picks up a failed one's partial (`Volume::continue_write_from_stream`,
//! with [`InMemoryVolume::with_continuable_writes`]).

use std::future::Future;
use std::path::Path;
use std::pin::Pin;

use super::device::ContentSink;
use super::{InMemoryContent, InMemoryEntry, InMemoryVolume};
use crate::ignore_poison::RwLockIgnorePoison;
use crate::metadata::MetadataClasses;
use crate::volume::{StreamLength, StreamWriteProgress, StreamWritten, VolumeError, VolumeReadStream};

/// An in-memory file's version: its very bytes (`InMemoryContent::is_same_as`)
/// and its date. Any rewrite stores new bytes, so it never matches.
#[derive(Clone)]
pub(super) struct InMemoryVersion {
    pub(super) content: InMemoryContent,
    pub(super) modified_at: Option<u64>,
}

impl InMemoryVersion {
    pub(super) fn of(entry: &InMemoryEntry) -> Self {
        Self {
            content: entry
                .content
                .clone()
                .unwrap_or_else(|| InMemoryContent::bytes(Vec::new())),
            modified_at: entry.metadata.modified_at,
        }
    }

    pub(super) fn matches(&self, entry: &InMemoryEntry) -> bool {
        entry
            .content
            .as_ref()
            .is_some_and(|content| content.is_same_as(&self.content))
            && entry.metadata.modified_at == self.modified_at
    }
}

impl InMemoryVolume {
    /// Makes this volume carry a failed write on (`Volume::continue_write_from_stream`):
    /// a write that fails mid-stream leaves the bytes it got at its name, as a disk
    /// does, and a continuation picks up from any byte count it heard. What lets an
    /// engine test copy onto this double through a long outage and keep its bytes.
    pub fn with_continuable_writes(mut self) -> Self {
        self.continues_writes = true;
        self
    }

    /// The body of `continue_write_from_stream`: the partial at `target`, cut to
    /// `at`, then the stream after it, landed as `write_from_stream` lands. Refuses
    /// a target that's gone or short, and a volume that doesn't continue writes.
    pub(super) fn continue_write_impl<'a>(
        &'a self,
        target: &'a Path,
        at: u64,
        length: StreamLength,
        mut stream: Box<dyn VolumeReadStream>,
        on_progress: &'a (dyn Fn(StreamWriteProgress) -> std::ops::ControlFlow<()> + Sync),
    ) -> Pin<Box<dyn Future<Output = Result<StreamWritten, VolumeError>> + Send + 'a>> {
        let (Some(size), true) = (length.known(), self.continues_writes) else {
            return Box::pin(async { Err(VolumeError::NotSupported) });
        };
        Box::pin(async move {
            let prefix = self
                .entries
                .read_ignore_poison()
                .get(&self.normalize(target))
                .and_then(|entry| entry.content.clone())
                .filter(|content| content.len() >= at)
                .ok_or(VolumeError::NotSupported)?;
            let mut carried = stream.metadata(self.metadata_caps.stores);
            carried.flags = carried.flags.intersection(self.metadata_caps.staged_flags);
            let kept = carried
                .clone()
                .restricted_to(MetadataClasses::ALL.without(self.write_refuses));
            let mut data = ContentSink::default();
            data.push(&prefix.read_at(0, at as usize));
            let mut bytes_written = at;
            while let Some(result) = stream.next_chunk().await {
                let chunk = match result {
                    Ok(chunk) => chunk,
                    Err(e) => {
                        let _ = self.replace_whole(target, data.finish());
                        return Err(e);
                    }
                };
                bytes_written += chunk.len() as u64;
                data.push(&chunk);
                self.breathe().await;
                if on_progress(StreamWriteProgress {
                    bytes_written,
                    expected_length: StreamLength::Known(size),
                }) == std::ops::ControlFlow::Break(())
                {
                    return Err(VolumeError::Cancelled("Operation cancelled".into()));
                }
            }
            self.replace_whole(target, data.finish())?;
            if let Some(secs) = stream
                .modified_at()
                .and_then(|date| date.duration_since(std::time::UNIX_EPOCH).ok())
            {
                self.set_modified_at(target, Some(secs.as_secs()));
            }
            if !kept.is_empty() {
                self.set_carried_metadata(target, kept);
            }
            Ok(self.written(target, bytes_written, &carried, stream.modified_at()))
        })
    }
}
