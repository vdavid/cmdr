//! What macOS and Linux answer alike: the POSIX `stat` fields, and an xattr read
//! that never takes more than its cap.

use std::ffi::CString;
use std::os::unix::fs::{MetadataExt, PermissionsExt};

use cmdr_about::fact::{Fact, UnknownWhy};
use cmdr_about::provenance::Download;

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
    /// The OS refused the read: an errno other than "no such attribute". ❌ Never
    /// "no record": the item may well carry one.
    Failed(i32),
}

impl XattrRead {
    /// The bytes a decoder takes: `None` for no record (or a read that failed, which
    /// [`download_with`] says apart), and for one over the cap a buffer one byte past
    /// it, which every decoder in `cmdr-about` reads as unparseable without parsing.
    pub(crate) fn for_decoder(&self, cap: usize) -> Option<Vec<u8>> {
        match self {
            XattrRead::Absent | XattrRead::Failed(_) => None,
            XattrRead::Value(bytes) => Some(bytes.clone()),
            XattrRead::TooLarge => Some(vec![0; cap + 1]),
        }
    }
}

/// Why an xattr read failed, by its errno.
pub(crate) fn why_of_errno(errno: i32) -> UnknownWhy {
    match errno {
        libc::EACCES | libc::EPERM => UnknownWhy::PermissionDenied,
        libc::ENOENT | libc::ENOTDIR => UnknownWhy::NotFound,
        // The filesystem keeps no extended attributes at all.
        libc::ENOTSUP => UnknownWhy::VolumeCantTell,
        _ => UnknownWhy::Unreachable,
    }
}

/// The download row from `record`, each xattr read capped through `S`. A read the OS
/// refused says why (as `through::download_through` does for a volume's reads),
/// unless the other xattr answered: ❌ a refusal is never "no download record".
pub(crate) fn download_with<S: XattrSyscalls>(
    path: &str,
    cap: usize,
    record: fn(super::ReadXattr<'_>) -> Fact<Download>,
) -> Fact<Download> {
    let mut failed = None;
    let fact = record(&mut |name| {
        let read = read_capped::<S>(path, name, cap);
        if let XattrRead::Failed(errno) = read {
            failed.get_or_insert(why_of_errno(errno));
        }
        read.for_decoder(cap)
    });
    match (failed, &fact) {
        (Some(why), Fact::Unknown(_)) => Fact::unknown(why),
        _ => fact,
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
        return XattrRead::Failed(libc::EINVAL);
    };
    let size = match S::size(&path, &name) {
        Ok(size) => size,
        Err(errno) if errno == S::NO_ATTR => return XattrRead::Absent,
        Err(errno) => return XattrRead::Failed(errno),
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
        Err(errno) => XattrRead::Failed(errno),
    }
}

/// The calling thread's errno.
pub(crate) fn errno() -> i32 {
    std::io::Error::last_os_error().raw_os_error().unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Syscalls that refuse every read with one errno.
    struct Refusing<const ERRNO: i32>;

    impl<const ERRNO: i32> XattrSyscalls for Refusing<ERRNO> {
        const NO_ATTR: i32 = -1;

        fn size(_path: &CString, _name: &CString) -> Result<usize, i32> {
            Err(ERRNO)
        }

        fn read(_path: &CString, _name: &CString, _buf: &mut [u8]) -> Result<usize, i32> {
            Err(ERRNO)
        }
    }

    /// Any record would read as a known download; none reads as `NoRecord`.
    fn record(read: crate::about::platform::ReadXattr<'_>) -> Fact<Download> {
        match (read("first"), read("second")) {
            (None, None) => Fact::unknown(UnknownWhy::NoRecord),
            _ => panic!("the fakes carry no value"),
        }
    }

    /// ❗ A download record Cmdr couldn't read is never "no download record": a
    /// `chmod 000` file with a quarantine answers `EACCES`, and that's what the row says.
    #[test]
    fn a_refused_read_says_why_and_never_no_record() {
        assert_eq!(
            download_with::<Refusing<{ libc::EACCES }>>("/x", 64, record),
            Fact::unknown(UnknownWhy::PermissionDenied)
        );
        assert_eq!(
            download_with::<Refusing<{ libc::EIO }>>("/x", 64, record),
            Fact::unknown(UnknownWhy::Unreachable)
        );
        assert_eq!(
            download_with::<Refusing<{ libc::ENOENT }>>("/x", 64, record),
            Fact::unknown(UnknownWhy::NotFound)
        );
        assert_eq!(
            download_with::<Refusing<-1>>("/x", 64, record),
            Fact::unknown(UnknownWhy::NoRecord),
            "an item with no such xattr has no record"
        );
    }
}
