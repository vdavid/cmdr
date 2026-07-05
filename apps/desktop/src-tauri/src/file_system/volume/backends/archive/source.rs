//! The byte-source seam: where an archive's raw bytes come from.
//!
//! rc-zip is sans-IO — its state machines ask for a byte range and we supply
//! it. [`ArchiveByteSource`] is that supplier: a positioned, blocking
//! `read_at`. Local archives read through [`LocalFileSource`] (a `pread` over
//! the real file); a remote parent volume's ranged read slots in later by
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
use std::sync::{Arc, Mutex};

use positioned_io::{RandomAccessFile, ReadAt, Size};

use crate::ignore_poison::IgnorePoison;

/// Default tail window a [`TailCachedSource`] prefetches: enough that a typical
/// zip's whole central directory (plus the EOCD it hunts backward for) lands in
/// ONE ranged read of a remote backend. A larger directory just triggers a
/// second read for its earlier part.
pub const DEFAULT_TAIL_CACHE_LEN: u64 = 256 * 1024;

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

/// Wraps a byte source with a lazily-loaded, cached **tail window**, so the
/// central-directory parse (rc-zip hunts the EOCD at the very end, then reads the
/// directory that precedes it — all near the tail) costs ONE ranged read of a
/// slow remote backend instead of many small `read_at`s.
///
/// A read landing at or after the tail start is served from the cached tail
/// (fetched once, on the first such read); anything earlier — an entry's
/// compressed bytes mid-file, or a central directory larger than the window —
/// falls through to the inner source. So a remote browse of a normal archive
/// issues a single tail `read_at` on the inner source; a big directory adds a
/// second. Local archives don't need this (a `pread` is already cheap) — it's
/// applied only to the remote parent-backed source.
pub struct TailCachedSource {
    inner: Arc<dyn ArchiveByteSource>,
    size: u64,
    /// First byte of the cached tail window (`size - tail_len`, saturating).
    tail_start: u64,
    /// The cached tail bytes `[tail_start, size)`, loaded on first tail read.
    tail: Mutex<Option<Arc<[u8]>>>,
}

impl TailCachedSource {
    /// Wraps `inner`, caching up to `tail_len` bytes at the end of the file.
    pub fn new(inner: Arc<dyn ArchiveByteSource>, tail_len: u64) -> Self {
        let size = inner.size();
        Self {
            inner,
            size,
            tail_start: size.saturating_sub(tail_len),
            tail: Mutex::new(None),
        }
    }

    /// Returns the cached tail, fetching it once from the inner source. The fetch
    /// happens under the lock, so a burst of concurrent tail reads triggers one
    /// backend round-trip.
    fn tail(&self) -> io::Result<Arc<[u8]>> {
        let mut guard = self.tail.lock_ignore_poison();
        if let Some(cached) = guard.as_ref() {
            return Ok(Arc::clone(cached));
        }
        let len = (self.size - self.tail_start) as usize;
        let mut buf = vec![0u8; len];
        let mut filled = 0usize;
        while filled < len {
            let n = self
                .inner
                .read_at(self.tail_start + filled as u64, &mut buf[filled..])?;
            if n == 0 {
                break;
            }
            filled += n;
        }
        buf.truncate(filled);
        let cached: Arc<[u8]> = Arc::from(buf);
        *guard = Some(Arc::clone(&cached));
        Ok(cached)
    }
}

impl ArchiveByteSource for TailCachedSource {
    fn size(&self) -> u64 {
        self.size
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() || offset >= self.size {
            return Ok(0);
        }
        // A read starting before the tail window (an entry mid-file, or a CD
        // bigger than the window) goes straight to the inner source.
        if offset < self.tail_start {
            return self.inner.read_at(offset, buf);
        }
        let tail = self.tail()?;
        let rel = (offset - self.tail_start) as usize;
        if rel >= tail.len() {
            return Ok(0);
        }
        let n = (tail.len() - rel).min(buf.len());
        buf[..n].copy_from_slice(&tail[rel..rel + n]);
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// A byte source that counts `read_at` calls, so tests can pin how many
    /// backend round-trips a tail cache turns into.
    struct CountingSource {
        bytes: Vec<u8>,
        reads: AtomicUsize,
    }

    impl CountingSource {
        fn new(bytes: Vec<u8>) -> Arc<Self> {
            Arc::new(Self {
                bytes,
                reads: AtomicUsize::new(0),
            })
        }
        fn read_count(&self) -> usize {
            self.reads.load(Ordering::Relaxed)
        }
    }

    impl ArchiveByteSource for CountingSource {
        fn size(&self) -> u64 {
            self.bytes.len() as u64
        }
        fn read_at(&self, offset: u64, buf: &mut [u8]) -> io::Result<usize> {
            self.reads.fetch_add(1, Ordering::Relaxed);
            let start = (offset as usize).min(self.bytes.len());
            let n = (self.bytes.len() - start).min(buf.len());
            buf[..n].copy_from_slice(&self.bytes[start..start + n]);
            Ok(n)
        }
    }

    #[test]
    fn tail_reads_hit_the_backend_once() {
        let inner = CountingSource::new((0..200u16).map(|n| n as u8).collect());
        let cache = TailCachedSource::new(inner.clone() as Arc<dyn ArchiveByteSource>, 64);
        // Several reads inside the 64-byte tail window: all served from ONE fetch.
        let mut buf = [0u8; 8];
        for off in [140u64, 160, 150, 199] {
            cache.read_at(off, &mut buf).expect("read");
        }
        assert_eq!(inner.read_count(), 1, "tail window fetched exactly once");
        // The served bytes are correct (offset 150 → byte value 150).
        let mut one = [0u8; 1];
        cache.read_at(150, &mut one).expect("read");
        assert_eq!(one[0], 150u8);
    }

    #[test]
    fn pre_tail_reads_fall_through_to_the_backend() {
        let inner = CountingSource::new(vec![7u8; 1000]);
        let cache = TailCachedSource::new(inner.clone() as Arc<dyn ArchiveByteSource>, 100);
        // Tail window is [900, 1000). A read before it (a central directory
        // larger than the window) is a second, separate backend read.
        let mut buf = [0u8; 16];
        cache.read_at(950, &mut buf).expect("tail read"); // 1 fetch (tail)
        cache.read_at(100, &mut buf).expect("pre-tail read"); // + 1 (fallthrough)
        cache.read_at(920, &mut buf).expect("tail read again"); // cached, no fetch
        assert_eq!(inner.read_count(), 2);
    }

    #[test]
    fn reads_past_eof_return_zero() {
        let inner = CountingSource::new(vec![1u8; 50]);
        let cache = TailCachedSource::new(inner as Arc<dyn ArchiveByteSource>, 64);
        let mut buf = [0u8; 8];
        assert_eq!(cache.read_at(50, &mut buf).expect("eof read"), 0);
        assert_eq!(cache.read_at(999, &mut buf).expect("far eof read"), 0);
    }
}
