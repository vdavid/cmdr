//! macOS: `fsync(2)` stops at the drive's cache, and `F_FULLFSYNC` (std's
//! `sync_data` and `sync_all`) is the device barrier past it, ~4 ms on an
//! internal SSD and tens of ms on a spinning disk.

use std::fs::File;
use std::io;
use std::os::unix::io::AsRawFd as _;

pub(crate) const NEEDS_DEVICE_BARRIER: bool = true;

/// `<unistd.h>`'s `SYNC_VOLUME_WAIT`: return once the volume's writes are done.
const SYNC_VOLUME_WAIT: libc::c_int = 0x02;

unsafe extern "C" {
    /// `<unistd.h>`, macOS 10.8+: `FSIOC_SYNC_VOLUME` on the volume holding
    /// `fd`. Not in the `libc` crate.
    fn fsync_volume_np(fd: libc::c_int, flags: libc::c_int) -> libc::c_int;
}

/// `F_FULLFSYNC`, falling back to `fsync(2)` where the filesystem doesn't offer
/// it: an SMB mount of a Samba share answers `ENOTSUP` (verified against the
/// Docker guest share, macOS 27.0, 2026-10-11). SQLite falls back the same way.
pub(crate) fn sync_file_data(file: &File) -> io::Result<()> {
    match file.sync_data() {
        Err(e) if e.raw_os_error() == Some(libc::ENOTSUP) => plain_fsync(file),
        other => other,
    }
}

pub(crate) fn sync_to_drive(file: &File) -> io::Result<()> {
    plain_fsync(file)
}

pub(crate) fn sync_dir_to_drive(dir: &File) -> io::Result<()> {
    plain_fsync(dir)
}

/// `F_FULLFSYNC`, with the unsupported class (an SMB mount's `ENOTSUP`) as
/// `Ok`: each item already had its `fsync`, which on a network mount is the
/// server's flush.
pub(crate) fn device_barrier(file: &File) -> io::Result<()> {
    // SAFETY: `file` is open for the whole call, so its fd is valid;
    // `F_FULLFSYNC` takes no argument and touches nothing but that fd.
    if unsafe { libc::fcntl(file.as_raw_fd(), libc::F_FULLFSYNC) } == 0 {
        return Ok(());
    }
    let e = io::Error::last_os_error();
    if super::super::is_sync_unsupported(&e) {
        Ok(())
    } else {
        Err(e)
    }
}

/// Whether `file` lives on a local volume (`MNT_LOCAL`), where its `fsync` can
/// wait for the next pass. A network mount's (`smbfs`, `nfs`, `webdav`) can't:
/// there the `fsync` IS the server's flush, and it's what makes the dates set
/// after it stick. A volume that can't be asked counts as not local.
pub(crate) fn defers_data_sync(file: &File) -> bool {
    let mut stat = std::mem::MaybeUninit::<libc::statfs>::uninit();
    // SAFETY: `file` is open for the whole call, so its fd is valid, and
    // `stat` is a writable `statfs` the kernel fills on success.
    if unsafe { libc::fstatfs(file.as_raw_fd(), stat.as_mut_ptr()) } != 0 {
        return false;
    }
    // SAFETY: `fstatfs` returned 0, so it initialized `stat`.
    let stat = unsafe { stat.assume_init() };
    stat.f_flags & (libc::MNT_LOCAL as u32) != 0
}

/// Every write on the volume holding `file` handed to its drive, waited for:
/// the catch-all for a written file the pass can't find by name.
///
/// ❗ It never reports a write error: the kernel discards `VFS_SYNC`'s answer
/// (`handle_sync_volume` in xnu's `bsd/vfs/vfs_syscalls.c`, `main` as of
/// 2026-11-12), so only the per-file `fsync` can say a write failed.
pub(crate) fn sync_volume(file: &File) -> io::Result<()> {
    // SAFETY: `file` is open for the whole call, so its fd is valid; the call
    // touches nothing but the volume behind it.
    if unsafe { fsync_volume_np(file.as_raw_fd(), SYNC_VOLUME_WAIT) } == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

fn plain_fsync(file: &File) -> io::Result<()> {
    // SAFETY: `file` is open for the whole call, so its fd is valid; `fsync`
    // touches nothing but that fd.
    if unsafe { libc::fsync(file.as_raw_fd()) } == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}
