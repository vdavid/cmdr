//! Content classification for the file viewer: decide whether a file should render
//! as inline media (an image, a PDF) or fall back to the text pipeline.
//!
//! The decision is driven by **magic bytes**, never the extension (a `.jpg` that is
//! really a PDF/HTML polyglot must never be handed to an `<embed>`/`<iframe>` that
//! could execute it). The extension is a tiebreaker only, and the SVG case is the
//! sole place where the extension is load-bearing (see [`classify_viewer_content`]).
//!
//! Only **local** files are eligible for media rendering in v1. MTP has no POSIX path
//! to serve, and SMB paths can block; both stay on the text pipeline. The caller
//! decides locality and passes it in as `is_local`.

use serde::Serialize;

use super::encoding::FileEncoding;

/// What the viewer should render a file as. The frontend branches on this:
/// `Image` -> `<img>`, `Pdf` -> `<embed>`, `Text` -> the line pipeline.
///
/// "Media" survives only as an internal code word (the `cmdr-media://` scheme and
/// this enum); users never see it. Future kinds (Markdown, Html) slot in here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ViewerContentKind {
    Text,
    Image,
    Pdf,
}

/// Number of head bytes the caller should read and pass to [`classify_viewer_content`].
/// Enough to cover every magic-byte signature plus the conservative SVG sniff (which
/// skips a BOM, an XML prolog, comments, and a DOCTYPE before the `<svg` root).
pub const CLASSIFY_HEAD_LEN: usize = 1024;

/// Classifies a file as [`ViewerContentKind`] from its head bytes, extension, and
/// locality. Pure: no I/O.
///
/// - **Magic bytes decide** the kind (and, downstream, the served `Content-Type`):
///   JPEG/PNG/GIF/WebP/BMP/TIFF/HEIC -> `Image`, `%PDF-` -> `Pdf`.
/// - **SVG is the one extension-gated case**: classified `Image` only when `ext` is
///   `svg` AND the first meaningful token (after a BOM, XML prolog, comments, and a
///   DOCTYPE) is an `<svg` root. This avoids false-positiving an HTML file that merely
///   embeds an inline `<svg>`.
/// - **Non-local -> always `Text`.** v1 scopes media rendering to local files.
pub fn classify_viewer_content(head: &[u8], ext: Option<&str>, is_local: bool) -> ViewerContentKind {
    if !is_local {
        return ViewerContentKind::Text;
    }

    if let Some(kind) = classify_by_magic(head) {
        return kind;
    }

    // SVG is text-shaped, so it has no binary magic. Only treat it as an image when
    // the extension claims SVG and the content actually opens with an `<svg` root.
    if ext.is_some_and(|e| e.eq_ignore_ascii_case("svg")) && looks_like_svg_root(head) {
        return ViewerContentKind::Image;
    }

    ViewerContentKind::Text
}

/// Share of C0 control bytes (outside the text set below) past which a head reads as
/// binary. The same 5% line `encoding::utf16_looks_like_text` draws for the mirror case
/// (UTF-16 code units that decode to controls), so the two heuristics agree.
const MAX_CONTROL_BYTE_RATIO: f64 = 0.05;

/// Whether a head that isn't media should be read as binary rather than text. Pure.
///
/// The viewer classifies every non-media file `Text` and leaves the warning to the
/// frontend's extension list; the agent needs the byte-level answer. Rule:
///
/// - A UTF-16 head is text: every other byte is a NUL by construction, so the NUL rule
///   below would call all of it binary. `detect_from_head` already required it to decode
///   to mostly-printable code units before naming it UTF-16.
/// - Otherwise a NUL anywhere means binary (no text encoding this viewer decodes puts a
///   bare NUL in text), and so does a share of C0 controls beyond `\t` `\n` `\r`, form
///   feed, and ESC (ANSI colour codes in a log are text) above [`MAX_CONTROL_BYTE_RATIO`].
pub fn looks_binary(head: &[u8], encoding: FileEncoding) -> bool {
    if matches!(encoding, FileEncoding::Utf16Le | FileEncoding::Utf16Be) {
        return false;
    }
    if head.is_empty() {
        return false;
    }
    if head.contains(&0) {
        return true;
    }
    let controls = head
        .iter()
        .filter(|b| matches!(b, 0x00..=0x1F) && !matches!(b, b'\t' | b'\n' | b'\r' | 0x0C | 0x1B))
        .count();
    (controls as f64 / head.len() as f64) > MAX_CONTROL_BYTE_RATIO
}

/// The MIME type to serve for a media kind, derived from the same magic bytes the
/// classifier used. Returns `None` for `Text` (text never flows through the scheme).
///
/// For images the exact subtype matters for WKWebView decode hints, so we re-sniff
/// the magic here rather than carry the subtype through the classifier. SVG has no
/// magic, so it's the extension-driven fallback when no raster magic matches.
pub fn media_mime(head: &[u8], kind: ViewerContentKind) -> Option<&'static str> {
    match kind {
        ViewerContentKind::Text => None,
        ViewerContentKind::Pdf => Some("application/pdf"),
        ViewerContentKind::Image => Some(image_mime(head)),
    }
}

