//! The one decoder for the packed attribute records `getattrlist(2)` and
//! `getattrlistbulk(2)` write (macOS).
//!
//! A caller says which attributes it wants with a [`Request`], hands the kernel
//! [`Request::attrlist`], and decodes what comes back with [`records`] (a bulk
//! buffer) or [`decode_record`] (one record). The request is the ONLY input to
//! the layout, so asking for one set and decoding another can't happen.
//!
//! Pure: no syscall, no `FileEntry`, no fallback policy. Each caller keeps its own
//! loop and decides what a missing attribute means for it. Every call must pass
//! `FSOPT_PACK_INVAL_ATTRS`: offsets come from the REQUESTED set, validity from
//! the returned one. The layout rules and why: `crates/cmdr-fs/DETAILS.md`
//! § "`attr_buf`: the packed attribute layout".

/// `ATTR_CMN_ERROR` from `<sys/attr.h>`; `libc` doesn't carry it.
const ATTR_CMN_ERROR: u32 = 0x2000_0000;

/// `enum vtype` from `<sys/vnode.h>`: a regular file.
pub const VREG: u32 = 1;
/// `enum vtype`: a directory.
pub const VDIR: u32 = 2;
/// `enum vtype`: a block device.
pub const VBLK: u32 = 3;
/// `enum vtype`: a character device.
pub const VCHR: u32 = 4;
/// `enum vtype`: a symbolic link.
pub const VLNK: u32 = 5;
/// `enum vtype`: a socket.
pub const VSOCK: u32 = 6;
/// `enum vtype`: a named pipe.
pub const VFIFO: u32 = 7;

/// A record's fixed header: its `u32` length, then the returned
/// `attribute_set_t` (five `u32` bitmaps).
const HEADER_LEN: usize = 24;

/// One attribute a [`Request`] can ask for. The variant order is the order the
/// kernel packs them in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Attr {
    /// `ATTR_CMN_ERROR`: why the kernel couldn't read this entry's attributes.
    Error,
    /// `ATTR_CMN_NAME`: the entry's name, raw bytes.
    Name,
    /// `ATTR_CMN_OBJTYPE`: the `vtype` ([`VREG`], [`VDIR`], [`VLNK`], ...).
    ObjType,
    /// `ATTR_CMN_CRTIME`: the birth time.
    CrTime,
    /// `ATTR_CMN_MODTIME`: the content modification time.
    ModTime,
    /// `ATTR_CMN_OWNERID`: the owner's uid.
    OwnerId,
    /// `ATTR_CMN_GRPID`: the group's gid.
    GrpId,
    /// `ATTR_CMN_ACCESSMASK`: the mode bits.
    AccessMask,
    /// `ATTR_CMN_FLAGS`: the `st_flags` word (`UF_HIDDEN`, `SF_FIRMLINK`, ...).
    Flags,
    /// `ATTR_CMN_FILEID`: the inode number.
    FileId,
    /// `ATTR_CMN_ADDEDTIME`: when the entry landed in its folder.
    AddedTime,
    /// `ATTR_DIR_MOUNTSTATUS`: a directory's mount status (`DIR_MNTSTATUS_MNTPOINT`).
    DirMountStatus,
    /// `ATTR_FILE_LINKCOUNT`: a non-directory's hard link count.
    FileLinkCount,
    /// `ATTR_FILE_ALLOCSIZE`: a non-directory's allocated bytes on disk.
    FileAllocSize,
    /// `ATTR_FILE_DATALENGTH`: a non-directory's logical size.
    FileDataLength,
}

/// Which `attribute_set_t` group an attribute lives in.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Group {
    Common,
    Dir,
    File,
}

/// How many bytes an attribute's slot takes.
#[derive(Clone, Copy)]
enum Width {
    U32,
    U64,
    /// `struct timespec`: `i64` seconds, then `i64` nanoseconds.
    Timespec,
    /// `attrreference_t`: an `i32` offset from the slot, then a `u32` length.
    Reference,
}

/// Every attribute, in packing order.
const PACKING_ORDER: [Attr; 15] = [
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
];

