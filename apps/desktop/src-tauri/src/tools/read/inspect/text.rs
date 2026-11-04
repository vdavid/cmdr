//! The text kind: a line window read through the viewer's own backends, or, with `find`,
//! the matching lines.
//!
//! `read_text` opens the backend the viewer would use (`file_viewer::headless`) and either
//! asks it for the requested lines, which `window_from_chunk` shapes into the model-facing
//! window (lines joined with `\n`, a trailing `\r` stripped, both caps applied, every cut
//! named), or hands it to `find::find_hits`. Never `range_read`: that stitches a selection
//! by UTF-16 offsets for the copy flow, and a line window needs none of it.

use std::path::Path;
use std::sync::atomic::AtomicBool;

use serde::Serialize;

use super::find::{KeyScan, find_hits_within, prescan_for_private_key};
use super::{MAX_LINE_CHARS, MAX_WINDOW_CHARS, ReadFailure, TextAsk, TextContent, WindowOpts};
use crate::file_viewer::encoding::FileEncoding;
use crate::file_viewer::headless::{open_scan_backend, open_text_backend};
use crate::file_viewer::{ChunkEnd, FileViewerBackend, LineChunk, SeekTarget};
use crate::search::format_size;
use crate::tools::read::is_false;
use crate::tools::sensitive::{Protected, refuse_private_key_text};

/// A window of lines from a text file, as the model reads it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextWindow {
    /// 1-based line the window starts at. Where `lineNumbersApproximate` is set, this is
    /// the backend's estimate, not the line asked for.
    pub start_line: usize,
    /// How many lines `content` holds.
    pub returned_lines: usize,
    /// The lines joined with `\n`. No trailing newline.
    pub content: String,
    /// `true` when lines exist past the window, or the char cap stopped the join early.
    pub truncated: bool,
    /// `true` when at least one line was cut at [`MAX_LINE_CHARS`]: the model must know a
    /// minified bundle IS one line, without reading all of it.
    pub lines_cut: bool,
    /// `true` when the deadline stopped the private-key pre-scan before this window's end, so
    /// the lines past `bytesScanned` were left out: they weren't checked.
    #[serde(skip_serializing_if = "is_false")]
    pub scan_incomplete: bool,
    /// Where the pre-scan stopped, when that cut this window.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes_scanned: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes_scanned_human: Option<String>,
}

/// Open the viewer backend for `path` and answer `ask`.
///
/// A private key refuses the whole file, so both asks start with `find`'s pre-scan on
/// `open_scan_backend`. A `find` stays on that backend: the scan streams from byte 0 and numbers
/// lines exactly as it goes, so it needs no index. A window then rides `open_text_backend` (the
/// line index, for exact line seeking), so a window on a large file reads it twice, and the
/// pre-scan spends the deadline first: when it leaves too little for the index, the window falls
/// back to approximate line numbers.
pub(super) fn read_text(
    path: &Path,
    encoding: FileEncoding,
    ask: &TextAsk,
    cancel: &AtomicBool,
) -> Result<TextContent, ReadFailure> {
    let scan_backend = open_scan_backend(path, encoding)?;
    let clean_until = match prescan_for_private_key(scan_backend.as_ref(), cancel)? {
        KeyScan::Clean => None,
        KeyScan::Found => return Err(Protected::PrivateKey.into()),
        KeyScan::StoppedAt(scanned) => Some(scanned),
    };
    match ask {
        TextAsk::Window(opts) => {
            drop(scan_backend);
            let opened = open_text_backend(path, encoding, cancel)?;
            let window = window_within(opened.backend.as_ref(), *opts, clean_until)?;
            Ok(TextContent {
                encoding: encoding.label().to_string(),
                total_lines: opened.backend.total_lines(),
                line_numbers_approximate: !opened.line_numbers_exact,
                window: Some(window),
                find: None,
            })
        }
        TextAsk::Find(matcher) => {
            let backend = scan_backend;
            Ok(TextContent {
                encoding: encoding.label().to_string(),
                total_lines: backend.total_lines(),
                line_numbers_approximate: false,
                window: None,
                find: Some(find_hits_within(backend.as_ref(), matcher, cancel, clean_until)?),
            })
        }
    }
}

