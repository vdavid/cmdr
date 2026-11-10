//! The reader traits a `Volume` hands back: [`VolumeReadStream`], one file's
//! bytes in chunks plus the date the file carries, [`VolumeRandomReader`], one
//! held-open file read by absolute offset, and [`SequentialExtract`], a one-pass
//! walk over a sequential archive's members. All are re-exported from `volume`,
//! beside the trait that returns them.

use std::future::Future;
use std::pin::Pin;

use super::{ExtractedFile, ScanStop, StreamLength, VolumeError};
use crate::metadata::{CarriedMetadata, MetadataClasses};

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
    /// and where each backend stands: `crates/cmdr-operations/src/transfer/volume/DETAILS.md`
    /// § "Copies keep the source's metadata".
    fn modified_at(&self) -> Option<std::time::SystemTime>;

    /// What the file being read carries beyond its bytes and its mtime
    /// (creation date, mode, user flags, xattrs), restricted to `wanted`: the
    /// classes the DESTINATION can store, so a source never reads what nobody
    /// keeps (a local source lists its xattrs only for a destination that
    /// stores them).
    ///
    /// Answered from what the open already learned (the stat, the listing
    /// entry, the CREATE response), ❌ never a new round trip: a class the open
    /// didn't return is simply left out. A destination asks once, before the
    /// first chunk.
    ///
    /// Default: nothing, for a stream with no source item behind it (fresh
    /// `create_file` bytes, a generated archive, a test double). ❗ A wrapper
    /// stream forwards its inner stream's answer. Which backend reports what:
    /// `crates/cmdr-operations/src/transfer/volume/DETAILS.md`
    /// § "Copies keep the source's metadata".
    fn metadata(&self, wanted: MetadataClasses) -> CarriedMetadata {
        let _ = wanted;
        CarriedMetadata::default()
    }

    /// [`metadata`](Self::metadata) with every xattr's VALUE left empty: enough
    /// to tell which classes the source has something in, for a copy onto a
    /// destination that can't keep them (the loss note). A local source answers
    /// with one name-only `flistxattr`, never reading a resource fork it would
    /// only throw away.
    ///
    /// Default: [`metadata`](Self::metadata) itself, which is right for every
    /// source whose xattrs cost nothing more to read. ❗ A wrapper stream
    /// forwards its inner stream's answer.
    fn metadata_outline(&self, wanted: MetadataClasses) -> CarriedMetadata {
        self.metadata(wanted)
    }

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

    /// Which version of the file this stream reads, for a copy to carry on
    /// from after a long outage ([`Volume::resume_read_stream`]). Taken from
    /// what the open already learned, ❌ never a round trip of its own.
    ///
    /// Default `None`: this stream can't be resumed, and a copy that loses it
    /// runs the file again from its first byte. ❗ The engine asks the stream
    /// the backend handed out, before wrapping it, so a wrapper needn't forward
    /// this.
    ///
    /// [`Volume::resume_read_stream`]: super::Volume::resume_read_stream
    fn source_version(&self) -> Option<super::SourceVersion> {
        None
    }

    /// Promptly release any scarce backend resource this stream holds across
    /// chunks, before the stream is dropped. After this call the stream is spent;
    /// `next_chunk` must not be called again on it.
    ///
    /// Default is a no-op, which is right for a stream that holds nothing between
    /// chunks. MTP overrides it: on a device without partial reads its stream
    /// keeps the one-per-device PTP session through a held-open transfer, and this
    /// stops that transfer (its `Drop` does the same on the runtime). The copy
    /// wrapper (`CheckpointStream`) parks in place rather than releasing anything,
    /// so nothing in the copy path calls it today; a stopped copy drops the stream.
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

/// One file a volume holds open and reads by absolute offset, `pread`-style.
///
/// For a caller that jumps around one file many times, like the file viewer
/// scrolling a multi-GB log on a share: [`Volume::read_range`](super::Volume::read_range)
/// opens and closes the file per call, which costs an extra round trip per read
/// on a network backend. This keeps the handle, so each read is one.
pub trait VolumeRandomReader: Send + Sync {
    /// The file's size as the open saw it. Reads clamp to it, so a file that grew
    /// since needs a fresh reader to read the new bytes.
    fn size(&self) -> u64;

    /// Up to `len` bytes at `offset`. Fewer only at end of file (as of the open);
    /// a read wholly past it yields an empty `Vec`. No cursor, so concurrent calls
    /// are independent.
    #[allow(
        clippy::type_complexity,
        reason = "async trait method returns a pinned boxed future by design"
    )]
    fn read_at(
        &self,
        offset: u64,
        len: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, VolumeError>> + Send + '_>>;

    /// Releases the handle. A reader dropped without this may hold a server-side
    /// handle until the backend's session ends.
    fn close(self: Box<Self>) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send>>;
}
