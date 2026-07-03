//! The byte-source seam: where an archive's raw bytes come from.
//!
//! rc-zip is sans-IO — its state machines ask for a byte range and we supply
//! it. [`ArchiveByteSource`] is that supplier: a positioned, blocking
//! `read_at`. Local archives read through [`LocalFileSource`] (a `pread` over
//! the real file); a remote parent volume's ranged read slots in later (M5) by
//! implementing this one trait, with no change to the parser or the entry
//! reader.
//!
//! It's deliberately **blocking** (not async): both the central-directory parse
//! and every entry decompress run on a `spawn_blocking` thread (CPU-bound work
//! off the async executor), so a synchronous `read_at` is the natural fit and
//! keeps the trait tiny. A remote implementation bridges its async ranged read
//! to this blocking call from inside that same blocking context.

use std::io;
use std::path::Path;
use std::sync::Arc;

use positioned_io::{RandomAccessFile, ReadAt, Size};

/// A positioned, blocking byte source for one archive.
///
/// `Send + Sync` so a single source can be shared (`Arc`) across concurrent
/// entry reads: each reader keeps its own decode state and read offset, and
/// `read_at` takes `&self` (a `pread`, no shared cursor), so parallel reads
/// don't contend on a seek position.
pub trait ArchiveByteSource: Send + Sync {
    /// Total size of the archive in bytes.
    fn size(&self) -> u64;

    /// Read into `buf` starting at `offset`, returning the number of bytes
    /// read. A return of `0` means end of file. Like `pread`, this may return a
    /// short read; callers loop as needed (the fsm drivers do).
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> io::Result<usize>;
}

impl<T: ArchiveByteSource + ?Sized> ArchiveByteSource for Arc<T> {
    fn size(&self) -> u64 {
        (**self).size()
    }
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> io::Result<usize> {
        (**self).read_at(offset, buf)
    }
}

/// A local-file archive source: a `pread` over the real file via
/// `positioned-io`. Opened read-only; holds the file handle for the source's
/// lifetime.
pub struct LocalFileSource {
    file: RandomAccessFile,
    size: u64,
}

impl LocalFileSource {
    /// Opens `path` read-only and records its size.
    pub fn open(path: &Path) -> io::Result<Self> {
        let file = RandomAccessFile::open(path)?;
        let size = file
            .size()?
            .ok_or_else(|| io::Error::other("archive file has no known size"))?;
        Ok(Self { file, size })
    }
}

impl ArchiveByteSource for LocalFileSource {
    fn size(&self) -> u64 {
        self.size
    }
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> io::Result<usize> {
        self.file.read_at(offset, buf)
    }
}

/// An in-memory archive source over a shared byte buffer.
///
/// Used by tests (hand-crafted hostile fixtures, no disk) and available for a
/// small archive already resident in memory. Cheap to clone (`Arc`).
#[derive(Clone)]
pub struct BytesSource {
    bytes: Arc<[u8]>,
}

impl BytesSource {
    /// Wraps an owned byte buffer as an archive source.
    pub fn new(bytes: impl Into<Arc<[u8]>>) -> Self {
        Self { bytes: bytes.into() }
    }
}

impl ArchiveByteSource for BytesSource {
    fn size(&self) -> u64 {
        self.bytes.len() as u64
    }
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> io::Result<usize> {
        let start = offset.min(self.bytes.len() as u64) as usize;
        let src = &self.bytes[start..];
        let n = src.len().min(buf.len());
        buf[..n].copy_from_slice(&src[..n]);
        Ok(n)
    }
}
