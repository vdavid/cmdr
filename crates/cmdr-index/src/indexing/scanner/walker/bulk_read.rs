//! macOS bulk directory reader via `getattrlistbulk(2)`.
//!
//! A local walk's dominant cost is one `lstat` per entry (measured: ~93% of the
//! filesystem-walk time, ~15 µs/entry — millions of syscalls on a full disk).
//! `getattrlistbulk` returns a batch of directory entries *with* their attributes
//! (name, type, size, allocated size, mtime, inode, link count) in a single
//! syscall per batch, so the visitor never stats an entry individually. This is
//! how the former third-party directory-walking crate enumerated on macOS; here it also carries the sizes
//! inline, eliminating the separate stat the old scanner still paid.
//!
//! Each batch is published through [`super::ReadProgress`] as it arrives, which is
//! how the walker's watchdog knows this read is working rather than hung.
//!
//! # Correctness is fallback-protected
//!
//! The packed record layout is `cmdr_fs::attr_buf`'s: this module asks for its
//! [`REQUEST`] and decodes through `attr_buf::records`, so no offset is written
//! down here. What a missing attribute MEANS is this module's policy, and it's
//! built to make a miss safe. An attribute the stat needs that wasn't returned
//! (or an unclassifiable type) sends that single entry back with no inline stat,
//! and the caller falls back to `symlink_metadata` for it — the slower-but-correct
//! path, never wrong data. A record with no recoverable name or type, or one the
//! decoder can't trust at all (and every record after it in the batch, since
//! nothing past it can be located), is counted in [`BulkDirRead::unusable`], and
//! both callers then re-read the whole directory with `read_dir` rather than use a
//! short listing. An `EINTR` does the same: the stream is never resumed on its fd
//! (`walker/DETAILS.md` § "The bulk reader"). The `bulk_matches_symlink_metadata` differential test asserts the
//! decoded values equal `std::fs::symlink_metadata` field-for-field across a rich
//! tree (files with known sizes, an empty dir, a symlink, a hardlink, a unicode
//! name), so a decoding bug is a failing test, not a silent corruption.

use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

use cmdr_fs::attr_buf::{self, Attr, Record, Request};
use cmdr_fs::log_private::LogPrivate;

use super::{InlineStat, RawDirEntry, RawFileType, ReadProgress};

/// The attributes the index asks for: everything a [`RawDirEntry`] and its
/// [`InlineStat`] carry.
const REQUEST: Request = Request::new(&[
    Attr::Name,
    Attr::ObjType,
    Attr::ModTime,
    Attr::FileId,
    Attr::FileLinkCount,
    Attr::FileAllocSize,
    Attr::FileDataLength,
]);

/// Read buffer per `getattrlistbulk` call. 64 KiB fits many entries per syscall.
const ATTR_BUF_SIZE: usize = 64 * 1024;

/// One directory's bulk read: the entries, plus how many records didn't become one.
pub(in crate::indexing) struct BulkDirRead {
    pub entries: Vec<RawDirEntry>,
    /// Records missing from `entries` entirely, because nothing usable could be
    /// read out of them (see [`decode_batch`]). Zero on every filesystem we've
    /// measured. A caller that can't survive a missing child — the reconcile diff
    /// DELETES index rows the live listing lacks — re-reads the directory with
    /// `read_dir` when this isn't zero.
    pub unusable: usize,
    /// A `getattrlistbulk` call failed with `EINTR`, so the stream is in doubt and
    /// `entries` is empty. Nothing says a call resumed on the same fd can't skip a
    /// batch, so neither caller resumes: both re-read the directory whole.
    pub interrupted: bool,
}

impl BulkDirRead {
    /// Why this read can't be used as the directory's listing, if it can't: the
    /// caller re-reads the directory whole through `read_dir` instead.
    pub(in crate::indexing) fn doubt(&self) -> Option<ReadDoubt> {
        if self.interrupted {
            Some(ReadDoubt::Interrupted)
        } else if self.unusable > 0 {
            Some(ReadDoubt::Unusable(self.unusable))
        } else {
            None
        }
    }
}

