use super::*;

/// Everything the local lister asks for.
const LISTER: Request = Request::new(&[
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

const ALL_COMMON: u32 = libc::ATTR_CMN_RETURNED_ATTRS
    | ATTR_CMN_ERROR
    | libc::ATTR_CMN_NAME
    | libc::ATTR_CMN_OBJTYPE
    | libc::ATTR_CMN_CRTIME
    | libc::ATTR_CMN_MODTIME
    | libc::ATTR_CMN_OWNERID
    | libc::ATTR_CMN_GRPID
    | libc::ATTR_CMN_ACCESSMASK
    | libc::ATTR_CMN_FLAGS
    | libc::ATTR_CMN_FILEID
    | libc::ATTR_CMN_ADDEDTIME;
const ALL_FILE: u32 = libc::ATTR_FILE_LINKCOUNT | libc::ATTR_FILE_ALLOCSIZE | libc::ATTR_FILE_DATALENGTH;

/// Lays a record out by hand, slot by slot, the way the kernel would. Each
/// cell writes its slots in order itself, so the tests never borrow the
/// decoder's own idea of the layout.
struct Rec {
    bytes: Vec<u8>,
    name: Option<(usize, Vec<u8>)>,
}

fn rec(common: u32, dir: u32, file: u32) -> Rec {
    let mut bytes = vec![0u8; 4];
    for word in [common, 0, dir, file, 0] {
        bytes.extend_from_slice(&word.to_ne_bytes());
    }
    Rec { bytes, name: None }
}

impl Rec {
    fn u32(mut self, v: u32) -> Self {
        self.bytes.extend_from_slice(&v.to_ne_bytes());
        self
    }

    fn u64(mut self, v: u64) -> Self {
        self.bytes.extend_from_slice(&v.to_ne_bytes());
        self
    }

    fn ts(mut self, secs: i64, nanos: i64) -> Self {
        self.bytes.extend_from_slice(&secs.to_ne_bytes());
        self.bytes.extend_from_slice(&nanos.to_ne_bytes());
        self
    }

    /// The `attrreference_t` slot; the name's bytes go after the last slot.
    fn name(mut self, name: &str) -> Self {
        self.name = Some((self.bytes.len(), name.as_bytes().to_vec()));
        self.bytes.extend_from_slice(&[0u8; 8]);
        self
    }

    fn build(mut self) -> Vec<u8> {
        if let Some((slot, name)) = self.name.take() {
            let start = self.bytes.len();
            self.bytes.extend_from_slice(&name);
            self.bytes.push(0);
            let offset = i32::try_from(start - slot).unwrap();
            let len = u32::try_from(name.len() + 1).unwrap();
            self.bytes[slot..slot + 4].copy_from_slice(&offset.to_ne_bytes());
            self.bytes[slot + 4..slot + 8].copy_from_slice(&len.to_ne_bytes());
        }
        while !self.bytes.len().is_multiple_of(4) {
            self.bytes.push(0);
        }
        let len = u32::try_from(self.bytes.len()).unwrap();
        self.bytes[0..4].copy_from_slice(&len.to_ne_bytes());
        self.bytes
    }
}

/// The common slots a lister record carries, up to and including the added time.
fn lister_common(returned_common: u32, dir: u32, file: u32, name: &str, obj_type: u32) -> Rec {
    rec(returned_common, dir, file)
        .u32(0)
        .name(name)
        .u32(obj_type)
        .ts(1_600_000_000, 5)
        .ts(1_700_000_000, 7)
        .u32(501)
        .u32(20)
        .u32(0o100_644)
        .u32(libc::UF_HIDDEN)
        .u64(4242)
        .ts(1_750_000_000, 9)
}

fn put_u32(bytes: &mut [u8], at: usize, v: u32) {
    bytes[at..at + 4].copy_from_slice(&v.to_ne_bytes());
}

#[test]
fn a_file_record_decodes_every_requested_attribute() {
    let bytes = lister_common(ALL_COMMON, 0, ALL_FILE, "file.txt", VREG)
        .u32(2)
        .u64(8192)
        .u64(5000)
        .build();
    let decoded = decode_record(&bytes, &LISTER).expect("a well-formed record decodes");
    assert_eq!(
        decoded,
        Record {
            error: 0,
            name: Some(b"file.txt".as_slice()),
            obj_type: Some(VREG),
            crtime: Some(Timespec {
                secs: 1_600_000_000,
                nanos: 5
            }),
            modtime: Some(Timespec {
                secs: 1_700_000_000,
                nanos: 7
            }),
            owner_id: Some(501),
            group_id: Some(20),
            access_mask: Some(0o100_644),
            flags: Some(libc::UF_HIDDEN),
            file_id: Some(4242),
            added_time: Some(Timespec {
                secs: 1_750_000_000,
                nanos: 9
            }),
            mount_status: None,
            link_count: Some(2),
            alloc_size: Some(8192),
            data_length: Some(5000),
        }
    );
}

/// The smbfs and exFAT shape that broke the prototype: an attribute the
/// filesystem doesn't support still takes its slot, so every later offset holds.
#[test]
fn an_invalid_attribute_in_the_middle_keeps_every_later_offset() {
    let returned = ALL_COMMON & !libc::ATTR_CMN_CRTIME & !libc::ATTR_CMN_ADDEDTIME;
    let bytes = rec(returned, 0, ALL_FILE)
        .u32(0)
        .name("smb.bin")
        .u32(VREG)
        .ts(0, 0)
        .ts(1_700_000_000, 0)
        .u32(501)
        .u32(20)
        .u32(0o644)
        .u32(0)
        .u64(77)
        .ts(0, 0)
        .u32(1)
        .u64(4096)
        .u64(123)
        .build();
    let decoded = decode_record(&bytes, &LISTER).expect("decodes");
    assert_eq!(decoded.crtime, None, "an unsupported birth time is absent, not 1970");
    assert_eq!(decoded.added_time, None);
    assert_eq!(decoded.modtime.map(|t| t.secs), Some(1_700_000_000));
    assert_eq!(decoded.file_id, Some(77));
    assert_eq!(decoded.link_count, Some(1));
    assert_eq!(decoded.alloc_size, Some(4096));
    assert_eq!(decoded.data_length, Some(123));
}

/// Validity comes from the returned bit alone: a slot the kernel left garbage
/// in still reads `None` when its bit is clear.
#[test]
fn a_missing_optional_bit_is_none_whatever_its_slot_holds() {
    let bytes = lister_common(ALL_COMMON & !libc::ATTR_CMN_ADDEDTIME, 0, ALL_FILE, "f", VREG)
        .u32(1)
        .u64(0)
        .u64(0)
        .build();
    let decoded = decode_record(&bytes, &LISTER).expect("decodes");
    assert_eq!(decoded.added_time, None);
    assert_eq!(decoded.data_length, Some(0), "a real zero size stays a real zero");
}

#[test]
fn a_directory_record_carries_the_dir_group_and_no_file_group() {
    let bytes = lister_common(ALL_COMMON, libc::ATTR_DIR_MOUNTSTATUS, 0, "Volumes", VDIR)
        .u32(libc::DIR_MNTSTATUS_MNTPOINT)
        .build();
    let decoded = decode_record(&bytes, &LISTER).expect("a dir record ends after its dir group");
    assert_eq!(decoded.obj_type, Some(VDIR));
    assert_eq!(decoded.mount_status, Some(libc::DIR_MNTSTATUS_MNTPOINT));
    assert_eq!(decoded.link_count, None);
    assert_eq!(decoded.alloc_size, None);
    assert_eq!(decoded.data_length, None);
    assert_eq!(decoded.name, Some(b"Volumes".as_slice()));
}

#[test]
fn a_symlink_record_carries_the_file_group() {
    let bytes = lister_common(ALL_COMMON, 0, ALL_FILE, "link", VLNK)
        .u32(1)
        .u64(0)
        .u64(11)
        .build();
    let decoded = decode_record(&bytes, &LISTER).expect("decodes");
    assert_eq!(decoded.obj_type, Some(VLNK));
    assert_eq!(decoded.mount_status, None);
    assert_eq!(decoded.link_count, Some(1));
    assert_eq!(decoded.data_length, Some(11));
}

/// What else an errored record packs is unspecified, so the decoder reads its
/// name and error and nothing past them (this record has nothing past them).
#[test]
fn an_errored_record_decodes_only_its_name_and_error() {
    let bytes = rec(
        libc::ATTR_CMN_RETURNED_ATTRS | ATTR_CMN_ERROR | libc::ATTR_CMN_NAME,
        0,
        0,
    )
    .u32(libc::EACCES as u32)
    .name("locked")
    .build();
    let decoded = decode_record(&bytes, &LISTER).expect("an errored record decodes");
    assert_eq!(
        decoded,
        Record {
            error: libc::EACCES as u32,
            name: Some(b"locked".as_slice()),
            ..Record::unread()
        }
    );
}

/// Without a valid type nobody knows which group the kernel packed, so the
/// decoder reads neither.
#[test]
fn an_invalid_type_leaves_both_groups_undecoded() {
    let bytes = lister_common(ALL_COMMON & !libc::ATTR_CMN_OBJTYPE, 0, ALL_FILE, "odd", 0)
        .u32(1)
        .u64(4096)
        .u64(10)
        .build();
    let decoded = decode_record(&bytes, &LISTER).expect("decodes");
    assert_eq!(decoded.obj_type, None);
    assert_eq!(decoded.file_id, Some(4242), "the common group still decodes");
    assert_eq!(decoded.link_count, None);
    assert_eq!(decoded.data_length, None);
    assert_eq!(decoded.mount_status, None);
}

#[test]
fn a_record_shorter_than_its_header_is_bad() {
    let mut bytes = vec![0u8; 32];
    put_u32(&mut bytes, 0, 20);
    assert_eq!(decode_record(&bytes, &LISTER), Err(BadRecord::Length(20)));
}

#[test]
fn a_record_longer_than_its_buffer_is_bad() {
    let mut bytes = lister_common(ALL_COMMON, 0, ALL_FILE, "f", VREG)
        .u32(1)
        .u64(0)
        .u64(0)
        .build();
    let too_long = u32::try_from(bytes.len() + 4).unwrap();
    put_u32(&mut bytes, 0, too_long);
    assert_eq!(decode_record(&bytes, &LISTER), Err(BadRecord::Length(too_long)));
}

#[test]
fn a_buffer_too_short_for_a_length_is_bad() {
    assert_eq!(decode_record(&[1, 0], &LISTER), Err(BadRecord::Length(0)));
}

#[test]
fn a_slot_past_the_record_end_is_truncated() {
    // A file record missing its data length: the request implies 8 more bytes.
    // No name, since a name's bytes would trail the slots and fill the gap.
    let bytes = rec(ALL_COMMON & !libc::ATTR_CMN_NAME, 0, ALL_FILE)
        .u32(0)
        .u64(0)
        .u32(VREG)
        .ts(0, 0)
        .ts(0, 0)
        .u32(0)
        .u32(0)
        .u32(0)
        .u32(0)
        .u64(0)
        .ts(0, 0)
        .u32(1)
        .u64(0)
        .build();
    assert_eq!(decode_record(&bytes, &LISTER), Err(BadRecord::Truncated));
}

#[test]
fn a_name_past_its_record_is_bad() {
    let mut bytes = lister_common(ALL_COMMON, 0, ALL_FILE, "f", VREG)
        .u32(1)
        .u64(0)
        .u64(0)
        .build();
    // The name reference sits right after the 24-byte header and the error.
    put_u32(&mut bytes, 32, 4096);
    assert_eq!(decode_record(&bytes, &LISTER), Err(BadRecord::Name));
}

#[test]
fn a_name_before_its_record_is_bad() {
    let mut bytes = lister_common(ALL_COMMON, 0, ALL_FILE, "f", VREG)
        .u32(1)
        .u64(0)
        .u64(0)
        .build();
    bytes[28..32].copy_from_slice(&(-100i32).to_ne_bytes());
    assert_eq!(decode_record(&bytes, &LISTER), Err(BadRecord::Name));
}

#[test]
fn an_empty_name_is_bad() {
    let mut bytes = lister_common(ALL_COMMON, 0, ALL_FILE, "f", VREG)
        .u32(1)
        .u64(0)
        .u64(0)
        .build();
    put_u32(&mut bytes, 32, 0);
    assert_eq!(decode_record(&bytes, &LISTER), Err(BadRecord::Name));
}

/// `getattrlist` hands back one record in a buffer bigger than it.
#[test]
fn a_record_decodes_from_a_buffer_longer_than_it() {
    let request = Request::new(&[Attr::AddedTime]);
    let mut bytes = rec(libc::ATTR_CMN_RETURNED_ATTRS | libc::ATTR_CMN_ADDEDTIME, 0, 0)
        .ts(1_750_000_000, 0)
        .build();
    bytes.extend_from_slice(&[0xAB; 24]);
    let decoded = decode_record(&bytes, &request).expect("decodes");
    assert_eq!(
        decoded.added_time.and_then(Timespec::secs_since_epoch),
        Some(1_750_000_000)
    );
    assert_eq!(decoded.name, None, "an unrequested name is absent");
}

/// The callers' `u64` buffers decode byte for byte: what the kernel wrote into
/// the words is exactly what the decoder reads. Miri runs this one.
#[test]
fn a_record_in_a_word_buffer_decodes_through_its_bytes() {
    let request = Request::new(&[Attr::AddedTime]);
    let bytes = rec(libc::ATTR_CMN_RETURNED_ATTRS | libc::ATTR_CMN_ADDEDTIME, 0, 0)
        .ts(1_750_000_000, 0)
        .build();
    let mut words = vec![0u64; bytes.len().div_ceil(8) + 1];
    for (word, chunk) in words.iter_mut().zip(bytes.chunks(8)) {
        let mut padded = [0u8; 8];
        padded[..chunk.len()].copy_from_slice(chunk);
        *word = u64::from_ne_bytes(padded);
    }

    let viewed = words_as_bytes(&words);
    assert_eq!(viewed.len(), words.len() * 8);
    assert_eq!(&viewed[..bytes.len()], bytes.as_slice());
    let decoded = decode_record(viewed, &request).expect("decodes");
    assert_eq!(
        decoded.added_time.and_then(Timespec::secs_since_epoch),
        Some(1_750_000_000)
    );
}

#[test]
fn records_walks_back_to_back_records_and_stops_at_the_first_bad_one() {
    let file = |name: &str| {
        lister_common(ALL_COMMON, 0, ALL_FILE, name, VREG)
            .u32(1)
            .u64(0)
            .u64(0)
            .build()
    };
    let mut buf = file("a");
    buf.extend(file("bb"));
    buf.extend([0u8; 4]); // a zero length: nothing past it can be located
    buf.extend(file("c"));

    let names: Vec<_> = records(&buf, 4, &LISTER)
        .map(|r| r.map(|rec| rec.name.map(<[u8]>::to_vec)))
        .collect();
    assert_eq!(
        names,
        vec![
            Ok(Some(b"a".to_vec())),
            Ok(Some(b"bb".to_vec())),
            Err(BadRecord::Length(0)),
        ]
    );
    assert_eq!(records(&buf, 1, &LISTER).count(), 1, "never past `count`");
}

#[test]
fn the_request_builds_the_attrlist_it_decodes() {
    let list = LISTER.attrlist();
    assert_eq!(list.bitmapcount, libc::ATTR_BIT_MAP_COUNT);
    assert_eq!(list.commonattr, ALL_COMMON);
    assert_eq!(list.dirattr, libc::ATTR_DIR_MOUNTSTATUS);
    assert_eq!(list.fileattr, ALL_FILE);
    assert_eq!(list.volattr, 0);

    let sizes = Request::new(&[Attr::FileDataLength]);
    assert!(sizes.contains(Attr::ObjType), "a group attribute needs the type");
    let added = Request::new(&[Attr::AddedTime]);
    assert!(!added.contains(Attr::ObjType));
    assert!(!added.contains(Attr::Name));
    assert_eq!(
        added.attrlist().commonattr,
        libc::ATTR_CMN_RETURNED_ATTRS | libc::ATTR_CMN_ADDEDTIME
    );
}

#[test]
fn type_bits_maps_each_vnode_type() {
    let bits: Vec<_> = [VREG, VDIR, VBLK, VCHR, VLNK, VSOCK, VFIFO, 0, 99]
        .into_iter()
        .map(type_bits)
        .collect();
    let want: Vec<u32> = [
        libc::S_IFREG,
        libc::S_IFDIR,
        libc::S_IFBLK,
        libc::S_IFCHR,
        libc::S_IFLNK,
        libc::S_IFSOCK,
        libc::S_IFIFO,
        0,
        0,
    ]
    .into_iter()
    .map(u32::from)
    .collect();
    assert_eq!(bits, want);
}

#[test]
fn timespec_before_1970_has_no_epoch_seconds() {
    assert_eq!(Timespec { secs: -1, nanos: 0 }.secs_since_epoch(), None);
    assert_eq!(Timespec { secs: 0, nanos: 0 }.secs_since_epoch(), Some(0));
}

// ── the index's request, over the index's own fixture ──

/// `cmdr-index`'s `bulk_read.rs` test builder, verbatim: its fixed layout for
/// the attributes it requests today.
fn index_build_record(name: &str, objtype: u32, returned_common: u32, returned_file: u32) -> Vec<u8> {
    const NAME_DATA_OFF: usize = 80;
    let name_bytes = name.as_bytes();
    let mut rec = vec![0u8; NAME_DATA_OFF + name_bytes.len() + 1];
    let put_u32 = |rec: &mut Vec<u8>, off: usize, v: u32| rec[off..off + 4].copy_from_slice(&v.to_ne_bytes());
    let put_i32 = |rec: &mut Vec<u8>, off: usize, v: i32| rec[off..off + 4].copy_from_slice(&v.to_ne_bytes());
    let put_u64 = |rec: &mut Vec<u8>, off: usize, v: u64| rec[off..off + 8].copy_from_slice(&v.to_ne_bytes());
    let put_i64 = |rec: &mut Vec<u8>, off: usize, v: i64| rec[off..off + 8].copy_from_slice(&v.to_ne_bytes());

    let len = rec.len();
    put_u32(&mut rec, 0, len as u32);
    put_u32(&mut rec, 4, returned_common);
    put_u32(&mut rec, 16, returned_file);
    put_i32(&mut rec, 24, (NAME_DATA_OFF - 24) as i32);
    put_u32(&mut rec, 28, name_bytes.len() as u32 + 1);
    put_u32(&mut rec, 32, objtype);
    put_i64(&mut rec, 36, 1_700_000_000);
    put_u64(&mut rec, 52, 4242);
    put_u32(&mut rec, 60, 3);
    put_u64(&mut rec, 64, 8192);
    put_u64(&mut rec, 72, 5000);
    rec[NAME_DATA_OFF..NAME_DATA_OFF + name_bytes.len()].copy_from_slice(name_bytes);
    rec
}

const INDEX: Request = Request::new(&[
    Attr::Name,
    Attr::ObjType,
    Attr::ModTime,
    Attr::FileId,
    Attr::FileLinkCount,
    Attr::FileAllocSize,
    Attr::FileDataLength,
]);
const INDEX_COMMON: u32 = libc::ATTR_CMN_RETURNED_ATTRS
    | libc::ATTR_CMN_NAME
    | libc::ATTR_CMN_OBJTYPE
    | libc::ATTR_CMN_MODTIME
    | libc::ATTR_CMN_FILEID;

#[test]
fn the_index_request_is_the_attrlist_the_index_sends() {
    let list = INDEX.attrlist();
    assert_eq!(list.commonattr, INDEX_COMMON);
    assert_eq!(list.dirattr, 0);
    assert_eq!(list.fileattr, ALL_FILE);
}

#[test]
fn the_index_request_decodes_what_its_own_parser_reads() {
    let bytes = index_build_record("file.txt", VREG, INDEX_COMMON, ALL_FILE);
    let decoded = decode_record(&bytes, &INDEX).expect("decodes");
    assert_eq!(decoded.name, Some(b"file.txt".as_slice()));
    assert_eq!(decoded.obj_type, Some(VREG));
    assert_eq!(
        decoded.modtime.and_then(Timespec::secs_since_epoch),
        Some(1_700_000_000)
    );
    assert_eq!(decoded.file_id, Some(4242));
    assert_eq!(decoded.link_count, Some(3));
    assert_eq!(decoded.alloc_size, Some(8192));
    assert_eq!(decoded.data_length, Some(5000));
    assert_eq!(decoded.crtime, None, "unrequested");
    assert_eq!(decoded.error, 0);

    // The index's degrade cases: a missing size bit, and a missing name.
    let half = index_build_record(
        "half.txt",
        VREG,
        INDEX_COMMON,
        libc::ATTR_FILE_LINKCOUNT | libc::ATTR_FILE_ALLOCSIZE,
    );
    let decoded = decode_record(&half, &INDEX).expect("decodes");
    assert_eq!(decoded.data_length, None);
    assert_eq!(decoded.alloc_size, Some(8192));

    let ghost = index_build_record("ghost", VREG, INDEX_COMMON & !libc::ATTR_CMN_NAME, ALL_FILE);
    assert_eq!(decode_record(&ghost, &INDEX).expect("decodes").name, None);

    let dir = index_build_record("sub", VDIR, INDEX_COMMON, 0);
    let decoded = decode_record(&dir, &INDEX).expect("decodes");
    assert_eq!(decoded.obj_type, Some(VDIR));
    assert_eq!(decoded.link_count, None);
}

// ── the kernel's own records ──

/// The hand-built cells above prove the decoder agrees with the rules; this
/// one proves the rules agree with the kernel, on whatever filesystem the temp
/// dir is (APFS on every Mac this runs on).
#[test]
#[cfg_attr(miri, ignore = "calls `getattrlistbulk`, which Miri can't cross")]
fn kernel_records_decode_to_what_lstat_reports() {
    use std::collections::HashMap;
    use std::ffi::CString;
    use std::os::fd::AsRawFd;
    use std::os::macos::fs::MetadataExt as _;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::MetadataExt;

    let dir = tempfile::tempdir().expect("temp dir");
    let root = dir.path();
    std::fs::write(root.join("small.txt"), b"hello world").unwrap();
    std::fs::write(root.join("larger.bin"), vec![7u8; 40_000]).unwrap();
    std::fs::create_dir(root.join("sub")).unwrap();
    crate::testing::symlink("small.txt", root.join("link")).unwrap();
    std::fs::hard_link(root.join("small.txt"), root.join("alias.txt")).unwrap();

    let handle = std::fs::File::open(root).unwrap();
    let mut list = LISTER.attrlist();
    let mut buf = vec![0u64; 8192];
    let buf_len = buf.len() * 8;
    // SAFETY: `handle` is an open directory, `list` a valid `attrlist`, and `buf`
    // writable for `buf_len` bytes, which is all the kernel writes.
    let n = unsafe {
        libc::getattrlistbulk(
            handle.as_raw_fd(),
            (&raw mut list).cast(),
            buf.as_mut_ptr().cast(),
            buf_len,
            u64::from(libc::FSOPT_PACK_INVAL_ATTRS),
        )
    };
    assert_eq!(n, 5, "one call returns all five entries");
    let bytes = words_as_bytes(&buf);

    let decoded: HashMap<Vec<u8>, Record<'_>> = records(bytes, 5, &LISTER)
        .map(|r| {
            let rec = r.expect("the kernel's records decode");
            (rec.name.expect("named").to_vec(), rec)
        })
        .collect();
    assert_eq!(decoded.len(), 5);

    for (name, rec) in &decoded {
        let path = root.join(std::ffi::OsStr::from_bytes(name));
        let md = std::fs::symlink_metadata(&path).unwrap();
        let what = String::from_utf8_lossy(name);
        let want_type = if md.is_symlink() {
            VLNK
        } else if md.is_dir() {
            VDIR
        } else {
            VREG
        };
        assert_eq!(rec.error, 0, "{what}");
        assert_eq!(rec.obj_type, Some(want_type), "{what}");
        assert_eq!(
            rec.access_mask.map(|m| (m & 0o7777) | type_bits(want_type)),
            Some(md.mode()),
            "{what}"
        );
        assert_eq!(rec.owner_id, Some(md.uid()), "{what}");
        assert_eq!(rec.group_id, Some(md.gid()), "{what}");
        assert_eq!(rec.flags, Some(md.st_flags()), "{what}");
        assert_eq!(rec.file_id, Some(md.ino()), "{what}");
        assert_eq!(
            rec.modtime,
            Some(Timespec {
                secs: md.mtime(),
                nanos: md.mtime_nsec()
            }),
            "{what}"
        );
        assert_eq!(
            rec.crtime,
            Some(Timespec {
                secs: md.st_birthtime(),
                nanos: md.st_birthtime_nsec()
            }),
            "{what}"
        );
        assert!(rec.added_time.is_some(), "APFS records an added time for {what}");
        if want_type == VDIR {
            assert_eq!(rec.mount_status, Some(0), "{what}");
            assert_eq!(rec.link_count, None, "{what}");
        } else {
            assert_eq!(rec.mount_status, None, "{what}");
            assert_eq!(rec.link_count.map(u64::from), Some(md.nlink()), "{what}");
            assert_eq!(rec.alloc_size, Some(md.blocks() * 512), "{what}");
            assert_eq!(rec.data_length, Some(md.len()), "{what}");
        }
    }

    // One `getattrlist` by path decodes the same added time the bulk record carried.
    let added_only = Request::new(&[Attr::AddedTime]);
    let mut list = added_only.attrlist();
    let mut one = [0u64; 8];
    let c_path = CString::new(root.join("small.txt").as_os_str().as_bytes()).unwrap();
    // SAFETY: `c_path` is NUL-terminated, `list` a valid `attrlist`, and `one`
    // writable for its 64 bytes.
    let rc = unsafe {
        libc::getattrlist(
            c_path.as_ptr(),
            (&raw mut list).cast(),
            one.as_mut_ptr().cast(),
            64,
            libc::FSOPT_NOFOLLOW | libc::FSOPT_PACK_INVAL_ATTRS,
        )
    };
    assert_eq!(rc, 0);
    assert_eq!(
        decode_record(words_as_bytes(&one), &added_only)
            .expect("decodes")
            .added_time,
        decoded[b"small.txt".as_slice()].added_time
    );
}
