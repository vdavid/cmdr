//! What macOS and Linux answer alike: the POSIX `stat` fields, and an xattr read
//! that never takes more than its cap.

use std::ffi::CString;
use std::os::unix::fs::{MetadataExt, PermissionsExt};

/// Any execute bit is set.
pub(crate) fn executable(metadata: &std::fs::Metadata) -> bool {
    metadata.permissions().mode() & 0o111 != 0
}

/// `st_blocks` is in 512-byte units on both OSes.
pub(crate) fn physical_size(metadata: &std::fs::Metadata) -> Option<u64> {
    Some(metadata.blocks().saturating_mul(512))
}

/// A folder's link count is its subfolders plus two, not other names for it.
pub(crate) fn hard_links(metadata: &std::fs::Metadata) -> Option<u64> {
    (!metadata.is_dir()).then(|| metadata.nlink())
}

/// One xattr's value, read without following a symlink.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum XattrRead {
    /// The item carries no such xattr.
    Absent,
    /// Its value, at most the cap.
    Value(Vec<u8>),
    /// Over the cap: its bytes were never read.
    TooLarge,
    /// The OS refused the read (an errno other than "no such attribute").
    Failed,
}

impl XattrRead {
    /// The bytes a decoder takes: `None` for no record, and for one over the cap
    /// a buffer one byte past it, which every decoder in `cmdr-about` reads as
    /// unparseable without parsing.
    pub(crate) fn for_decoder(&self, cap: usize) -> Option<Vec<u8>> {
        match self {
            XattrRead::Absent | XattrRead::Failed => None,
            XattrRead::Value(bytes) => Some(bytes.clone()),
            XattrRead::TooLarge => Some(vec![0; cap + 1]),
        }
    }
}

/// The two calls an OS answers an xattr read with.
pub(crate) trait XattrSyscalls {
    /// The value's size, or the errno.
    fn size(path: &CString, name: &CString) -> Result<usize, i32>;
    /// Reads into `buf`, answering how many bytes landed, or the errno.
    fn read(path: &CString, name: &CString, buf: &mut [u8]) -> Result<usize, i32>;
    /// The errno for "no such attribute".
    const NO_ATTR: i32;
}

/// Reads `name` on `path`, refusing anything over `cap` before reading a byte of it.
pub(crate) fn read_capped<S: XattrSyscalls>(path: &str, name: &str, cap: usize) -> XattrRead {
    let (Ok(path), Ok(name)) = (CString::new(path), CString::new(name)) else {
        return XattrRead::Failed;
    };
    let size = match S::size(&path, &name) {
        Ok(size) => size,
        Err(errno) if errno == S::NO_ATTR => return XattrRead::Absent,
        Err(_) => return XattrRead::Failed,
    };
    if size > cap {
        return XattrRead::TooLarge;
    }
    let mut buf = vec![0; size];
    match S::read(&path, &name, &mut buf) {
        Ok(read) => {
            buf.truncate(read);
            XattrRead::Value(buf)
        }
        Err(errno) if errno == S::NO_ATTR => XattrRead::Absent,
        // Grew between the two calls: past the size Cmdr agreed to read.
        Err(errno) if errno == libc::ERANGE => XattrRead::TooLarge,
        Err(_) => XattrRead::Failed,
    }
}

/// The calling thread's errno.
pub(crate) fn errno() -> i32 {
    std::io::Error::last_os_error().raw_os_error().unwrap_or(0)
}
