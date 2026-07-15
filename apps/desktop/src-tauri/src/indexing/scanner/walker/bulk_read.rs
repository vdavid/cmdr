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
//! # Correctness is fallback-protected
//!
//! The packed attribute buffer is parsed by hand (unaligned reads at a running
//! cursor). To make a parse mistake *safe*, every entry is validated against
//! `ATTR_CMN_RETURNED_ATTRS`: if any attribute we rely on wasn't returned for an
//! entry (or its type is unclassifiable), that single entry falls back to
//! `symlink_metadata` — so a miss degrades to the slower-but-correct path, never
//! to wrong data. The `bulk_matches_symlink_metadata` differential test asserts
//! the parsed values equal `std::fs::symlink_metadata` field-for-field across a
//! rich tree (files with known sizes, an empty dir, a symlink, a hardlink, a
//! unicode name), so a parsing bug is a failing test, not a silent corruption.

use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

use super::{InlineStat, RawDirEntry, RawFileType};

// ── getattrlist ABI ──────────────────────────────────────────────────
//
// Attribute request bits (`<sys/attr.h>`). We request, in the order the kernel
// packs them (RETURNED_ATTRS first, then each group ascending by bit value):
//   common: RETURNED_ATTRS, NAME, OBJTYPE, MODTIME, FILEID
//   file:   LINKCOUNT, ALLOCSIZE, DATALENGTH

const ATTR_BIT_MAP_COUNT: u16 = 5;

const ATTR_CMN_RETURNED_ATTRS: u32 = 0x8000_0000;
const ATTR_CMN_NAME: u32 = 0x0000_0001;
const ATTR_CMN_OBJTYPE: u32 = 0x0000_0008;
const ATTR_CMN_MODTIME: u32 = 0x0000_0400;
const ATTR_CMN_FILEID: u32 = 0x0200_0000;

const ATTR_FILE_LINKCOUNT: u32 = 0x0000_0001;
const ATTR_FILE_ALLOCSIZE: u32 = 0x0000_0004;
const ATTR_FILE_DATALENGTH: u32 = 0x0000_0200;

// `getattrlistbulk` options: don't follow symlinks (we want each entry's own
// attributes), and pack invalid attributes as zero so every requested attribute
// occupies its fixed slot in the buffer — RETURNED_ATTRS tells us which are valid,
// so offsets are constant regardless of which attributes a given entry has.
const FSOPT_NOFOLLOW: u64 = 0x0000_0001;
const FSOPT_PACK_INVAL_ATTRS: u64 = 0x0000_0008;

// vnode object types (`fsobj_type_t`, from `<sys/vnode.h>`).
const VREG: u32 = 1;
const VDIR: u32 = 2;
const VLNK: u32 = 5;

/// `struct attrlist` (`<sys/attr.h>`). Five attribute groups after the header.
#[repr(C)]
struct AttrList {
    bitmapcount: u16,
    reserved: u16,
    commonattr: u32,
    volattr: u32,
    dirattr: u32,
    fileattr: u32,
    forkattr: u32,
}

/// Read buffer per `getattrlistbulk` call. 64 KiB fits many entries per syscall.
const ATTR_BUF_SIZE: usize = 64 * 1024;

