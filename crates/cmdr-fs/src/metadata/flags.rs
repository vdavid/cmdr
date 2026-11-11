//! The user flags a copy carries: a small typed set over the BSD `st_flags`
//! bits, so nothing outside this file handles a raw flag word.

use std::ops::BitOr;
use std::path::Path;

use super::ClassOutcome;
#[cfg(target_os = "macos")]
use crate::log_private::LogPrivate;

/// A set of the user-settable flags Finder shows. `SF_*` system flags are never
/// in it: a non-root process can't set or clear them, and a copy never tries.
// DEFAULT-OK: the empty set says "no flags to add or clear", which is what a
// default means wherever one is built: nothing claims the item has none.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UserFlags(u8);

impl UserFlags {
    /// No flags.
    pub const NONE: Self = Self(0);
    /// `UF_HIDDEN`: Finder's Hidden.
    pub const HIDDEN: Self = Self(1);
    /// `UF_IMMUTABLE` (`uchg`): Finder's Locked. Refuses a rename and an unlink.
    pub const LOCKED: Self = Self(1 << 1);
    /// `UF_APPEND` (`uappnd`). Refuses a rename and an unlink like Locked.
    pub const APPEND_ONLY: Self = Self(1 << 2);
    /// `UF_NODUMP`.
    pub const NO_DUMP: Self = Self(1 << 3);
    /// The flags that refuse a rename or an unlink of the item wearing them.
    pub const RENAME_BLOCKERS: Self = Self(Self::LOCKED.0 | Self::APPEND_ONLY.0);

    /// Whether the set is empty.
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Whether every flag in `other` is in this set.
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// The flags of this set that are also in `other`.
    pub const fn intersection(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }

    /// This set without the flags in `other`.
    pub const fn without(self, other: Self) -> Self {
        Self(self.0 & !other.0)
    }
}

impl BitOr for UserFlags {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

#[cfg(target_os = "macos")]
impl UserFlags {
    const BSD_BITS: [(UserFlags, u32); 4] = [
        (UserFlags::HIDDEN, libc::UF_HIDDEN),
        (UserFlags::LOCKED, libc::UF_IMMUTABLE),
        (UserFlags::APPEND_ONLY, libc::UF_APPEND),
        (UserFlags::NO_DUMP, libc::UF_NODUMP),
    ];

    /// The set's bits out of a raw `st_flags` word; every other bit is ignored.
    pub(super) fn from_bsd(raw: u32) -> Self {
        Self::BSD_BITS
            .iter()
            .filter(|(_, bit)| raw & bit != 0)
            .fold(Self::NONE, |set, (flag, _)| set | *flag)
    }

    /// The raw `st_flags` bits of this set.
    pub(super) fn to_bsd(self) -> u32 {
        Self::BSD_BITS
            .iter()
            .filter(|(flag, _)| self.contains(*flag))
            .fold(0, |raw, (_, bit)| raw | bit)
    }
}

/// `path`'s own flags word (never a symlink's target's).
#[cfg(target_os = "macos")]
pub(super) fn read_raw(path: &Path) -> std::io::Result<u32> {
    use std::os::macos::fs::MetadataExt as _;
    std::fs::symlink_metadata(path).map(|meta| meta.st_flags())
}

// `lchflags(2)` is in libSystem, but the `libc` this workspace locks doesn't
// declare it yet.
#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn lchflags(path: *const libc::c_char, flags: libc::c_uint) -> libc::c_int;
}

/// Sets `path`'s own flags word to exactly `raw` (`lchflags`, never following a link).
#[cfg(target_os = "macos")]
pub(super) fn write_raw(path: &Path, raw: u32) -> std::io::Result<()> {
    use std::os::unix::ffi::OsStrExt as _;
    let c_path = std::ffi::CString::new(path.as_os_str().as_bytes())
        .map_err(|_| std::io::Error::from(std::io::ErrorKind::InvalidInput))?;
    // SAFETY: `c_path` is a NUL-terminated path that outlives the call, `lchflags`
    // reads nothing else, and the declaration above matches its libSystem signature.
    if unsafe { lchflags(c_path.as_ptr(), raw) } == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

/// Adds `flags` to what `path` already wears.
#[cfg(target_os = "macos")]
pub(super) fn add_flags(path: &Path, flags: UserFlags) -> ClassOutcome {
    match read_raw(path).and_then(|raw| write_raw(path, raw | flags.to_bsd())) {
        Ok(()) => ClassOutcome::Kept,
        Err(e) => {
            log::debug!(target: "metadata", "couldn't set the flags on {}: {e}", LogPrivate::path(path));
            ClassOutcome::Refused
        }
    }
}

/// Linux's `chattr` flags need `CAP_LINUX_IMMUTABLE`, and Hidden is a dot-name.
#[cfg(any(target_os = "linux", target_family = "wasm"))]
pub(super) fn add_flags(_path: &Path, _flags: UserFlags) -> ClassOutcome {
    ClassOutcome::NotApplicable
}
