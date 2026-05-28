// Some helpers (`find_newlines`, `EncodingGroup`, `decode_line` re-exports) are
// consumed by tests and FE-facing IPC layers; the `#[deny(unused)]` lint at the
// crate root would flag them otherwise.
#![allow(dead_code, reason = "module-level helpers used by tests and IPC consumers")]

//! File encoding detection, newline scanning, and per-line decoding.
//!
//! Three concerns live here:
//!
//! 1. `FileEncoding` — the enum of encodings the viewer supports, plus BOM and
//!    label metadata. Auto-detection via `detect()` reads the first 64 KB of a
//!    file (BOM sniff + UTF-8 fast path + UTF-16 parity heuristic + Western
//!    Latin-1 fallback).
//! 2. `NewlineScanner` / `find_newlines` — emits the absolute byte offset of
//!    every `0x0A` byte that constitutes a `U+000A` code unit. ASCII-compatible
//!    encodings use the SIMD-accelerated `memchr` fast path; UTF-16 uses an
//!    explicit alignment / carry-byte scanner so a code unit straddling a chunk
//!    boundary doesn't flip parity for the next chunk.
//! 3. `decode_line` — bytes → `String`, with the existing `from_utf8_lossy`
//!    fast path for UTF-8 and `encoding_rs` for everything else.

use std::fs::File;
use std::io::Read;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// User-selectable text encoding for the file viewer.
///
/// The variants are deliberately narrow: every entry is something a user is
/// likely to need (UTF-8 + BOM, the Western single-byte family, UTF-16 in both
/// orders). EBCDIC, UTF-32, UTF-7, and the various DOS / Mac code pages are
/// out of scope until requested; `encoding_rs` supports them so extending later
/// is just an enum + dropdown addition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum FileEncoding {
    Utf8,
    Utf8WithBom,
    Windows1252,
    Iso8859_1,
    MacRoman,
    UsAscii,
    Utf16Le,
    Utf16Be,
}

/// Coarse grouping for the encoding dropdown's `<optgroup>` split.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum EncodingGroup {
    Unicode,
    Western,
}

impl FileEncoding {
    /// True when a lone `0x0A` byte means newline and nothing else, allowing the
    /// SIMD-accelerated `memchr(b'\n', _)` newline scan to apply directly.
    ///
    /// **False for UTF-16**: a `0x0A` byte can appear inside the high or low byte
    /// of a non-newline code unit (for instance `U+010A LATIN CAPITAL LETTER A
    /// WITH BREVE` in LE is `0A 01`), so `memchr` would emit spurious newlines.
    /// UTF-16 takes the [`NewlineScanner`] path instead.
    pub fn is_ascii_newline_compatible(self) -> bool {
        match self {
            Self::Utf8 | Self::Utf8WithBom | Self::Windows1252 | Self::Iso8859_1 | Self::MacRoman | Self::UsAscii => {
                true
            }
            Self::Utf16Le | Self::Utf16Be => false,
        }
    }

