//! The bulk lister's own policy: which filesystems take it, what a doubtful
//! record becomes, and that a doubtful stream re-lists through `read_dir` from
//! scratch. What it yields on a healthy stream is `reading_parity_test.rs`, which
//! runs every cell through both listers.

use cmdr_fs::testing::symlink;
use std::collections::HashSet;
use std::fs::{self, File};
use std::io;
use std::path::Path;

use cmdr_fs::attr_buf::{Record, Timespec, VDIR, VLNK, VREG};
use cmdr_fs::entry::FileEntry;
use cmdr_fs::testing::TestDir;

use super::{Restart, SF_FIRMLINK, bulk_eligible, entry_from_record, kernel_batch, read_entries_with};
use crate::reading::{AddedAt, Lister, ListingTally, Pick, get_single_entry, minimal_row};

// ============================================================================
// Which filesystems take the bulk lister
// ============================================================================

fn statfs_named(fs_type: &str) -> libc::statfs {
    // SAFETY: (test) `statfs` is plain old data (integers and char arrays), so
    // all-zero bytes are a valid value of it.
    let mut stat: libc::statfs = unsafe { std::mem::zeroed() };
    for (slot, byte) in stat.f_fstypename.iter_mut().zip(fs_type.bytes()) {
        *slot = byte as libc::c_char;
    }
    stat
}

/// The only automated proof that FAT, exFAT, and smbfs stay on `read_dir`: bulk
/// is an allowlist of the two filesystems it was measured faster and exact on.
#[test]
fn only_apfs_and_hfs_take_the_bulk_lister() {
    for fs_type in ["apfs", "hfs"] {
        assert!(bulk_eligible(&statfs_named(fs_type)), "bulk for {fs_type}");
    }
    for fs_type in [
        "smbfs", "exfat", "msdos", "nfs", "afpfs", "webdav", "macfuse", "apfsx", "hfs+", "",
    ] {
        assert!(!bulk_eligible(&statfs_named(fs_type)), "read_dir for {fs_type}");
    }
}

// ============================================================================
// What a record becomes
// ============================================================================

const fn ts(secs: i64) -> Option<Timespec> {
    Some(Timespec { secs, nanos: 0 })
}

/// A healthy file record whose values are deliberately NOT the file's: a cell
/// that expects the file's own `lstat` would see them if the record leaked.
fn file_record(name: &[u8]) -> Record<'_> {
    Record {
        error: 0,
        name: Some(name),
        obj_type: Some(VREG),
        crtime: ts(1_111_111_111),
        modtime: ts(1_222_222_222),
        owner_id: Some(4_000_000),
        group_id: Some(4_000_001),
        access_mask: Some(0o100_604),
        flags: Some(0),
        file_id: Some(777),
        added_time: ts(1_333_333_333),
        mount_status: None,
        link_count: Some(3),
        alloc_size: Some(12_288),
        data_length: Some(9_999),
    }
}

fn dir_record(name: &[u8]) -> Record<'_> {
    Record {
        obj_type: Some(VDIR),
        access_mask: Some(0o040_705),
        mount_status: Some(0),
        link_count: None,
        alloc_size: None,
        data_length: None,
        ..file_record(name)
    }
}

/// A folder holding `plain.txt` and `folder`.
fn scratch(label: &str) -> TestDir {
    let dir = TestDir::new(label);
    fs::write(dir.join("plain.txt"), b"hello").expect("plain.txt");
    fs::create_dir(dir.join("folder")).expect("folder");
    dir
}

fn decode(rec: &Record<'_>, dir: &Path) -> Result<FileEntry, Restart> {
    entry_from_record(rec, dir, &HashSet::new(), AddedAt::Fetch)
}

fn assert_same(entry: &FileEntry, expected: &FileEntry, what: &str) {
    assert_eq!(format!("{entry:#?}"), format!("{expected:#?}"), "{what}");
}

