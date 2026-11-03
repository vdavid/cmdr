//! The per-path probes the benchmark prices beside the listers: `getxattr` for
//! Finder's last-used date (#694), and what a volume says it supports. The
//! listers themselves are production's (`reading::list_directory_with`).

use std::ffi::CString;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

/// The Finder "Last opened" xattr. 16 bytes: a little-endian `struct timespec`
/// (`i64` seconds, `i64` nanoseconds).
pub const LAST_USED_XATTR: &[u8] = b"com.apple.lastuseddate#PS\0";

/// Reads `com.apple.lastuseddate#PS` without following symlinks. `None` when
/// absent (`ENOATTR`) or malformed.
pub fn read_last_used(path: &Path) -> Option<u64> {
    let c_path = CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut buf = [0u8; 16];
    // SAFETY: both strings are NUL-terminated; `buf` is writable for 16 bytes.
    let n = unsafe {
        libc::getxattr(
            c_path.as_ptr(),
            LAST_USED_XATTR.as_ptr().cast(),
            buf.as_mut_ptr().cast(),
            buf.len(),
            0,
            libc::XATTR_NOFOLLOW,
        )
    };
    if n != 16 {
        return None;
    }
    u64::try_from(i64::from_le_bytes(buf[..8].try_into().ok()?)).ok()
}

/// What a volume says about the attributes the bulk lister relies on: whether
/// `getattrlist` is native (`VOL_CAP_INT_ATTRLIST`), and whether
/// `ATTR_CMN_ADDEDTIME` is valid / native per `ATTR_VOL_ATTRIBUTES`.
pub struct VolumeAttrSupport {
    pub attrlist_native: bool,
    pub addedtime_valid: bool,
    pub addedtime_native: bool,
    pub flags_valid: bool,
}

pub fn volume_attr_support(path: &Path) -> io::Result<VolumeAttrSupport> {
    let c_path = CString::new(path.as_os_str().as_bytes())?;
    let mut al = libc::attrlist {
        bitmapcount: libc::ATTR_BIT_MAP_COUNT,
        reserved: 0,
        commonattr: 0,
        volattr: libc::ATTR_VOL_INFO | libc::ATTR_VOL_CAPABILITIES | libc::ATTR_VOL_ATTRIBUTES,
        dirattr: 0,
        fileattr: 0,
        forkattr: 0,
    };
    let mut buf = [0u64; 64];
    // SAFETY: `al` is a valid `attrlist`, `buf` is writable for 512 bytes.
    let rc = unsafe { libc::getattrlist(c_path.as_ptr(), (&raw mut al).cast(), buf.as_mut_ptr().cast(), 512, 0) };
    if rc != 0 {
        return Err(io::Error::last_os_error());
    }
    let b = cmdr_fs::attr_buf::words_as_bytes(&buf);
    // A volume-attributes buffer, not a record `cmdr_fs::attr_buf` decodes: u32
    // length, then vol_capabilities_attr_t (capabilities[4] + valid[4], u32 each),
    // then vol_attributes_attr_t (validattr + nativeattr, attribute_set_t each).
    let caps_off = 4;
    let interfaces_valid = read_u32(b, caps_off + 16 + 4 * libc::VOL_CAPABILITIES_INTERFACES)?;
    let interfaces = read_u32(b, caps_off + 4 * libc::VOL_CAPABILITIES_INTERFACES)?;
    let attrs_off = caps_off + 32;
    let valid_common = read_u32(b, attrs_off)?;
    let native_common = read_u32(b, attrs_off + 20)?;
    Ok(VolumeAttrSupport {
        attrlist_native: interfaces_valid & libc::VOL_CAP_INT_ATTRLIST != 0
            && interfaces & libc::VOL_CAP_INT_ATTRLIST != 0,
        addedtime_valid: valid_common & libc::ATTR_CMN_ADDEDTIME != 0,
        addedtime_native: native_common & libc::ATTR_CMN_ADDEDTIME != 0,
        flags_valid: valid_common & libc::ATTR_CMN_FLAGS != 0,
    })
}

fn read_u32(b: &[u8], at: usize) -> io::Result<u32> {
    b.get(at..at + 4)
        .map(|s| u32::from_le_bytes(s.try_into().expect("4 bytes")))
        .ok_or_else(|| io::Error::other("truncated u32"))
}