    /// Bytes that constitute the BOM at the file start, if any.
    ///
    /// Drives the [`same_byte_layout`] predicate: when two encodings share a BOM
    /// (or both have none), the second encoding can be applied to an open file
    /// without re-scanning newlines.
    pub fn bom_bytes(self) -> &'static [u8] {
        match self {
            Self::Utf8WithBom => &[0xEF, 0xBB, 0xBF],
            Self::Utf16Le => &[0xFF, 0xFE],
            Self::Utf16Be => &[0xFE, 0xFF],
            _ => &[],
        }
    }

    /// Sentence-case display label for the dropdown.
    pub fn label(self) -> &'static str {
        match self {
            Self::Utf8 => "UTF-8",
            Self::Utf8WithBom => "UTF-8 with BOM",
            Self::Windows1252 => "Western (Windows-1252)",
            Self::Iso8859_1 => "Western (ISO-8859-1)",
            Self::MacRoman => "Western (Mac Roman)",
            Self::UsAscii => "US-ASCII",
            Self::Utf16Le => "UTF-16 LE",
            Self::Utf16Be => "UTF-16 BE",
        }
    }

    /// Which `<optgroup>` this encoding belongs in.
    pub fn group(self) -> EncodingGroup {
        match self {
            Self::Utf8 | Self::Utf8WithBom | Self::Utf16Le | Self::Utf16Be => EncodingGroup::Unicode,
            Self::Windows1252 | Self::Iso8859_1 | Self::MacRoman | Self::UsAscii => EncodingGroup::Western,
        }
    }

    /// Maps to the `encoding_rs` static encoding used by `decode_line`.
    ///
    /// **Iso8859_1 is NOT mapped via `encoding_rs::WINDOWS_1252`** because the
    /// two disagree on the `0x80-0x9F` range: Windows-1252 reassigns those
    /// bytes to characters like `€` (`0x80`), while strict ISO-8859-1 leaves
    /// them as the C1 control codes `U+0080-U+009F`. The viewer handles ISO
    /// directly via a manual 1:1 byte → codepoint table in [`decode_line`];
    /// this method is unused for the `Iso8859_1` variant (and asserts the
    /// invariant via `unreachable!`).
    pub fn as_static(self) -> &'static encoding_rs::Encoding {
        match self {
            Self::Utf8 | Self::Utf8WithBom | Self::UsAscii => encoding_rs::UTF_8,
            Self::Windows1252 => encoding_rs::WINDOWS_1252,
            Self::Iso8859_1 => unreachable!("ISO-8859-1 decoding is handled manually in decode_line"),
            Self::MacRoman => encoding_rs::MACINTOSH,
            Self::Utf16Le => encoding_rs::UTF_16LE,
            Self::Utf16Be => encoding_rs::UTF_16BE,
        }
    }
}

/// Predicate that gates the instant-swap path in `viewer_set_encoding`.
///
/// Returns true when the byte-offset layout of a file is identical under both
/// encodings: same BOM and both ASCII-newline-compatible. In that case only
/// per-line decoding changes; the newline index stays valid and no rebuild is
/// needed.
///
/// **UTF-16 LE ↔ BE is deliberately not instant**: any non-ASCII codepoint puts
/// the `0x0A` byte at a different offset under each order. The dropdown
/// rebuilds in the background like an LE → UTF-8 switch.
pub fn same_byte_layout(a: FileEncoding, b: FileEncoding) -> bool {
    a.is_ascii_newline_compatible() && b.is_ascii_newline_compatible() && a.bom_bytes() == b.bom_bytes()
}

/// Reads the file head and infers the encoding.
///
/// Read budget: 4 bytes for the BOM sniff, then 64 KB for the heuristics. Even
/// on a 100 GB file this never reads more than 64 KB, so the cost is bounded by
/// disk seek latency.
///
/// Decision order:
///
/// 1. **BOM match** (4 bytes): UTF-8 → `Utf8WithBom`, LE / BE → `Utf16Le` /
///    `Utf16Be`. Wins outright.
/// 2. **Valid UTF-8 in the first 64 KB** → `Utf8`.
/// 3. **UTF-16 parity heuristic** on the first 64 KB: ASCII text encoded as
///    UTF-16 LE has its `0x00` high byte at every **odd** offset; BE puts the
///    `0x00` at every **even** offset. ≥30% of pairs matching either pattern is
///    enough confidence.
/// 4. **Fallback** → `Windows1252` (the right default for high-bit Latin-1
///    bytes; ISO-8859-1 is a strict subset).
///
/// Returns `Utf8` for empty files (sensible default; matches the viewer's
/// existing empty-file behaviour).
pub fn detect(path: &Path) -> std::io::Result<FileEncoding> {
    let mut file = File::open(path)?;
    let mut head = vec![0u8; 64 * 1024];
    let read = file.read(&mut head)?;
    head.truncate(read);
    Ok(detect_from_head(&head))
}

