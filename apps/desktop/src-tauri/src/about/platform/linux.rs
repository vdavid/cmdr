//! Linux: the download record from the `user.xdg.*` xattrs Chromium writes, and
//! nothing that needs Spotlight, code signing, or a default-app lookup (the plan
//! leaves those rows out here, so the three answers below are never asked).

use std::ffi::CString;

use cmdr_about::card::OsCaps;
use cmdr_about::fact::{AboutTarget, Fact, NotApplicableWhy, UnknownWhy};
use cmdr_about::payload::{AppRef, Signature, Usage};
use cmdr_about::provenance::{Download, MAX_XATTR_BYTES, xdg_download_fact};

use super::StatExtras;
use super::unix::{XattrSyscalls, errno, executable, hard_links, physical_size, read_capped};

pub(crate) fn os_caps() -> OsCaps {
    OsCaps {
        use_history: false,
        code_signing: false,
        default_app: false,
    }
}

pub(crate) fn stat_extras(metadata: &std::fs::Metadata) -> StatExtras {
    StatExtras {
        executable: executable(metadata),
        online_only: false,
        physical_size: physical_size(metadata),
        hard_links: hard_links(metadata),
    }
}

/// `lgetxattr`: the link itself, never what it points at.
struct Linux;

impl XattrSyscalls for Linux {
    const NO_ATTR: i32 = libc::ENODATA;

    fn size(path: &CString, name: &CString) -> Result<usize, i32> {
        // SAFETY: both pointers are live NUL-terminated strings for the call; a null
        // buffer with size 0 asks only for the value's size.
        let size = unsafe { libc::lgetxattr(path.as_ptr(), name.as_ptr(), std::ptr::null_mut(), 0) };
        usize::try_from(size).map_err(|_| errno())
    }

    fn read(path: &CString, name: &CString, buf: &mut [u8]) -> Result<usize, i32> {
        // SAFETY: `buf` is a live, writable slice of exactly `buf.len()` bytes, and the
        // kernel writes at most that many.
        let read = unsafe { libc::lgetxattr(path.as_ptr(), name.as_ptr(), buf.as_mut_ptr().cast(), buf.len()) };
        usize::try_from(read).map_err(|_| errno())
    }
}

pub(crate) fn download(path: &str) -> Fact<Download> {
    let origin = read_capped::<Linux>(path, "user.xdg.origin.url", MAX_XATTR_BYTES).for_decoder(MAX_XATTR_BYTES);
    let referrer = read_capped::<Linux>(path, "user.xdg.referrer.url", MAX_XATTR_BYTES).for_decoder(MAX_XATTR_BYTES);
    xdg_download_fact(origin.as_deref(), referrer.as_deref())
}

pub(crate) fn last_opened(_path: &str) -> Fact<Usage> {
    Fact::unknown(UnknownWhy::NoRecord)
}

pub(crate) fn default_app(_path: &str) -> Fact<AppRef> {
    Fact::unknown(UnknownWhy::NoRecord)
}

pub(crate) fn signature(_path: &str, _target: AboutTarget, _extension: Option<&str>) -> Fact<Signature> {
    Fact::NotApplicable(NotApplicableWhy::NoCodeSigningOnThisOs)
}