impl Attr {
    /// The attribute's group, its bit within that group's bitmap, and its slot width.
    const fn spec(self) -> (Group, u32, Width) {
        match self {
            Self::Error => (Group::Common, ATTR_CMN_ERROR, Width::U32),
            Self::Name => (Group::Common, libc::ATTR_CMN_NAME, Width::Reference),
            Self::ObjType => (Group::Common, libc::ATTR_CMN_OBJTYPE, Width::U32),
            Self::CrTime => (Group::Common, libc::ATTR_CMN_CRTIME, Width::Timespec),
            Self::ModTime => (Group::Common, libc::ATTR_CMN_MODTIME, Width::Timespec),
            Self::OwnerId => (Group::Common, libc::ATTR_CMN_OWNERID, Width::U32),
            Self::GrpId => (Group::Common, libc::ATTR_CMN_GRPID, Width::U32),
            Self::AccessMask => (Group::Common, libc::ATTR_CMN_ACCESSMASK, Width::U32),
            Self::Flags => (Group::Common, libc::ATTR_CMN_FLAGS, Width::U32),
            Self::FileId => (Group::Common, libc::ATTR_CMN_FILEID, Width::U64),
            Self::AddedTime => (Group::Common, libc::ATTR_CMN_ADDEDTIME, Width::Timespec),
            Self::DirMountStatus => (Group::Dir, libc::ATTR_DIR_MOUNTSTATUS, Width::U32),
            Self::FileLinkCount => (Group::File, libc::ATTR_FILE_LINKCOUNT, Width::U32),
            Self::FileAllocSize => (Group::File, libc::ATTR_FILE_ALLOCSIZE, Width::U64),
            Self::FileDataLength => (Group::File, libc::ATTR_FILE_DATALENGTH, Width::U64),
        }
    }
}

/// A typed set of attributes to ask the kernel for, and the only description of
/// the record layout [`decode_record`] reads. `ATTR_CMN_RETURNED_ATTRS` is always
/// in it (validity comes from it), and so is [`Attr::ObjType`] whenever a dir or
/// file attribute is (it decides which of those groups a record packs).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Request {
    common: u32,
    dir: u32,
    file: u32,
}

impl Request {
    /// The request for exactly `attrs` (plus the two implied above). Order and
    /// duplicates in `attrs` don't matter.
    #[must_use]
    pub const fn new(attrs: &[Attr]) -> Self {
        let mut request = Self {
            common: libc::ATTR_CMN_RETURNED_ATTRS,
            dir: 0,
            file: 0,
        };
        let mut i = 0;
        while i < attrs.len() {
            let (group, bit, _) = attrs[i].spec();
            match group {
                Group::Common => request.common |= bit,
                Group::Dir => request.dir |= bit,
                Group::File => request.file |= bit,
            }
            i += 1;
        }
        if request.dir != 0 || request.file != 0 {
            request.common |= libc::ATTR_CMN_OBJTYPE;
        }
        request
    }

    /// Whether this request asks for `attr`.
    #[must_use]
    pub const fn contains(&self, attr: Attr) -> bool {
        let (group, bit, _) = attr.spec();
        let set = match group {
            Group::Common => self.common,
            Group::Dir => self.dir,
            Group::File => self.file,
        };
        set & bit != 0
    }

    /// The `struct attrlist` to hand `getattrlist` / `getattrlistbulk`, with
    /// `FSOPT_PACK_INVAL_ATTRS` in the call's options (the decoder relies on it).
    #[must_use]
    pub const fn attrlist(&self) -> libc::attrlist {
        libc::attrlist {
            bitmapcount: libc::ATTR_BIT_MAP_COUNT,
            reserved: 0,
            commonattr: self.common,
            volattr: 0,
            dirattr: self.dir,
            fileattr: self.file,
            forkattr: 0,
        }
    }
}

/// A packed `struct timespec`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Timespec {
    /// Seconds since the Unix epoch; negative before 1970.
    pub secs: i64,
    /// Nanoseconds past `secs`.
    pub nanos: i64,
}

impl Timespec {
    /// Whole seconds since the epoch, or `None` before 1970 (what `FileEntry`'s
    /// dates and the index's mtime hold).
    #[must_use]
    pub fn secs_since_epoch(self) -> Option<u64> {
        u64::try_from(self.secs).ok()
    }
}

