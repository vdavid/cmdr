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

/// Serializes a zip whose flagged entries are WinZip-AES-encrypted (AE-2) with
/// `password` at the given key size, using the `zip` crate's own writer (the
/// `aes-crypto` feature the production decrypt path also relies on). Entry order
/// is preserved, so each entry's central-directory ordinal is its index here —
/// what the AES ordinal-alignment test relies on. Unlike ZipCrypto (hand-rolled
/// below for an independent-implementation cross-check), AES has no trivial
/// hand-serialization, so the writer is the fixture source.
pub fn build_aes_zip(entries: &[CryptoFixtureFile], password: &str, mode: zip::AesMode) -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for entry in entries {
        let mut opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        if entry.encrypted {
            opts = opts.with_aes_encryption(mode, password);
        }
        writer.start_file(&*entry.name, opts).unwrap();
        writer.write_all(&entry.content).unwrap();
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

// ── Legacy PKWARE ZipCrypto fixtures ─────────────────────────────────────────
//
// The `zip` crate can't WRITE ZipCrypto from external code (its
// `with_deprecated_encryption` is `pub(crate)`), so a minimal, spec-correct
// STORED-entry serializer builds encrypted fixtures here. ZipCrypto is a fixed,
// simple stream cipher, so this stays compatible with the `zip` crate's
// `by_index_decrypt` (the production read path) and with rc-zip's central-directory
// parse. STORED only (compressed == uncompressed) keeps the serializer trivial; the
// decrypt path is identical for stored and deflated entries. (AES fixtures, by
// contrast, use the `zip` writer directly — see [`build_aes_zip`].)

/// One entry for [`build_zipcrypto_zip`]: name, plaintext content, and whether to
/// ZipCrypto-encrypt it (so a single fixture can mix encrypted and plain entries).
pub struct CryptoFixtureFile {
    pub name: String,
    pub content: Vec<u8>,
    pub encrypted: bool,
}

pub fn encrypted_entry(name: impl Into<String>, content: impl Into<Vec<u8>>) -> CryptoFixtureFile {
    CryptoFixtureFile {
        name: name.into(),
        content: content.into(),
        encrypted: true,
    }
}

pub fn plain_entry(name: impl Into<String>, content: impl Into<Vec<u8>>) -> CryptoFixtureFile {
    CryptoFixtureFile {
        name: name.into(),
        content: content.into(),
        encrypted: false,
    }
}

/// General-purpose bit-flag 0 (entry is encrypted).
const GP_ENCRYPTED: u16 = 1 << 0;
/// The ZipCrypto encryption header prepended to each encrypted entry's data.
const ZIPCRYPTO_HEADER_LEN: usize = 12;

/// Serializes a minimal STORED-entry zip, ZipCrypto-encrypting the flagged
/// entries with `password`. Entry order is preserved, so the central-directory
/// ordinal of each entry is its index here — exactly what the ordinal-alignment
/// test relies on.
pub fn build_zipcrypto_zip(entries: &[CryptoFixtureFile], password: &str) -> Vec<u8> {
    let mut body = Vec::new();
    let mut central = Vec::new();
    let pw = password.as_bytes();

    for entry in entries {
        let name = entry.name.as_bytes();
        let crc = crc32(&entry.content);
        let local_offset = body.len() as u32;
        let flags = if entry.encrypted { GP_ENCRYPTED } else { 0 };

        // The stored bytes: for an encrypted entry, the 12-byte ZipCrypto header
        // (11 arbitrary bytes + a check byte = the CRC's high byte) followed by the
        // encrypted content, all run through the same keystream.
        let stored = if entry.encrypted {
            let mut cipher = ZipCrypto::new(pw);
            let mut header = [0u8; ZIPCRYPTO_HEADER_LEN];
            header[..11].copy_from_slice(&[0x41; 11]); // deterministic "random" prefix
            header[11] = (crc >> 24) as u8; // check byte the reader validates
            let mut out = Vec::with_capacity(ZIPCRYPTO_HEADER_LEN + entry.content.len());
            for &b in header.iter().chain(entry.content.iter()) {
                out.push(cipher.encrypt(b));
            }
            out
        } else {
            entry.content.clone()
        };
        let compressed_size = stored.len() as u32;
        let uncompressed_size = entry.content.len() as u32;

        // Local file header (signature PK\x03\x04), method 0 (stored), no data
        // descriptor (so the reader validates the check byte against the CRC).
        body.extend_from_slice(&[0x50, 0x4b, 0x03, 0x04]);
        body.extend_from_slice(&20u16.to_le_bytes()); // version needed
        body.extend_from_slice(&flags.to_le_bytes());
        body.extend_from_slice(&0u16.to_le_bytes()); // method: stored
        body.extend_from_slice(&0u16.to_le_bytes()); // mod time
        body.extend_from_slice(&0x21u16.to_le_bytes()); // mod date (1980-01-01, valid)
        body.extend_from_slice(&crc.to_le_bytes());
        body.extend_from_slice(&compressed_size.to_le_bytes());
        body.extend_from_slice(&uncompressed_size.to_le_bytes());
        body.extend_from_slice(&(name.len() as u16).to_le_bytes());
        body.extend_from_slice(&0u16.to_le_bytes()); // extra len
        body.extend_from_slice(name);
        body.extend_from_slice(&stored);

        // Central-directory header (signature PK\x01\x02).
        central.extend_from_slice(&[0x50, 0x4b, 0x01, 0x02]);
        central.extend_from_slice(&20u16.to_le_bytes()); // version made by
        central.extend_from_slice(&20u16.to_le_bytes()); // version needed
        central.extend_from_slice(&flags.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes()); // method
        central.extend_from_slice(&0u16.to_le_bytes()); // mod time
        central.extend_from_slice(&0x21u16.to_le_bytes()); // mod date
        central.extend_from_slice(&crc.to_le_bytes());
        central.extend_from_slice(&compressed_size.to_le_bytes());
        central.extend_from_slice(&uncompressed_size.to_le_bytes());
        central.extend_from_slice(&(name.len() as u16).to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes()); // extra len
        central.extend_from_slice(&0u16.to_le_bytes()); // comment len
        central.extend_from_slice(&0u16.to_le_bytes()); // disk number start
        central.extend_from_slice(&0u16.to_le_bytes()); // internal attrs
        central.extend_from_slice(&0u32.to_le_bytes()); // external attrs
        central.extend_from_slice(&local_offset.to_le_bytes());
        central.extend_from_slice(name);
    }

    let cd_offset = body.len() as u32;
    let cd_size = central.len() as u32;
    let count = entries.len() as u16;
    body.extend_from_slice(&central);
    // End of central directory (signature PK\x05\x06).
    body.extend_from_slice(&[0x50, 0x4b, 0x05, 0x06]);
    body.extend_from_slice(&0u16.to_le_bytes()); // disk number
    body.extend_from_slice(&0u16.to_le_bytes()); // cd start disk
    body.extend_from_slice(&count.to_le_bytes()); // records this disk
    body.extend_from_slice(&count.to_le_bytes()); // total records
    body.extend_from_slice(&cd_size.to_le_bytes());
    body.extend_from_slice(&cd_offset.to_le_bytes());
    body.extend_from_slice(&0u16.to_le_bytes()); // comment len
    body
}

// ── AES-encrypted 7z fixtures ────────────────────────────────────────────────
//
// Built with `sevenz-rust2`'s writer (dev-only `compress` + `aes256` features);
// the shipped path stays decode-only. Round-tripping through the same crate the
// read path decodes with is enough to exercise the decrypt seam — 7z AES is a
// fixed spec (`AES256_SHA256`), so the writer output matches what `7z` produces.

/// Builds an AES-256-encrypted 7z with `password`. `encrypt_header` picks the two
/// real shapes `7z` produces: `false` = content-encrypted (`-mhe=off`; the metadata
/// header stays plaintext, so listing needs no password, only extraction does);
/// `true` = header-encrypted (`-mhe=on`; the metadata is itself encrypted, so even
/// listing needs the password). Content methods run compress-then-encrypt.
pub fn build_encrypted_7z(files: &[(&str, &[u8])], password: &str, encrypt_header: bool) -> Vec<u8> {
    use sevenz_rust2::encoder_options::AesEncoderOptions;
    use sevenz_rust2::{EncoderConfiguration, EncoderMethod, Password};
    let mut writer = sevenz_rust2::ArchiveWriter::new(Cursor::new(Vec::new())).expect("7z writer");
    writer.set_content_methods(vec![
        EncoderConfiguration::new(EncoderMethod::LZMA2),
        AesEncoderOptions::new(Password::new(password)).into(),
    ]);
    writer.set_encrypt_header(encrypt_header);
    for (name, data) in files {
        let entry = sevenz_rust2::ArchiveEntry::new_file(name);
        writer.push_archive_entry(entry, Some(*data)).expect("push entry");
    }
    writer.finish().expect("finish 7z").into_inner()
}

/// The traditional PKWARE ZipCrypto stream cipher (three 32-bit keys). Matches
/// what the `zip` crate decrypts, so a fixture built here round-trips through
/// `by_index_decrypt`.
struct ZipCrypto {
    key0: u32,
    key1: u32,
    key2: u32,
}

impl ZipCrypto {
    fn new(password: &[u8]) -> Self {
        let mut keys = Self {
            key0: 0x1234_5678,
            key1: 0x2345_6789,
            key2: 0x3456_7890,
        };
        for &b in password {
            keys.update(b);
        }
        keys
    }

    fn update(&mut self, byte: u8) {
        self.key0 = crc32_byte(self.key0, byte);
        self.key1 = self
            .key1
            .wrapping_add(self.key0 & 0xff)
            .wrapping_mul(134_775_813)
            .wrapping_add(1);
        self.key2 = crc32_byte(self.key2, (self.key1 >> 24) as u8);
    }

    fn keystream_byte(&self) -> u8 {
        let temp = (self.key2 | 2) & 0xffff;
        ((temp.wrapping_mul(temp ^ 1)) >> 8) as u8
    }

    fn encrypt(&mut self, plain: u8) -> u8 {
        let cipher = plain ^ self.keystream_byte();
        self.update(plain);
        cipher
    }
}

/// One CRC-32 table entry, computed on the fly (no static table needed for tiny
/// fixtures).
fn crc32_table(index: u32) -> u32 {
    let mut c = index;
    for _ in 0..8 {
        c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
    }
    c
}

/// One-byte CRC-32 update (the ZipCrypto/zip polynomial).
fn crc32_byte(crc: u32, byte: u8) -> u32 {
    (crc >> 8) ^ crc32_table((crc ^ byte as u32) & 0xff)
}

/// Standard CRC-32 of `data` (init `0xFFFFFFFF`, final inversion) — the value that
/// goes in the zip headers and whose high byte is the ZipCrypto check byte.
fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc = crc32_byte(crc, b);
    }
    !crc
}
