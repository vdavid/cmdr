//! The text kind: a line window read through the viewer's own backends.
//!
//! `read_text` opens the backend the viewer would use (`file_viewer::headless`), asks it
//! for the requested lines, and `window_from_chunk` shapes the chunk into the model-facing
//! window: lines joined with `\n`, a trailing `\r` stripped, both caps applied, and every
//! cut named. Never `range_read`: that stitches a selection by UTF-16 offsets for the copy
//! flow, and a line window needs none of it.

use std::path::Path;
use std::sync::atomic::AtomicBool;

use serde::Serialize;

use super::{MAX_LINE_CHARS, MAX_WINDOW_CHARS, TextContent, WindowOpts};
use crate::file_viewer::encoding::FileEncoding;
use crate::file_viewer::headless::open_text_backend;
use crate::file_viewer::{LineChunk, SeekTarget, ViewerError};

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
}

/// Open the viewer backend for `path` and read one window. The file is opened once per
/// call; there is no per-page re-read because a call is one page.
pub(super) fn read_text(
    path: &Path,
    encoding: FileEncoding,
    opts: WindowOpts,
    cancel: &AtomicBool,
) -> Result<TextContent, ViewerError> {
    let opened = open_text_backend(path, encoding, cancel)?;
    // One line more than the window tells us exactly whether more exist, on every
    // backend, without leaning on `total_lines` (unknown on the ByteSeek fallback).
    let chunk = opened
        .backend
        .get_lines(&SeekTarget::Line(opts.start_line - 1), opts.max_lines + 1)?;
    Ok(TextContent {
        encoding: encoding.label().to_string(),
        total_lines: opened.backend.total_lines(),
        line_numbers_approximate: !opened.line_numbers_exact,
        window: window_from_chunk(&chunk, opts, MAX_WINDOW_CHARS, MAX_LINE_CHARS),
    })
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
    let past_eof = chunk.total_lines.is_some_and(|total| requested >= total) || chunk.lines.is_empty();
    if past_eof {
        return TextWindow {
            start_line: opts.start_line,
            returned_lines: 0,
            content: String::new(),
            truncated: false,
            lines_cut: false,
        };
    }

    let more_lines_exist = chunk.lines.len() > opts.max_lines;
    let candidates = &chunk.lines[..chunk.lines.len().min(opts.max_lines)];

    let mut content = String::new();
    let mut returned = 0usize;
    let mut chars_used = 0usize;
    let mut lines_cut = false;
    let mut char_cap_hit = false;
    for raw in candidates {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        let (line, cut) = cut_chars(line, max_line_chars);
        let line_chars = line.chars().count();
        // The separator counts too; the first line always fits (it's at most `max_line_chars`).
        if returned > 0 && chars_used + 1 + line_chars > max_chars {
            char_cap_hit = true;
            break;
        }
        if returned > 0 {
            content.push('\n');
            chars_used += 1;
        }
        content.push_str(line);
        chars_used += line_chars;
        returned += 1;
        lines_cut |= cut;
    }

    TextWindow {
        start_line: chunk.first_line_number + 1,
        returned_lines: returned,
        content,
        truncated: more_lines_exist || char_cap_hit,
        lines_cut,
    }
}

/// Cut `line` to `max` chars (never mid-codepoint), reporting whether it was cut.
fn cut_chars(line: &str, max: usize) -> (&str, bool) {
    match line.char_indices().nth(max) {
        Some((byte_idx, _)) => (&line[..byte_idx], true),
        None => (line, false),
    }
}