/// Pure version of [`detect`]: same logic against an in-memory buffer.
///
/// Pulled out so tests don't need a tempfile, and so the same code path serves
/// streaming detection in a future "paste here to view" flow.
pub fn detect_from_head(head: &[u8]) -> FileEncoding {
    if head.is_empty() {
        return FileEncoding::Utf8;
    }
    if head.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return FileEncoding::Utf8WithBom;
    }
    if head.starts_with(&[0xFF, 0xFE]) {
        return FileEncoding::Utf16Le;
    }
    if head.starts_with(&[0xFE, 0xFF]) {
        return FileEncoding::Utf16Be;
    }
    // UTF-16 parity check has to run BEFORE the UTF-8 fast path: ASCII text encoded
    // as UTF-16 (interleaved with `0x00` bytes) is technically valid UTF-8 — every
    // `0x00` is a legal U+0000 codepoint — so a naive `from_utf8(_).is_ok()` would
    // misclassify it as Utf8. The parity heuristic is restrictive enough (≥30% zero
    // bytes in a fixed parity slot) that it won't fire on real UTF-8 text.
    if let Some(utf16) = detect_utf16_parity(head) {
        return utf16;
    }
    if std::str::from_utf8(head).is_ok() {
        return FileEncoding::Utf8;
    }
    FileEncoding::Windows1252
}

/// UTF-16 parity heuristic (no-BOM case).
///
/// ASCII text under UTF-16 LE: `00 XX 00 XX …`. The low byte `XX` lives at the
/// **even** offset of each pair, the high byte `0x00` lives at the **odd**
/// offset. So `odd_zeros / total_pairs > 0.30` → LE.
///
/// ASCII text under UTF-16 BE: `XX 00 XX 00 …`. Low at odd, high at even. So
/// `even_zeros / total_pairs > 0.30` → BE.
///
/// Returns `None` when neither side clears the 30% threshold. The 30% number is
/// the same one Chromium's chardet implementation uses for ASCII-dominant
/// UTF-16 streams; in practice ASCII text in UTF-16 has ~100% zero-byte parity
/// and pathologically mixed text drops below 5%.
fn detect_utf16_parity(buf: &[u8]) -> Option<FileEncoding> {
    let total_pairs = buf.len() / 2;
    if total_pairs == 0 {
        return None;
    }
    let mut even_zeros: usize = 0;
    let mut odd_zeros: usize = 0;
    for k in 0..total_pairs {
        if buf[2 * k] == 0 {
            even_zeros += 1;
        }
        if buf[2 * k + 1] == 0 {
            odd_zeros += 1;
        }
    }
    let total = total_pairs as f64;
    let threshold = 0.30;
    let le_score = odd_zeros as f64 / total;
    let be_score = even_zeros as f64 / total;
    if le_score >= be_score && le_score > threshold {
        Some(FileEncoding::Utf16Le)
    } else if be_score > threshold {
        Some(FileEncoding::Utf16Be)
    } else {
        None
    }
}

/// Stateless convenience for finding newline byte offsets in a single buffer.
///
/// Used by `FullLoadBackend` (the whole file fits in memory) and tests. For
/// streaming reads use [`NewlineScanner`].
pub fn find_newlines(buf: &[u8], encoding: FileEncoding) -> Vec<usize> {
    if encoding.is_ascii_newline_compatible() {
        return memchr::memchr_iter(b'\n', buf).collect();
    }
    let mut scanner = NewlineScanner::new(encoding, 0);
    let mut out = Vec::new();
    scanner.feed(buf, |off| out.push(off as usize));
    out
}

/// Streaming newline scanner that carries one byte across chunk boundaries.
///
/// For ASCII-compatible encodings it's a thin wrapper around `memchr_iter` with
/// `file_offset` bookkeeping. For UTF-16 it maintains alignment and emits the
/// absolute file offset of every `0x0A` byte that is part of a `U+000A` code
/// unit (matching `memchr_iter` semantics — the offset of the byte itself, not
/// the code-unit pair).
pub struct NewlineScanner {
    encoding: FileEncoding,
    /// For UTF-16 only: when a chunk has an odd number of bytes left over, the
    /// trailing byte is parked here so the next chunk's first byte completes
    /// the pair. `None` means "next read starts on a code-unit boundary."
    carry: Option<u8>,
    /// Absolute file offset of the next byte to be fed.
    file_offset: u64,
}

impl NewlineScanner {
    /// Build a scanner whose first `feed` call sees byte `start_offset` of the
    /// file. `start_offset` should be `0` for fresh scans and a previous
    /// LineIndex's `total_bytes` for tail-mode extension.
    pub fn new(encoding: FileEncoding, start_offset: u64) -> Self {
        Self {
            encoding,
            carry: None,
            file_offset: start_offset,
        }
    }

