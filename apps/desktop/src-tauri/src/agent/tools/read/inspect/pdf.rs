//! The PDF kind: the header version, the exact page count, title and author, and the text
//! of a page range (or, with `find`, the matching lines across pages), through
//! `pdf-extract` (which re-exports `lopdf`, so the structure and the text come from one
//! dependency).
//!
//! Every call into the parser runs inside `crash_reporter::contain_panics`: `pdf-extract`
//! carries ~100 panic sites on untrusted input, and a malformed page has to cost one
//! `warn` line and an `unparseable` mark, never a crash report. The closures wrap the
//! foreign calls only; the shapers around them are ours and keep reporting.

use std::ops::ControlFlow;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use pdf_extract::Document;
use serde::Serialize;

use super::find::{FIND_SNIPPET_CHARS, FindHits, FindLine, MAX_FIND_LINES, snippet_around_byte};
use super::text::cut_chars;
use super::{MAX_WINDOW_CHARS, TextAsk, WindowOpts};
use crate::agent::tools::read::is_false;
use crate::crash_reporter::contain_panics;
use crate::file_viewer::{MAX_SEARCH_MATCHES, Matcher};

/// Pages in the window when the caller doesn't say.
pub(crate) const DEFAULT_MAX_PAGES: usize = 3;
/// The most pages one window holds; a larger ask is clamped (the window reports what it
/// returned, so the clamp is visible). A 300-page manual is read in slices, by choice.
pub(crate) const MAX_MAX_PAGES: usize = 20;
/// A PDF over this many bytes is not parsed: the parser loads the whole file into memory
/// and walks its object table, and 64 MiB is already a scanned book. The row still carries
/// the header version.
pub(crate) const MAX_PDF_BYTES: u64 = 64 * 1024 * 1024;
/// One page's text is cut here. Two dense pages fill a row's `MAX_WINDOW_CHARS`; a single
/// page over it (a dense two-column layout) is rare and says so through `truncated`. The
/// cap is high on purpose: a whole page the model can ask for again beats a slice of one it
/// can't (there is no offset inside a page).
pub(crate) const MAX_PAGE_CHARS: usize = 8_000;
const _: () = assert!(MAX_PAGE_CHARS * 2 <= MAX_WINDOW_CHARS, "two dense pages fit one row");

// ── Result DTOs ─────────────────────────────────────────────────────────────

/// Why a PDF's text isn't in the row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PdfTextUnavailable {
    /// The document is encrypted. The tool has no password path (the viewer prompts the
    /// user; the agent can't), so neither the text nor the Info strings are read.
    Encrypted,
    /// Over [`MAX_PDF_BYTES`]; the file wasn't parsed.
    TooLarge,
    /// The parser refused the file (or panicked on it, contained). The header version is
    /// still answered when the first bytes carry one.
    Unparseable,
}

/// One page's text, as the model reads it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfPage {
    /// 1-based, as a PDF viewer numbers pages.
    pub page: usize,
    /// The page's text, trimmed. Empty for a page with no text layer (a scan, a drawing).
    pub text: String,
    /// `true` when the text was cut at [`MAX_PAGE_CHARS`].
    pub truncated: bool,
    /// `true` when the parser couldn't decode this page (its `text` is then empty and says
    /// nothing about a text layer).
    #[serde(skip_serializing_if = "is_false")]
    pub unparseable: bool,
}

/// A window of pages, the PDF twin of `TextWindow`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfWindow {
    /// 1-based page the window starts at, as asked.
    pub page_start: usize,
    /// How many pages `pages` holds.
    pub returned_pages: usize,
    pub pages: Vec<PdfPage>,
    /// `true` when pages exist past the window: past `maxPages`, or the row's char cap or
    /// the deadline stopped the loop early (then `returnedPages` is short of the ask).
    pub truncated: bool,
}

