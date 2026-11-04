//! Where a text session's bytes come from: one handle, opened once per session.
//!
//! Every backend reads through a [`SharedSource`] it was handed at open, so a fetch, a
//! search, a tail extend, and the background index scan all share one open file. A
//! per-request `open` is a network round trip on a mount, and a scrollbar drag fires a
//! fetch every 100 ms, which is how jumps in a big file on SMB ran out of their 2 s
//! budget (#303).
//!
//! Reads are positioned (`pread`-shaped): no cursor, so concurrent readers never
//! disturb each other. `DETAILS.md` § "Byte sources".

use std::fs::File;
use std::os::unix::fs::FileExt;
use std::path::Path;
use std::sync::Arc;

use super::ViewerError;

/// A file the viewer reads by absolute offset.
pub trait ByteSource: Send + Sync {
    /// Fill `buf` with the bytes at `offset`, returning how many were filled. Short only
    /// at end of file (or a file that shrank under us).
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize, ViewerError>;
}

/// The handle a session and all its backends share.
pub type SharedSource = Arc<dyn ByteSource>;

/// A local (or OS-mounted) file, held open.
pub struct LocalFileSource {
    file: File,
}

impl LocalFileSource {
    pub fn open(path: &Path) -> Result<Self, ViewerError> {
        Ok(Self {
            file: File::open(path)?,
        })
    }

    pub fn shared(path: &Path) -> Result<SharedSource, ViewerError> {
        Ok(Arc::new(Self::open(path)?))
    }
}

impl From<File> for LocalFileSource {
    fn from(file: File) -> Self {
        Self { file }
    }
}

impl ByteSource for LocalFileSource {
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize, ViewerError> {
        let mut filled = 0usize;
        while filled < buf.len() {
            let read = self.file.read_at(&mut buf[filled..], offset + filled as u64)?;
            if read == 0 {
                break;
            }
            filled += read;
        }
        Ok(filled)
    }
}

/// An open source plus the two facts every backend needs at open.
#[derive(Clone)]
pub struct OpenedFile {
    pub source: SharedSource,
    /// The size as of open. Tail mode moves a backend past it with `extend_to`.
    pub total_bytes: u64,
    /// Last path component, as the title bar shows it.
    pub file_name: String,
}

/// Opens a local (or OS-mounted) file for a session: stat once, then hold the handle.
pub fn open_local(path: &Path) -> Result<OpenedFile, ViewerError> {
    let metadata = std::fs::metadata(path).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => ViewerError::NotFound {
            path: path.display().to_string(),
        },
        _ => ViewerError::from(e),
    })?;
    if metadata.is_dir() {
        return Err(ViewerError::IsDirectory);
    }
    Ok(OpenedFile {
        source: LocalFileSource::shared(path)?,
        total_bytes: metadata.len(),
        file_name: file_name_of(path),
    })
}

/// The name a session shows for `path`: its last component, or the whole path when it
/// has none.
pub fn file_name_of(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

/// Up to `limit` bytes from the start of a `total_bytes`-long file, for the encoding
/// sniff and the BOM check. Never asks past the end (see `FileSource::read_window`).
pub fn read_head(source: &dyn ByteSource, total_bytes: u64, limit: usize) -> Result<Vec<u8>, ViewerError> {
    let mut head = vec![0u8; total_bytes.min(limit as u64) as usize];
    let read = source.read_at(0, &mut head)?;
    head.truncate(read);
    Ok(head)
}
