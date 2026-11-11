//! The POSIX questions the shared code asks about a local path: whether this
//! process may write a folder, a file's BSD flags, a
//! mount point, an extended attribute, and the home folder. Natively each is the
//! call its caller made before; in the browser build, the answer of a host with
//! no local disk.

use std::path::{Path, PathBuf};

/// What `access(path, W_OK)` said.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WriteProbe {
    /// This process may write there (or the build has no disk to ask).
    Allowed,
    /// Refused, with the errno (`EACCES` when the OS gave none).
    Refused(i32),
    /// The path holds an interior NUL, so the OS can't be handed it.
    Unrepresentable,
}

/// `access(path, W_OK)`. ❗ A syscall: on a hung network mount it blocks like any.
pub(crate) fn write_probe(path: &Path) -> WriteProbe {
    #[cfg(unix)]
    {
        use std::ffi::CString;
        use std::os::unix::ffi::OsStrExt;

        let Ok(c_path) = CString::new(path.as_os_str().as_bytes()) else {
            return WriteProbe::Unrepresentable;
        };
        // SAFETY: `c_path` is a valid NUL-terminated C string that outlives the call.
        if unsafe { libc::access(c_path.as_ptr(), libc::W_OK) } == 0 {
            WriteProbe::Allowed
        } else {
            WriteProbe::Refused(std::io::Error::last_os_error().raw_os_error().unwrap_or(libc::EACCES))
        }
    }
    #[cfg(target_family = "wasm")]
    {
        let _ = path;
        WriteProbe::Allowed
    }
}

/// What `lstat` says of a path's BSD flags (`st_flags`).
#[cfg(target_os = "macos")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BsdFlags {
    /// The flags `lstat` read.
    Read(u32),
    /// `lstat` failed (the entry may be gone).
    Unreadable,
    /// The path holds an interior NUL, so the OS can't be handed it.
    Unrepresentable,
}

/// `lstat(path).st_flags`.
#[cfg(target_os = "macos")]
pub(crate) fn bsd_flags(path: &Path) -> BsdFlags {
    use std::ffi::CString;
    use std::mem::MaybeUninit;
    use std::os::unix::ffi::OsStrExt;

    let Ok(c_path) = CString::new(path.as_os_str().as_bytes()) else {
        return BsdFlags::Unrepresentable;
    };
    let mut stat = MaybeUninit::<libc::stat>::uninit();
    // SAFETY: `c_path` is a valid NUL-terminated C string and `stat` a valid pointer to
    // writable memory the size of a `libc::stat`.
    let result = unsafe { libc::lstat(c_path.as_ptr(), stat.as_mut_ptr()) };
    if result != 0 {
        return BsdFlags::Unreadable;
    }
    // SAFETY: `lstat` returned 0, so it filled the whole struct.
    BsdFlags::Read(unsafe { stat.assume_init() }.st_flags)
}

/// Whether `path` itself carries a lock flag (Finder's Locked, `chflags uchg`, or the
/// system and append-only kinds): a fact about the item, read with `lstat`, never a
/// guess from a path or a message.
pub(crate) fn is_locked_item(path: &Path) -> bool {
    #[cfg(target_os = "macos")]
    {
        use std::os::macos::fs::MetadataExt;

        const LOCKS: u32 = libc::UF_IMMUTABLE | libc::UF_APPEND | libc::SF_IMMUTABLE | libc::SF_APPEND;
        std::fs::symlink_metadata(path).is_ok_and(|meta| meta.st_flags() & LOCKS != 0)
    }
    #[cfg(any(target_os = "linux", target_family = "wasm"))]
    {
        let _ = path;
        false
    }
}

/// Whether `root` is where a filesystem is mounted, by the OS mount table; `None`
/// when the table can't say (or there's no local disk).
pub(crate) fn is_mount_point(root: &Path) -> Option<bool> {
    let root = root.to_string_lossy();
    #[cfg(target_os = "macos")]
    {
        cmdr_local::mount_table::is_mount_point(&root)
    }
    #[cfg(target_os = "linux")]
    {
        cmdr_local::linux_mounts::is_mount_point(&root)
    }
    #[cfg(any(windows, target_family = "wasm"))]
    {
        let _ = root;
        None
    }
}

/// The extended attribute `name` on `path` (`xattr::get`).
#[cfg(target_os = "macos")]
pub(crate) fn read_xattr(path: &Path, name: &str) -> std::io::Result<Option<Vec<u8>>> {
    xattr::get(path, name)
}

/// The person's home folder (`None` in the browser build, which has none).
pub(crate) fn home_dir() -> Option<PathBuf> {
    #[cfg(not(target_family = "wasm"))]
    {
        dirs::home_dir()
    }
    #[cfg(target_family = "wasm")]
    {
        None
    }
}
