//! The per-OS bodies of the metadata classes: what each OS reads and writes for the mode, the
//! xattrs, the ACL, the user flags, the dates, and the removal blockers. The ORDER the classes
//! go on in, and what each outcome means, stay in the class modules above (`staged.rs`,
//! `mod.rs`'s `apply_landed`, `blockers.rs`); everything here answers one class on one item.
//!
//! Windows has no arm bodies yet (`windows.rs`): a Windows build fails here, at each class it
//! still has to answer, rather than inheriting another OS's.

use std::ffi::OsString;
use std::fs::{File, Metadata};
use std::path::Path;
use std::time::SystemTime;

use super::items::{StagedItem, StagedTimes, Xattr};
use super::{ClassOutcome, MetadataCaps, ParentOpening, RemovalLock, Unblock, UserFlags};

#[cfg(unix)]
mod unix;

#[cfg_attr(target_os = "macos", path = "macos.rs")]
#[cfg_attr(target_os = "linux", path = "linux.rs")]
#[cfg_attr(target_os = "windows", path = "windows.rs")]
#[cfg_attr(target_family = "wasm", path = "wasm.rs")]
mod imp;

const _: fn(&Metadata) -> Option<u32> = mode_of;
const _: fn(&Path, u32) -> ClassOutcome = write_mode_at;
const _: fn(StagedItem<'_>, u32) -> ClassOutcome = write_mode_staged;
const _: fn() -> u32 = probe_umask;
const _: fn(&OsString) -> bool = carries_xattr;
const _: fn(&File) -> Vec<Xattr> = local_xattrs;
const _: fn(&File) -> Vec<Xattr> = local_xattr_names;
const _: fn(&Path) -> Vec<Xattr> = xattrs_at;
const _: fn(StagedItem<'_>, &[Xattr]) -> ClassOutcome = write_xattrs;
const _: fn(&Path) -> Option<Acl> = read_acl;
const _: fn(&Path, &Acl) -> ClassOutcome = write_acl;
const _: fn(&Metadata) -> UserFlags = flags_of;
const _: fn(&Path, UserFlags) -> ClassOutcome = add_flags;
const _: fn(&Path) -> Unblock = clear_blockers;
const _: fn(&Path) -> RemovalLock = removal_lock;
const _: fn(&Path) -> ParentOpening = open_parent_folder;
const _: bool = CREATED_SETTABLE;
const _: MetadataCaps = LOCAL_DISK_CAPS;
const _: fn(StagedItem<'_>, Option<SystemTime>, StagedTimes) -> std::io::Result<()> = set_dates;
const _: fn(&std::io::Error) -> bool = imp::is_unsupported;

/// An ACL as the helper carries it: opaque entries, read and written whole.
pub use imp::Acl;

/// Whether this OS lets a copy set a birth date.
pub(super) use imp::CREATED_SETTABLE;
/// What local disk keeps of a copy routed through the volume engine, and what its streams
/// report off the open's `stat` and fd (`cmdr-local`'s `LocalPosixVolume::METADATA_CAPS` is
/// this). Every user flag
/// waits for the landing (`staged_flags` is empty): the copy puts them on once the file has
/// its final name, since Locked refuses the rename and the set goes on as one.
pub use imp::LOCAL_DISK_CAPS;
/// Adds `flags` to what `path` already wears.
pub(super) use imp::add_flags;
/// Whether a copy carries the xattr `name` (macOS: everything but provenance; Linux: only
/// `user.*`).
pub(super) use imp::carries_xattr;
/// Clears `path`'s own rename and unlink blockers (`blockers.rs`).
pub(super) use imp::clear_blockers;
/// The user flags a stat shows.
pub(super) use imp::flags_of;
/// The names of those xattrs, each with an empty value.
pub(super) use imp::local_xattr_names;
/// The xattrs worth carrying off the open `file`. A file with none costs one `flistxattr`.
pub(super) use imp::local_xattrs;
/// A local item's permission bits (`0o7777`), off the stat the caller holds.
pub(super) use imp::mode_of;
/// Opens the folder holding `child` for one removal (`blockers.rs`).
pub(super) use imp::open_parent_folder;
/// The process umask, probed once (a throwaway `0777` folder).
pub(super) use imp::probe_umask;
/// The source's ACL, without following a link; `None` when it has none worth carrying.
pub(super) use imp::read_acl;
/// Whether `path` itself wears a removal lock (`blockers.rs`).
pub(super) use imp::removal_lock;
/// Birth, mtime, and atime in one call.
pub(super) use imp::set_dates;
/// The ACL onto the landed item.
pub(super) use imp::write_acl;
/// `chmod` by path. Only ever a folder, which isn't a link, so following one can't reach
/// anywhere else.
pub(super) use imp::write_mode_at;
/// The mode on a staged item (through its fd where it has one); a link takes none.
pub(super) use imp::write_mode_staged;
/// Each carried xattr onto the staged item, one class: `Refused` if any one was.
pub(super) use imp::write_xattrs;
/// The xattrs a copy carries off the item at `path` itself, never following a link.
pub(super) use imp::xattrs_at;

/// macOS: a removal lock's verdict from a raw flags word, for the cells that pin it.
#[cfg(all(test, target_os = "macos"))]
pub(super) use imp::lock_of;

/// What a failed class write means: `Unsupported`, or this OS's own "can't hold this"
/// (the arm's `is_unsupported`), is a destination that can't hold the class at all, a
/// different answer from refusing a value.
pub(super) fn refusal(e: &std::io::Error) -> ClassOutcome {
    if imp::is_unsupported(e) || e.kind() == std::io::ErrorKind::Unsupported {
        ClassOutcome::NotApplicable
    } else {
        ClassOutcome::Refused
    }
}
