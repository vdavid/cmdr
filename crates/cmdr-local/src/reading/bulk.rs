//! The `getattrlistbulk` lister (macOS), the one APFS and HFS+ listings take:
//! one syscall per batch of entries instead of one `lstat` each, with the added
//! date in the same record.
//!
//! The record layout is `cmdr_fs::attr_buf`'s alone; this module owns the loop
//! and the policy. ❗ It never drops, invents, or reshapes an entry, so a listing
//! ends one of three ways:
//!
//! - Every record trusted: their entries.
//! - A record in doubt (errored, or a required attribute invalid): that one
//!   entry is its own `lstat`, exactly what `read_dir` would have built.
//! - The stream in doubt (a bad record, `EINTR`, a refusal `read_dir` survives):
//!   every bulk entry is discarded and the folder is listed again through
//!   `read_dir` from scratch. [`Failure`] carries no entries, so a partial or
//!   merged listing can't be returned.
//!
//! Whichever lister ran, a bulk-eligible folder's entries carry the added date,
//! so an Added sort never sees a hole a stream hiccup made. `DETAILS.md` § "The
//! local reader".

use std::collections::HashSet;
use std::ffi::{CString, OsStr};
use std::fs::{self, File, OpenOptions};
use std::io;
use std::os::fd::AsRawFd;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

use cmdr_fs::attr_buf::{self, Attr, BadRecord, Record, Request, Timespec, VDIR, VLNK, VREG, type_bits};
use cmdr_fs::entry::{FileEntry, get_group_name, get_owner_name, is_hidden_by_name};
use cmdr_fs::log_private::LogPrivate;

use super::{AddedAt, ListingTally, Pick, entry_from_lstat, get_single_entry, minimal_row, read_dir_entries};
use crate::statfs::statfs_string;

/// `SF_FIRMLINK` from `<sys/stat.h>`: the `st_flags` bit a firmlink carries.
/// `libc` doesn't carry it.
const SF_FIRMLINK: u32 = 0x0080_0000;

/// Everything a listing entry is built from.
const LISTING: Request = Request::new(&[
    Attr::Error,
    Attr::Name,
    Attr::ObjType,
    Attr::CrTime,
    Attr::ModTime,
    Attr::OwnerId,
    Attr::GrpId,
    Attr::AccessMask,
    Attr::Flags,
    Attr::FileId,
    Attr::AddedTime,
    Attr::DirMountStatus,
    Attr::FileLinkCount,
    Attr::FileAllocSize,
    Attr::FileDataLength,
]);

/// The added date alone, for one path.
const ADDED_TIME: Request = Request::new(&[Attr::AddedTime]);

/// The batch buffer: 64 KiB, measured flat from 16 to 512 KiB at 100k entries
/// (`docs/notes/getattrlistbulk-listing-benchmarks-2026-10.md`), in `u64`s so
/// the allocation is 8-aligned. Reads from it are unaligned anyway: the kernel
/// packs to 4 bytes, and `attr_buf` reads every value through `from_ne_bytes`.
const BUF_WORDS: usize = 64 * 1024 / 8;

/// Why a bulk stream can't be trusted, so the listing starts over through
/// `read_dir`. Logged, never shown.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Restart {
    /// The call failed with an errno `read_dir` may survive: `EACCES` (a readable
    /// but unsearchable folder fails the whole call where `read_dir` lists its
    /// names), `ENOTSUP` / `EINVAL`, or `EINTR`, which isn't retried because
    /// nothing says a resumed call can't skip a batch.
    Errno(i32),
    /// A record `attr_buf` couldn't locate or bound: nothing after it is trusted.
    BadRecord(BadRecord),
    /// A record with no name: there's nothing to stat.
    NoName,
    /// A record in doubt whose `lstat` failed too, with no valid type to build
    /// today's minimal row from: guessing its icon would invent a shape.
    Untyped,
}

/// How a bulk listing failed. Neither arm carries entries.
enum Failure {
    /// Re-list through `read_dir`.
    Restart(Restart),
    /// An error `read_dir` would hit too: it reaches the caller.
    Io(io::Error),
}