/// The image subtype for `head`, falling back to `image/svg+xml` when no raster
/// magic matches (the classifier only ever reaches `Image` for raster magic or a
/// confirmed SVG root, so the fallback is exactly the SVG case).
fn image_mime(head: &[u8]) -> &'static str {
    match classify_by_magic(head) {
        Some(ViewerContentKind::Image) => raster_image_mime(head).unwrap_or("application/octet-stream"),
        _ => "image/svg+xml",
    }
}

/// Magic-byte classification for the closed set of formats WKWebView decodes natively.
/// Returns `None` for anything without a recognized binary signature (text, SVG, etc.).
fn classify_by_magic(head: &[u8]) -> Option<ViewerContentKind> {
    if head.starts_with(b"%PDF-") {
        return Some(ViewerContentKind::Pdf);
    }
    if raster_image_mime(head).is_some() {
        return Some(ViewerContentKind::Image);
    }
    None
}

/// Returns the MIME type if `head` starts with a known raster image signature, else
/// `None`. The single source of truth for "is this raster image magic?".
fn raster_image_mime(head: &[u8]) -> Option<&'static str> {
    // JPEG: FF D8 FF
    if head.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some("image/jpeg");
    }
    // PNG: 89 50 4E 47 0D 0A 1A 0A
    if head.starts_with(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]) {
        return Some("image/png");
    }
    // GIF: "GIF8" (covers GIF87a and GIF89a)
    if head.starts_with(b"GIF8") {
        return Some("image/gif");
    }
    // WebP: "RIFF" .... "WEBP"
    if head.len() >= 12 && head.starts_with(b"RIFF") && &head[8..12] == b"WEBP" {
        return Some("image/webp");
    }
    // BMP: "BM"
    if head.starts_with(b"BM") {
        return Some("image/bmp");
    }
    // TIFF: "II*\0" (little-endian) or "MM\0*" (big-endian)
    if head.starts_with(&[0x49, 0x49, 0x2A, 0x00]) || head.starts_with(&[0x4D, 0x4D, 0x00, 0x2A]) {
        return Some("image/tiff");
    }
    // HEIC: ISO-BMFF `ftyp` box at offset 4, with a HEIF/HEIC brand.
    if is_heic(head) {
        return Some("image/heic");
    }
    None
}

/// HEIC detection: an ISO base media file (`ftyp` box at offset 4) whose major brand
/// is one of the HEIF/HEIC family. We check the major brand (bytes 8..12); the brand
/// list mirrors what ImageIO treats as HEIC-family still images.
fn is_heic(head: &[u8]) -> bool {
    if head.len() < 12 || &head[4..8] != b"ftyp" {
        return false;
    }
    matches!(
        &head[8..12],
        b"heic" | b"heix" | b"mif1" | b"msf1" | b"heim" | b"heis" | b"hevc" | b"hevx"
    )
}

/// Conservative SVG sniff: returns true when the first meaningful token, after
/// skipping a UTF-8 BOM, leading whitespace, an XML prolog (`<?xml ... ?>`), any
/// number of comments (`<!-- ... -->`), and a DOCTYPE (`<!DOCTYPE ... >`), is an
/// `<svg` element open tag.
fn looks_like_svg_root(head: &[u8]) -> bool {
    let mut rest = head;
    // Strip a UTF-8 BOM if present.
    if let Some(stripped) = rest.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        rest = stripped;
    }
    loop {
        rest = skip_ascii_whitespace(rest);
        if rest.starts_with(b"<?") {
            // XML prolog / processing instruction: skip to the closing "?>".
            match find_subslice(rest, b"?>") {
                Some(i) => rest = &rest[i + 2..],
                None => return false,
            }
        } else if rest.starts_with(b"<!--") {
            // Comment: skip to the closing "-->".
            match find_subslice(&rest[4..], b"-->") {
                Some(i) => rest = &rest[4 + i + 3..],
                None => return false,
            }
        } else if rest.starts_with(b"<!") {
            // DOCTYPE (or other declaration): skip to the next ">".
            match find_subslice(rest, b">") {
                Some(i) => rest = &rest[i + 1..],
                None => return false,
            }
        } else {
            break;
        }
    }
    // The root element must be `<svg`, followed by whitespace, `>`, or `/`.
    let Some(after) = rest.strip_prefix(b"<svg") else {
        return false;
    };
    match after.first() {
        None => true, // truncated head right after "<svg": accept (the head cap clipped it)
        Some(b) => b.is_ascii_whitespace() || *b == b'>' || *b == b'/',
    }
}

fn skip_ascii_whitespace(mut s: &[u8]) -> &[u8] {
    while let Some((first, rest)) = s.split_first() {
        if first.is_ascii_whitespace() {
            s = rest;
        } else {
            break;
        }
    }
    s
}

/// Returns the index of the first occurrence of `needle` in `haystack`, or `None`.
fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}