/// A trusted record becomes its entry with no `lstat`: the name below doesn't
/// exist on disk, and every field comes out of the record.
#[test]
fn a_trusted_record_builds_its_entry_from_its_attributes() {
    let dir = TestDir::new("bulk_trusted");
    let entry = decode(&file_record(b"not_on_disk.txt"), &dir).expect("a trusted record decodes");
    assert_eq!(entry.name, "not_on_disk.txt");
    assert_eq!(Path::new(&entry.path), dir.join("not_on_disk.txt"));
    assert!(!entry.is_directory && !entry.is_symlink);
    assert_eq!(entry.size, Some(9_999));
    assert_eq!(entry.physical_size, Some(12_288));
    assert_eq!(entry.inode, Some(777), "three links: the file id is the inode");
    assert_eq!(entry.modified_at, Some(1_222_222_222));
    assert_eq!(entry.created_at, Some(1_111_111_111));
    assert_eq!(entry.added_at, Some(1_333_333_333));
    assert_eq!(entry.permissions, 0o100_604);

    let single_link = Record {
        link_count: Some(1),
        crtime: None,
        added_time: None,
        ..file_record(b"single.txt")
    };
    let entry = decode(&single_link, &dir).expect("decodes");
    assert_eq!(entry.inode, None, "one link: no inode");
    assert_eq!(entry.created_at, None, "an invalid birth time is None, never 0");
    assert_eq!(entry.added_at, None, "an invalid added time is None, never 0");
}

/// A mask without the `S_IFMT` bits (HFS+ may answer that way) still yields the
/// whole `st_mode`, and one with them isn't doubled.
#[test]
fn permissions_take_their_type_bits_from_the_record_type() {
    let dir = TestDir::new("bulk_mode");
    for mask in [0o644, 0o100_644] {
        let rec = Record {
            access_mask: Some(mask),
            ..file_record(b"f.txt")
        };
        assert_eq!(
            decode(&rec, &dir).expect("decodes").permissions,
            0o100_644,
            "mask {mask:o}"
        );
    }
}

/// An errored record says nothing trustworthy past its name, so the entry is its
/// own `lstat`, exactly what the re-stat yields.
#[test]
fn an_errored_record_takes_its_own_lstat() {
    let dir = scratch("bulk_errored");
    for name in ["plain.txt", "folder"] {
        let rec = Record {
            error: libc::EIO as u32,
            ..file_record(name.as_bytes())
        };
        let entry = decode(&rec, &dir).expect("lstat answers");
        let restat = get_single_entry(&dir.join(name)).expect("re-stat");
        assert_same(&entry, &restat, name);
    }
}

/// Every required attribute, cleared one at a time: the entry is its own
/// `lstat`, never the record's other values.
#[test]
fn a_record_missing_a_required_attribute_takes_its_own_lstat() {
    let dir = scratch("bulk_required");
    let plain = b"plain.txt".as_slice();
    let folder = b"folder".as_slice();
    let doubtful: Vec<(&str, Record<'_>)> = vec![
        (
            "type",
            Record {
                obj_type: None,
                ..file_record(plain)
            },
        ),
        (
            "mtime",
            Record {
                modtime: None,
                ..file_record(plain)
            },
        ),
        (
            "owner",
            Record {
                owner_id: None,
                ..file_record(plain)
            },
        ),
        (
            "group",
            Record {
                group_id: None,
                ..file_record(plain)
            },
        ),
        (
            "mode",
            Record {
                access_mask: None,
                ..file_record(plain)
            },
        ),
        (
            "flags",
            Record {
                flags: None,
                ..file_record(plain)
            },
        ),
        (
            "file id",
            Record {
                file_id: None,
                ..file_record(plain)
            },
        ),
        (
            "link count",
            Record {
                link_count: None,
                ..file_record(plain)
            },
        ),
        (
            "alloc size",
            Record {
                alloc_size: None,
                ..file_record(plain)
            },
        ),
        (
            "data length",
            Record {
                data_length: None,
                ..file_record(plain)
            },
        ),
        (
            "folder mtime",
            Record {
                modtime: None,
                ..dir_record(folder)
            },
        ),
        (
            "folder mount status",
            Record {
                mount_status: None,
                ..dir_record(folder)
            },
        ),
    ];
    for (what, rec) in doubtful {
        let name = std::str::from_utf8(rec.name.expect("named")).expect("UTF-8");
        let entry = decode(&rec, &dir).unwrap_or_else(|why| panic!("{what}: {why:?}"));
        let restat = get_single_entry(&dir.join(name)).expect("re-stat");
        assert_same(&entry, &restat, &format!("no {what}: the entry is the lstat"));
    }
}