/// Every entry of `path`, unsorted: in bulk when `pick` says so (production:
/// the folder is bulk-eligible), through `read_dir` otherwise or when the bulk
/// stream turns out doubtful.
pub(super) fn read_entries(
    path: &Path,
    dot_hidden: &HashSet<String>,
    tally: Option<&ListingTally>,
    pick: Pick,
) -> io::Result<Vec<FileEntry>> {
    read_entries_with(path, dot_hidden, tally, pick, &mut kernel_batch)
}

/// [`read_entries`] with the batch source as a parameter, so a test can break
/// the stream partway through.
fn read_entries_with(
    path: &Path,
    dot_hidden: &HashSet<String>,
    tally: Option<&ListingTally>,
    pick: Pick,
    batches: &mut dyn FnMut(&File, &mut [u64]) -> io::Result<usize>,
) -> io::Result<Vec<FileEntry>> {
    // The same open `opendir` does, so access and TCC errors are today's.
    let dir = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY)
        .open(path)?;
    let eligible = fd_is_bulk_eligible(&dir);
    let added = if eligible { AddedAt::Fetch } else { AddedAt::Skip };
    let use_bulk = match pick {
        Pick::ByFilesystem => eligible,
        #[cfg(any(test, feature = "testing"))]
        Pick::Forced(lister) => lister == super::Lister::Bulk,
    };
    if use_bulk {
        match list(&dir, path, dot_hidden, tally, added, batches) {
            Ok(entries) => return Ok(entries),
            Err(Failure::Io(err)) => return Err(err),
            Err(Failure::Restart(why)) => {
                log::debug!(
                    "bulk listing of {} re-lists through read_dir: {why:?}",
                    LogPrivate::dir_path(path)
                );
                if let Some(tally) = tally {
                    tally.reset();
                }
            }
        }
    }
    drop(dir);
    read_dir_entries(path, dot_hidden, tally, added)
}

/// The whole bulk stream of `dir`, or why it can't be trusted.
fn list(
    dir: &File,
    path: &Path,
    dot_hidden: &HashSet<String>,
    tally: Option<&ListingTally>,
    added: AddedAt,
    batches: &mut dyn FnMut(&File, &mut [u64]) -> io::Result<usize>,
) -> Result<Vec<FileEntry>, Failure> {
    let mut buf = vec![0u64; BUF_WORDS];
    let mut entries = Vec::new();
    loop {
        let count = match batches(dir, &mut buf) {
            Ok(0) => return Ok(entries),
            Ok(count) => count,
            Err(err) => {
                return Err(match err.raw_os_error() {
                    Some(errno @ (libc::EINTR | libc::EACCES | libc::ENOTSUP | libc::EINVAL)) => {
                        Failure::Restart(Restart::Errno(errno))
                    }
                    _ => Failure::Io(err),
                });
            }
        };
        for record in attr_buf::records(attr_buf::words_as_bytes(&buf), count, &LISTING) {
            let record = record.map_err(|bad| Failure::Restart(Restart::BadRecord(bad)))?;
            let entry = entry_from_record(&record, path, dot_hidden, added).map_err(Failure::Restart)?;
            if let Some(tally) = tally {
                tally.record(&entry);
            }
            entries.push(entry);
        }
    }
}

/// One `getattrlistbulk` call into `buf`: how many records it wrote, 0 at the
/// end of the folder.
fn kernel_batch(dir: &File, buf: &mut [u64]) -> io::Result<usize> {
    let mut attrlist = LISTING.attrlist();
    // SAFETY: `dir` is an open directory fd for the call's duration (the `File`
    // owns it); `attrlist` is a valid `struct attrlist` we own; `buf` is
    // writable for exactly the byte length passed, and the kernel writes no more.
    let count = unsafe {
        libc::getattrlistbulk(
            dir.as_raw_fd(),
            (&raw mut attrlist).cast(),
            buf.as_mut_ptr().cast(),
            size_of_val(buf),
            u64::from(libc::FSOPT_PACK_INVAL_ATTRS),
        )
    };
    usize::try_from(count).map_err(|_| io::Error::last_os_error())
}

/// The attributes an entry is built from, present only when every one the
/// record must carry is valid.
struct Trusted {
    obj_type: u32,
    modtime: Timespec,
    owner_id: u32,
    group_id: u32,
    access_mask: u32,
    flags: u32,
    file_id: u64,
    /// A folder's mount status; `None` for anything else.
    mount_status: Option<u32>,
    /// A non-folder's link count, allocated size, and length; `None` for a folder.
    file: Option<(u32, u64, u64)>,
}