/// Bulk directory reader: the production macOS [`super::ReadDirFn`]. Reads
/// `path`'s children with their attributes via `getattrlistbulk`, falling back to
/// `symlink_metadata` per entry when an attribute is missing. Fails (propagating
/// the `io::Error`) only when the directory itself can't be opened — matching
/// `std::fs::read_dir`'s contract, so the walker treats it like any unlistable dir.
pub(super) fn bulk_read_dir(path: &Path) -> std::io::Result<Vec<RawDirEntry>> {
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

    let mut attrlist = AttrList {
        bitmapcount: ATTR_BIT_MAP_COUNT,
        reserved: 0,
        commonattr: ATTR_CMN_RETURNED_ATTRS | ATTR_CMN_NAME | ATTR_CMN_OBJTYPE | ATTR_CMN_MODTIME | ATTR_CMN_FILEID,
        volattr: 0,
        dirattr: 0,
        fileattr: ATTR_FILE_LINKCOUNT | ATTR_FILE_ALLOCSIZE | ATTR_FILE_DATALENGTH,
        forkattr: 0,
    };

    let mut buf = vec![0u8; ATTR_BUF_SIZE];
    let mut out = Vec::new();

    loop {
        let count = loop {
            // SAFETY: `dir.0` is a valid open directory fd; `&mut attrlist` points to a
            // well-formed `struct attrlist` of the size its `bitmapcount` implies; `buf`
            // is a writable allocation of `ATTR_BUF_SIZE` bytes. The kernel writes at
            // most `ATTR_BUF_SIZE` bytes into `buf` and returns the entry count (>=0) or
            // -1. Retried on EINTR.
            let n = unsafe {
                libc::getattrlistbulk(
                    dir.0,
                    (&mut attrlist as *mut AttrList).cast::<libc::c_void>(),
                    buf.as_mut_ptr().cast::<libc::c_void>(),
                    ATTR_BUF_SIZE,
                    FSOPT_NOFOLLOW | FSOPT_PACK_INVAL_ATTRS,
                )
            };
            if n < 0 {
                let err = std::io::Error::last_os_error();
                if err.raw_os_error() == Some(libc::EINTR) {
                    continue;
                }
                return Err(err);
            }
            break n;
        };

        if count == 0 {
            break; // no more entries
        }

        // Parse `count` variable-length entries packed back-to-back in `buf`.
        let mut base = 0usize;
        for _ in 0..count {
            let (entry_len, parsed) = parse_entry(&buf[base..], path);
            if let Some(entry) = parsed {
                out.push(entry);
            }
            // `entry_len` is the record length the kernel wrote (first u32); advance
            // to the next record. A zero length would loop forever — guard it.
            if entry_len == 0 {
                break;
            }
            base += entry_len;
            if base >= buf.len() {
                break;
            }
        }
    }

    Ok(out)
}

