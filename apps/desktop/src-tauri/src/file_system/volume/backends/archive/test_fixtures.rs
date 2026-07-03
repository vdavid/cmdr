//! Fixture builders for the archive tests.
//!
//! Clean fixtures are built programmatically with the `zip` crate (no binary
//! blobs checked in). Hostile fixtures — traversal names, an encrypted flag, a
//! corrupt record count, a non-UTF-8 name — start from a clean zip and patch
//! raw bytes, which keeps them tiny and their intent obvious.

use std::io::{Cursor, Write};

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

/// A file to write into a fixture zip.
pub struct FixtureFile {
    pub name: String,
    pub content: Vec<u8>,
    pub method: CompressionMethod,
    /// Force a zip64 entry (zip64 extra field + zip64 EOCD) regardless of size.
    pub zip64: bool,
}

pub fn stored(name: impl Into<String>, content: impl Into<Vec<u8>>) -> FixtureFile {
    FixtureFile {
        name: name.into(),
        content: content.into(),
        method: CompressionMethod::Stored,
        zip64: false,
    }
}

pub fn deflated(name: impl Into<String>, content: impl Into<Vec<u8>>) -> FixtureFile {
    FixtureFile {
        name: name.into(),
        content: content.into(),
        method: CompressionMethod::Deflated,
        zip64: false,
    }
}

/// An explicit directory entry (trailing slash, no content).
pub fn dir(name: impl Into<String>) -> FixtureFile {
    FixtureFile {
        name: name.into(),
        content: Vec::new(),
        method: CompressionMethod::Stored,
        zip64: false,
    }
}

/// A stored entry forced into the zip64 layout (no 4 GB payload needed).
pub fn zip64_stored(name: impl Into<String>, content: impl Into<Vec<u8>>) -> FixtureFile {
    FixtureFile {
        name: name.into(),
        content: content.into(),
        method: CompressionMethod::Stored,
        zip64: true,
    }
}

/// Builds a zip archive from the given entries and returns the raw bytes. A
/// `name` ending in `/` is written as a directory entry.
pub fn build_zip(entries: &[FixtureFile]) -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for entry in entries {
        let opts = SimpleFileOptions::default()
            .compression_method(entry.method)
            .large_file(entry.zip64);
        if entry.name.ends_with('/') {
            writer.add_directory(entry.name.trim_end_matches('/'), opts).unwrap();
        } else {
            writer.start_file(&*entry.name, opts).unwrap();
            writer.write_all(&entry.content).unwrap();
        }
    }
    writer.finish().unwrap().into_inner()
}

/// Replaces every occurrence of `from` with `to` in `bytes`. Requires equal
/// length so header offsets and record sizes are preserved (patching a name in
/// both its local and central-directory headers without shifting the layout).
pub fn patch_equal_len(bytes: &mut [u8], from: &[u8], to: &[u8]) {
    assert_eq!(from.len(), to.len(), "patch replacement must preserve length");
    let mut idx = 0;
    let mut hits = 0;
    while idx + from.len() <= bytes.len() {
        if &bytes[idx..idx + from.len()] == from {
            bytes[idx..idx + from.len()].copy_from_slice(to);
            idx += from.len();
            hits += 1;
        } else {
            idx += 1;
        }
    }
    assert!(hits > 0, "patch pattern {from:?} not found");
}

/// Central-directory file header signature (`PK\x01\x02`).
const CD_HEADER_SIG: [u8; 4] = [0x50, 0x4b, 0x01, 0x02];
/// End-of-central-directory record signature (`PK\x05\x06`).
const EOCD_SIG: [u8; 4] = [0x50, 0x4b, 0x05, 0x06];

/// Sets general-purpose bit-flag 0 (encrypted) on the first central-directory
/// file header. rc-zip reads entry flags from the central directory, so this is
/// enough to make the entry parse as encrypted.
pub fn set_first_entry_encrypted(bytes: &mut [u8]) {
    let cd = find(bytes, &CD_HEADER_SIG).expect("central-directory header not found");
    // GP flag is 2 bytes at offset 8 within the CD file header.
    bytes[cd + 8] |= 0x01;
}

/// Overstates the record count in the end-of-central-directory record so the
/// central-directory parse reads fewer headers than claimed — a corrupt archive
/// that still has a valid EOCD signature.
pub fn overstate_record_count(bytes: &mut [u8]) {
    let eocd = find(bytes, &EOCD_SIG).expect("EOCD not found");
    // "records on this disk" (offset 8) and "total records" (offset 10), u16 LE.
    bytes[eocd + 8..eocd + 10].copy_from_slice(&99u16.to_le_bytes());
    bytes[eocd + 10..eocd + 12].copy_from_slice(&99u16.to_le_bytes());
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}