impl Trusted {
    /// The record's attributes, or `None` when it's errored or any required one
    /// is invalid. The birth and added times are optional: invalid means `None`.
    fn of(record: &Record<'_>) -> Option<Self> {
        if record.error != 0 {
            return None;
        }
        let obj_type = record.obj_type?;
        let is_dir = obj_type == VDIR;
        Some(Self {
            obj_type,
            modtime: record.modtime?,
            owner_id: record.owner_id?,
            group_id: record.group_id?,
            access_mask: record.access_mask?,
            flags: record.flags?,
            file_id: record.file_id?,
            mount_status: if is_dir { Some(record.mount_status?) } else { None },
            file: if is_dir {
                None
            } else {
                Some((record.link_count?, record.alloc_size?, record.data_length?))
            },
        })
    }

    /// A mount point or a firmlink: the record describes the folder it COVERS
    /// (the stub on the underlying volume), not what `lstat` sees there.
    fn covers_another_folder(&self) -> bool {
        self.mount_status
            .is_some_and(|status| status & libc::DIR_MNTSTATUS_MNTPOINT != 0)
            || (self.obj_type == VDIR && self.flags & SF_FIRMLINK != 0)
    }
}

/// The entry one record describes, in `dir`.
fn entry_from_record(
    record: &Record<'_>,
    dir: &Path,
    dot_hidden: &HashSet<String>,
    added: AddedAt,
) -> Result<FileEntry, Restart> {
    let raw_name = OsStr::from_bytes(record.name.ok_or(Restart::NoName)?);
    let path = dir.join(raw_name);
    let name = raw_name.to_string_lossy().into_owned();

    let Some(attrs) = Trusted::of(record) else {
        // In doubt: the entry's own `lstat` is today's answer. When that fails,
        // today's minimal row needs to know whether it's a link, which only an
        // unerrored record's valid type can say.
        let hint = if record.error == 0 { record.obj_type } else { None };
        return match entry_from_lstat(&path, name.clone(), dot_hidden, added) {
            Some(entry) => Ok(entry),
            None => hint
                .map(|obj_type| minimal_row(&path, name, obj_type == VLNK, dot_hidden))
                .ok_or(Restart::Untyped),
        };
    };

    if attrs.covers_another_folder() {
        // Wholly the re-stat, added date included, as `read_dir` would list it;
        // the minimal row if that fails, ❌ never the stub.
        return Ok(match get_single_entry(&path) {
            Ok(entry) => FileEntry {
                is_hidden: entry.is_hidden || dot_hidden.contains(&entry.name),
                ..entry
            },
            Err(_) => minimal_row(&path, name, false, dot_hidden),
        });
    }

    // `getattrlistbulk` never follows a link: a link's folderness is its
    // target's, one `stat` away, while everything else stays the link's own.
    let is_symlink = attrs.obj_type == VLNK;
    let target_is_dir = is_symlink && fs::metadata(&path).is_ok_and(|m| m.is_dir());
    let is_dir = attrs.obj_type == VDIR || target_is_dir;
    // `Metadata::is_file()`: a regular file, never a link, device, or socket.
    let regular = attrs.file.filter(|_| attrs.obj_type == VREG);

    Ok(FileEntry {
        size: regular.map(|(_, _, length)| length),
        physical_size: regular.map(|(_, alloc_size, _)| alloc_size),
        // See `entry_from_metadata`: hardlinked files only.
        inode: regular.filter(|&(links, _, _)| links > 1).map(|_| attrs.file_id),
        modified_at: attrs.modtime.secs_since_epoch(),
        created_at: record.crtime.and_then(Timespec::secs_since_epoch),
        added_at: record.added_time.and_then(Timespec::secs_since_epoch),
        // APFS's mask already carries `S_IFMT`; masking makes it right either way.
        permissions: (attrs.access_mask & 0o7777) | type_bits(attrs.obj_type),
        owner: get_owner_name(attrs.owner_id),
        group: get_group_name(attrs.group_id),
        is_hidden: is_hidden_by_name(&name) || attrs.flags & libc::UF_HIDDEN != 0 || dot_hidden.contains(&name),
        ..FileEntry::new(name, path.to_string_lossy().into_owned(), is_dir, is_symlink)
    })
}