/// One decoded record. Each attribute is `Some` only when it was requested AND
/// its returned bit is set: an unsupported one reads `None`, never zero. A dir
/// attribute is `None` on a non-directory and a file one on a directory, since
/// the kernel doesn't pack them there; both are `None` when the type itself
/// isn't valid, because then nobody knows which group was packed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Record<'a> {
    /// `ATTR_CMN_ERROR`: an errno, or 0. When non-zero, only `name` is decoded:
    /// what else an errored record packs is unspecified.
    pub error: u32,
    /// The name's bytes, without the trailing NUL.
    pub name: Option<&'a [u8]>,
    /// The `vtype` ([`VREG`], [`VDIR`], [`VLNK`], ...).
    pub obj_type: Option<u32>,
    /// The birth time.
    pub crtime: Option<Timespec>,
    /// The content modification time.
    pub modtime: Option<Timespec>,
    /// The owner's uid.
    pub owner_id: Option<u32>,
    /// The group's gid.
    pub group_id: Option<u32>,
    /// The mode bits. APFS includes the `S_IFMT` type bits; other filesystems may not.
    pub access_mask: Option<u32>,
    /// The `st_flags` word.
    pub flags: Option<u32>,
    /// The inode number.
    pub file_id: Option<u64>,
    /// When the entry landed in its folder.
    pub added_time: Option<Timespec>,
    /// A directory's mount status.
    pub mount_status: Option<u32>,
    /// A non-directory's hard link count.
    pub link_count: Option<u32>,
    /// A non-directory's allocated bytes on disk.
    pub alloc_size: Option<u64>,
    /// A non-directory's logical size.
    pub data_length: Option<u64>,
}

impl Record<'_> {
    /// A record nothing has been read into yet: every attribute absent, no error.
    const fn unread() -> Self {
        Self {
            error: 0,
            name: None,
            obj_type: None,
            crtime: None,
            modtime: None,
            owner_id: None,
            group_id: None,
            access_mask: None,
            flags: None,
            file_id: None,
            added_time: None,
            mount_status: None,
            link_count: None,
            alloc_size: None,
            data_length: None,
        }
    }
}

/// Why a record can't be decoded at all. Any of these means the buffer isn't
/// what the request said it would be, so a caller distrusts the rest of it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BadRecord {
    /// The length field is shorter than the 24-byte header or runs past the
    /// buffer (the value it held; 0 when the buffer can't even hold the field).
    Length(u32),
    /// A requested attribute's slot runs past the record's end.
    Truncated,
    /// The name reference is empty or points outside the record.
    Name,
}

/// Decodes the record at the front of `record`, which may run past its end (a
/// `getattrlist` buffer is usually bigger than what the kernel wrote).
///
/// # Errors
///
/// [`BadRecord`] when the record's length or name is implausible, or a slot the
/// request implies runs past the record.
pub fn decode_record<'a>(record: &'a [u8], req: &Request) -> Result<Record<'a>, BadRecord> {
    decode_at(record, req).map(|(decoded, _)| decoded)
}

/// The records of one `getattrlistbulk` call: `count` records packed back to
/// back at the front of `buf`. Yields each, and stops after the first
/// [`BadRecord`], since nothing past it can be located.
#[must_use]
pub fn records<'a>(buf: &'a [u8], count: usize, req: &Request) -> Records<'a> {
    Records {
        buf,
        req: *req,
        remaining: count,
        offset: 0,
    }
}

/// The bytes of a `u64` buffer the kernel wrote records into, for [`records`]
/// and [`decode_record`]. Callers hand the kernel `u64`s so the allocation is
/// 8-aligned; the decoder reads bytes.
#[must_use]
pub fn words_as_bytes(words: &[u64]) -> &[u8] {
    // SAFETY: `u8` has alignment 1 and no invalid bit patterns, `words` is fully
    // initialized, and the byte slice covers exactly `words`' memory and borrows
    // it, so nothing can write to it meanwhile. Miri runs this
    // (`desktop-rust-miri`).
    unsafe { std::slice::from_raw_parts(words.as_ptr().cast::<u8>(), size_of_val(words)) }
}

/// The iterator [`records`] returns.
#[derive(Debug)]
pub struct Records<'a> {
    buf: &'a [u8],
    req: Request,
    remaining: usize,
    offset: usize,
}

impl<'a> Iterator for Records<'a> {
    type Item = Result<Record<'a>, BadRecord>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        let rest = self.buf.get(self.offset..).unwrap_or_default();
        match decode_at(rest, &self.req) {
            Ok((decoded, len)) => {
                self.remaining -= 1;
                self.offset += len;
                Some(Ok(decoded))
            }
            Err(bad) => {
                self.remaining = 0;
                Some(Err(bad))
            }
        }
    }
}