    /// Feed a chunk; the callback receives the absolute file offset of each
    /// newline byte. Returns the number of newlines reported in this call so
    /// callers can keep a running total without re-scanning.
    pub fn feed<F: FnMut(u64)>(&mut self, buf: &[u8], mut callback: F) -> usize {
        // ASCII-compatible fast path. memchr returns relative offsets; we add
        // file_offset to make them absolute.
        if self.encoding.is_ascii_newline_compatible() {
            let mut count = 0;
            for rel in memchr::memchr_iter(b'\n', buf) {
                callback(self.file_offset + rel as u64);
                count += 1;
            }
            self.file_offset += buf.len() as u64;
            return count;
        }

        let le = matches!(self.encoding, FileEncoding::Utf16Le);
        let mut count = 0;
        let mut pos = 0;

        // Stitch a carried byte with the first byte of `buf` to form a complete pair.
        //
        // Offset semantics (we report the offset of the byte that holds `0x0A`):
        //   LE pair = [low, high]; `U+000A` => low = 0x0A, high = 0x00.
        //     carry path: carry was the low byte at file_offset - 1, so the
        //       0x0A byte sits at file_offset - 1.
        //     aligned path: low is at buf[i], so absolute offset is
        //       file_offset + i.
        //   BE pair = [high, low]; `U+000A` => high = 0x00, low = 0x0A.
        //     carry path: carry was the high byte at file_offset - 1, so the
        //       0x0A is at buf[0], absolute offset file_offset.
        //     aligned path: low is at buf[i + 1], absolute offset
        //       file_offset + i + 1.
        if let Some(carry) = self.carry.take() {
            if buf.is_empty() {
                // Re-park: nothing to stitch against.
                self.carry = Some(carry);
                return 0;
            }
            let pair = if le {
                u16::from_le_bytes([carry, buf[0]])
            } else {
                u16::from_be_bytes([carry, buf[0]])
            };
            if pair == 0x000A {
                let off = if le { self.file_offset - 1 } else { self.file_offset };
                callback(off);
                count += 1;
            }
            pos = 1;
        }

        // Consume aligned pairs. The tail-odd byte (if any) is parked for the next call.
        let remaining = buf.len() - pos;
        let tail_odd = remaining % 2 == 1;
        let pair_end = buf.len() - if tail_odd { 1 } else { 0 };
        let mut i = pos;
        while i + 1 < pair_end {
            let pair = if le {
                u16::from_le_bytes([buf[i], buf[i + 1]])
            } else {
                u16::from_be_bytes([buf[i], buf[i + 1]])
            };
            if pair == 0x000A {
                let off = if le { i as u64 } else { (i + 1) as u64 };
                callback(self.file_offset + off);
                count += 1;
            }
            i += 2;
        }
        if tail_odd {
            self.carry = Some(buf[buf.len() - 1]);
        }
        self.file_offset += buf.len() as u64;
        count
    }
}

/// Decode `bytes` as a string using the given encoding.
///
/// UTF-8 takes the existing `from_utf8_lossy` fast path to avoid the
/// `encoding_rs` allocation in the hottest case (the viewer's default). Every
/// other encoding goes through `encoding_rs::Encoding::decode_without_bom_handling`,
/// which is correct because the line bytes don't contain the BOM (the BOM lives
/// at the file head, not in the per-line slice).
pub fn decode_line(bytes: &[u8], encoding: FileEncoding) -> String {
    if matches!(
        encoding,
        FileEncoding::Utf8 | FileEncoding::Utf8WithBom | FileEncoding::UsAscii
    ) {
        return String::from_utf8_lossy(bytes).into_owned();
    }
    if matches!(encoding, FileEncoding::Iso8859_1) {
        // Strict ISO-8859-1: byte N decodes to U+00XX with no remapping. The
        // 0x80-0x9F range stays as C1 control codes, unlike Windows-1252
        // which reassigns them to characters like `€` (0x80). Implemented
        // manually because `encoding_rs` doesn't ship a strict ISO-8859-1
        // decoder — it aliases the label to Windows-1252.
        let mut out = String::with_capacity(bytes.len());
        for &b in bytes {
            out.push(b as char);
        }
        return out;
    }
    let (cow, _had_errors) = encoding.as_static().decode_without_bom_handling(bytes);
    cow.into_owned()
}