/// Whether a folder on this filesystem lists in bulk: APFS and HFS+ only, where
/// it was measured faster and field-for-field exact. An allowlist, so an
/// unknown filesystem keeps the proven `read_dir` path; smbfs (bulk re-enumerates
/// over the wire where `lstat` hits a cache: 2.2× slower), exFAT and FAT (VFS
/// emulation, nothing gained, FAT's allocated size reads 0), and NFS stay there.
pub(super) fn bulk_eligible(stat: &libc::statfs) -> bool {
    matches!(statfs_string(&stat.f_fstypename).as_str(), "apfs" | "hfs")
}

/// [`bulk_eligible`] for the filesystem an open folder lives on (about a
/// microsecond). `false` if `fstatfs` fails, which keeps `read_dir`.
fn fd_is_bulk_eligible(dir: &File) -> bool {
    let mut stat = std::mem::MaybeUninit::<libc::statfs>::uninit();
    // SAFETY: `dir` owns an open fd for the call's duration, and `stat` is a
    // correctly typed out-buffer the kernel fills on success.
    let rc = unsafe { libc::fstatfs(dir.as_raw_fd(), stat.as_mut_ptr()) };
    // SAFETY: `fstatfs` returned 0, so the kernel fully initialized `stat`.
    rc == 0 && bulk_eligible(unsafe { stat.assume_init_ref() })
}

/// Whether the folder holding `path` lists in bulk, the question
/// `get_single_entry` asks so a re-stat carries the added date its folder's
/// listing does. Asks the PARENT: `statfs` on the path itself would follow a
/// final symlink to wherever it points.
pub(super) fn parent_is_bulk_eligible(path: &Path) -> bool {
    match path.parent() {
        Some(parent) if parent.as_os_str().is_empty() => dir_is_bulk_eligible(Path::new(".")),
        Some(parent) => dir_is_bulk_eligible(parent),
        None => dir_is_bulk_eligible(path),
    }
}

/// Whether a listing of the folder `dir` takes the bulk lister: one `statfs`,
/// which follows a link the way opening the folder does, so it answers for the
/// filesystem the listing's `fstatfs` sees. `false` if `statfs` fails.
pub(super) fn dir_is_bulk_eligible(dir: &Path) -> bool {
    let Ok(c_path) = CString::new(dir.as_os_str().as_bytes()) else {
        return false;
    };
    let mut stat = std::mem::MaybeUninit::<libc::statfs>::uninit();
    // SAFETY: `c_path` is NUL-terminated and outlives the call, and `stat` is a
    // correctly typed out-buffer the kernel fills on success.
    let rc = unsafe { libc::statfs(c_path.as_ptr(), stat.as_mut_ptr()) };
    // SAFETY: `statfs` returned 0, so the kernel fully initialized `stat`.
    rc == 0 && bulk_eligible(unsafe { stat.assume_init_ref() })
}

/// `path`'s added date from one `getattrlist`, without following a link: the
/// value a bulk record of its folder carries. `None` when the call fails or the
/// filesystem doesn't record one.
pub(super) fn added_at_by_path(path: &Path) -> Option<u64> {
    let c_path = CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut attrlist = ADDED_TIME.attrlist();
    // Header (24 bytes) plus one `timespec` (16): 40 of these 64.
    let mut buf = [0u64; 8];
    // SAFETY: `c_path` is NUL-terminated and outlives the call; `attrlist` is a
    // valid `struct attrlist` we own; `buf` is writable for exactly the byte
    // length passed, and the kernel writes no more.
    let rc = unsafe {
        libc::getattrlist(
            c_path.as_ptr(),
            (&raw mut attrlist).cast(),
            buf.as_mut_ptr().cast(),
            size_of_val(&buf),
            libc::FSOPT_NOFOLLOW | libc::FSOPT_PACK_INVAL_ATTRS,
        )
    };
    if rc != 0 {
        return None;
    }
    attr_buf::decode_record(attr_buf::words_as_bytes(&buf), &ADDED_TIME)
        .ok()?
        .added_time?
        .secs_since_epoch()
}

#[cfg(test)]
#[path = "bulk_test.rs"]
mod tests;