/// The asked-for window, holding no line past `clean_until`, where a stopped pre-scan got to: a
/// key's body always follows its `BEGIN`, so what's before that byte is known clean and what's
/// after it isn't. The armor check over the fetched rows is on top of the pre-scan: it joins rows
/// the scan reads apart, so it sees armor straddling a segment break.
pub(super) fn window_within(
    backend: &dyn FileViewerBackend,
    opts: WindowOpts,
    clean_until: Option<u64>,
) -> Result<TextWindow, ReadFailure> {
    // One line more than the window tells us exactly whether more exist, on every backend,
    // without leaning on `total_lines` (unknown on the ByteSeek fallback). Not the call's cancel
    // flag: a deadline that stopped the pre-scan or the line index has set it, and this fetch is
    // bounded.
    let mut chunk = backend.get_lines(
        &SeekTarget::Row(opts.start_line - 1),
        opts.max_lines + 1,
        &AtomicBool::new(false),
    )?;
    refuse_private_key_text(&joined_rows(&chunk))?;
    let Some(limit) = clean_until else {
        return Ok(window_from_chunk(&chunk, opts, MAX_WINDOW_CHARS, MAX_LINE_CHARS));
    };
    let fetched = chunk.rows.len();
    let kept = rows_ending_by(&chunk, limit);
    chunk.rows.truncate(kept);
    if kept < fetched {
        // The cut chunk no longer reaches the end of the file, so its last row's newline
        // isn't the file's final one to show.
        chunk.end = ChunkEnd::CountReached;
    }
    let mut window = window_from_chunk(&chunk, opts, MAX_WINDOW_CHARS, MAX_LINE_CHARS);
    if kept < fetched {
        window.truncated = true;
    }
    // The extra row being the only one cut leaves every line asked for: nothing's missing.
    if kept < fetched && kept < opts.max_lines {
        window.scan_incomplete = true;
        window.bytes_scanned = Some(limit);
        window.bytes_scanned_human = Some(format_size(limit));
    }
    Ok(window)
}

/// How many of a chunk's leading rows end at or before `limit`.
fn rows_ending_by(chunk: &LineChunk, limit: u64) -> usize {
    let row_end = |index: usize| {
        chunk
            .rows
            .get(index + 1)
            .map_or(chunk.end_byte_offset, |next| next.byte_offset)
    };
    (0..chunk.rows.len())
        .take_while(|&index| row_end(index) <= limit)
        .count()
}

/// A chunk's rows as the file holds them: a row Cmdr broke at a segment boundary continues into
/// the next, so armor straddling one still reads whole.
fn joined_rows(chunk: &LineChunk) -> String {
    let mut text = String::new();
    for row in &chunk.rows {
        text.push_str(&row.text);
        if !row.continues {
            text.push('\n');
        }
    }
    text
}

/// Shape a chunk (fetched with `opts.max_lines + 1` lines) into a window. Pure.
///
/// - A request past the end of the file is an empty window with `truncated: false`. The
///   exact backends clamp a past-the-end target to the last line, so the tell is
///   `total_lines` (when known) or an empty chunk.
/// - `truncated` is set when the chunk held more lines than the window, or when the char
///   cap stopped the join before every candidate line was emitted.
/// - Each line loses one trailing `\r` (the backends keep it on CRLF files; the model
///   gains nothing from it) and is cut at `max_line_chars`, which sets `lines_cut`.
pub(crate) fn window_from_chunk(
    chunk: &LineChunk,
    opts: WindowOpts,
    max_chars: usize,
    max_line_chars: usize,
) -> TextWindow {
    let requested = opts.start_line - 1;
    let past_eof = (chunk.total_rows.is_exact() && requested >= chunk.total_rows.rows()) || chunk.rows.is_empty();
    if past_eof {
        return TextWindow {
            start_line: opts.start_line,
            returned_lines: 0,
            content: String::new(),
            truncated: false,
            lines_cut: false,
            scan_incomplete: false,
            bytes_scanned: None,
            bytes_scanned_human: None,
        };
    }

    let more_lines_exist = chunk.rows.len() > opts.max_lines;
    let candidates = &chunk.rows[..chunk.rows.len().min(opts.max_lines)];

    let mut content = String::new();
    let mut returned = 0usize;
    let mut chars_used = 0usize;
    let mut lines_cut = false;
    let mut char_cap_hit = false;
    // ❗ The separator is a newline only after a row the FILE ended. A row Cmdr broke
    // at a segment boundary continues into the next one, so joining those two with a
    // `\n` would hand the model a line break the file does not contain.
    let mut separator_owed = false;
    for row in candidates {
        let raw = &row.text;
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        let (line, cut) = cut_chars(line, max_line_chars);
        let line_chars = line.chars().count();
        let separator = usize::from(separator_owed);
        // The separator counts too; the first line always fits (it's at most `max_line_chars`).
        if returned > 0 && chars_used + separator + line_chars > max_chars {
            char_cap_hit = true;
            break;
        }
        if separator_owed {
            content.push('\n');
            chars_used += 1;
        }
        content.push_str(line);
        chars_used += line_chars;
        returned += 1;
        lines_cut |= cut;
        separator_owed = !row.continues;
    }
    // A window that reaches the end of a file ending in a newline carries that newline:
    // no empty row follows it, so the last row's own flag is where the fact lives.
    let reached_eof = returned == chunk.rows.len() && chunk.end == ChunkEnd::EndOfFile;
    if reached_eof && chunk.rows.last().is_some_and(|row| row.ends_at_newline) {
        content.push('\n');
    }

    TextWindow {
        start_line: chunk.first_row_number + 1,
        returned_lines: returned,
        content,
        truncated: more_lines_exist || char_cap_hit,
        lines_cut,
        scan_incomplete: false,
        bytes_scanned: None,
        bytes_scanned_human: None,
    }
}

/// Cut `line` to `max` chars (never mid-codepoint), reporting whether it was cut.
pub(super) fn cut_chars(line: &str, max: usize) -> (&str, bool) {
    match line.char_indices().nth(max) {
        Some((byte_idx, _)) => (&line[..byte_idx], true),
        None => (line, false),
    }
}
