//! The `inspect_file` agent tool: "what is this file?" answered from the file itself.
//!
//! One call returns everything the agent needs to describe a file the user pointed at:
//! metadata (size, modified, extension, MIME guess), what the bytes actually are (magic
//! sniff, never trust the extension alone), and a typed content section per kind:
//!
//! - **text**: a windowed slice of the decoded text (`offset` + `maxChars`, so a long
//!   file is paged, never dumped), with honest `truncated` / `totalChars` counts.
//! - **image**: dimensions + format from the header, and a pointer to `image_facts`
//!   for what's IN the picture. Image BYTES never cross (the DTO is text-only).
//! - **pdf**: the header version and an estimated page count. Text extraction isn't
//!   wired yet, and the result says so rather than returning an empty string.
//! - **binary** / **archive**: metadata and the sniffed format only.
//!
//! ## Privacy
//!
//! This is the first tool that hands a provider the CONTENTS of an arbitrary user file
//! (bounded, on request, one path per call). It rides the same Ask Cmdr consent gate as
//! `image_facts`; the consent copy has to name it before this ships to users.
//!
//! ## Never hangs the turn
//!
//! The read runs on a blocking thread under [`READ_TIMEOUT`]. A dead mount answers
//! `unreachable`, never a wedged turn (`Rock solid` principle 2).

use std::io::{ErrorKind, Read, Seek, SeekFrom};
use std::path::Path;
use std::time::Duration;

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Runtime};

use crate::mcp::{ToolError, ToolResult};
use crate::search::format_size;

/// Default window of text returned when the caller doesn't say.
const DEFAULT_MAX_CHARS: usize = 4_000;
/// The most text one call returns. Past this the caller pages with `offset`.
const MAX_MAX_CHARS: usize = 16_000;
/// How much of a file is read for sniffing and text decoding. Text past this is paged
/// by re-reading from `offset`; binary files never need more than the head.
const SNIFF_BYTES: usize = 8 * 1024;
/// The most bytes read for one text window: the window itself is chars, and UTF-8 is at
/// most four bytes per char.
const MAX_TEXT_READ_BYTES: u64 = (MAX_MAX_CHARS * 4) as u64;
/// A file this large is never scanned for a total character count; the count is then
/// reported absent, not guessed.
const MAX_COUNT_BYTES: u64 = 8 * 1024 * 1024;
/// A read that takes longer than this is a hung mount, not a slow disk.
const READ_TIMEOUT: Duration = Duration::from_secs(5);

// ── Result DTOs ─────────────────────────────────────────────────────────────

/// What the bytes say the file is, from its leading signature. `Unknown` means no
/// signature matched, which for a non-text file is the honest answer, not a guess.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Detected {
    Pdf,
    Png,
    Jpeg,
    Gif,
    Webp,
    Heic,
    Tiff,
    Bmp,
    Zip,
    Gzip,
    SevenZip,
    Rar,
    Tar,
    Sqlite,
    MachO,
    Elf,
    Utf8Text,
    Unknown,
}

/// The content family the `content` section is shaped for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    Text,
    Image,
    Pdf,
    Archive,
    Binary,
    Empty,
}

/// A window of a text file.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextContent {
    pub content: String,
    /// The char offset `content` starts at (what the caller asked for).
    pub offset: usize,
    /// How many chars `content` holds.
    pub returned: usize,
    /// Total chars in the file. Absent when the file is too large to count.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_chars: Option<usize>,
    /// `true` when text exists past `offset + returned`. Ask again with a larger `offset`.
    pub truncated: bool,
}

/// What the header of an image says. Never pixels.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageContent {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    /// Where the picture's contents live: `image_facts` (OCR text + tags from the index).
    pub hint: &'static str,
}

/// What the PDF header and object table say.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfContent {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Counted from uncompressed `/Type /Page` objects: a lower bound for PDFs that pack
    /// their objects into compressed streams. Absent when none were found.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_count_estimate: Option<usize>,
    pub hint: &'static str,
}

/// The per-kind content section.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Content {
    Text(TextContent),
    Image(ImageContent),
    Pdf(PdfContent),
    Archive {},
    Binary {},
    Empty {},
}