/// What a PDF says about itself, and the text asked for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfContent {
    /// From the `%PDF-x.y` header: "1.7". Absent when the first bytes don't carry one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Exact, from the page tree. Absent when the file wasn't parsed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_count: Option<usize>,
    /// The Info dictionary's `Title`, when present and non-blank.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// The Info dictionary's `Author`, when present and non-blank.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    /// `false` when every page decoded in this call held whitespace only (a scan, not an
    /// empty document); `true` when any held text. Absent when no page was decoded (text
    /// unavailable, a range past the end, or every page unparseable).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_text_layer: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_unavailable: Option<PdfTextUnavailable>,
    /// The page window. Absent with `find`, and when text is unavailable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pages: Option<PdfWindow>,
    /// The matching lines, each with its `page`. Present exactly when the call carried
    /// `find` and the text was available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub find: Option<FindHits>,
}

impl PdfContent {
    /// A row with no text: the header version and the reason.
    fn without_text(version: Option<String>, reason: PdfTextUnavailable) -> Self {
        Self {
            version,
            page_count: None,
            title: None,
            author: None,
            has_text_layer: None,
            text_unavailable: Some(reason),
            pages: None,
            find: None,
        }
    }
}

// ── Reading ─────────────────────────────────────────────────────────────────

/// Read `path` as a PDF under the production size cap. `head` is the classifier's head
/// buffer, for the version when the file isn't parsed. Blocking; `cancel` is checked
/// between pages.
pub(super) fn read_pdf(
    path: &Path,
    size: u64,
    head: &[u8],
    ask: &TextAsk,
    cancel: &AtomicBool,
) -> Result<PdfContent, std::io::Error> {
    read_pdf_with_cap(path, size, head, ask, cancel, MAX_PDF_BYTES)
}

/// [`read_pdf`] with the size cap injected (tests shrink it rather than write 64 MiB).
///
/// The parser's own header read is one of the things that can refuse a file, so the
/// version comes from our [`header_version`] over the head bytes first and the parsed
/// document only confirms it.
pub(super) fn read_pdf_with_cap(
    path: &Path,
    size: u64,
    head: &[u8],
    ask: &TextAsk,
    cancel: &AtomicBool,
    cap: u64,
) -> Result<PdfContent, std::io::Error> {
    let header_version = header_version(head);
    if size > cap {
        return Ok(PdfContent::without_text(header_version, PdfTextUnavailable::TooLarge));
    }
    let bytes = std::fs::read(path)?;
    let Some(Ok(doc)) = contain_panics(|| Document::load_mem(&bytes)) else {
        return Ok(PdfContent::without_text(
            header_version,
            PdfTextUnavailable::Unparseable,
        ));
    };
    let version = Some(doc.version.clone()).filter(|v| !v.is_empty()).or(header_version);
    // The page tree is what every later step walks; a tree that panics the parser is a
    // file it can't serve, whatever the header says.
    let Some(page_count) = contain_panics(|| doc.get_pages().len()) else {
        return Ok(PdfContent::without_text(version, PdfTextUnavailable::Unparseable));
    };
    if contain_panics(|| doc.is_encrypted()).unwrap_or(true) {
        // Strings and streams are ciphertext; the Info dictionary is not read either, so
        // an undecodable title is never guessed at.
        return Ok(PdfContent {
            page_count: Some(page_count),
            ..PdfContent::without_text(version, PdfTextUnavailable::Encrypted)
        });
    }

    let extract = |page: usize| extract_page(&doc, page);
    let (pages, find, has_text_layer) = match ask {
        TextAsk::Window(opts) => {
            let (window, has_text) =
                window_from_pages(page_count, *opts, cancel, MAX_WINDOW_CHARS, MAX_PAGE_CHARS, extract);
            (Some(window), None, has_text)
        }
        TextAsk::Find(matcher) => {
            let (hits, has_text) = find_in_pages(page_count, matcher, cancel, extract);
            (None, Some(hits), has_text)
        }
    };
    Ok(PdfContent {
        version,
        page_count: Some(page_count),
        title: info_string(&doc, b"Title"),
        author: info_string(&doc, b"Author"),
        has_text_layer,
        text_unavailable: None,
        pages,
        find,
    })
}

