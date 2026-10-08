//! The two stream traits a `Volume` hands back: [`VolumeReadStream`], one
//! file's bytes in chunks plus the date the file carries, and
//! [`SequentialExtract`], a one-pass walk over a sequential archive's members.
//! Both are re-exported from `volume`, beside the trait that returns them.

use std::future::Future;
use std::pin::Pin;

use super::{ExtractedFile, ScanStop, StreamLength, VolumeError};

/// A stream of bytes read from a volume.
///
/// This is an async interface for reading file data in chunks. Used for
/// streaming transfers between volumes. `next_chunk` is async (returns a
/// pinned boxed future) so that network-backed volumes (MTP, SMB) can
/// yield to the runtime instead of blocking. `total_size` and `bytes_read`
/// stay sync because they return cached values.
pub trait VolumeReadStream: Send {
    /// Returns the next chunk of data, or None if complete.
    #[allow(
        clippy::type_complexity,
        reason = "async trait method returns a pinned boxed future by design"
    )]
    fn next_chunk(&mut self) -> Pin<Box<dyn Future<Output = Option<Result<Vec<u8>, VolumeError>>> + Send + '_>>;

    /// Final byte length, when it is known before the stream reaches EOF.
    fn total_size(&self) -> StreamLength;

    /// Bytes read so far (for progress tracking).
    fn bytes_read(&self) -> u64;

    /// When the file being read was last modified, from the stat or listing the
    /// open already did (no extra round trip). Every destination that can store a
    /// date writes this one, so a copy keeps the source's date.
    ///
    /// Required on purpose, so a new backend can't silently drop dates: a source
    /// that knows its file's date returns it, and only a stream with no
    /// meaningful date (fresh `create_file` bytes, a generated archive, a git
    /// blob, a test double) returns `None`, which leaves the destination's own
    /// date. ❗ A wrapper stream forwards its inner stream's answer. The contract
    /// and where each backend stands: `apps/desktop/src-tauri/src/file_system/write_operations/transfer/volume/DETAILS.md`
    /// § "Copies keep the source's date".
    fn modified_at(&self) -> Option<std::time::SystemTime>;

    /// The operation's Cancel and pause, for a destination that reads AHEAD of
    /// the wire. A pause parks the stream's own `next_chunk`, which stops a
    /// destination that sends each piece as it reads it. One that buffers first
    /// (S3 fills a whole part, then sends it) has the source drained long before
    /// its bytes go out, so it parks its requests on this instead.
    ///
    /// Default: nobody can pause this stream. ❗ A wrapper stream forwards its
    /// inner stream's answer, unless it IS the operation's checkpoint.
    fn stop_signal(&self) -> ScanStop {
        ScanStop::none()
    }

    /// Promptly release any scarce backend resource this stream holds across
    /// chunks, before the stream is dropped. After this call the stream is spent;
    /// `next_chunk` must not be called again on it.
    ///
    /// Default is a no-op, and that's what every current backend uses: reads that
    /// could otherwise pin a scarce resource (MTP's one-per-device PTP session)
    /// are bounded windows that hold nothing between chunks, so the copy wrapper
    /// (`CheckpointStream`) parks in place rather than releasing anything. This
    /// stays a trait hook for a hypothetical future backend whose stream genuinely
    /// holds a resource across chunks; nothing in the copy path calls it today.
    #[allow(
        clippy::type_complexity,
        reason = "async trait method returns a pinned boxed future by design"
    )]
    fn cancel_and_release(&mut self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(async {})
    }
}

/// A ONE-PASS extractor over a subtree of a SEQUENTIAL source (a compressed tar
/// or solid 7z), where a per-entry random read re-decodes the prefix and makes a
/// subtree extract O(n²). Decoding the stream a single time, it yields each file
/// in ARCHIVE order: [`next_file`](Self::next_file) advances to the next member,
/// then [`current_stream`](Self::current_stream) hands its bytes to the
/// destination's `write_from_stream`.
///
/// The copy engine drives it after creating the destination directory structure
/// from the tree (cheap, no decode), so this never yields directories. Dropping
/// the extractor stops the underlying decoder (drop-based cancellation).
pub trait SequentialExtract: Send {
    /// Advances to the next file member (draining any unread bytes of the current
    /// one), or `Ok(None)` at the end of the subtree.
    #[allow(
        clippy::type_complexity,
        reason = "async trait method returns a pinned boxed future by design"
    )]
    fn next_file(&mut self) -> Pin<Box<dyn Future<Output = Result<Option<ExtractedFile>, VolumeError>> + Send + '_>>;

    /// An owned read stream over the CURRENT member's decoded bytes, to hand to
    /// the destination's `write_from_stream`. Valid until the next
    /// [`next_file`](Self::next_file); call exactly once per member.
    fn current_stream(&self) -> Box<dyn VolumeReadStream>;
}