/// The tool result. A typed status the model relays honestly.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum InspectResult {
    Ok {
        path: String,
        name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        extension: Option<String>,
        size_bytes: u64,
        size_human: String,
        /// Last modified, RFC 3339 UTC. Absent when the filesystem doesn't say.
        #[serde(skip_serializing_if = "Option::is_none")]
        modified: Option<String>,
        /// MIME type guessed from the extension. Absent when the extension is unknown.
        #[serde(skip_serializing_if = "Option::is_none")]
        mime: Option<String>,
        /// What the leading bytes say. Disagrees with `mime` when the extension lies.
        detected: Detected,
        content: Content,
    },
    /// The path is a folder: use `list_dir` for its children.
    Folder { path: String },
    /// Nothing at that path.
    Missing { path: String },
    /// The file exists but Cmdr can't read it (permissions, or Full Disk Access is off).
    Unreadable { path: String },
    /// The read didn't answer within [`READ_TIMEOUT`]: a disconnected drive or a hung mount.
    Unreachable { path: String },
}

// ── Params ────────────────────────────────────────────────────────────────────

pub fn inspect_file_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "path": { "type": "string", "description": "Absolute path of the file (~ is expanded)." },
            "maxChars": { "type": "integer", "minimum": 1, "maximum": MAX_MAX_CHARS,
                "description": "Text files: how many characters to return (default 4000, max 16000)." },
            "offset": { "type": "integer", "minimum": 0,
                "description": "Text files: character offset to start from, for paging a long file." }
        },
        "required": ["path"],
        "additionalProperties": false
    })
}

struct Params {
    path: String,
    max_chars: usize,
    offset: usize,
}

fn parse_params(params: &Value) -> Result<Params, ToolError> {
    let path = params
        .get("path")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(super::expand_user_path)
        .ok_or_else(|| ToolError::invalid_params("Missing 'path' parameter (an absolute file path)"))?;
    let max_chars = match params.get("maxChars") {
        None | Some(Value::Null) => DEFAULT_MAX_CHARS,
        Some(v) => v
            .as_u64()
            .filter(|n| *n >= 1)
            .map(|n| (n as usize).min(MAX_MAX_CHARS))
            .ok_or_else(|| ToolError::invalid_params("'maxChars' must be a positive integer"))?,
    };
    let offset = match params.get("offset") {
        None | Some(Value::Null) => 0,
        Some(v) => v
            .as_u64()
            .map(|n| n as usize)
            .ok_or_else(|| ToolError::invalid_params("'offset' must be a non-negative integer"))?,
    };
    Ok(Params { path, max_chars, offset })
}

// ── Sniffing (pure) ───────────────────────────────────────────────────────────

/// Classify a file from its leading bytes. Signature first; a file with no known
/// signature is text when it decodes as UTF-8 without NULs.
pub(crate) fn sniff(head: &[u8]) -> Detected {
    let starts = |sig: &[u8]| head.starts_with(sig);
    if starts(b"%PDF-") {
        Detected::Pdf
    } else if starts(b"\x89PNG\r\n\x1a\n") {
        Detected::Png
    } else if starts(b"\xFF\xD8\xFF") {
        Detected::Jpeg
    } else if starts(b"GIF87a") || starts(b"GIF89a") {
        Detected::Gif
    } else if head.len() >= 12 && &head[0..4] == b"RIFF" && &head[8..12] == b"WEBP" {
        Detected::Webp
    } else if head.len() >= 12 && &head[4..8] == b"ftyp" && (&head[8..12] == b"heic" || &head[8..12] == b"heix" || &head[8..12] == b"mif1") {
        Detected::Heic
    } else if starts(b"II*\0") || starts(b"MM\0*") {
        Detected::Tiff
    } else if starts(b"BM") && head.len() > 14 {
        Detected::Bmp
    } else if starts(b"PK\x03\x04") || starts(b"PK\x05\x06") {
        Detected::Zip
    } else if starts(b"\x1F\x8B") {
        Detected::Gzip
    } else if starts(b"7z\xBC\xAF\x27\x1C") {
        Detected::SevenZip
    } else if starts(b"Rar!\x1A\x07") {
        Detected::Rar
    } else if head.len() > 262 && &head[257..262] == b"ustar" {
        Detected::Tar
    } else if starts(b"SQLite format 3\0") {
        Detected::Sqlite
    } else if starts(b"\xCF\xFA\xED\xFE") || starts(b"\xCA\xFE\xBA\xBE") {
        Detected::MachO
    } else if starts(b"\x7FELF") {
        Detected::Elf
    } else if looks_like_text(head) {
        Detected::Utf8Text
    } else {
        Detected::Unknown
    }
}

