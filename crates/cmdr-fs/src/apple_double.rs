//! AppleDouble sidecars: the `._name` files macOS keeps beside `name` on a volume
//! that can't store extended attributes itself (FAT, exFAT, an SMB share without
//! named streams).
//!
//! On such a volume the KERNEL owns `._name`: `getxattr` on `name` reads it,
//! unlinking or renaming `name` takes it along, and `rmdir` sweeps a folder that
//! holds nothing else (xnu `kpi_vfs.c` `VNOP_REMOVE` / `VNOP_RENAME` and
//! `vfs_subr.c` `rmdir_remove_orphaned_appleDouble`, all under
//! `CONFIG_APPLEDOUBLE`). A copy that carries `name`'s xattrs therefore already
//! carries everything in it, and a walk that ALSO copies `._name` lands a stray
//! 4,096-byte hidden file on APFS that Finder never makes.
//!
//! The rule is the kernel's own pairing test (`dot_underbar_check_paired_vnode`,
//! xnu `vfs_subr.c`): a name longer than `._` that starts with `._`, whose sibling
//! without the prefix exists in the same directory, on a mount with no native
//! xattr support (`VOL_CAP_INT_EXTENDED_ATTR` clear, the bit xnu turns into
//! `MNTK_EXTENDED_ATTRS`). Everything else is an ordinary file and copies: a real
//! `._foo` on APFS, an orphan `._foo` whose `foo` is gone (nothing reads it as
//! xattrs, so it is the only copy of whatever it holds), and every `._` file on
//! Linux, where no kernel layer serves it.
//!
//! `copyfile(3)` itself never skips one (its `COPYFILE_RECURSIVE` walk copies
//! every entry; checked against apple-oss-distributions/copyfile `copyfile.c`,
//! 2026-10-12), so this is ours to decide; Finder doesn't copy them either.

use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::Path;
use std::sync::Mutex;

use crate::ignore_poison::IgnorePoison;

/// The pairing test, given whether the directory's volume keeps sidecars and a
/// way to ask whether a sibling exists. Pure, so the rule is testable without a
/// FAT volume, which no automated test may attach.
///
/// `sibling_exists` is asked only for a `._`-prefixed name on a sidecar-keeping
/// volume, with the name minus its prefix.
pub fn is_sidecar_name(name: &OsStr, volume_keeps_sidecars: bool, sibling_exists: impl FnOnce(&OsStr) -> bool) -> bool {
    if !volume_keeps_sidecars {
        return false;
    }
    match name.as_encoded_bytes().strip_prefix(b"._") {
        // SAFETY: `sibling` is `name` minus a leading ASCII `._`, which splits it
        // at an ASCII boundary, so it is valid encoded bytes of the same encoding.
        Some(sibling) if !sibling.is_empty() => sibling_exists(unsafe { OsStr::from_encoded_bytes_unchecked(sibling) }),
        _ => false,
    }
}

/// Whether the kernel keeps AppleDouble sidecars on the volume holding `path`:
/// `true` exactly when the volume reports no native xattr support.
///
/// An unanswerable probe says `false`, so a doubt copies the file rather than
/// dropping it. Off macOS there is no such kernel layer, so always `false`.
pub fn volume_keeps_sidecars(path: &Path) -> bool {
    #[cfg(target_os = "macos")]
    {
        native_xattrs(path).is_some_and(|native| !native)
    }
    #[cfg(any(target_os = "linux", target_os = "windows", target_family = "wasm"))]
    {
        let _ = path;
        false
    }
}

/// Reads `VOL_CAP_INT_EXTENDED_ATTR` off the volume holding `path`, or `None`
/// when `getattrlist` refuses or the volume doesn't vouch for the bit.
#[cfg(target_os = "macos")]
fn native_xattrs(path: &Path) -> Option<bool> {
    use std::os::unix::ffi::OsStrExt;

    #[repr(C, packed(4))]
    struct VolCapsBuf {
        length: u32,
        caps: libc::vol_capabilities_attr_t,
    }

    let c_path = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut attrs = libc::attrlist {
        bitmapcount: libc::ATTR_BIT_MAP_COUNT,
        reserved: 0,
        commonattr: 0,
        volattr: libc::ATTR_VOL_INFO | libc::ATTR_VOL_CAPABILITIES,
        dirattr: 0,
        fileattr: 0,
        forkattr: 0,
    };
    // SAFETY: `VolCapsBuf` is plain integers, for which all-zero is a valid value.
    let mut buf: VolCapsBuf = unsafe { std::mem::zeroed() };
    // SAFETY: `c_path` is a NUL-terminated string that outlives the call, `attrs`
    // asks for exactly the volume-capabilities group, and `buf` is a writable
    // buffer of the size passed, laid out as the kernel packs that answer (a
    // `u32` length, then the `vol_capabilities_attr_t`).
    let rc = unsafe {
        libc::getattrlist(
            c_path.as_ptr(),
            (&raw mut attrs).cast(),
            (&raw mut buf).cast(),
            size_of::<VolCapsBuf>(),
            0,
        )
    };
    if rc != 0 {
        return None;
    }
    let caps = buf.caps;
    let interfaces = libc::VOL_CAPABILITIES_INTERFACES;
    let bit = libc::VOL_CAP_INT_EXTENDED_ATTR;
    if caps.valid[interfaces] & bit == 0 {
        return None;
    }
    Some(caps.capabilities[interfaces] & bit != 0)
}

/// The pairing test as one walk asks it, entry by entry: the volume probe runs
/// once per device and only when a `._` name turns up, so a walk over APFS
/// never pays for it.
pub struct SidecarRule {
    probe: Box<dyn Fn(&Path) -> bool + Send + Sync>,
    keeps_sidecars_by_device: Mutex<HashMap<u64, bool>>,
}

impl SidecarRule {
    /// The rule against the real volume probe, [`volume_keeps_sidecars`].
    pub fn new() -> Self {
        Self::with_probe(volume_keeps_sidecars)
    }

    /// The rule against an injected probe, so a test on APFS can play a FAT
    /// volume.
    pub fn with_probe(probe: impl Fn(&Path) -> bool + Send + Sync + 'static) -> Self {
        Self {
            probe: Box::new(probe),
            keeps_sidecars_by_device: Mutex::new(HashMap::new()),
        }
    }

    /// Whether `name`, listed in `dir` (on device `dir_device`), is a sidecar
    /// the kernel serves as its sibling's xattrs, so a walk leaves it out.
    pub fn is_sidecar(&self, dir: &Path, dir_device: u64, name: &OsStr) -> bool {
        if !name.as_encoded_bytes().starts_with(b"._") {
            return false;
        }
        let keeps = *self
            .keeps_sidecars_by_device
            .lock_ignore_poison()
            .entry(dir_device)
            .or_insert_with(|| (self.probe)(dir));
        // `symlink_metadata` resolves the sibling the way the kernel's own lookup
        // does, case folding included on a case-insensitive FAT.
        is_sidecar_name(name, keeps, |sibling| {
            std::fs::symlink_metadata(dir.join(sibling)).is_ok()
        })
    }
}

impl Default for SidecarRule {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "apple_double_test.rs"]
mod tests;