/// The `x.y` of a `%PDF-x.y` header, when the first bytes carry one. Pure, and ours.
pub(crate) fn header_version(head: &[u8]) -> Option<String> {
    let rest = head.strip_prefix(b"%PDF-")?;
    let end = rest
        .iter()
        .position(|b| !(b.is_ascii_digit() || *b == b'.'))
        .unwrap_or(rest.len());
    let version = std::str::from_utf8(&rest[..end]).ok()?;
    (!version.is_empty()).then(|| version.to_string())
}

/// One Info dictionary string (`Title`, `Author`), decoded from PDFDocEncoding or UTF-16
/// by `lopdf`, trimmed, and absent when missing, undecodable, or blank. The dereferences
/// are the parser's, so they run contained.
fn info_string(doc: &Document, key: &[u8]) -> Option<String> {
    contain_panics(|| {
        let info = doc.trailer.get(b"Info").ok()?;
        let (_, info) = doc.dereference(info).ok()?;
        let value = info.as_dict().ok()?.get(key).ok()?;
        let (_, value) = doc.dereference(value).ok()?;
        pdf_extract::decode_text_string(value).ok()
    })
    .flatten()
    .map(|s| s.trim().to_string())
    .filter(|s| !s.is_empty())
}

/// One page's decoded text, or the parser's refusal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PageText {
    Text(String),
    Unparseable,
}

/// The text of one page (1-based) through `pdf-extract`'s plain-text device, contained: a
/// page the parser refuses, or panics on, is `Unparseable`, and the rest of the document
/// still reads.
fn extract_page(doc: &Document, page: usize) -> PageText {
    let Ok(page_num) = u32::try_from(page) else {
        return PageText::Unparseable;
    };
    let mut text = String::new();
    let outcome = contain_panics(|| {
        let mut output = pdf_extract::PlainTextOutput::new(&mut text);
        pdf_extract::output_doc_page(doc, &mut output, page_num)
    });
    match outcome {
        Some(Ok(())) => PageText::Text(text),
        // A refusal or a contained panic; the buffer may hold a partial page, which would
        // read as the page's text and isn't.
        Some(Err(_)) | None => PageText::Unparseable,
    }
}

// ── The page loops (pure over `extract`) ─────────────────────────────────────

/// The page window: `opts.page_start` onward, at most `opts.max_pages`, each page trimmed
/// and cut at `max_page_chars`, the loop stopping before a page that would take the row
/// past `max_chars` (whole pages only, so the model can ask for the next one by number;
/// the first page always fits) and when `cancel` flips. Pure over `extract`.
///
/// Returns the window and the text-layer verdict: `Some(true)` when any decoded page held
/// text, `Some(false)` when every decoded page was whitespace, `None` when none decoded.
pub(crate) fn window_from_pages(
    page_count: usize,
    opts: WindowOpts,
    cancel: &AtomicBool,
    max_chars: usize,
    max_page_chars: usize,
    mut extract: impl FnMut(usize) -> PageText,
) -> (PdfWindow, Option<bool>) {
    let first = opts.page_start;
    let last = page_count.min(first.saturating_add(opts.max_pages).saturating_sub(1));
    let mut pages = Vec::new();
    let mut chars_used = 0usize;
    let mut verdict = TextLayerVerdict::default();
    for page in first..=last {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let (text, truncated, unparseable) = match extract(page) {
            PageText::Text(raw) => {
                let (cut, was_cut) = cut_chars(raw.trim(), max_page_chars);
                (cut.to_string(), was_cut, false)
            }
            PageText::Unparseable => (String::new(), false, true),
        };
        let page_chars = text.chars().count();
        if !pages.is_empty() && chars_used + page_chars > max_chars {
            break;
        }
        chars_used += page_chars;
        if !unparseable {
            verdict.saw(&text);
        }
        pages.push(PdfPage {
            page,
            text,
            truncated,
            unparseable,
        });
    }
    // Pages exist past the returned ones: past the ask, or the cap or the flag stopped
    // the loop. A start past the end has nothing past it.
    let truncated = first <= page_count && first + pages.len() <= page_count;
    (
        PdfWindow {
            page_start: first,
            returned_pages: pages.len(),
            pages,
            truncated,
        },
        verdict.into_option(),
    )
}