/// UTF-8 without NUL bytes. A trailing partial multibyte sequence (the head cut a char)
/// still counts as text.
fn looks_like_text(head: &[u8]) -> bool {
    if head.is_empty() || head.contains(&0) {
        return false;
    }
    match std::str::from_utf8(head) {
        Ok(_) => true,
        Err(e) => e.error_len().is_none(),
    }
}

fn kind_for(detected: Detected, size: u64) -> Kind {
    if size == 0 {
        return Kind::Empty;
    }
    match detected {
        Detected::Utf8Text => Kind::Text,
        Detected::Png | Detected::Jpeg | Detected::Gif | Detected::Webp | Detected::Heic | Detected::Tiff | Detected::Bmp => {
            Kind::Image
        }
        Detected::Pdf => Kind::Pdf,
        Detected::Zip | Detected::Gzip | Detected::SevenZip | Detected::Rar | Detected::Tar => Kind::Archive,
        Detected::Sqlite | Detected::MachO | Detected::Elf | Detected::Unknown => Kind::Binary,
    }
}

/// `%PDF-1.7` → `"1.7"`, plus a count of `/Type /Page` objects (not `/Pages`).
pub(crate) fn pdf_facts(bytes: &[u8]) -> PdfContent {
    let version = bytes
        .strip_prefix(b"%PDF-")
        .map(|rest| rest.iter().take_while(|b| b.is_ascii_digit() || **b == b'.').map(|b| *b as char).collect::<String>())
        .filter(|v| !v.is_empty());
    let mut count = 0usize;
    for needle in [&b"/Type /Page"[..], &b"/Type/Page"[..]] {
        let mut i = 0;
        while let Some(pos) = find(&bytes[i..], needle) {
            let end = i + pos + needle.len();
            if bytes.get(end) != Some(&b's') {
                count += 1;
            }
            i = end;
        }
    }
    PdfContent {
        version,
        page_count_estimate: (count > 0).then_some(count),
        hint: "Text extraction from PDFs isn't available yet; describe it from the name, size, and page count.",
    }
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

/// Cut `text` to a char window. Pure, so paging is unit-testable.
pub(crate) fn window(text: &str, offset: usize, max_chars: usize, total_chars: Option<usize>) -> TextContent {
    let content: String = text.chars().skip(offset).take(max_chars).collect();
    let returned = content.chars().count();
    let more = text.chars().nth(offset + returned).is_some();
    TextContent {
        content,
        offset,
        returned,
        total_chars,
        truncated: more || total_chars.is_some_and(|t| offset + returned < t),
    }
}

// ── The blocking read ─────────────────────────────────────────────────────────

fn inspect_blocking(path: &str, max_chars: usize, offset: usize) -> InspectResult {
    let p = Path::new(path);
    let meta = match std::fs::metadata(p) {
        Ok(m) => m,
        Err(e) if e.kind() == ErrorKind::NotFound => return InspectResult::Missing { path: path.to_string() },
        Err(_) => return InspectResult::Unreadable { path: path.to_string() },
    };
    if meta.is_dir() {
        return InspectResult::Folder { path: path.to_string() };
    }
    let mut file = match std::fs::File::open(p) {
        Ok(f) => f,
        Err(_) => return InspectResult::Unreadable { path: path.to_string() },
    };
    let size = meta.len();
    let mut head = vec![0u8; SNIFF_BYTES.min(size as usize)];
    if file.read_exact(&mut head).is_err() {
        return InspectResult::Unreadable { path: path.to_string() };
    }
    let detected = sniff(&head);
    let content = match kind_for(detected, size) {
        Kind::Empty => Content::Empty {},
        Kind::Archive => Content::Archive {},
        Kind::Binary => Content::Binary {},
        Kind::Pdf => Content::Pdf(pdf_facts(&read_up_to(&mut file, MAX_COUNT_BYTES).unwrap_or(head))),
        Kind::Image => {
            let dims = image::ImageReader::open(p)
                .ok()
                .and_then(|r| r.with_guessed_format().ok())
                .and_then(|r| r.into_dimensions().ok());
            Content::Image(ImageContent {
                width: dims.map(|d| d.0),
                height: dims.map(|d| d.1),
                hint: "For what's in the picture (recognized text, tags), call image_facts with this path.",
            })
        }
        Kind::Text => {
            let all = if size <= MAX_COUNT_BYTES { read_up_to(&mut file, size).ok() } else { None };
            match all {
                Some(bytes) => {
                    let text = String::from_utf8_lossy(&bytes);
                    let total = text.chars().count();
                    Content::Text(window(&text, offset, max_chars, Some(total)))
                }
                None => {
                    // Too big to count: read a bounded window from the byte offset that the
                    // char offset can't be better than, which is only exact for ASCII.
                    let bytes = file
                        .seek(SeekFrom::Start(offset as u64))
                        .and_then(|_| read_up_to(&mut file, MAX_TEXT_READ_BYTES))
                        .unwrap_or_default();
                    let text = String::from_utf8_lossy(&bytes);
                    let mut w = window(&text, 0, max_chars, None);
                    w.offset = offset;
                    w.truncated = true;
                    Content::Text(w)
                }
            }
        }
    };
    let modified = meta
        .modified()
        .ok()
        .map(|t| chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339_opts(chrono::SecondsFormat::Secs, true));
    let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let extension = p.extension().map(|e| e.to_string_lossy().to_lowercase());
    let mime = mime_guess::from_path(p).first().map(|m| m.essence_str().to_string());
    InspectResult::Ok {
        path: path.to_string(),
        name,
        extension,
        size_bytes: size,
        size_human: format_size(size),
        modified,
        mime,
        detected,
        content,
    }
}

/// Read from the current position up to `limit` bytes (the whole file when it's smaller),
/// after rewinding to the start.
fn read_up_to(file: &mut std::fs::File, limit: u64) -> std::io::Result<Vec<u8>> {
    file.seek(SeekFrom::Start(0))?;
    let mut buf = Vec::new();
    file.take(limit).read_to_end(&mut buf)?;
    Ok(buf)
}

/// Handler: inspect one file on a blocking thread, bounded by [`READ_TIMEOUT`].
pub async fn execute_inspect_file<R: Runtime>(_app: &AppHandle<R>, params: &Value) -> ToolResult {
    let Params { path, max_chars, offset } = parse_params(params)?;
    let p = path.clone();
    let job = tokio::task::spawn_blocking(move || inspect_blocking(&p, max_chars, offset));
    let result = match tokio::time::timeout(READ_TIMEOUT, job).await {
        Ok(Ok(r)) => r,
        Ok(Err(e)) => return Err(ToolError::internal(e.to_string())),
        Err(_) => InspectResult::Unreachable { path },
    };
    serde_json::to_value(result).map_err(|e| ToolError::internal(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sniff_recognizes_signatures_and_falls_back_to_text() {
        assert_eq!(sniff(b"%PDF-1.7\n%\xE2\xE3"), Detected::Pdf);
        assert_eq!(sniff(b"\x89PNG\r\n\x1a\n...."), Detected::Png);
        assert_eq!(sniff(b"\xFF\xD8\xFF\xE0"), Detected::Jpeg);
        assert_eq!(sniff(b"PK\x03\x04junk"), Detected::Zip);
        assert_eq!(sniff("hello wörld\n".as_bytes()), Detected::Utf8Text);
        assert_eq!(sniff(b"abc\0def"), Detected::Unknown, "a NUL means binary, never text");
        assert_eq!(sniff(b""), Detected::Unknown);
        // A head that cuts a multibyte char mid-sequence is still text.
        let cut = &"ab€".as_bytes()[..3];
        assert_eq!(sniff(cut), Detected::Utf8Text);
    }

    #[test]
    fn pdf_facts_reads_version_and_counts_pages_not_the_pages_node() {
        let pdf = b"%PDF-1.4\n1 0 obj << /Type /Pages /Kids [] >> endobj 2 0 obj << /Type /Page >> endobj 3 0 obj <</Type/Page>> endobj";
        let facts = pdf_facts(pdf);
        assert_eq!(facts.version.as_deref(), Some("1.4"));
        assert_eq!(facts.page_count_estimate, Some(2));
        assert_eq!(pdf_facts(b"%PDF-1.7\n").page_count_estimate, None, "no pages found is absent, not zero");
    }

    #[test]
    fn window_pages_by_chars_and_flags_truncation_honestly() {
        let text = "αβγδε";
        let w = window(text, 1, 2, Some(5));
        assert_eq!(w.content, "βγ");
        assert_eq!((w.offset, w.returned, w.total_chars, w.truncated), (1, 2, Some(5), true));
        let last = window(text, 3, 10, Some(5));
        assert_eq!(last.content, "δε");
        assert!(!last.truncated, "the tail of the file is not truncated");
        let past = window(text, 9, 4, Some(5));
        assert_eq!(past.returned, 0);
    }

    #[test]
    fn inspect_blocking_answers_text_folder_missing_and_binary() {
        let dir = tempfile::tempdir().unwrap();
        let txt = dir.path().join("notes.md");
        std::fs::write(&txt, "# Title\nline two\n").unwrap();
        match inspect_blocking(txt.to_str().unwrap(), 5, 2) {
            InspectResult::Ok { detected, content: Content::Text(t), mime, extension, .. } => {
                assert_eq!(detected, Detected::Utf8Text);
                assert_eq!(t.content, "Title");
                assert_eq!(t.total_chars, Some(17));
                assert!(t.truncated);
                assert_eq!(extension.as_deref(), Some("md"));
                assert_eq!(mime.as_deref(), Some("text/markdown"));
            }
            other => panic!("expected a text result, got {other:?}"),
        }
        assert!(matches!(
            inspect_blocking(dir.path().to_str().unwrap(), 10, 0),
            InspectResult::Folder { .. }
        ));
        assert!(matches!(
            inspect_blocking(dir.path().join("nope.txt").to_str().unwrap(), 10, 0),
            InspectResult::Missing { .. }
        ));
        let bin = dir.path().join("blob.dat");
        std::fs::write(&bin, [0u8, 1, 2, 3, 0xFF]).unwrap();
        assert!(matches!(
            inspect_blocking(bin.to_str().unwrap(), 10, 0),
            InspectResult::Ok { detected: Detected::Unknown, content: Content::Binary {}, .. }
        ));
        let png = dir.path().join("dot.png");
        image::RgbaImage::new(3, 2).save(&png).unwrap();
        match inspect_blocking(png.to_str().unwrap(), 10, 0) {
            InspectResult::Ok { content: Content::Image(i), .. } => assert_eq!((i.width, i.height), (Some(3), Some(2))),
            other => panic!("expected an image result, got {other:?}"),
        }
    }

    #[test]
    fn params_default_and_cap_the_window() {
        let p = parse_params(&serde_json::json!({ "path": "/a/b.txt" })).unwrap();
        assert_eq!((p.max_chars, p.offset), (DEFAULT_MAX_CHARS, 0));
        let p = parse_params(&serde_json::json!({ "path": "/a/b.txt", "maxChars": 999_999, "offset": 7 })).unwrap();
        assert_eq!((p.max_chars, p.offset), (MAX_MAX_CHARS, 7));
        assert!(parse_params(&serde_json::json!({ "path": "  " })).is_err());
        assert!(parse_params(&serde_json::json!({ "path": "/x", "maxChars": 0 })).is_err());
    }
}