/// A doubtful record whose `lstat` fails too is today's minimal row when the
/// record still says what type it is (unerrored, type valid)...
#[test]
fn a_doubtful_record_that_cant_be_lstated_is_the_minimal_row_when_its_type_is_known() {
    let dir = TestDir::new("bulk_minimal");
    for (obj_type, is_link) in [(VLNK, true), (VREG, false), (VDIR, false)] {
        let rec = Record {
            obj_type: Some(obj_type),
            modtime: None,
            ..file_record(b"gone")
        };
        let entry = decode(&rec, &dir).expect("a typed record yields a row");
        let expected = minimal_row(&dir.join("gone"), "gone".to_string(), is_link, &HashSet::new());
        assert_same(&entry, &expected, &format!("type {obj_type}"));
    }
}

/// ...and a restart when nothing says what it is: guessing a minimal row's icon
/// would invent a shape `read_dir` might not.
#[test]
fn a_doubtful_record_that_cant_be_lstated_and_has_no_type_restarts() {
    let dir = TestDir::new("bulk_untyped");
    let errored = Record {
        error: libc::EIO as u32,
        ..file_record(b"gone")
    };
    assert_eq!(decode(&errored, &dir).err(), Some(Restart::Untyped), "errored");
    let untyped = Record {
        obj_type: None,
        ..file_record(b"gone")
    };
    assert_eq!(decode(&untyped, &dir).err(), Some(Restart::Untyped), "untyped");
    let nameless = Record {
        name: None,
        ..file_record(b"")
    };
    assert_eq!(decode(&nameless, &dir).err(), Some(Restart::NoName), "nameless");
}

/// A mount point or a firmlink reports the folder it covers, so its entry is
/// built wholly from the re-stat, added date included.
#[test]
fn a_mount_point_or_firmlink_record_is_its_restat() {
    let dir = scratch("bulk_mount");
    let mount = Record {
        mount_status: Some(libc::DIR_MNTSTATUS_MNTPOINT),
        ..dir_record(b"folder")
    };
    let firmlink = Record {
        flags: Some(SF_FIRMLINK),
        ..dir_record(b"folder")
    };
    let restat = get_single_entry(&dir.join("folder")).expect("re-stat");
    for (what, rec) in [("mount point", mount), ("firmlink", firmlink)] {
        let entry = decode(&rec, &dir).expect("re-stat answers");
        assert_same(&entry, &restat, what);
    }

    let vanished = Record {
        mount_status: Some(libc::DIR_MNTSTATUS_MNTPOINT),
        ..dir_record(b"gone")
    };
    let entry = decode(&vanished, &dir).expect("still a row");
    let expected = minimal_row(&dir.join("gone"), "gone".to_string(), false, &HashSet::new());
    assert_same(
        &entry,
        &expected,
        "a mount point that can't be re-stat'ed is the minimal row, never the stub",
    );
}

/// `/.hidden` reaches bulk entries too, re-stat'ed ones included.
#[test]
fn dot_hidden_reaches_bulk_entries() {
    let dir = scratch("bulk_dot_hidden");
    let dot_hidden: HashSet<String> = ["not_on_disk.txt", "folder"].map(str::to_string).into();
    let entry =
        entry_from_record(&file_record(b"not_on_disk.txt"), &dir, &dot_hidden, AddedAt::Fetch).expect("decodes");
    assert!(entry.is_hidden, "a record's entry");
    let mount = Record {
        mount_status: Some(libc::DIR_MNTSTATUS_MNTPOINT),
        ..dir_record(b"folder")
    };
    let entry = entry_from_record(&mount, &dir, &dot_hidden, AddedAt::Fetch).expect("decodes");
    assert!(entry.is_hidden, "a re-stat'ed entry");
}

// ============================================================================
// A doubtful stream re-lists through `read_dir` from scratch
// ============================================================================

const STREAM_FILES: usize = 40;

/// A folder of [`STREAM_FILES`] files plus a folder and a link, listed through
/// `read_dir` as the reference: what a restarted listing must equal exactly.
fn stream_fixture(label: &str) -> (TestDir, Vec<String>) {
    let dir = TestDir::new(label);
    for i in 0..STREAM_FILES {
        fs::write(dir.join(format!("file-{i:03}.txt")), b"x").expect("a file");
    }
    fs::create_dir(dir.join("sub")).expect("a folder");
    symlink("sub", dir.join("link")).expect("a link");
    let reference = sorted_debug(
        read_entries_with(
            &dir,
            &HashSet::new(),
            None,
            Pick::Forced(Lister::ReadDir),
            &mut |_, _| panic!("read_dir never asks for a batch"),
        )
        .expect("read_dir lists"),
    );
    (dir, reference)
}

