//! Both Unixes: POSIX links, `geteuid`, and `mkfifo`.

use std::io;
use std::path::Path;

/// See the re-export in `mod.rs`.
pub fn symlink<T: AsRef<Path>, L: AsRef<Path>>(target: T, link: L) -> io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

/// See the re-export in `mod.rs`.
pub fn is_root() -> bool {
    // SAFETY: `geteuid` takes no arguments, touches no memory of ours, and can't fail.
    unsafe { libc::geteuid() == 0 }
}

/// See the re-export in `mod.rs`.
pub fn can_create_symlinks() -> bool {
    true
}

/// See the re-export in `mod.rs`.
pub fn make_fifo(path: &Path) -> io::Result<()> {
    use std::os::unix::ffi::OsStrExt as _;
    let c_path = std::ffi::CString::new(path.as_os_str().as_bytes())
        .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
    // SAFETY: `c_path` is a valid NUL-terminated C string that outlives the call, and
    // `mkfifo` only reads it; the mode is a plain integer.
    if unsafe { libc::mkfifo(c_path.as_ptr(), 0o600) } == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}
