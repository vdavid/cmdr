//! What a source stream told the destination about the file beyond its bytes,
//! kept so a stream carried on after an outage tells it the same.
//!
//! A destination asks a stream's `metadata`, `metadata_outline`, and
//! `modified_at` once, before the first chunk, and puts the answers on the file
//! after the last one. A copy carried on from a byte offset hands the
//! destination a NEW stream (`Volume::resume_read_stream`), and a backend
//! needn't learn the metadata again to resume, so [`remember`] records the
//! first stream's answers, and [`Remembered::answering`] gives them to the
//! resumed one. `transfer/DETAILS.md` § "Carrying a file on after a long
//! outage".

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::metadata::{CarriedMetadata, MetadataClasses};
use cmdr_fs::volume::{ScanStop, SourceVersion, StreamLength, VolumeError, VolumeReadStream};

/// The first stream's answers, by what each question asked.
// DEFAULT-OK: an empty record is "no answer remembered", and a resumed stream then asks its own.
#[derive(Clone, Default)]
pub(super) struct Remembered(Arc<Mutex<Answers>>);

// DEFAULT-OK: nothing asked yet is exactly what a fresh stream has been told.
#[derive(Default)]
struct Answers {
    metadata: Vec<(MetadataClasses, CarriedMetadata)>,
    outline: Vec<(MetadataClasses, CarriedMetadata)>,
    /// `Some` once asked: the answer itself may be `None`.
    modified_at: Option<Option<std::time::SystemTime>>,
}

/// Wraps `stream` so every metadata answer it gives is recorded. Everything
/// else forwards untouched.
pub(super) fn remember(stream: Box<dyn VolumeReadStream>) -> (Box<dyn VolumeReadStream>, Remembered) {
    let remembered = Remembered::default();
    let wrapped = Box::new(Recording {
        inner: stream,
        remembered: remembered.clone(),
    });
    (wrapped, remembered)
}

impl Remembered {
    /// `resumed`, answering every metadata question the first stream was asked
    /// with the first stream's answer. A question it was never asked goes to
    /// `resumed`.
    pub(super) fn answering(&self, resumed: Box<dyn VolumeReadStream>) -> Box<dyn VolumeReadStream> {
        Box::new(Answering {
            inner: resumed,
            remembered: self.clone(),
        })
    }
}

/// The first stream: answers as itself, and records the answers.
struct Recording {
    inner: Box<dyn VolumeReadStream>,
    remembered: Remembered,
}

/// A resumed stream: answers as the first one did.
struct Answering {
    inner: Box<dyn VolumeReadStream>,
    remembered: Remembered,
}

fn recall(answers: &[(MetadataClasses, CarriedMetadata)], wanted: MetadataClasses) -> Option<CarriedMetadata> {
    answers
        .iter()
        .find(|(asked, _)| *asked == wanted)
        .map(|(_, answer)| answer.clone())
}

impl VolumeReadStream for Recording {
    fn next_chunk(&mut self) -> Pin<Box<dyn Future<Output = Option<Result<Vec<u8>, VolumeError>>> + Send + '_>> {
        self.inner.next_chunk()
    }

    fn total_size(&self) -> StreamLength {
        self.inner.total_size()
    }

    fn bytes_read(&self) -> u64 {
        self.inner.bytes_read()
    }

    fn modified_at(&self) -> Option<std::time::SystemTime> {
        let answer = self.inner.modified_at();
        self.remembered.0.lock_ignore_poison().modified_at = Some(answer);
        answer
    }

    fn metadata(&self, wanted: MetadataClasses) -> CarriedMetadata {
        let answer = self.inner.metadata(wanted);
        let mut answers = self.remembered.0.lock_ignore_poison();
        answers.metadata.retain(|(asked, _)| *asked != wanted);
        answers.metadata.push((wanted, answer.clone()));
        answer
    }

    fn metadata_outline(&self, wanted: MetadataClasses) -> CarriedMetadata {
        let answer = self.inner.metadata_outline(wanted);
        let mut answers = self.remembered.0.lock_ignore_poison();
        answers.outline.retain(|(asked, _)| *asked != wanted);
        answers.outline.push((wanted, answer.clone()));
        answer
    }

    fn stop_signal(&self) -> ScanStop {
        self.inner.stop_signal()
    }

    fn source_version(&self) -> Option<SourceVersion> {
        self.inner.source_version()
    }

    fn cancel_and_release(&mut self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        self.inner.cancel_and_release()
    }
}

impl VolumeReadStream for Answering {
    fn next_chunk(&mut self) -> Pin<Box<dyn Future<Output = Option<Result<Vec<u8>, VolumeError>>> + Send + '_>> {
        self.inner.next_chunk()
    }

    fn total_size(&self) -> StreamLength {
        self.inner.total_size()
    }

    fn bytes_read(&self) -> u64 {
        self.inner.bytes_read()
    }

    fn modified_at(&self) -> Option<std::time::SystemTime> {
        let recalled = self.remembered.0.lock_ignore_poison().modified_at;
        recalled.unwrap_or_else(|| self.inner.modified_at())
    }

    fn metadata(&self, wanted: MetadataClasses) -> CarriedMetadata {
        let recalled = recall(&self.remembered.0.lock_ignore_poison().metadata, wanted);
        recalled.unwrap_or_else(|| self.inner.metadata(wanted))
    }

    fn metadata_outline(&self, wanted: MetadataClasses) -> CarriedMetadata {
        let recalled = recall(&self.remembered.0.lock_ignore_poison().outline, wanted);
        recalled.unwrap_or_else(|| self.inner.metadata_outline(wanted))
    }

    fn stop_signal(&self) -> ScanStop {
        self.inner.stop_signal()
    }

    fn source_version(&self) -> Option<SourceVersion> {
        self.inner.source_version()
    }

    fn cancel_and_release(&mut self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        self.inner.cancel_and_release()
    }
}