fn sorted_debug(mut entries: Vec<FileEntry>) -> Vec<String> {
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    entries.iter().map(|e| format!("{e:#?}")).collect()
}

/// One real batch into a small slice of the buffer (a few records), so the
/// stream is mid-listing when the second call goes wrong.
fn small_kernel_batch(dir: &File, buf: &mut [u64]) -> io::Result<usize> {
    let n = kernel_batch(dir, &mut buf[..128])?;
    assert!(n > 0 && n < STREAM_FILES, "precondition: a partial first batch ({n})");
    Ok(n)
}

/// Lists the fixture in bulk with `second` answering the second batch, and
/// asserts the result is the `read_dir` listing exactly, the tally counting it once.
fn assert_restarts_from_scratch(label: &str, second: &dyn Fn(&mut [u64]) -> io::Result<usize>) {
    let (dir, reference) = stream_fixture(label);
    let tally = ListingTally::default();
    let mut calls = 0;
    let listed = read_entries_with(
        &dir,
        &HashSet::new(),
        Some(&tally),
        Pick::Forced(Lister::Bulk),
        &mut |fd, buf| {
            calls += 1;
            if calls == 1 {
                small_kernel_batch(fd, buf)
            } else {
                second(buf)
            }
        },
    )
    .unwrap_or_else(|e| panic!("{label}: the restart lists: {e}"));
    assert_eq!(calls, 2, "{label}: the second batch was asked for");
    assert_eq!(
        sorted_debug(listed),
        reference,
        "{label}: exactly the read_dir listing, nothing merged"
    );
    let progress = tally.snapshot();
    assert_eq!(
        (progress.files, progress.dirs),
        (STREAM_FILES, 2),
        "{label}: the tally counts each entry once (the link to a folder counts as one)"
    );
}

#[test]
fn a_bad_record_mid_stream_relists_through_read_dir() {
    assert_restarts_from_scratch("bulk_bad_record", &|buf| {
        // A record length shorter than its own header.
        buf[0] = 3;
        Ok(1)
    });
}

#[test]
fn a_record_with_no_name_mid_stream_relists_through_read_dir() {
    assert_restarts_from_scratch("bulk_no_name", &|buf| {
        // A whole record (header plus the common slots, 112 bytes) whose returned
        // set is empty: no name, no type, so no group packed either.
        buf[..14].fill(0);
        buf[0] = 112;
        Ok(1)
    });
}

/// `EINTR` restarts too: nothing says a resumed call can't skip a batch.
#[test]
fn an_interrupted_stream_relists_through_read_dir() {
    assert_restarts_from_scratch("bulk_eintr", &|_| Err(io::Error::from_raw_os_error(libc::EINTR)));
}

#[test]
fn a_refused_stream_relists_through_read_dir() {
    for errno in [libc::EACCES, libc::ENOTSUP, libc::EINVAL] {
        assert_restarts_from_scratch(&format!("bulk_errno_{errno}"), &|_| {
            Err(io::Error::from_raw_os_error(errno))
        });
    }
}

/// Any other errno reaches the caller, as a `read_dir` iteration error does.
#[test]
fn another_errno_reaches_the_caller() {
    let (dir, _) = stream_fixture("bulk_eio");
    let result = read_entries_with(&dir, &HashSet::new(), None, Pick::Forced(Lister::Bulk), &mut |_, _| {
        Err(io::Error::from_raw_os_error(libc::EIO))
    });
    assert_eq!(result.err().and_then(|e| e.raw_os_error()), Some(libc::EIO));
}

/// Production picks bulk on APFS (the scratch dir's filesystem) and lists the
/// whole fixture through it.
#[test]
fn production_lists_apfs_in_bulk() {
    let (dir, reference) = stream_fixture("bulk_production");
    let mut calls = 0;
    let listed = read_entries_with(&dir, &HashSet::new(), None, Pick::ByFilesystem, &mut |fd, buf| {
        calls += 1;
        kernel_batch(fd, buf)
    })
    .expect("lists");
    assert!(calls >= 2, "the bulk stream ran to its end (calls: {calls})");
    assert_eq!(sorted_debug(listed), reference, "bulk lists what read_dir does");
}