/// Why a [`BulkDirRead`] isn't a whole listing. Logged, never shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::indexing) enum ReadDoubt {
    /// This many records didn't become an entry.
    Unusable(usize),
    /// The stream was interrupted (`EINTR`) partway through.
    Interrupted,
}

/// Bulk directory reader: the production macOS [`super::ReadDirFn`]. Reads
/// `path`'s children with their attributes via `getattrlistbulk`, falling back to
/// `symlink_metadata` per entry when an attribute is missing, and to a whole
/// [`super::std_read_dir`] when a record couldn't be read at all. Fails
/// (propagating the `io::Error`) only when the directory itself can't be read —
/// matching `std::fs::read_dir`'s contract, so the walker treats it like any
/// unlistable dir.
pub(super) fn bulk_read_dir(path: &Path, progress: &ReadProgress) -> std::io::Result<Vec<RawDirEntry>> {
    let read = bulk_read_dir_inner(path, progress)?;
    complete_or_reread(path, progress, read)
}

/// `read`'s entries when it's a whole listing; otherwise the directory read again
/// whole through [`super::std_read_dir`]. A short listing would leave a real child
/// out of the scan: no row, while its parent is stamped listed.
fn complete_or_reread(path: &Path, progress: &ReadProgress, read: BulkDirRead) -> std::io::Result<Vec<RawDirEntry>> {
    let Some(doubt) = read.doubt() else {
        return Ok(read.entries);
    };
    log_reread("scan", path, doubt);
    super::std_read_dir(path, progress)
}

/// The one log line both callers write when a bulk read is re-read whole.
pub(in crate::indexing) fn log_reread(caller: &str, path: &Path, doubt: ReadDoubt) {
    match doubt {
        ReadDoubt::Unusable(n) => log::warn!(
            "{caller}: bulk read of {} couldn't decode {}, re-reading it with read_dir",
            LogPrivate::dir_path(&path),
            cmdr_fs::pluralize::pluralize_with(n as u64, "entry", "entries"),
        ),
        // Debug: an interrupted call is a signal landing on the thread, not a
        // diagnosis, and the re-read makes it harmless.
        ReadDoubt::Interrupted => log::debug!(
            "{caller}: bulk read of {} was interrupted (EINTR), re-reading it with read_dir",
            LogPrivate::dir_path(&path),
        ),
    }
}

/// [`bulk_read_dir`] for a caller with no watchdog reading a [`ReadProgress`] (the
/// serial reconcile walk, whose whole read is capped by its `GuardedReader`), and
/// which needs the `unusable` count to decide whether it can trust the listing.
pub(in crate::indexing) fn bulk_read_dir_unwatched(path: &Path) -> std::io::Result<BulkDirRead> {
    bulk_read_dir_inner(path, &ReadProgress::default())
}

fn bulk_read_dir_inner(path: &Path, progress: &ReadProgress) -> std::io::Result<BulkDirRead> {
    bulk_read_dir_with(path, progress, &mut kernel_batch)
}