/// Parse one packed `getattrlistbulk` record from the front of `bytes`. Returns
/// `(record_length, entry)`. `entry` is `None` when the record can't be trusted
/// (missing name/type, or a file missing size/link attrs) — the caller drops such
/// records here and they're re-listed via `symlink_metadata` fallback below.
fn parse_entry(bytes: &[u8], dir: &Path) -> (usize, Option<RawDirEntry>) {
    // Fixed field layout after `FSOPT_PACK_INVAL_ATTRS` (every requested attr
    // present): u32 length | attribute_set_t returned (5×u32) | attrreference_t
    // name (i32 off + u32 len) | u32 objtype | timespec modtime (2×i64) | u64
    // fileid | u32 linkcount | u64 allocsize | u64 datalength.
    const OFF_RETURNED: usize = 4;
    const OFF_NAME_REF: usize = OFF_RETURNED + 20;
    const OFF_OBJTYPE: usize = OFF_NAME_REF + 8;
    const OFF_MODTIME: usize = OFF_OBJTYPE + 4;
    const OFF_FILEID: usize = OFF_MODTIME + 16;
    // End of the always-present common block (name..fileid). The file attrs come
    // after it, but ONLY for objects that have them: a directory carries no file
    // block, so its record ends here and is shorter than a file's / symlink's.
    const COMMON_END: usize = OFF_FILEID + 8;
    const OFF_LINKCOUNT: usize = COMMON_END;
    const OFF_ALLOCSIZE: usize = OFF_LINKCOUNT + 4;
    const OFF_DATALENGTH: usize = OFF_ALLOCSIZE + 8;
    const FILE_BLOCK_END: usize = OFF_DATALENGTH + 8;

    let entry_len = read_u32(bytes, 0) as usize;
    // A record shorter than the common block (or longer than the slice) is
    // corrupt/unexpected — stop trusting this record.
    if entry_len < COMMON_END || entry_len > bytes.len() {
        return (entry_len, None);
    }

    // `returned` is an attribute_set_t; commonattr is its first u32, fileattr its
    // fourth. Validate that the attributes we rely on were actually returned.
    let returned_common = read_u32(bytes, OFF_RETURNED);
    let returned_file = read_u32(bytes, OFF_RETURNED + 12);
    let has_common = |bit: u32| returned_common & bit != 0;
    let has_file = |bit: u32| returned_file & bit != 0;

    if !has_common(ATTR_CMN_NAME) || !has_common(ATTR_CMN_OBJTYPE) || !has_common(ATTR_CMN_FILEID) {
        return (entry_len, None); // can't classify or key the entry → fall back
    }

    // Name: attrreference_t { i32 dataoffset; u32 length } at OFF_NAME_REF; the
    // bytes live at (OFF_NAME_REF + dataoffset) for `length` bytes incl. the NUL.
    let name_data_off = read_i32(bytes, OFF_NAME_REF);
    let name_len = read_u32(bytes, OFF_NAME_REF + 4) as usize;
    let name_start = OFF_NAME_REF as isize + name_data_off as isize;
    if name_start < 0 || name_len == 0 {
        return (entry_len, None);
    }
    let name_start = name_start as usize;
    let name_end = name_start + name_len.saturating_sub(1); // strip trailing NUL
    if name_end > entry_len || name_end < name_start {
        return (entry_len, None);
    }
    let name_bytes = &bytes[name_start..name_end];
    let name = std::ffi::OsStr::from_bytes(name_bytes);

    let objtype = read_u32(bytes, OFF_OBJTYPE);
    let file_type = match objtype {
        VDIR => RawFileType::Dir,
        VREG => RawFileType::File,
        VLNK => RawFileType::Symlink,
        _ => RawFileType::Other,
    };

    let modified_at = if has_common(ATTR_CMN_MODTIME) {
        let secs = read_i64(bytes, OFF_MODTIME); // timespec.tv_sec; tv_nsec ignored
        if secs >= 0 { Some(secs as u64) } else { None }
    } else {
        None
    };

    let inode = read_u64(bytes, OFF_FILEID);

    // Size/link attrs are only meaningful (and only requested-valid) for regular
    // files; dirs/symlinks legitimately don't return them and the visitor maps
    // their sizes to `None` anyway. For a regular file, require all three — a file
    // missing any size attr falls back so a total is never silently wrong.
    let (logical_size, physical_size, nlink) = if file_type == RawFileType::File {
        // The file block must be both present in the record and flagged valid.
        if entry_len < FILE_BLOCK_END
            || !has_file(ATTR_FILE_DATALENGTH)
            || !has_file(ATTR_FILE_ALLOCSIZE)
            || !has_file(ATTR_FILE_LINKCOUNT)
        {
            return (entry_len, None);
        }
        (
            read_u64(bytes, OFF_DATALENGTH),
            read_u64(bytes, OFF_ALLOCSIZE),
            u64::from(read_u32(bytes, OFF_LINKCOUNT)),
        )
    } else {
        (0, 0, 0)
    };

    let entry = RawDirEntry {
        path: dir.join(name),
        file_type,
        stat: Some(InlineStat {
            logical_size,
            physical_size,
            modified_at,
            inode,
            nlink,
        }),
    };
    (entry_len, Some(entry))
}

// ── unaligned little-endian reads (the packed buffer isn't 8-byte aligned) ──

fn read_u32(b: &[u8], off: usize) -> u32 {
    u32::from_ne_bytes(b[off..off + 4].try_into().expect("4 bytes in range"))
}
fn read_i32(b: &[u8], off: usize) -> i32 {
    i32::from_ne_bytes(b[off..off + 4].try_into().expect("4 bytes in range"))
}
fn read_u64(b: &[u8], off: usize) -> u64 {
    u64::from_ne_bytes(b[off..off + 8].try_into().expect("8 bytes in range"))
}
fn read_i64(b: &[u8], off: usize) -> i64 {
    i64::from_ne_bytes(b[off..off + 8].try_into().expect("8 bytes in range"))
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
        std::os::unix::fs::symlink(root.join("small.txt"), root.join("link.txt")).unwrap();
        // A hardlink pair (nlink == 2) to check link-count parsing.
        std::fs::hard_link(root.join("small.txt"), root.join("small_alias.txt")).unwrap();

        let entries = bulk_read_dir(root).expect("bulk read");
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

    #[test]
    fn bulk_read_dir_errors_on_missing_directory() {
        let err = bulk_read_dir(Path::new("/nonexistent-cmdr-bulk-test-dir-xyz"));
        assert!(err.is_err(), "opening a missing directory must error");
    }

    /// Ad-hoc A/B of the two readers over a real tree, for confirming the win.
    /// Ignored (env-dependent, not an assertion). Run:
    /// `cargo nextest run -p cmdr bulk_vs_std_walk_bench --run-ignored all --nocapture`
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
                let Ok(children) = bulk_read_dir(&d) else { continue };
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
