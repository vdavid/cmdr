//! The POSIX readers macOS and Linux share.

use std::path::Path;

/// Free and total bytes of the volume holding `path`, via `statfs`.
pub(crate) fn volume_space(path: &Path) -> (Option<u64>, Option<u64>) {
    use std::os::unix::ffi::OsStrExt;
    let Ok(c_path) = std::ffi::CString::new(path.as_os_str().as_bytes()) else {
        return (None, None);
    };
    // SAFETY: `libc::statfs` is a C struct of plain integers, so an all-zero bit pattern is a
    // valid (empty) value to initialize before the OS fills it in.
    let mut stat: libc::statfs = unsafe { std::mem::zeroed() };
    // SAFETY: `c_path` is a valid NUL-terminated path and `stat` is a live, correctly-sized
    // `statfs` the call writes into; we read its fields only after `rc == 0`.
    let rc = unsafe { libc::statfs(c_path.as_ptr(), &mut stat) };
    if rc != 0 {
        return (None, None);
    }
    let block_size = stat.f_bsize as u64;
    let total = (stat.f_blocks as u64).saturating_mul(block_size);
    let free = (stat.f_bavail as u64).saturating_mul(block_size);
    (Some(free), Some(total))
}