/// [`bulk_read_dir_inner`] with the batch source as a parameter, so a test can
/// interrupt the stream partway through.
fn bulk_read_dir_with(
    path: &Path,
    progress: &ReadProgress,
    batches: &mut dyn FnMut(libc::c_int, &mut [u8]) -> std::io::Result<usize>,
) -> std::io::Result<BulkDirRead> {
    let c_path = CString::new(path.as_os_str().as_bytes())
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, "path contains a NUL byte"))?;

    // Open the directory. O_RDONLY is enough to enumerate; the descriptor is
    // closed on every return path via `DirFd`'s Drop.
    // SAFETY: `c_path` is a valid NUL-terminated C string that outlives the call;
    // `open` reads it and returns a new fd or -1. No Rust aliasing is involved.
    let fd = unsafe { libc::open(c_path.as_ptr(), libc::O_RDONLY) };
    if fd < 0 {
        return Err(std::io::Error::last_os_error());
    }
    let dir = DirFd(fd);

    let mut buf = vec![0u8; ATTR_BUF_SIZE];
    let mut out = Vec::new();
    let mut unusable = 0usize;

    loop {
        let count = match batches(dir.0, &mut buf) {
            Ok(0) => break, // no more entries
            Ok(count) => count,
            // The stream is in doubt: nothing says a call resumed on this fd can't
            // skip a batch, and the reconcile diff DELETES rows a listing lacks. Drop
            // what we have, and let the caller re-read the directory whole (#708).
            Err(err) if err.raw_os_error() == Some(libc::EINTR) => {
                return Ok(BulkDirRead {
                    entries: Vec::new(),
                    unusable: 0,
                    interrupted: true,
                });
            }
            Err(err) => return Err(err),
        };

        // Publish the batch before decoding it: a directory of any size keeps this
        // climbing every syscall, which is how the watchdog tells this read apart
        // from one blocked forever in `getattrlistbulk` on a dead mount.
        progress.record_entries(count as u64);

        unusable += decode_batch(&buf, count, path, &mut out);
    }

    Ok(BulkDirRead {
        entries: out,
        unusable,
        interrupted: false,
    })
}

/// One `getattrlistbulk` call on `dir` into `buf`: how many records it wrote, 0
/// at the end of the directory.
fn kernel_batch(dir: libc::c_int, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut attrlist = REQUEST.attrlist();
    // SAFETY: `dir` is a valid open directory fd, owned by the caller's `DirFd` for
    // the call's duration; `&mut attrlist` points to a well-formed `struct attrlist`
    // of the size its `bitmapcount` implies; `buf` is writable for exactly the byte
    // length passed, and the kernel writes no more. It returns the entry count
    // (>=0) or -1.
    let n = unsafe {
        libc::getattrlistbulk(
            dir,
            (&raw mut attrlist).cast(),
            buf.as_mut_ptr().cast(),
            buf.len(),
            u64::from(libc::FSOPT_NOFOLLOW | libc::FSOPT_PACK_INVAL_ATTRS),
        )
    };
    usize::try_from(n).map_err(|_| std::io::Error::last_os_error())
}

/// Decodes one `getattrlistbulk` batch of `count` records into `out`, and returns
/// how many of them didn't become an entry. Every record is accounted for: an
/// entry, or one `unusable`. A [`attr_buf::BadRecord`] means nothing past it can
/// be located, so it and every record after it count as unusable.
fn decode_batch(buf: &[u8], count: usize, dir: &Path, out: &mut Vec<RawDirEntry>) -> usize {
    let mut decoded = 0usize;
    let mut unusable = 0usize;
    for record in attr_buf::records(buf, count, &REQUEST) {
        let Ok(record) = record else { break };
        decoded += 1;
        match entry_from_record(&record, dir) {
            Some(entry) => out.push(entry),
            None => unusable += 1,
        }
    }
    unusable + (count - decoded)
}

/// One decoded record as a [`RawDirEntry`], degrading rather than dropping:
/// - A record with no name or no type is the one thing nothing can be recovered
///   from: `None`, which the caller counts as unusable.
/// - Anything else comes back as an entry. Its `stat` is `None` (the caller stats
///   it itself) when an attribute the stat needs wasn't returned, or when the type
///   isn't file / dir / symlink and so carries no inline size (fifo, socket,
///   device node). It must stay an entry: the reconcile diff DELETES index rows
///   the live listing lacks, so dropping it would delete the file from the index.
fn entry_from_record(record: &Record<'_>, dir: &Path) -> Option<RawDirEntry> {
    let name = std::ffi::OsStr::from_bytes(record.name?);
    let file_type = match record.obj_type? {
        attr_buf::VDIR => RawFileType::Dir,
        attr_buf::VREG => RawFileType::File,
        attr_buf::VLNK => RawFileType::Symlink,
        _ => RawFileType::Other,
    };
    let modified_at = record.modtime.and_then(attr_buf::Timespec::secs_since_epoch);
    let stat = record.file_id.and_then(|inode| match file_type {
        // A regular file needs all three: one missing size attribute sends the
        // whole entry to the stat fallback, never a total we didn't read.
        RawFileType::File => Some(InlineStat {
            logical_size: record.data_length?,
            physical_size: record.alloc_size?,
            modified_at,
            inode,
            nlink: u64::from(record.link_count?),
        }),
        // Dirs and symlinks carry no meaningful size; both mappings resolve it to `None`.
        RawFileType::Dir | RawFileType::Symlink => Some(InlineStat {
            logical_size: 0,
            physical_size: 0,
            modified_at,
            inode,
            nlink: 0,
        }),
        RawFileType::Other => None,
    });
    Some(RawDirEntry {
        path: dir.join(name),
        file_type,
        stat,
    })
}

