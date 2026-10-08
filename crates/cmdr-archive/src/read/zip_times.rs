//! Which zip entries record only an MS-DOS time, and what that time means.
//!
//! The DOS date-and-time field carries no zone. By convention (Info-ZIP, Windows
//! Explorer, macOS Archive Utility, `unzip -l`) it's the writer's wall clock, so
//! we read it as local time. An entry with a timestamp extra field (Info-ZIP `UT`,
//! NTFS, PKWARE Unix) records the exact UTC second, and rc-zip already reads that
//! one right. rc-zip reads a DOS-only entry as UTC, though, and its `Entry` doesn't
//! say which source a time came from, so [`dos_only_wall_clocks`] walks the
//! central directory a second time, by hand, for just that: each record's DOS
//! field and the tags of its extra fields.
//!
//! The walk is a cross-check, never a second source of truth for anything else:
//! when it can't line up with rc-zip's parse (a read fails, the layout surprises
//! it, a CRC differs), it answers `None` and every entry keeps rc-zip's time.

use std::io::{BufReader, Read};

use chrono::{LocalResult, NaiveDate, NaiveDateTime, Offset, TimeZone};
use rc_zip::Entry;

use super::source::ArchiveByteSource;

/// End of central directory record: signature, then fixed fields to a 2-byte
/// comment length at offset 20.
const EOCD_SIGNATURE: &[u8; 4] = b"PK\x05\x06";
const EOCD_LEN: usize = 22;
/// The EOCD sits within the file's last `EOCD_LEN` + 65,535 (max comment) bytes.
const EOCD_SEARCH_LEN: u64 = EOCD_LEN as u64 + u16::MAX as u64;
/// Zip64 end of central directory locator, right before the EOCD.
const ZIP64_LOCATOR_SIGNATURE: &[u8; 4] = b"PK\x06\x07";
const ZIP64_LOCATOR_LEN: usize = 20;
/// Zip64 end of central directory record, fixed part.
const ZIP64_EOCD_SIGNATURE: &[u8; 4] = b"PK\x06\x06";
const ZIP64_EOCD_LEN: usize = 56;
/// Central directory file header, fixed part.
const CD_HEADER_SIGNATURE: &[u8; 4] = b"PK\x01\x02";
const CD_HEADER_LEN: usize = 46;

/// The extra fields rc-zip takes an entry's modified time from (its
/// `Entry::set_extra_field`, rc-zip 5.4.1).
const EXTENDED_TIMESTAMP_TAG: u16 = 0x5455;
const NTFS_TAG: u16 = 0x000a;
const PKWARE_UNIX_TAG: u16 = 0x000d;

/// For each entry in central-directory order: its DOS date and time when that's
/// the only time it records, `None` when an extra field records a better one (or
/// the DOS field isn't a real date, which rc-zip reads as the epoch either way).
///
/// `None` overall when the walk can't line up with `entries`, rc-zip's parse of
/// the same directory: then the caller keeps rc-zip's times for every entry.
pub(super) fn dos_only_wall_clocks(
    source: &dyn ArchiveByteSource,
    entries: &[Entry],
) -> Option<Vec<Option<NaiveDateTime>>> {
    let (offset, size) = locate_central_directory(source)?;
    let mut reader = BufReader::with_capacity(64 * 1024, SourceAt { source, offset });
    let mut consumed = 0u64;
    let mut variable = Vec::new();
    let mut out = Vec::with_capacity(entries.len());
    for entry in entries {
        let mut header = [0u8; CD_HEADER_LEN];
        reader.read_exact(&mut header).ok()?;
        if &header[..4] != CD_HEADER_SIGNATURE || le_u32(&header, 16) != entry.crc32 {
            return None;
        }
        let name_len = usize::from(le_u16(&header, 28));
        let extra_len = usize::from(le_u16(&header, 30));
        let comment_len = usize::from(le_u16(&header, 32));
        variable.resize(name_len + extra_len + comment_len, 0);
        reader.read_exact(&mut variable).ok()?;
        consumed += (CD_HEADER_LEN + variable.len()) as u64;
        if consumed > size {
            return None;
        }
        let extra = &variable[name_len..name_len + extra_len];
        out.push(if has_timestamp_extra(extra) {
            None
        } else {
            dos_wall_clock(le_u16(&header, 14), le_u16(&header, 12))
        });
    }
    Some(out)
}

/// A wall-clock time in `zone` as Unix seconds.
///
/// An ambiguous clock (the hour a fall-back repeats) takes the earlier instant;
/// a skipped one (the hour a spring-forward jumps over, which no clock in that
/// zone showed) takes the offset in force just before the jump.
pub(super) fn wall_clock_to_unix<Tz: TimeZone>(wall: NaiveDateTime, zone: &Tz) -> i64 {
    match zone.from_local_datetime(&wall) {
        LocalResult::Single(at) | LocalResult::Ambiguous(at, _) => at.timestamp(),
        LocalResult::None => {
            let offset = zone.offset_from_utc_datetime(&wall).fix();
            wall.and_utc().timestamp() - i64::from(offset.local_minus_utc())
        }
    }
}