/// Decodes one record and returns it with its declared length.
fn decode_at<'a>(bytes: &'a [u8], req: &Request) -> Result<(Record<'a>, usize), BadRecord> {
    let declared = u32_at(bytes, 0).ok_or(BadRecord::Length(0))?;
    let record = usize::try_from(declared)
        .ok()
        .filter(|&len| len >= HEADER_LEN)
        .and_then(|len| bytes.get(..len))
        .ok_or(BadRecord::Length(declared))?;
    // The returned `attribute_set_t`: common, vol, dir, file, fork.
    let returned = |group: Group| {
        let at = match group {
            Group::Common => 4,
            Group::Dir => 12,
            Group::File => 16,
        };
        u32_at(record, at).unwrap_or(0)
    };

    let mut out = Record::unread();
    let mut cursor = HEADER_LEN;
    for attr in PACKING_ORDER {
        if !req.contains(attr) {
            continue;
        }
        if out.error != 0 && attr != Attr::Name {
            break;
        }
        let (group, bit, width) = attr.spec();
        let packed = match group {
            Group::Common => true,
            Group::Dir => out.obj_type == Some(VDIR),
            Group::File => out.obj_type.is_some_and(|t| t != VDIR),
        };
        if !packed {
            continue;
        }
        let slot = cursor;
        cursor += width.len();
        if cursor > record.len() {
            return Err(BadRecord::Truncated);
        }
        // `ATTR_CMN_ERROR` is read whatever its bit says: it's 0 on a healthy record.
        if attr != Attr::Error && returned(group) & bit == 0 {
            continue;
        }
        let u32_slot = || u32_at(record, slot);
        let u64_slot = || u64_at(record, slot);
        let time_slot = || timespec_at(record, slot);
        match attr {
            Attr::Error => out.error = u32_slot().unwrap_or(0),
            Attr::Name => out.name = Some(name_at(record, slot)?),
            Attr::ObjType => out.obj_type = u32_slot(),
            Attr::CrTime => out.crtime = time_slot(),
            Attr::ModTime => out.modtime = time_slot(),
            Attr::OwnerId => out.owner_id = u32_slot(),
            Attr::GrpId => out.group_id = u32_slot(),
            Attr::AccessMask => out.access_mask = u32_slot(),
            Attr::Flags => out.flags = u32_slot(),
            Attr::FileId => out.file_id = u64_slot(),
            Attr::AddedTime => out.added_time = time_slot(),
            Attr::DirMountStatus => out.mount_status = u32_slot(),
            Attr::FileLinkCount => out.link_count = u32_slot(),
            Attr::FileAllocSize => out.alloc_size = u64_slot(),
            Attr::FileDataLength => out.data_length = u64_slot(),
        }
    }
    Ok((out, record.len()))
}

impl Width {
    const fn len(self) -> usize {
        match self {
            Self::U32 => 4,
            Self::U64 | Self::Reference => 8,
            Self::Timespec => 16,
        }
    }
}

/// The name an `attrreference_t` at `slot` points to: `length` bytes (the
/// trailing NUL included) at `slot + offset`, inside the record.
fn name_at(record: &[u8], slot: usize) -> Result<&[u8], BadRecord> {
    let offset = i32_at(record, slot).ok_or(BadRecord::Name)?;
    let len = u32_at(record, slot + 4).ok_or(BadRecord::Name)?;
    let len = usize::try_from(len)
        .ok()
        .filter(|&len| len > 0)
        .ok_or(BadRecord::Name)?;
    let start = slot
        .checked_add_signed(isize::try_from(offset).map_err(|_| BadRecord::Name)?)
        .ok_or(BadRecord::Name)?;
    let end = start.checked_add(len - 1).ok_or(BadRecord::Name)?;
    record.get(start..end).ok_or(BadRecord::Name)
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_ne_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

fn i32_at(b: &[u8], at: usize) -> Option<i32> {
    Some(i32::from_ne_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

fn u64_at(b: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_ne_bytes(b.get(at..at + 8)?.try_into().ok()?))
}

fn i64_at(b: &[u8], at: usize) -> Option<i64> {
    Some(i64::from_ne_bytes(b.get(at..at + 8)?.try_into().ok()?))
}

fn timespec_at(b: &[u8], at: usize) -> Option<Timespec> {
    Some(Timespec {
        secs: i64_at(b, at)?,
        nanos: i64_at(b, at + 8)?,
    })
}

/// The `S_IFMT` bits `st_mode` carries for a `vtype`, so a mode built from
/// [`Record::access_mask`] matches `Metadata::mode()`. 0 for an unknown type.
#[must_use]
pub fn type_bits(obj_type: u32) -> u32 {
    u32::from(match obj_type {
        VREG => libc::S_IFREG,
        VDIR => libc::S_IFDIR,
        VBLK => libc::S_IFBLK,
        VCHR => libc::S_IFCHR,
        VLNK => libc::S_IFLNK,
        VSOCK => libc::S_IFSOCK,
        VFIFO => libc::S_IFIFO,
        _ => 0,
    })
}

#[cfg(test)]
#[path = "attr_buf_test.rs"]
mod tests;