/// Owns an open directory fd, closing it on drop (every `bulk_read_dir` return).
struct DirFd(libc::c_int);

impl Drop for DirFd {
    fn drop(&mut self) {
        // SAFETY: `self.0` is a valid fd this type exclusively owns (returned by a
        // successful `open`), closed exactly once here.
        unsafe {
            libc::close(self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// The load-bearing safety net: every field the bulk reader parses must equal
    /// what `std::fs::symlink_metadata` reports, across a tree covering the tricky
    /// cases. A parsing/offset bug fails here rather than silently writing wrong
    /// sizes into the index.
    #[test]
    fn bulk_matches_symlink_metadata() {
        use std::os::unix::fs::MetadataExt;

        let dir = tempfile::tempdir().expect("temp dir");
        let root = dir.path();
        std::fs::write(root.join("empty.bin"), b"").unwrap();
        std::fs::write(root.join("small.txt"), b"hello world").unwrap(); // 11 bytes
        std::fs::write(root.join("larger.bin"), vec![7u8; 40_000]).unwrap();
        std::fs::write(root.join("náïve-unïcode.txt"), b"unicode name").unwrap();
        std::fs::create_dir(root.join("subdir")).unwrap();
        std::fs::create_dir(root.join("empty_dir")).unwrap();
        cmdr_fs::testing::symlink(root.join("small.txt"), root.join("link.txt")).unwrap();
        // A hardlink pair (nlink == 2) to check link-count parsing.
        std::fs::hard_link(root.join("small.txt"), root.join("small_alias.txt")).unwrap();

        let progress = ReadProgress::default();
        let entries = bulk_read_dir(root, &progress).expect("bulk read");
        assert_eq!(
            progress.entries(),
            entries.len() as u64,
            "the reader must publish every entry it delivered, or the watchdog can't see it working"
        );
        // getattrlistbulk omits "." and ".."; we created 8 named entries.
        assert_eq!(
            entries.len(),
            8,
            "all entries returned (got {:?})",
            entries.iter().map(|e| &e.path).collect::<Vec<_>>()
        );

        let by_name: HashMap<String, &RawDirEntry> = entries
            .iter()
            .map(|e| (e.path.file_name().unwrap().to_string_lossy().into_owned(), e))
            .collect();

        for (name, entry) in &by_name {
            let std_md = std::fs::symlink_metadata(&entry.path).expect("symlink_metadata");
            let stat = entry.stat.expect("bulk reader always fills stat on success");

            // Type agrees.
            let want_type = if std_md.is_symlink() {
                RawFileType::Symlink
            } else if std_md.is_dir() {
                RawFileType::Dir
            } else {
                RawFileType::File
            };
            assert_eq!(entry.file_type, want_type, "type mismatch for {name}");

            // Inode + mtime agree for every entry.
            assert_eq!(stat.inode, std_md.ino(), "inode mismatch for {name}");
            assert_eq!(
                stat.modified_at,
                Some(std_md.mtime() as u64),
                "mtime mismatch for {name}"
            );

            // Sizes + nlink agree for regular files (dirs/symlinks don't carry them).
            if want_type == RawFileType::File {
                assert_eq!(stat.logical_size, std_md.len(), "logical size mismatch for {name}");
                assert_eq!(
                    stat.physical_size,
                    std_md.blocks() * 512,
                    "physical size mismatch for {name}"
                );
                assert_eq!(stat.nlink, std_md.nlink(), "nlink mismatch for {name}");
            }
        }

        // Spot-check the hardlink pair really reported nlink == 2.
        assert_eq!(
            by_name["small.txt"].stat.unwrap().nlink,
            2,
            "hardlinked file should report nlink 2"
        );
    }

    // ── synthetic records (the fallback branches a real filesystem never takes) ──

    /// Build one packed record in the exact layout [`REQUEST`] packs, so the
    /// degrade-don't-drop branches can be exercised without a filesystem that
    /// withholds attributes. `returned_common` / `returned_file` are the
    /// `ATTR_CMN_RETURNED_ATTRS` bitmaps the kernel would report.
    fn build_record(name: &str, objtype: u32, returned_common: u32, returned_file: u32) -> Vec<u8> {
        const NAME_DATA_OFF: usize = 80; // right after the file block
        let name_bytes = name.as_bytes();
        let mut rec = vec![0u8; NAME_DATA_OFF + name_bytes.len() + 1];
        let put_u32 = |rec: &mut Vec<u8>, off: usize, v: u32| rec[off..off + 4].copy_from_slice(&v.to_ne_bytes());
        let put_i32 = |rec: &mut Vec<u8>, off: usize, v: i32| rec[off..off + 4].copy_from_slice(&v.to_ne_bytes());
        let put_u64 = |rec: &mut Vec<u8>, off: usize, v: u64| rec[off..off + 8].copy_from_slice(&v.to_ne_bytes());
        let put_i64 = |rec: &mut Vec<u8>, off: usize, v: i64| rec[off..off + 8].copy_from_slice(&v.to_ne_bytes());

        let len = rec.len();
        put_u32(&mut rec, 0, len as u32);
        put_u32(&mut rec, 4, returned_common); // returned.commonattr
        put_u32(&mut rec, 16, returned_file); // returned.fileattr
        put_i32(&mut rec, 24, (NAME_DATA_OFF - 24) as i32); // attrreference_t.dataoffset
        put_u32(&mut rec, 28, name_bytes.len() as u32 + 1); // .length, incl. NUL
        put_u32(&mut rec, 32, objtype);
        put_i64(&mut rec, 36, 1_700_000_000); // modtime.tv_sec
        put_u64(&mut rec, 52, 4242); // fileid
        put_u32(&mut rec, 60, 3); // linkcount
        put_u64(&mut rec, 64, 8192); // allocsize
        put_u64(&mut rec, 72, 5000); // datalength
        rec[NAME_DATA_OFF..NAME_DATA_OFF + name_bytes.len()].copy_from_slice(name_bytes);
        rec
    }

    const ALL_COMMON: u32 = libc::ATTR_CMN_RETURNED_ATTRS
        | libc::ATTR_CMN_NAME
        | libc::ATTR_CMN_OBJTYPE
        | libc::ATTR_CMN_MODTIME
        | libc::ATTR_CMN_FILEID;
    const ALL_FILE: u32 = libc::ATTR_FILE_LINKCOUNT | libc::ATTR_FILE_ALLOCSIZE | libc::ATTR_FILE_DATALENGTH;

    /// Decodes `buf` as a batch of `count` records from `/dir`: the entries and
    /// the unusable count.
    fn decode(buf: &[u8], count: usize) -> (Vec<RawDirEntry>, usize) {
        let mut out = Vec::new();
        let unusable = decode_batch(buf, count, Path::new("/dir"), &mut out);
        (out, unusable)
    }

    /// The builder itself is trustworthy: a fully-attributed file record decodes
    /// into exactly the values written into it.
    #[test]
    fn a_fully_attributed_record_parses_every_field() {
        let rec = build_record("file.txt", attr_buf::VREG, ALL_COMMON, ALL_FILE);
        let (entries, unusable) = decode(&rec, 1);
        assert_eq!(unusable, 0);
        let entry = entries.into_iter().next().expect("a fully-attributed record parses");
        assert_eq!(entry.path, Path::new("/dir/file.txt"));
        assert_eq!(entry.file_type, RawFileType::File);
        let stat = entry.stat.expect("a fully-attributed file carries its stat");
        assert_eq!(stat.logical_size, 5000);
        assert_eq!(stat.physical_size, 8192);
        assert_eq!(stat.modified_at, Some(1_700_000_000));
        assert_eq!(stat.inode, 4242);
        assert_eq!(stat.nlink, 3);
    }

    /// A file whose size attributes weren't returned must still come back AS AN
    /// ENTRY, with no inline stat, so the caller stats it itself. Dropping it
    /// instead would make the entry invisible to the reconcile diff, which deletes
    /// index rows a listing lacks — a missing attribute would delete the file.
    #[test]
    fn a_file_missing_its_size_attrs_keeps_the_entry_and_drops_only_the_stat() {
        let rec = build_record(
            "half.txt",
            attr_buf::VREG,
            ALL_COMMON,
            libc::ATTR_FILE_LINKCOUNT | libc::ATTR_FILE_ALLOCSIZE,
        );
        let (entries, unusable) = decode(&rec, 1);
        assert_eq!(unusable, 0);
        let entry = entries
            .into_iter()
            .next()
            .expect("the entry survives a missing size attribute");
        assert_eq!(entry.path, Path::new("/dir/half.txt"));
        assert_eq!(entry.file_type, RawFileType::File);
        assert!(
            entry.stat.is_none(),
            "an unattributed file must fall back to a stat, never report a guessed size"
        );
    }

    /// A missing inode is the same degrade: the entry stays, its stat goes.
    #[test]
    fn a_record_missing_its_inode_keeps_the_entry_and_drops_only_the_stat() {
        let rec = build_record("dir", attr_buf::VDIR, ALL_COMMON & !libc::ATTR_CMN_FILEID, 0);
        let (entries, unusable) = decode(&rec, 1);
        assert_eq!(unusable, 0);
        let entry = entries.into_iter().next().expect("the entry survives a missing inode");
        assert_eq!(entry.file_type, RawFileType::Dir);
        assert!(entry.stat.is_none(), "no inode, no inline stat");
    }

    /// Anything that isn't a file, dir, or symlink (fifo, socket, device node)
    /// carries no meaningful inline size, so it falls back to a stat rather than
    /// reporting zeros.
    #[test]
    fn an_unclassifiable_type_falls_back_to_a_stat() {
        let rec = build_record("pipe", attr_buf::VFIFO, ALL_COMMON, ALL_FILE);
        let (entries, _) = decode(&rec, 1);
        let entry = entries.into_iter().next().expect("an odd file type is still an entry");
        assert_eq!(entry.file_type, RawFileType::Other);
        assert!(entry.stat.is_none(), "an odd type reports no inline stat");
    }

    /// A record with no usable name is the one case nothing can be recovered from.
    /// It must be *counted* as unusable, not silently skipped: the reconcile
    /// re-reads such a directory the slow way rather than diff a short listing.
    #[test]
    fn a_record_without_a_name_is_unusable() {
        let rec = build_record("ghost", attr_buf::VREG, ALL_COMMON & !libc::ATTR_CMN_NAME, ALL_FILE);
        let (entries, unusable) = decode(&rec, 1);
        assert!(entries.is_empty(), "a nameless record can't be turned into an entry");
        assert_eq!(unusable, 1);
    }

    /// A file record too short for its file group is one `attr_buf` won't decode
    /// (`BadRecord::Truncated`), and nothing after it in the batch can be located.
    /// It and every later record count as unusable, so both callers re-read the
    /// directory with `read_dir`: a short listing never reaches the reconcile
    /// diff, which would DELETE the missing rows.
    #[test]
    fn a_truncated_record_and_everything_after_it_are_unusable() {
        // The common block (60 bytes), then a 4-byte name: the file group's 20
        // bytes would run to 80, past the record's 64.
        let mut short = build_record("abc", attr_buf::VREG, ALL_COMMON, ALL_FILE);
        short.truncate(64);
        short[0..4].copy_from_slice(&64u32.to_ne_bytes());
        short[24..28].copy_from_slice(&(60i32 - 24).to_ne_bytes());
        short[60..64].copy_from_slice(b"abc\0");
        assert_eq!(
            attr_buf::decode_record(&short, &REQUEST),
            Err(attr_buf::BadRecord::Truncated),
            "the record exercises the truncated-file-group branch"
        );

        let mut batch = short;
        batch.extend(build_record("after.txt", attr_buf::VREG, ALL_COMMON, ALL_FILE));
        let (entries, unusable) = decode(&batch, 2);
        assert!(entries.is_empty(), "nothing from a batch past a bad record is trusted");
        assert_eq!(
            unusable, 2,
            "the bad record and the healthy one after it are both accounted for"
        );
    }

    /// A walk never keeps a short listing either: an unusable record sends the
    /// whole directory through `read_dir`, whose entries the visitor stats itself.
    #[test]
    fn the_walk_rereads_a_directory_with_an_unusable_record() {
        let dir = tempfile::tempdir().expect("temp dir");
        std::fs::write(dir.path().join("a.txt"), b"a").unwrap();
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        let short = BulkDirRead {
            entries: Vec::new(),
            unusable: 1,
            interrupted: false,
        };
        let mut entries = complete_or_reread(dir.path(), &ReadProgress::default(), short).expect("re-read");
        entries.sort_by(|a, b| a.path.cmp(&b.path));
        let names: Vec<_> = entries.iter().map(|e| e.path.file_name().unwrap().to_owned()).collect();
        assert_eq!(names, ["a.txt", "sub"], "every child is back");
        assert!(
            entries.iter().all(|e| e.stat.is_none()),
            "read_dir entries are stated by the visitor"
        );
    }

    /// The kernel's batches, except the second call fails with `EINTR`: the stream
    /// breaks after entries were already read. Calls after it go back to the kernel,
    /// so a reader that resumed would finish (and look whole).
    fn interrupt_after_first_batch() -> impl FnMut(libc::c_int, &mut [u8]) -> std::io::Result<usize> {
        let mut calls = 0;
        move |fd, buf| {
            calls += 1;
            if calls == 2 {
                Err(std::io::Error::from_raw_os_error(libc::EINTR))
            } else {
                kernel_batch(fd, buf)
            }
        }
    }

    /// #708: an `EINTR` mid-stream is never resumed on the same fd, since a resumed
    /// call may have skipped a batch. The read comes back interrupted and EMPTY, so
    /// no caller can mistake what it got before the interrupt for the whole listing.
    #[test]
    fn an_interrupted_bulk_read_is_marked_in_doubt_and_keeps_no_entries() {
        let dir = tempfile::tempdir().expect("temp dir");
        std::fs::write(dir.path().join("a.txt"), b"a").unwrap();
        std::fs::write(dir.path().join("b.txt"), b"b").unwrap();
        let read = bulk_read_dir_with(dir.path(), &ReadProgress::default(), &mut interrupt_after_first_batch())
            .expect("an interrupt isn't an unreadable directory");
        assert_eq!(read.doubt(), Some(ReadDoubt::Interrupted));
        assert!(read.entries.is_empty(), "a stream in doubt hands back no entries");
    }

    /// The walk re-reads an interrupted directory whole through `read_dir`.
    #[test]
    fn the_walk_rereads_an_interrupted_directory() {
        let dir = tempfile::tempdir().expect("temp dir");
        std::fs::write(dir.path().join("a.txt"), b"a").unwrap();
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        let progress = ReadProgress::default();
        let read = bulk_read_dir_with(dir.path(), &progress, &mut interrupt_after_first_batch()).expect("read");
        let mut entries = complete_or_reread(dir.path(), &progress, read).expect("re-read");
        entries.sort_by(|a, b| a.path.cmp(&b.path));
        let names: Vec<_> = entries.iter().map(|e| e.path.file_name().unwrap().to_owned()).collect();
        assert_eq!(names, ["a.txt", "sub"], "every child is back");
    }

    /// A healthy directory parses every record, so the fallback re-read never
    /// fires in practice.
    #[test]
    fn a_healthy_directory_reports_no_unusable_records() {
        let dir = tempfile::tempdir().expect("temp dir");
        std::fs::write(dir.path().join("a.txt"), b"a").unwrap();
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        let read = bulk_read_dir_unwatched(dir.path()).expect("bulk read");
        assert_eq!(read.unusable, 0, "no record on a real volume is unusable");
        assert_eq!(read.entries.len(), 2);
    }

    #[test]
    fn bulk_read_dir_errors_on_missing_directory() {
        let err = bulk_read_dir(
            Path::new("/nonexistent-cmdr-bulk-test-dir-xyz"),
            &ReadProgress::default(),
        );
        assert!(err.is_err(), "opening a missing directory must error");
    }

    /// Ad-hoc A/B of the two readers over a real tree, for confirming the win.
    /// Ignored (env-dependent, not an assertion). Run:
    /// `cargo nextest run -p cmdr-index bulk_vs_std_walk_bench --run-ignored all --nocapture`
    #[test]
    #[ignore = "benchmark, not a correctness check; run manually with --nocapture"]
    fn bulk_vs_std_walk_bench() {
        use std::time::Instant;

        let root = std::env::var("CMDR_BENCH_DIR").unwrap_or_else(|_| "/usr".to_string());
        let root = Path::new(&root);

        // (A) getattrlistbulk: name + type + sizes inline, no per-entry stat.
        let walk_bulk = |root: &Path| -> u64 {
            let mut n = 0u64;
            let mut stack = vec![root.to_path_buf()];
            while let Some(d) = stack.pop() {
                let Ok(children) = bulk_read_dir(&d, &ReadProgress::default()) else {
                    continue;
                };
                for c in children {
                    n += 1;
                    let _ = c.stat; // sizes already in hand
                    if c.file_type == RawFileType::Dir {
                        stack.push(c.path);
                    }
                }
            }
            n
        };
        // (B) read_dir + per-entry symlink_metadata (the portable path).
        let walk_std = |root: &Path| -> u64 {
            let mut n = 0u64;
            let mut stack = vec![root.to_path_buf()];
            while let Some(d) = stack.pop() {
                let Ok(rd) = std::fs::read_dir(&d) else { continue };
                for e in rd.flatten() {
                    n += 1;
                    let Ok(md) = std::fs::symlink_metadata(e.path()) else {
                        continue;
                    };
                    let _ = (md.len(), md.modified());
                    if md.is_dir() {
                        stack.push(e.path());
                    }
                }
            }
            n
        };

        let _ = walk_bulk(root); // warm the cache
        let t = Instant::now();
        let nb = walk_bulk(root);
        let bulk_ms = t.elapsed().as_millis();
        let t = Instant::now();
        let ns = walk_std(root);
        let std_ms = t.elapsed().as_millis();

        // Emit via a stderr handle rather than `println!`/`eprintln!` (crate-banned).
        use std::io::Write;
        let mut err = std::io::stderr();
        let _ = writeln!(err, "tree {} : {nb} (bulk) / {ns} (std) entries", root.display());
        let _ = writeln!(err, "getattrlistbulk : {bulk_ms} ms");
        let _ = writeln!(err, "read_dir+lstat  : {std_ms} ms");
        if bulk_ms > 0 {
            let _ = writeln!(err, "speedup: {:.1}x", std_ms as f64 / bulk_ms as f64);
        }
    }
}