/// Where the central directory starts and how long it is, found the way rc-zip
/// finds it: the EOCD, then the zip64 record its locator points at, and the
/// directory ending where that record (or the EOCD) begins.
fn locate_central_directory(source: &dyn ArchiveByteSource) -> Option<(u64, u64)> {
    let file_size = source.size();
    let tail_start = file_size.saturating_sub(EOCD_SEARCH_LEN);
    let tail = read_exact_at(source, tail_start, usize::try_from(file_size - tail_start).ok()?)?;
    let eocd_in_tail = (0..=tail.len().checked_sub(EOCD_LEN)?).rev().find(|&i| {
        &tail[i..i + 4] == EOCD_SIGNATURE && i + EOCD_LEN + usize::from(le_u16(&tail, i + 20)) <= tail.len()
    })?;
    let eocd_offset = tail_start + eocd_in_tail as u64;

    let zip64_record_offset = eocd_offset
        .checked_sub(ZIP64_LOCATOR_LEN as u64)
        .and_then(|at| read_exact_at(source, at, ZIP64_LOCATOR_LEN))
        .filter(|locator| &locator[..4] == ZIP64_LOCATOR_SIGNATURE)
        .map(|locator| le_u64(&locator, 8));
    let (directory_end, directory_size) = match zip64_record_offset {
        Some(record_offset) => {
            let record = read_exact_at(source, record_offset, ZIP64_EOCD_LEN)?;
            if &record[..4] != ZIP64_EOCD_SIGNATURE {
                return None;
            }
            (record_offset, le_u64(&record, 40))
        }
        None => (eocd_offset, u64::from(le_u32(&tail, eocd_in_tail + 12))),
    };
    Some((directory_end.checked_sub(directory_size)?, directory_size))
}

/// Whether an entry's extra fields hold a modified time rc-zip prefers over the
/// DOS field.
fn has_timestamp_extra(mut extra: &[u8]) -> bool {
    while extra.len() >= 4 {
        let tag = le_u16(extra, 0);
        let len = usize::from(le_u16(extra, 2));
        let Some(payload) = extra.get(4..4 + len) else {
            return false;
        };
        let carries_mtime = match tag {
            // Flags byte, bit 0 = "modified time follows", then that time.
            EXTENDED_TIMESTAMP_TAG => payload.len() >= 5 && payload[0] & 1 != 0,
            NTFS_TAG => ntfs_has_times(payload),
            // atime, mtime (u32 each), uid, gid (u16 each).
            PKWARE_UNIX_TAG => payload.len() >= 12,
            _ => false,
        };
        if carries_mtime {
            return true;
        }
        extra = &extra[4 + len..];
    }
    false
}

/// Whether an NTFS extra field holds attribute 1 (modified, accessed, created).
fn ntfs_has_times(payload: &[u8]) -> bool {
    // Four reserved bytes, then tag-size-value attributes.
    let mut attrs = payload.get(4..).unwrap_or_default();
    while attrs.len() >= 4 {
        let tag = le_u16(attrs, 0);
        let len = usize::from(le_u16(attrs, 2));
        if tag == 0x0001 && len >= 24 && attrs.len() >= 4 + 24 {
            return true;
        }
        let Some(rest) = attrs.get(4 + len..) else {
            return false;
        };
        attrs = rest;
    }
    false
}

/// An MS-DOS date and time as a wall clock, or `None` when it isn't a real one.
fn dos_wall_clock(date: u16, time: u16) -> Option<NaiveDateTime> {
    let day = u32::from(date & 0x1f);
    let month = u32::from((date >> 5) & 0x0f);
    let year = i32::from(date >> 9) + 1980;
    let second = u32::from(time & 0x1f) * 2;
    let minute = u32::from((time >> 5) & 0x3f);
    let hour = u32::from(time >> 11);
    NaiveDate::from_ymd_opt(year, month, day)?.and_hms_opt(hour, minute, second)
}

/// `len` bytes at `offset`, or `None` on a short or failed read.
fn read_exact_at(source: &dyn ArchiveByteSource, offset: u64, len: usize) -> Option<Vec<u8>> {
    let mut buf = vec![0u8; len];
    let mut reader = SourceAt { source, offset };
    reader.read_exact(&mut buf).ok()?;
    Some(buf)
}

/// A forward `Read` over a byte source from a starting offset.
struct SourceAt<'a> {
    source: &'a dyn ArchiveByteSource,
    offset: u64,
}

impl Read for SourceAt<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.source.read_at(self.offset, buf)?;
        self.offset += n as u64;
        Ok(n)
    }
}

fn le_u16(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([bytes[at], bytes[at + 1]])
}

fn le_u32(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

fn le_u64(bytes: &[u8], at: usize) -> u64 {
    let mut word = [0u8; 8];
    word.copy_from_slice(&bytes[at..at + 8]);
    u64::from_le_bytes(word)
}