/// `find` over a PDF: every page in order, each page's text split into lines and searched
/// with the call's `Matcher`, until [`MAX_FIND_LINES`] lines are carried (the page that
/// fills the cap is finished, then the scan stops: decoding pages only to count is not
/// worth the deadline), the viewer's match cap, or `cancel`. Pure over `extract`.
pub(crate) fn find_in_pages(
    page_count: usize,
    matcher: &Matcher,
    cancel: &AtomicBool,
    mut extract: impl FnMut(usize) -> PageText,
) -> (FindHits, Option<bool>) {
    let mut lines: Vec<FindLine> = Vec::new();
    let mut total_matches = 0usize;
    let mut matches_capped = false;
    let mut matching_lines = 0usize;
    let mut pages_scanned = 0usize;
    let mut verdict = TextLayerVerdict::default();
    'pages: for page in 1..=page_count {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let raw = match extract(page) {
            PageText::Text(raw) => raw,
            PageText::Unparseable => {
                pages_scanned = page;
                continue;
            }
        };
        // Trimmed as the window trims, so a hit's `line` is the line the window shows.
        let text = raw.trim();
        verdict.saw(text);
        for (index, line) in text.lines().enumerate() {
            let mut on_line = 0usize;
            let mut first_match_byte = None;
            matcher.find_matches(line, |start, _end| {
                if total_matches >= MAX_SEARCH_MATCHES {
                    matches_capped = true;
                    return ControlFlow::Break(());
                }
                total_matches += 1;
                on_line += 1;
                first_match_byte.get_or_insert(start);
                ControlFlow::Continue(())
            });
            if let Some(first_match_byte) = first_match_byte {
                matching_lines += 1;
                if lines.len() < MAX_FIND_LINES {
                    lines.push(FindLine {
                        page: Some(page),
                        line: index + 1,
                        matches: on_line,
                        text: snippet_around_byte(line, first_match_byte, FIND_SNIPPET_CHARS),
                    });
                }
            }
            if matches_capped {
                break 'pages;
            }
        }
        pages_scanned = page;
        if lines.len() >= MAX_FIND_LINES {
            break;
        }
    }
    // The cap is its own reason to stop and says so through `matchesCapped`; the flag and
    // the line cap leave pages undecoded, which is what `scanIncomplete` reports.
    let scan_incomplete = !matches_capped && pages_scanned < page_count;
    let returned_lines = lines.len();
    (
        FindHits {
            total_matches,
            matches_capped,
            truncated: returned_lines < matching_lines,
            lines,
            returned_lines,
            scan_incomplete,
            pages_scanned: scan_incomplete.then_some(pages_scanned),
            bytes_scanned: None,
            bytes_scanned_human: None,
            total_bytes: None,
            total_bytes_human: None,
        },
        verdict.into_option(),
    )
}

/// What the decoded pages said about a text layer, as the loops accumulate it.
#[derive(Default)]
struct TextLayerVerdict {
    decoded_any: bool,
    saw_text: bool,
}

impl TextLayerVerdict {
    /// One decoded page's (trimmed) text.
    fn saw(&mut self, trimmed: &str) {
        self.decoded_any = true;
        self.saw_text |= !trimmed.is_empty();
    }

    fn into_option(self) -> Option<bool> {
        self.decoded_any.then_some(self.saw_text)
    }
}
